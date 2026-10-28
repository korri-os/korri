use super::{
    relay,
    settings::StunList,
    signal::{Answer, Offer, SIGNAL_LIFETIME},
    stun, tunnel, Environment, InterfaceReader, LiveRoute, Routes, ALPN,
};
use crate::{
    config::snapshot::{ConfigSnapshotCoordinator, SnapshotAuthorization},
    federation::{connect, interfaces, FederationDirectory, PeerState},
    peer_rpc::{unix_time, PeerCredentials},
    relay::{
        CoordinatedRelays, CoordinationCommand, RelayCoordinator, RelayList,
        WebSocketRelayTransport,
    },
};
use futures::StreamExt;
use iroh::{
    endpoint::{Builder, Connection},
    Endpoint, EndpointAddr, EndpointId, RelayMode, TransportAddr,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    net::{IpAddr, SocketAddr},
    path::Path,
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};
use tokio::{
    task::{JoinHandle, JoinSet},
    time::{timeout, Instant},
};

#[cfg(test)]
#[path = "controller_tests.rs"]
mod tests;

/// Routers own this guard. The task owns no copy of it. Dropping the last
/// router cancels its endpoint, relay, connections and forwarding tasks.
#[derive(Clone)]
pub(crate) struct ReachLifetime(Arc<Lifetime>);
struct Lifetime {
    task: JoinHandle<()>,
    routes: Routes,
}
impl Drop for Lifetime {
    fn drop(&mut self) {
        self.task.abort();
        self.routes.clear();
    }
}
impl std::fmt::Debug for ReachLifetime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ReachLifetime")
            .field(&Arc::strong_count(&self.0))
            .finish()
    }
}
pub(crate) fn start(
    root: &Path,
    directory: FederationDirectory,
    credentials: PeerCredentials,
    config_root: Option<&Path>,
    peer_router: axum::Router,
    relay_port: u16,
    environment: Environment,
) -> Option<ReachLifetime> {
    // Some unit tests compose routers synchronously. Korrid constructs its
    // production routers inside its runtime. It starts no private runtime.
    let runtime = tokio::runtime::Handle::try_current().ok()?;
    let key = match crate::identity::iroh::load_or_create(root) {
        Ok(key) => key,
        Err(_) => {
            log::warn!("reach transport key is unavailable");
            return None;
        }
    };
    let routes = directory.reach.clone();
    let config = config_root.map(ConfigSnapshotCoordinator::new);
    let controller = Controller {
        directory,
        credentials,
        config,
        key,
        peer_router,
        relay_port,
        environment,
        endpoint: None,
        tasks: JoinSet::new(),
        relay_task: None,
        bindings: Arc::new(RwLock::new(BTreeMap::new())),
        dialing: Arc::new(Mutex::new(BTreeSet::new())),
        relay_addresses: Arc::new(RwLock::new(Vec::new())),
        pending: BTreeMap::new(),
        processed: BTreeMap::new(),
        retry: BTreeMap::new(),
        relaying: Arc::new(RwLock::new(BTreeMap::new())),
    };
    let task = runtime.spawn(controller.run());
    Some(ReachLifetime(Arc::new(Lifetime { task, routes })))
}

struct Pending {
    request_id: String,
    expires_at: u64,
    token: crate::federation::WorkToken,
}

type RelayAnswers = Arc<RwLock<BTreeMap<String, (u64, Vec<SocketAddr>)>>>;

struct Controller {
    directory: FederationDirectory,
    credentials: PeerCredentials,
    config: Option<ConfigSnapshotCoordinator>,
    key: iroh::SecretKey,
    peer_router: axum::Router,
    relay_port: u16,
    environment: Environment,
    endpoint: Option<Endpoint>,
    tasks: JoinSet<()>,
    relay_task: Option<JoinHandle<()>>,
    bindings: relay::Bindings,
    dialing: Arc<Mutex<BTreeSet<String>>>,
    relay_addresses: Arc<RwLock<Vec<SocketAddr>>>,
    pending: BTreeMap<String, Pending>,
    processed: BTreeMap<(String, String), u64>,
    retry: BTreeMap<String, (Instant, u32)>,
    relaying: RelayAnswers,
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.directory.reach.clear();
        if let Some(task) = &self.relay_task {
            task.abort();
        }
    }
}
impl Controller {
    fn settings(&self) -> io::Result<(Option<RelayList>, StunList)> {
        // Read both lists from ONE authorized snapshot, then apply the same
        // explicit JSON-array environment override as the existing Linux path.
        let state = self.config.as_ref().map(|config| config.reload());
        if state
            .as_ref()
            .is_some_and(|s| s.authorization != SnapshotAuthorization::Authorized)
        {
            return Err(io::Error::other("reach settings are unauthorized"));
        }
        let relays = if let Some(relays) = &self.environment.relays {
            Some(relays.clone())
        } else {
            RelayList::from_linux_environment(std::env::var("KORRID_RELAYS").ok().as_deref())
                .map_err(io::Error::other)?
                .or(state
                    .as_ref()
                    .map(|s| RelayList::from_device_settings(&s.snapshot))
                    .transpose()
                    .map_err(io::Error::other)?
                    .flatten())
        };
        let stun = StunList::from_linux_environment(std::env::var("KORRID_STUN").ok().as_deref())
            .map_err(io::Error::other)?
            .or(state
                .as_ref()
                .map(|s| StunList::from_device_settings(&s.snapshot))
                .transpose()
                .map_err(io::Error::other)?
                .flatten())
            .unwrap_or_default();
        Ok((relays, stun))
    }
    async fn reset(&mut self) {
        self.directory.reach.clear();
        if let Some(task) = self.relay_task.take() {
            task.abort();
            let _ = task.await;
        }
        self.tasks.abort_all();
        while self.tasks.join_next().await.is_some() {}
        if let Some(endpoint) = self.endpoint.take() {
            endpoint.close().await;
        }
        self.pending.clear();
        self.retry.clear();
        self.processed.clear();
        self.relaying.write().expect("relay answers").clear();
        self.bindings.write().expect("reach bindings").clear();
        self.relay_addresses
            .write()
            .expect("relay addresses")
            .clear();
    }
    async fn refresh_endpoint(&mut self) {
        // There is no healthy incoming or outgoing connection. Keep the
        // independent embedded TCP relay and its clients alive during rebind.
        self.directory.reach.clear_connections(false);
        self.tasks.abort_all();
        while self.tasks.join_next().await.is_some() {}
        if let Some(endpoint) = self.endpoint.take() {
            endpoint.close().await;
        }
        self.pending.clear();
        // Keep retry backoff, bindings and still-current relay answers. The
        // next endpoint runs STUN again before bind and sends a fresh offer.
    }
    async fn ensure_endpoint(&mut self, stun: &StunList) -> io::Result<Endpoint> {
        if let Some(endpoint) = &self.endpoint {
            return Ok(endpoint.clone());
        }
        let addresses = (self.environment.interfaces)()?;
        let ips = addresses
            .iter()
            .filter(|a| a.interface != "tailscale0" && relay::allowed_relay_ip(a.address))
            .map(|a| a.address)
            .collect::<BTreeSet<_>>();
        if ips.is_empty() {
            return Err(io::Error::other("no non-VPN interface"));
        }
        // Same-port public STUN precedes bind. Builder::empty has no address
        // lookup and no relay defaults. Explicit interface binds exclude VPNs
        // from QUIC's internal address exchange too, not only from the signal.
        let (port, external) = stun::discover(&stun.0).await?;
        let mut builder = Builder::empty()
            .crypto_provider(iroh::tls::default_provider())
            .secret_key(self.key.clone())
            .alpns(vec![ALPN.to_vec()])
            .clear_ip_transports()
            .relay_mode(RelayMode::Custom(iroh::RelayMap::empty()));
        let default = |ipv4: bool| {
            addresses
                .iter()
                .find(|a| {
                    a.has_default_route && a.address.is_ipv4() == ipv4 && ips.contains(&a.address)
                })
                .map(|a| a.address)
                .or_else(|| ips.iter().copied().find(|ip| ip.is_ipv4() == ipv4))
        };
        let defaults = [default(true), default(false)];
        for ip in &ips {
            if !defaults.contains(&Some(*ip)) {
                builder = builder
                    .bind_addr_with_opts(
                        SocketAddr::new(*ip, port),
                        iroh::endpoint::BindOpts::default()
                            .set_prefix_len(if ip.is_ipv4() { 32 } else { 128 })
                            .set_is_default_route(false),
                    )
                    .map_err(io::Error::other)?;
            }
        }
        for ip in defaults.into_iter().flatten() {
            builder = builder
                .bind_addr(SocketAddr::new(ip, port))
                .map_err(io::Error::other)?;
        }
        let endpoint = builder.bind().await.map_err(io::Error::other)?;
        for address in external {
            endpoint.add_external_addr(address).await;
        }
        let access = relay::Membership {
            directory: self.directory.clone(),
            bindings: self.bindings.clone(),
            local: self.key.public(),
        };
        let relay_addresses = self.relay_addresses.clone();
        if self.relay_task.is_none() {
            self.relay_task = Some(tokio::spawn(relay::supervise(
                self.environment.relay.clone(),
                access.clone(),
                relay::listener_ips(&addresses),
                relay_addresses,
                self.relay_port,
            )));
        }
        let recovery = self.directory.reach.recovery.clone();
        recovery.lock().expect("reach recovery").started();
        let incoming_endpoint = endpoint.clone();
        let router = self.peer_router.clone();
        self.tasks.spawn(async move {
            let mut sessions = JoinSet::new();
            loop {
                tokio::select! {
                    incoming = incoming_endpoint.accept(), if sessions.len() < 64 => {
                        let Some(incoming) = incoming else {break};
                        let (router,access,recovery) = (router.clone(),access.clone(),recovery.clone());
                        sessions.spawn(async move {
                            let Ok(Ok(connection)) = timeout(Duration::from_secs(10),incoming).await else {return};
                            // QUIC authenticates remote_id. The member's
                            // device-signed signal must bind that id.
                            if !access.allows(connection.remote_id()) {
                                connection.close(1u32.into(),b"not a bound federation device");return;
                            }
                            let device = if connection.remote_id() == access.local {
                                access.directory.credentials.public_key().ok()
                            } else {
                                access.bindings.read().expect("reach bindings").get(&connection.remote_id()).cloned()
                            };
                            let Some(device) = device else {connection.close(1u32.into(),b"unknown device");return};
                            if !recovery.lock().expect("reach recovery").register(device,connection.clone()) {return;}
                            let monitor = async {
                                loop {
                                    tokio::time::sleep(Duration::from_secs(5)).await;
                                    if !access.allows(connection.remote_id()) {
                                        connection.close(1u32.into(),b"federation membership changed");break;
                                    }
                                }
                            };
                            tokio::select! {
                                _ = tunnel::serve(connection.clone(),router) => {},
                                _ = monitor => {},
                            }
                        });
                    }
                    _ = sessions.join_next(), if !sessions.is_empty() => {}
                }
            }
        });
        self.directory
            .reach
            .active
            .store(true, std::sync::atomic::Ordering::Release);
        self.endpoint = Some(endpoint.clone());
        // Give the first bounded facts read time to publish a usable listener
        // before the first answer. Battery and unknown metering still deny it.
        for _ in 0..20 {
            if !self
                .relay_addresses
                .read()
                .expect("relay addresses")
                .is_empty()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(endpoint)
    }
    async fn run(mut self) {
        loop {
            let interfaces = self.environment.interfaces.clone();
            let previous = network_snapshot(&interfaces);
            let changed = network_changed_with(
                previous,
                move || network_snapshot(&interfaces),
                interfaces::ADDRESS_POLL,
            );
            tokio::select! {
                _ = self.drive() => break,
                _ = changed => self.reset().await,
            }
        }
    }
    async fn drive(&mut self) {
        let mut old_settings = None;
        loop {
            if self.endpoint.is_some()
                && self
                    .directory
                    .reach
                    .recovery
                    .lock()
                    .expect("reach recovery")
                    .take_refresh(Instant::now())
            {
                log::info!("refreshing iroh endpoint after connection loss and two failed reconnect attempts");
                self.refresh_endpoint().await;
            }
            let settings = self.settings();
            let Ok((Some(relays), stun)) = settings else {
                self.reset().await;
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            };
            if old_settings.as_ref() != Some(&(relays.clone(), stun.clone())) {
                self.reset().await;
                old_settings = Some((relays.clone(), stun.clone()));
            }
            let Ok(identity) = self.credentials.identity_snapshot() else {
                break;
            };
            if !matches!(
                identity.state(),
                crate::identity::IdentityState::Owned { .. }
            ) {
                self.reset().await;
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
            // Only members send or answer reach signals. A device with no other
            // member, such as a local-only automatic owner, has nobody to reach
            // and must not contact the public relays.
            if self
                .directory
                .snapshot()
                .is_ok_and(|peers| peers.is_empty())
            {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            let Ok(signal) =
                CoordinatedRelays::new(relays, identity, Arc::new(WebSocketRelayTransport::new()))
            else {
                break;
            };
            let now = unix_time();
            self.processed.retain(|_, expires| *expires > now);
            for (device, pending) in self
                .pending
                .iter()
                .filter(|(_, pending)| pending.expires_at <= now)
            {
                handle_signalling_failure(&self.directory, device, &pending.token).await;
            }
            self.pending.retain(|_, pending| pending.expires_at > now);
            self.relaying
                .write()
                .expect("relay answers")
                .retain(|_, (expires, _)| *expires > now);
            self.receive_reach(&signal, &stun, now).await;
            let Ok(peers) = self.directory.snapshot() else {
                self.reset().await;
                continue;
            };
            let devices = peers
                .iter()
                .map(|p| p.device_public_key.clone())
                .collect::<BTreeSet<_>>();
            self.directory
                .reach
                .recovery
                .lock()
                .expect("reach recovery")
                .retain_members(&devices);
            self.directory
                .reach
                .live
                .write()
                .expect("reach routes")
                .retain(|device, route| {
                    let keep =
                        devices.contains(device) && route.connection.close_reason().is_none();
                    if !keep {
                        route
                            .connection
                            .close(0u32.into(), b"membership or connection changed");
                    }
                    keep
                });
            for peer in peers.into_iter().take(1024) {
                self.offer_peer(peer, &signal, &stun, now).await;
            }
            while self.tasks.try_join_next().is_some() {}
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
    async fn receive_reach<T: crate::relay::RelayTransport + 'static>(
        &mut self,
        signal: &CoordinatedRelays<T>,
        stun: &StunList,
        now: u64,
    ) {
        if let Ok(mut batches) = signal.receive_batches(now) {
            while let Some(received) = batches.next().await {
                for command in received.commands {
                    // Relay reads can wait several seconds. Bind against the
                    // current clock, not the timestamp before that I/O.
                    let now = unix_time();
                    match command {
                        CoordinationCommand::ReachOffer {
                            sender_device_public_key,
                            offer,
                        } => {
                            if self.processed.len() >= 4096
                                || self.processed.contains_key(&(
                                    sender_device_public_key.clone(),
                                    offer.request_id.clone(),
                                ))
                            {
                                continue;
                            }
                            if bind_offer(
                                &self.directory,
                                &self.bindings,
                                &sender_device_public_key,
                                &offer,
                                now,
                            )
                            .is_err()
                            {
                                continue;
                            }
                            let Ok(endpoint) = self.ensure_endpoint(stun).await else {
                                continue;
                            };
                            let direct = endpoint_addresses(&endpoint);
                            let relay_addresses = self
                                .relay_addresses
                                .read()
                                .expect("relay addresses")
                                .clone();
                            let answer = Answer {
                                endpoint_id: self.key.public().to_string(),
                                direct_addresses: direct,
                                expires_at: now + SIGNAL_LIFETIME,
                                request_id: offer.request_id.clone(),
                                can_relay_now: !relay_addresses.is_empty(),
                                relay_addresses,
                            };
                            let command = CoordinationCommand::ReachAnswer {
                                sender_device_public_key: self
                                    .credentials
                                    .public_key()
                                    .unwrap_or_default(),
                                answer,
                            };
                            if signal
                                .queue_command(&sender_device_public_key, command, now)
                                .await
                                .is_ok_and(|p| p.accepted())
                            {
                                self.processed.insert(
                                    (sender_device_public_key.clone(), offer.request_id.clone()),
                                    offer.expires_at,
                                );
                            }
                            self.dial(
                                sender_device_public_key,
                                offer.endpoint_id,
                                offer.direct_addresses,
                                Vec::new(),
                            );
                        }
                        CoordinationCommand::ReachAnswer {
                            sender_device_public_key,
                            answer,
                        } => {
                            if self
                                .pending
                                .get(&sender_device_public_key)
                                .is_none_or(|pending| {
                                    pending.request_id != answer.request_id
                                        || pending.expires_at <= now
                                })
                            {
                                continue;
                            }
                            if answer.validate(now).is_err() {
                                continue;
                            }
                            let offer = Offer {
                                endpoint_id: answer.endpoint_id.clone(),
                                direct_addresses: answer.direct_addresses.clone(),
                                expires_at: answer.expires_at,
                                request_id: answer.request_id.clone(),
                            };
                            if bind_offer(
                                &self.directory,
                                &self.bindings,
                                &sender_device_public_key,
                                &offer,
                                now,
                            )
                            .is_err()
                            {
                                continue;
                            }
                            self.pending.remove(&sender_device_public_key);
                            // A current answer is the only source of relay URLs.
                            // Each URL names the actual embedded TCP listener.
                            self.relaying.write().expect("relay answers").insert(
                                sender_device_public_key.clone(),
                                (answer.expires_at, answer.relay_addresses.clone()),
                            );
                            self.dial(
                                sender_device_public_key,
                                answer.endpoint_id,
                                answer.direct_addresses,
                                Vec::new(),
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    async fn offer_peer(
        &mut self,
        peer: crate::federation::PeerSnapshot,
        signal: &impl RelayCoordinator,
        stun: &StunList,
        now: u64,
    ) {
        let device = peer.device_public_key.clone();
        let live = self.directory.reach.connection(&device).is_some();
        let wait = if live {
            // Refresh current relay eligibility even on an existing
            // connection. A 60-second answer is not a durable record.
            self.relaying
                .read()
                .expect("relay answers")
                .get(&device)
                .is_some_and(|(expires, _)| *expires > now.saturating_add(30))
        } else {
            self.retry
                .get(&device)
                .is_some_and(|(at, _)| *at > Instant::now())
        };
        if self.pending.contains_key(&device) || wait {
            return;
        }
        let Ok(token) = self.directory.begin_peer_work(&device) else {
            return;
        };
        let failures = if live {
            0
        } else {
            self.retry.get(&device).map_or(0, |(_, n)| *n)
        };
        self.retry.insert(
            device.clone(),
            (
                Instant::now() + retry_delay(failures),
                failures.saturating_add(1),
            ),
        );
        let mut candidates = peer
            .current_endpoint
            .or(peer.remembered_endpoint)
            .map(|e| e.candidates)
            .unwrap_or_default();
        super::nearby_seam::prepend(&device, &mut candidates);
        // Existing LAN/VPN race first. Do not send HTTP to every route.
        // A successful signed read clears the error.
        if let Ok(index) = connect::select_origin(&candidates).await {
            let client = crate::upstream_native::NativeClient::new_secure(
                candidates[index].clone(),
                device.clone(),
                self.credentials.clone(),
            );
            let native = client.session_status().await;
            if native.as_ref().is_err_and(|error| {
                !matches!(error, crate::upstreams::UpstreamError::Unreachable(_))
            }) {
                set_operation_failed(&self.directory, &device, &token);
                // A verified application refusal still refuses this operation.
                // It must not suppress the independent reach offer that learns
                // this member's relay listener. Invalid or unsigned replies do.
                if !matches!(
                    native.as_ref(),
                    Err(crate::upstreams::UpstreamError::Tagged { .. })
                ) {
                    return;
                }
            }
            if native.is_ok() {
                let _ = self
                    .directory
                    .set_peer_state(&token, &device, PeerState::Ready);
                self.retry.insert(
                    device.clone(),
                    (Instant::now() + Duration::from_secs(30), 0),
                );
                if self.endpoint.is_none() {
                    return;
                }
                // Ready consumes this token. A missing optional
                // relay answer must not make a native-reachable peer fail.
            }
        }
        let Ok(endpoint) = self.ensure_endpoint(stun).await else {
            set_failed(&self.directory, &device, &token);
            return;
        };
        let request_id = hex::encode(rand::random::<[u8; 32]>());
        let now = unix_time();
        let offer = Offer {
            endpoint_id: self.key.public().to_string(),
            direct_addresses: endpoint_addresses(&endpoint),
            expires_at: now + SIGNAL_LIFETIME,
            request_id: request_id.clone(),
        };
        self.pending.insert(
            device.clone(),
            Pending {
                request_id,
                expires_at: offer.expires_at,
                token: token.clone(),
            },
        );
        let command = CoordinationCommand::ReachOffer {
            sender_device_public_key: self.credentials.public_key().unwrap_or_default(),
            offer,
        };
        if !signal
            .queue_command(&device, command, now)
            .await
            .is_ok_and(|p| p.accepted())
        {
            self.pending.remove(&device);
            handle_signalling_failure(&self.directory, &device, &token).await;
        }
    }
    fn dial(
        &mut self,
        device: String,
        endpoint_id: String,
        addresses: Vec<SocketAddr>,
        relays: Vec<SocketAddr>,
    ) {
        let Some(endpoint) = self.endpoint.clone() else {
            return;
        };
        if refresh_open_route(&mut self.tasks, &self.directory, &device)
            || !self
                .dialing
                .lock()
                .expect("reach dialing")
                .insert(device.clone())
        {
            return;
        }
        struct Dialing {
            device: String,
            pending: Arc<Mutex<BTreeSet<String>>>,
        }
        impl Drop for Dialing {
            fn drop(&mut self) {
                self.pending
                    .lock()
                    .expect("reach dialing")
                    .remove(&self.device);
            }
        }
        let guard = Dialing {
            device: device.clone(),
            pending: self.dialing.clone(),
        };
        let Ok(token) = self.directory.begin_peer_work(&device) else {
            return;
        };
        let (directory, routes) = (self.directory.clone(), self.directory.reach.clone());
        let relay_answers = self.relaying.clone();
        self.tasks.spawn(async move {
            let _guard = guard;
            let Ok(id) = endpoint_id.parse::<EndpointId>() else {
                return;
            };
            let result = connect_iroh(&endpoint, id, addresses, || {
                // A signed answer can arrive while the direct phase is active.
                // Read the SAME answer table at fallback, not a start-time copy.
                // Recheck expiry and current membership before using a listener.
                let mut relays = relays;
                if let Ok(peers) = directory.snapshot() {
                    for (device, (expires, addresses)) in
                        relay_answers.read().expect("relay answers").iter()
                    {
                        if *expires > unix_time()
                            && peers.iter().any(|peer| peer.device_public_key == *device)
                        {
                            relays.extend(addresses);
                        }
                    }
                }
                relays.sort();
                relays.dedup();
                relays
            })
            .await;
            match result {
                Ok(connection) => {
                    if !routes
                        .recovery
                        .lock()
                        .expect("reach recovery")
                        .register(device.clone(), connection.clone())
                    {
                        return;
                    }
                    if let Some(old) = routes
                        .live
                        .write()
                        .expect("reach routes")
                        .insert(device.clone(), LiveRoute { connection })
                    {
                        old.connection.close(0u32.into(), b"connection replaced");
                    }
                    routes.changed.notify_waiters();
                    // A QUIC connection proves transport identity, but Ready
                    // still requires the unchanged signed HTTP peer envelope.
                    let client = crate::upstream_native::NativeClient::new_secure_candidates(
                        Vec::new(),
                        device.clone(),
                        directory.credentials.clone(),
                    )
                    .with_directory(directory.clone());
                    let _ = client.session_status().await;
                }
                Err(_) => {
                    routes
                        .recovery
                        .lock()
                        .expect("reach recovery")
                        .failed(&device);
                    let mut candidates = directory
                        .snapshot()
                        .ok()
                        .and_then(|peers| peers.into_iter().find(|p| p.device_public_key == device))
                        .and_then(|p| p.current_endpoint.or(p.remembered_endpoint))
                        .map(|e| e.candidates)
                        .unwrap_or_default();
                    super::nearby_seam::prepend(&device, &mut candidates);
                    let native = crate::upstream_native::NativeClient::new_secure_candidates(
                        candidates,
                        device.clone(),
                        directory.credentials.clone(),
                    );
                    match native.session_status().await {
                        Ok(_) => {
                            let _ = directory.set_peer_state(&token, &device, PeerState::Ready);
                        }
                        Err(crate::upstreams::UpstreamError::Unreachable(_)) => {
                            set_failed(&directory, &device, &token)
                        }
                        Err(_) => set_operation_failed(&directory, &device, &token),
                    }
                }
            }
        });
    }
}

type NetworkSnapshot = Vec<(String, IpAddr, bool)>;
fn network_snapshot(read: &InterfaceReader) -> Option<NetworkSnapshot> {
    read().ok().map(|addresses| {
        addresses
            .into_iter()
            .map(|a| (a.interface, a.address, a.has_default_route))
            .collect()
    })
}
pub(super) async fn network_changed_with(
    mut previous: Option<NetworkSnapshot>,
    read: impl Fn() -> Option<NetworkSnapshot>,
    poll: Duration,
) {
    // This watcher runs beside relay I/O. A slow or dead public relay must not
    // delay closing the old endpoint and refreshing same-port STUN.
    loop {
        tokio::time::sleep(poll).await;
        if let Some(current) = read() {
            if previous
                .as_ref()
                .is_some_and(|previous| previous != &current)
            {
                return;
            }
            previous = Some(current);
        }
    }
}

pub(super) fn bind_offer(
    directory: &FederationDirectory,
    bindings: &relay::Bindings,
    sender: &str,
    offer: &Offer,
    now: u64,
) -> io::Result<EndpointId> {
    let id = offer.validate(now).map_err(io::Error::other)?;
    if !directory
        .snapshot()
        .map_err(io::Error::other)?
        .iter()
        .any(|p| p.device_public_key == sender)
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "signal author is not a member device",
        ));
    }
    let mut bindings = bindings
        .write()
        .map_err(|_| io::Error::other("reach bindings unavailable"))?;
    if bindings.get(&id).is_some_and(|device| device != sender) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "endpoint is bound to another device",
        ));
    }
    bindings.retain(|_, device| device != sender);
    bindings.insert(id, sender.to_owned());
    Ok(id)
}
pub(super) fn endpoint_addresses(endpoint: &Endpoint) -> Vec<SocketAddr> {
    endpoint
        .addr()
        .addrs
        .into_iter()
        .filter_map(|a| match a {
            TransportAddr::Ip(a) if !relay::is_tailscale(a.ip()) => Some(a),
            _ => None,
        })
        .take(16)
        .collect()
}
pub(super) async fn connect_iroh(
    endpoint: &Endpoint,
    id: EndpointId,
    addresses: Vec<SocketAddr>,
    relays: impl FnOnce() -> Vec<SocketAddr>,
) -> io::Result<Connection> {
    let mut address = EndpointAddr::new(id);
    for direct in addresses {
        if !relay::is_tailscale(direct.ip()) {
            address.addrs.insert(TransportAddr::Ip(direct));
        }
    }
    let started = Instant::now();
    if let Ok(Ok(connection)) = timeout(
        Duration::from_secs(5),
        endpoint.connect(address.clone(), ALPN),
    )
    .await
    {
        check_remote(&connection, id)?;
        return Ok(connection);
    }
    tokio::time::sleep(Duration::from_secs(5).saturating_sub(started.elapsed())).await;
    for relay in relays().into_iter().take(8) {
        let url: iroh::RelayUrl = format!("http://{relay}")
            .parse()
            .map_err(io::Error::other)?;
        endpoint
            .insert_relay(
                url.clone(),
                Arc::new(iroh_relay::RelayConfig::new(url.clone(), None)),
            )
            .await;
        address.addrs.insert(TransportAddr::Relay(url));
    }
    if address
        .addrs
        .iter()
        .all(|a| !matches!(a, TransportAddr::Relay(_)))
    {
        return Err(io::Error::other(
            "direct methods failed and no federation relay is reachable",
        ));
    }
    let connection = timeout(Duration::from_secs(10), endpoint.connect(address, ALPN))
        .await
        .map_err(io::Error::other)?
        .map_err(io::Error::other)?;
    check_remote(&connection, id)?;
    Ok(connection)
}
pub(super) fn check_remote(connection: &Connection, expected: EndpointId) -> io::Result<()> {
    if connection.remote_id() != expected {
        connection.close(1u32.into(), b"wrong signal-bound endpoint");
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "QUIC endpoint differs from the signed offer",
        ));
    }
    Ok(())
}
pub(super) fn retry_delay(failures: u32) -> Duration {
    Duration::from_secs(5u64.saturating_mul(1u64.checked_shl(failures).unwrap_or(u64::MAX)))
        .min(Duration::from_secs(300))
}
pub(super) fn refresh_open_route(
    tasks: &mut JoinSet<()>,
    directory: &FederationDirectory,
    device: &str,
) -> bool {
    let Some(connection) = directory.reach.connection(device) else {
        return false;
    };
    let Ok(token) = directory.begin_peer_work(device) else {
        return true;
    };
    let (directory, device) = (directory.clone(), device.to_owned());
    tasks.spawn(async move {
        // Signalling refreshed the route. A signed reply must refresh its state.
        let client = crate::upstream_native::NativeClient::new_secure_candidates(
            Vec::new(),
            device.clone(),
            directory.credentials.clone(),
        )
        .with_directory(directory.clone());
        if client.probe_signed_connection(&connection).await.is_ok() {
            let _ = directory.set_peer_state(&token, &device, PeerState::Ready);
        }
    });
    true
}

pub(super) async fn handle_signalling_failure(
    directory: &FederationDirectory,
    device: &str,
    token: &crate::federation::WorkToken,
) {
    // A missing relay answer says nothing about a working data connection.
    // Check the signed peer response before changing its reachability state.
    let Some(connection) = directory.reach.connection(device) else {
        set_failed(directory, device, token);
        return;
    };
    let client = crate::upstream_native::NativeClient::new_secure_candidates(
        Vec::new(),
        device.to_owned(),
        directory.credentials.clone(),
    )
    .with_directory(directory.clone());
    match client.probe_signed_connection(&connection).await {
        Ok(()) => {
            let _ = directory.set_peer_state(token, device, PeerState::Ready);
        }
        Err(crate::upstreams::UpstreamError::Unreachable(_)) => {
            set_failed(directory, device, token)
        }
        Err(_) => set_operation_failed(directory, device, token),
    }
}

fn set_operation_failed(
    directory: &FederationDirectory,
    device: &str,
    token: &crate::federation::WorkToken,
) {
    let _ = directory.set_peer_state(
        token,
        device,
        PeerState::Failed {
            error: crate::upstream_native::PEER_OPERATION_FAILED_MESSAGE.into(),
        },
    );
}
fn set_failed(directory: &FederationDirectory, device: &str, token: &crate::federation::WorkToken) {
    let _ = directory.set_peer_state(
        token,
        device,
        PeerState::Failed {
            error: directory.unreachable_sentence(device),
        },
    );
}
