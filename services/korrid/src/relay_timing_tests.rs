use super::*;
use crate::federation::reach::signal::{Answer, Offer, SIGNAL_LIFETIME};

// Both are real transports. The unavailable configured relay accepts local TCP
// but never completes its WebSocket handshake. The working relay stores actual
// signed/encrypted coordination events in the existing in-process network.
struct CoordinationTransports {
    network: InProcessRelayNetwork,
    websocket: WebSocketRelayTransport,
    unavailable: String,
}
impl RelayTransport for CoordinationTransports {
    fn publish<'a>(
        &'a self,
        relay: &'a str,
        event: &'a str,
        auth: Arc<dyn RelayAuthSigner>,
    ) -> BoxFuture<'a, Result<(), RelayError>> {
        if relay == self.unavailable {
            self.websocket.publish(relay, event, auth)
        } else {
            self.network.publish(relay, event, auth)
        }
    }
    fn read<'a>(
        &'a self,
        relay: &'a str,
        filter: &'a RelayFilter,
        auth: Arc<dyn RelayAuthSigner>,
    ) -> BoxFuture<'a, Result<Vec<RelayEvent>, RelayError>> {
        if relay == self.unavailable {
            self.websocket.read(relay, filter, auth)
        } else {
            self.network.read(relay, filter, auth)
        }
    }
}

async fn reach_exchange(include_unavailable: bool) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let unavailable = format!("ws://{}", listener.local_addr().unwrap());
    let accepting = tokio::spawn(async move {
        let mut connections = Vec::new();
        loop {
            connections.push(listener.accept().await.unwrap().0);
        }
    });
    let mut urls = vec!["wss://working.invalid".into()];
    if include_unavailable {
        urls.push(unavailable.clone());
    }
    let relays = RelayList::configured(urls).unwrap();
    let transport = Arc::new(CoordinationTransports {
        network: InProcessRelayNetwork::new(&relays),
        websocket: WebSocketRelayTransport::new(),
        unavailable,
    });
    let (a_root, b_root) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let owner = nostr::key::Keys::generate();
    let a = tests::owned_identity(a_root.path(), &owner, 1000);
    let b = tests::owned_identity(b_root.path(), &owner, 1000);
    let (a_key, b_key) = (
        a.device_public_key().unwrap().to_owned(),
        b.device_public_key().unwrap().to_owned(),
    );
    let (a, b) = (
        CoordinatedRelays::new(relays.clone(), a, transport.clone()).unwrap(),
        CoordinatedRelays::new(relays, b, transport).unwrap(),
    );
    // The existing coordination API receives the caller's Unix clock. Keep
    // that clock in step with Tokio's paused elapsed time, without changing any
    // production lifetime, retry interval, attempt count or timeout.
    let start = tokio::time::Instant::now();
    let now = || 1000 + start.elapsed().as_secs();
    let offer = Offer {
        endpoint_id: iroh::SecretKey::generate().public().to_string(),
        direct_addresses: Vec::new(),
        expires_at: now() + SIGNAL_LIFETIME,
        request_id: "ab".repeat(32),
    };
    let pending_expires = offer.expires_at;
    let offer_sent = tokio::time::Instant::now();
    assert!(a
        .queue_command(
            &b_key,
            CoordinationCommand::ReachOffer {
                sender_device_public_key: a_key.clone(),
                offer,
            },
            now()
        )
        .await
        .unwrap()
        .accepted());
    let offer_publication = offer_sent.elapsed();
    let received = b.receive(now()).await.unwrap();
    let offer = received
        .commands
        .into_iter()
        .find_map(|command| match command {
            CoordinationCommand::ReachOffer {
                sender_device_public_key,
                offer,
            } if sender_device_public_key == a_key => Some(offer),
            _ => None,
        })
        .expect("working relay did not deliver the authenticated offer");
    assert!(
        offer.validate(now()).is_ok(),
        "offer expired before answer construction"
    );
    let answer = Answer {
        endpoint_id: iroh::SecretKey::generate().public().to_string(),
        direct_addresses: Vec::new(),
        expires_at: now() + SIGNAL_LIFETIME,
        request_id: offer.request_id.clone(),
        can_relay_now: false,
        relay_addresses: Vec::new(),
    };
    assert!(b
        .queue_command(
            &a_key,
            CoordinationCommand::ReachAnswer {
                sender_device_public_key: b_key.clone(),
                answer,
            },
            now()
        )
        .await
        .unwrap()
        .accepted());
    let received = a.receive(now()).await.unwrap();
    let answer = received
        .commands
        .into_iter()
        .find_map(|command| match command {
            CoordinationCommand::ReachAnswer {
                sender_device_public_key,
                answer,
            } if sender_device_public_key == b_key => Some(answer),
            _ => None,
        })
        .expect("working relay did not deliver the authenticated answer");
    assert!(answer.validate(now()).is_ok(), "answer itself expired");
    assert!(
        answer.request_id == offer.request_id,
        "answer does not match the pending offer"
    );
    accepting.abort();
    // The production reach controller rejects a matching answer when the
    // ORIGINAL pending offer has expired, even if the answer itself is fresh.
    assert!(now() < pending_expires,
        "signed reach exchange outlived its existing pending lifetime: elapsed_ms={}, offer_publish_ms={}, lifetime_s={SIGNAL_LIFETIME}",
        start.elapsed().as_millis(), offer_publication.as_millis());
}

#[tokio::test(start_paused = true)]
async fn live_relay_delivers_signed_reach_exchange_before_pending_expiry() {
    reach_exchange(false).await;
}

#[tokio::test(start_paused = true)]
async fn unavailable_configured_relay_does_not_expire_working_signed_reach_exchange() {
    reach_exchange(true).await;
}
