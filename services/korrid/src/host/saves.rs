//! Host-owned save operations. The executor owns all live-session exclusion.
use super::*;
use crate::saves::{
    peer::SavePeerService, record::snapshot_id, store::NativeSaveStore, types::*, SaveError,
    SaveRepository,
};

fn failure(error: SaveError) -> RpcFailure {
    RpcFailure {
        code: match error {
            SaveError::Split => "SavesSplit",
            SaveError::InUse => "ActiveSessionConflict",
            SaveError::Missing => "SaveContentUnavailable",
            _ => "SavesUnavailable",
        }
        .into(),
        message: match error {
            SaveError::Split => "Saves split between two devices. Choose one before launch.",
            SaveError::InUse => "Stop this game before changing its saves.",
            SaveError::Missing => "This snapshot's content is not available on this device.",
            _ => "Save history could not be read or updated.",
        }
        .into(),
    }
}
fn now() -> String {
    chrono::DateTime::from_timestamp(crate::peer_rpc::unix_time() as i64, 0)
        .expect("current UTC time")
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

impl HostRuntime {
    fn save_store(&self) -> NativeSaveStore {
        NativeSaveStore::new(&self.private_state_root)
    }
    fn save_device(&self) -> Result<&str, RpcFailure> {
        self.device_public_key
            .as_deref()
            .ok_or_else(|| failure(SaveError::Invalid("device identity".into())))
    }
    fn save_declaration(
        &self,
        game: &str,
        runner: Option<&str>,
    ) -> Result<
        Option<(
            crate::launcher::plugin_launch::PluginLaunchInput,
            crate::launcher::plugin_launch::PreparedLaunch,
        )>,
        RpcFailure,
    > {
        // Declaration reads are not starts. Use the normal route/prepare
        // producer without the session's require_ready gate; missing runtime
        // inputs may still make the runner decline, and that game is deferred.
        let root = self.route_root()?;
        let registry = self.route_registry()?;
        let (snapshot, _) = crate::config::settings::runner_choice_snapshot(root)
            .map_err(crate::game_routes::settings_failure)?;
        let route =
            crate::config::resolver::resolve_linux_route(root, &snapshot, &registry, game, runner)
                .map_err(|error| crate::route_diagnostic_failure(&error))?;
        let launch = crate::launcher::linux_plugin::launch_route(
            root,
            &snapshot,
            &registry,
            &route,
            None,
            None,
            &self.launch_person()?,
        )
        .map_err(|error| crate::game_routes::unavailable(error.to_string()))?;
        crate::launcher::plugin_launch::prepare_command(&launch.command)
            .and_then(|command| crate::launcher::plugin_launch::declared_saves(&command))
            .map_err(|_| failure(SaveError::Invalid("save declaration".into())))
    }
    fn save_mark(&self, game: &str) -> Result<SaveMark, RpcFailure> {
        let person = &self.launch_person()?;
        let store = self.save_store();
        if store.reconcile(person, game).map_err(failure)? == SaveMark::Split {
            return Ok(SaveMark::Split);
        }
        if let (Ok(root), Ok(registry)) = (self.route_root(), self.route_registry()) {
            if let Ok((snapshot, _)) = crate::config::settings::runner_choice_snapshot(root) {
                if crate::config::resolver::resolve_linux_route(
                    root, &snapshot, &registry, game, None,
                )
                .is_ok_and(|route| route.runner_id == "@korri:steam/steam")
                {
                    return Ok(SaveMark::SyncedBySteam);
                }
            }
        }
        if self
            .config
            .as_ref()
            .is_ok_and(|config| config.games.iter().any(|g| g.id == game))
        {
            return Ok(SaveMark::NotSynced);
        }
        match self.save_declaration(game, None) {
            Ok(Some((input, _))) if input.runner_id == "@korri:steam/steam" => {
                return Ok(SaveMark::SyncedBySteam)
            }
            Ok(Some((_, prepared))) if prepared.plan.save_locations.is_empty() => {
                return Ok(SaveMark::NotSynced)
            }
            _ => {}
        }
        Ok(
            if store
                .confirmed_elsewhere(person, game, self.save_device()?)
                .map_err(failure)?
            {
                SaveMark::Synced
            } else {
                SaveMark::NotChecked
            },
        )
    }
    pub(crate) async fn save_status(&self) -> Result<SaveStatus, RpcFailure> {
        let host = self.clone();
        tokio::task::spawn_blocking(move || {
            let person = &host.launch_person()?;
            let store = host.save_store();
            let mut games: std::collections::BTreeSet<String> = store
                .heads(person)
                .map_err(failure)?
                .games
                .into_iter()
                .map(|g| g.game_id)
                .collect();
            if let Ok(catalog) = host.catalog_snapshot_blocking(Some(person)) {
                games.extend(catalog.games.into_iter().map(|g| g.id));
            }
            let mut statuses = Vec::new();
            for game in games {
                let status = host.save_mark(&game)?;
                let heads = store
                    .heads(person)
                    .map_err(failure)?
                    .games
                    .into_iter()
                    .find(|g| g.game_id == game)
                    .map(|g| g.heads)
                    .unwrap_or_default();
                statuses.push(GameSaveStatus {
                    game_id: game,
                    status,
                    heads,
                });
            }
            Ok(SaveStatus { games: statuses })
        })
        .await
        .map_err(host_worker_failure)?
    }
    fn save_history_blocking(&self, game: &str) -> Result<SaveHistory, RpcFailure> {
        let person = &self.launch_person()?;
        let store = self.save_store();
        store.reconcile(person, game).map_err(failure)?;
        let current = match store.launch_base(person, game, self.save_device()?) {
            Ok(id) => id,
            Err(SaveError::Split) => None,
            Err(error) => return Err(failure(error)),
        };
        let mut records: Vec<_> = store
            .records(person, game)
            .map_err(failure)?
            .into_iter()
            .map(|record| {
                Ok(SaveHistoryEntry {
                    snapshot_id: snapshot_id(&record).map_err(failure)?,
                    record,
                })
            })
            .collect::<Result<_, RpcFailure>>()?;
        records.sort_by(|a, b| {
            b.record
                .ended_at
                .cmp(&a.record.ended_at)
                .then_with(|| a.snapshot_id.cmp(&b.snapshot_id))
        });
        Ok(SaveHistory { records, current })
    }
    pub(crate) async fn save_history(&self, game: String) -> Result<SaveHistory, RpcFailure> {
        let host = self.clone();
        tokio::task::spawn_blocking(move || host.save_history_blocking(&game))
            .await
            .map_err(host_worker_failure)?
    }
    pub(crate) async fn save_pick(
        &self,
        request: SavePickRequest,
        restore: bool,
    ) -> Result<SaveHistory, RpcFailure> {
        if restore {
            let person = self.launch_person()?;
            let game = request.game_id.clone();
            let id = request.snapshot_id.clone();
            let store = self.save_store();
            let complete = tokio::task::spawn_blocking(move || store.complete(&person, &game, &id))
                .await
                .map_err(host_worker_failure)?
                .map_err(failure)?;
            if !complete {
                let peer = self.save_service.get().ok_or_else(|| RpcFailure {
                    code: "SavePayloadUnavailable".into(),
                    message:
                        "Connect a device that has this save snapshot, then try restore again."
                            .into(),
                })?;
                peer.fetch_snapshot_payload(
                    &request.game_id,
                    &request.snapshot_id,
                    tokio::time::Instant::now() + std::time::Duration::from_secs(30),
                )
                .await
                .map_err(|error| error.failure())?;
            }
        }
        let host = self.clone();
        tokio::task::spawn_blocking(move || {
            host.control()?
                .with_idle_session(|| {
                    let person = &host.launch_person()?;
                    let store = host.save_store();
                    let operation = if restore {
                        NativeSaveStore::restore
                    } else {
                        NativeSaveStore::choose
                    };
                    operation(
                        &store,
                        person,
                        &request.game_id,
                        &request.snapshot_id,
                        host.save_device()?,
                        &now(),
                    )
                    .map_err(failure)?;
                    host.control()?.notify_save_snapshot(&request.game_id);
                    host.save_history_blocking(&request.game_id)
                })
                .map_err(|error| {
                    if error.code == "ActiveSessionConflict" {
                        failure(SaveError::InUse)
                    } else {
                        error
                    }
                })
        })
        .await
        .map_err(host_worker_failure)?
    }
    fn save_device_label(&self, device: &str, peers: &[crate::federation::PeerSnapshot]) -> String {
        if self.device_public_key.as_deref() == Some(device) {
            return self
                .config
                .as_ref()
                .map(|config| config.label.clone())
                .unwrap_or_else(|_| "this device".into());
        }
        peers
            .iter()
            .find(|peer| peer.device_public_key == device)
            .and_then(|peer| {
                peer.current_endpoint
                    .as_ref()
                    .or(peer.remembered_endpoint.as_ref())
            })
            .and_then(|endpoint| endpoint.label.clone())
            .unwrap_or_else(|| "another device".into())
    }

    fn save_check_details(
        &self,
        request: &SaveCheckRequest,
        person: &str,
        peers: &[crate::federation::PeerSnapshot],
    ) -> Result<SaveCheck, RpcFailure> {
        if self.launch_person()? != person {
            return Err(RpcFailure {
                code: "SaveIdentityChanged".into(),
                message: "The device owner changed. Check saves again.".into(),
            });
        }
        let status = self.save_mark(&request.game_id)?;
        let history = self.save_history_blocking(&request.game_id)?;
        let latest = history.current.as_ref().and_then(|id| {
            history
                .records
                .iter()
                .find(|record| &record.snapshot_id == id)
        });
        let mut warnings = Vec::new();
        if let Some(latest) = latest {
            if let Ok(Some((input, prepared))) =
                self.save_declaration(&request.game_id, request.runner_id.as_deref())
            {
                if latest.record.release_id.is_some()
                    && latest.record.release_id != prepared.release_id
                {
                    let title = self
                        .route_root()
                        .ok()
                        .and_then(|root| crate::config::settings::runner_choice_snapshot(root).ok())
                        .and_then(|(snapshot, _)| {
                            snapshot
                                .library
                                .get(&request.game_id)
                                .and_then(|game| game.title.clone())
                        })
                        .unwrap_or_else(|| "this game".into());
                    let device = self.save_device_label(&latest.record.device, peers);
                    warnings.push(format!(
                        "These saves came from a different release of {title}, played on {device}."
                    ));
                }
                if latest.record.runner_id != input.runner_id
                    && latest.record.locations.iter().any(|location| {
                        location.kind == SaveKind::Savestate && !location.files.is_empty()
                    })
                {
                    warnings.push(
                        "These savestates came from a different runner and may not load.".into(),
                    );
                }
            }
        }
        Ok(SaveCheck {
            status,
            warnings,
            in_use: Vec::new(),
        })
    }

    pub(crate) async fn save_check(
        &self,
        request: SaveCheckRequest,
        peer: Option<Arc<SavePeerService>>,
        directory: Option<crate::federation::FederationDirectory>,
        arrived: tokio::time::Instant,
    ) -> Result<SaveCheck, RpcFailure> {
        // The portal waits 3 seconds. This is one total request budget, not a
        // fresh network budget after preparation. Reserve 100 ms for publication
        // and leave a further 500 ms for the HTTP response to reach the portal.
        let deadline = arrived + std::time::Duration::from_millis(2500);
        let peer_deadline = deadline - std::time::Duration::from_millis(100);
        let host = self.clone();
        let game = request.game_id.clone();
        let remembered = tokio::task::spawn_blocking(move || {
            let person = host.launch_person()?;
            let warnings = host
                .save_store()
                .in_use_warnings(&person, &game)
                .map_err(failure)?;
            Ok::<_, RpcFailure>((person, warnings))
        });
        let (person, in_use) = tokio::time::timeout_at(deadline, remembered)
            .await
            .map_err(|_| RpcFailure {
                code: "SavesUnavailable".into(),
                message: "Saved warnings could not be read in time. Try again.".into(),
            })?
            .map_err(host_worker_failure)??;
        let mut answer = SaveCheck {
            status: SaveMark::NotChecked,
            warnings: Vec::new(),
            in_use,
        };
        let peers = directory
            .and_then(|directory| directory.snapshot().ok())
            .unwrap_or_default();
        let host = self.clone();
        let selected = request.clone();
        let owner = person.clone();
        let known_peers = peers.clone();
        let details = tokio::task::spawn_blocking(move || {
            host.save_check_details(&selected, &owner, &known_peers)
        });
        match tokio::time::timeout_at(peer_deadline, details).await {
            Ok(Ok(Ok(details))) => {
                answer.status = details.status;
                answer.warnings = details.warnings;
            }
            Ok(Ok(Err(error))) => return Err(error),
            Ok(Err(error)) => return Err(host_worker_failure(error)),
            Err(_) => return Ok(answer),
        }
        let Some(peer) = peer else {
            if answer.status == SaveMark::Synced {
                answer.status = SaveMark::NotChecked;
            }
            return Ok(answer);
        };
        let checks = match tokio::time::timeout_at(
            deadline,
            peer.pull_before_launch(&request.game_id, peer_deadline),
        )
        .await
        {
            Ok(Ok(checks)) => checks,
            _ => {
                if answer.status == SaveMark::Synced {
                    answer.status = SaveMark::NotChecked;
                }
                return Ok(answer);
            }
        };
        let mut observations = Vec::new();
        let mut answered = false;
        let mut failed = false;
        let mut missing_records = std::collections::BTreeSet::new();
        for check in checks {
            missing_records.extend(check.heads_without_records);
            failed |= check.transfer.is_err();
            if let Some(in_use) = check.in_use {
                answered = true;
                let since = match in_use {
                    SaveInUse::Active(active) => Some(active.since),
                    SaveInUse::Idle(_) => None,
                };
                // An idle reply clears its persisted in-use claim below, but
                // cannot clear remembered newer/incomplete-head risk by itself.
                // Keep the conservative warning if publication hits the deadline.
                if let Some(since) = since {
                    answer
                        .in_use
                        .retain(|warning| warning.device != check.device);
                    answer.in_use.push(SaveInUseWarning {
                        device: check.device.clone(),
                        since,
                    });
                }
                observations.push((check.device, since));
            }
        }
        if answer.status == SaveMark::Synced && (!answered || failed) {
            answer.status = SaveMark::NotChecked;
        }
        for device in missing_records {
            let label = self.save_device_label(&device, &peers);
            answer.warnings.push(format!("Saves were reported by {label}, but their records have not arrived. You can launch anyway."));
        }
        let host = self.clone();
        let game = request.game_id.clone();
        let owner = person.clone();
        let refreshed = tokio::task::spawn_blocking(move || {
            if host.launch_person()? != owner {
                return Err(RpcFailure {
                    code: "SaveIdentityChanged".into(),
                    message: "The device owner changed. Check saves again.".into(),
                });
            }
            let store = host.save_store();
            for (device, since) in observations {
                store
                    .remember_in_use(&owner, &game, &device, since)
                    .map_err(failure)?;
            }
            let mut result = host.save_check_details(&request, &owner, &peers)?;
            result.in_use = store.in_use_warnings(&owner, &game).map_err(failure)?;
            if result.status == SaveMark::NotChecked && answered && !failed {
                result.status = SaveMark::Synced;
            }
            if result.status == SaveMark::Synced && (!answered || failed) {
                result.status = SaveMark::NotChecked;
            }
            Ok::<_, RpcFailure>(result)
        });
        match tokio::time::timeout_at(deadline, refreshed).await {
            Ok(Ok(Ok(mut result))) => {
                for warning in answer.warnings {
                    if !result.warnings.contains(&warning) {
                        result.warnings.push(warning);
                    }
                }
                Ok(result)
            }
            Ok(Ok(Err(error))) => Err(error),
            Ok(Err(error)) => Err(host_worker_failure(error)),
            Err(_) => Ok(answer), // Never discard remembered or newly received risk.
        }
    }
    pub(crate) fn save_in_use(&self, person: &str, game: &str) -> Result<Option<u64>, SaveError> {
        let live = self
            .control()
            .map_err(|e| SaveError::Storage(e.message))?
            .save_in_use(person, game)
            .map_err(SaveError::Storage)?;
        Ok(live.or_else(|| self.launch_reservations.in_use(person, game)))
    }
    /// Baseline migrated per-game files before any peer learns our heads. This
    /// shares the launch transition guard and evaluates prepare once per game.
    pub(crate) async fn baseline_saves(&self) -> Result<bool, RpcFailure> {
        let host = self.clone();
        tokio::task::spawn_blocking(move || {
            match host.control()?.with_idle_session(|| {
                let Some(person) = host.owner_public_key() else {
                    return Ok(());
                };
                let Ok(root) = host.route_root() else {
                    return Ok(());
                };
                let (snapshot, _) = crate::config::settings::runner_choice_snapshot(root)
                    .map_err(crate::game_routes::settings_failure)?;
                let store = host.save_store();
                let device = host.save_device()?;
                let account = crate::saves::account_root(root, &person)
                    .map_err(|reason| failure(SaveError::Invalid(reason)))?;
                fn has_files(path: &Path) -> Result<bool, RpcFailure> {
                    let metadata = match std::fs::symlink_metadata(path) {
                        Ok(value) => value,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            return Ok(false)
                        }
                        Err(error) => return Err(failure(SaveError::Storage(error.to_string()))),
                    };
                    if metadata.file_type().is_symlink() {
                        return Err(failure(SaveError::Invalid("symlink in save root".into())));
                    }
                    if metadata.is_file() {
                        return Ok(true);
                    }
                    if !metadata.is_dir() {
                        return Err(failure(SaveError::Invalid(
                            "special file in save root".into(),
                        )));
                    }
                    for entry in std::fs::read_dir(path)
                        .map_err(|e| failure(SaveError::Storage(e.to_string())))?
                    {
                        if has_files(
                            &entry
                                .map_err(|e| failure(SaveError::Storage(e.to_string())))?
                                .path(),
                        )? {
                            return Ok(true);
                        }
                    }
                    Ok(false)
                }
                for game in snapshot.library.keys() {
                    let captured = (|| -> Result<(), RpcFailure> {
                        if store
                            .records(&person, game)
                            .map_err(failure)?
                            .iter()
                            .any(|r| r.device == device)
                        {
                            return Ok(());
                        }
                        let root = account
                            .join("saves")
                            .join(super::play_log::encode_uri_component(game));
                        if !has_files(&root)? {
                            return Ok(());
                        }
                        if let Some((input, prepared)) = host.save_declaration(game, None)? {
                            if input.runner_id == "@korri:steam/steam"
                                || prepared.plan.save_locations.is_empty()
                            {
                                return Ok(());
                            }
                            crate::saves::capture_initial(
                                &store,
                                &person,
                                &prepared.plan.save_locations,
                                SnapshotRecord {
                                    parents: vec![],
                                    game_id: game.clone(),
                                    release_id: prepared.release_id,
                                    runner_id: input.runner_id,
                                    device: device.into(),
                                    ended_at: now(),
                                    duration_seconds: 0,
                                    interrupted: false,
                                    locations: vec![],
                                },
                            )
                            .map_err(failure)?;
                        }
                        Ok(())
                    })();
                    if let Err(error) = captured {
                        eprintln!("korrid save baseline deferred for one game: {}", error.code);
                    }
                }
                Ok(())
            }) {
                Ok(()) => Ok(true),
                Err(error)
                    if matches!(
                        error.code.as_str(),
                        "ActiveSessionConflict" | "HostRecoveryBlocked"
                    ) =>
                {
                    Ok(false)
                }
                Err(error) => Err(error),
            }
        })
        .await
        .map_err(host_worker_failure)?
    }
    async fn save_contact(
        &self,
        service: &SavePeerService,
        device: &str,
    ) -> Result<(), crate::saves::peer::PeerSaveError> {
        self.baseline_saves()
            .await
            .map_err(|error| SaveError::Storage(error.message))?;
        service.on_contact(device).await
    }

    /// Capture and publication hold the same transition guard. A journal that
    /// survived a crash also pins captured bytes until completion is replayed.
    fn prune_saves(&self, game: &str) -> Result<bool, RpcFailure> {
        match self.control()?.with_idle_session(|| {
            self.save_store()
                .prune(&self.launch_person()?, game, self.save_device()?)
                .map_err(failure)
        }) {
            Ok(()) => Ok(true),
            Err(error)
                if matches!(
                    error.code.as_str(),
                    "ActiveSessionConflict" | "HostRecoveryBlocked"
                ) =>
            {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    pub(crate) fn configure_save_peer(
        &self,
        peer: Arc<SavePeerService>,
        directory: crate::federation::FederationDirectory,
    ) {
        let _ = self.save_service.set(peer.clone());
        let Ok(control) = self.control() else {
            return;
        };
        let (sender, mut snapshots) = tokio::sync::mpsc::unbounded_channel::<String>();
        control.set_save_push(sender);
        let mut contacts = directory.save_contacts();
        let host = self.clone();
        tokio::spawn(async move {
            let mut pending_prunes = std::collections::BTreeSet::new();
            let mut pending_baseline = !host.baseline_saves().await.unwrap_or(false);
            let mut prune_tick = tokio::time::interval(std::time::Duration::from_secs(1));
            prune_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            if let Ok(peers) = directory.snapshot() {
                for contact in peers {
                    if let Err(error) = host.save_contact(&peer, &contact.device_public_key).await {
                        eprintln!("korrid save contact: {}", error.failure().code);
                    }
                }
            }
            loop {
                tokio::select! {
                    game=snapshots.recv()=>match game {Some(game)=> {
                        match peer.after_snapshot(&game).await {Ok(results)=>for (_,result) in results {if let Err(error)=result {eprintln!("korrid save push: {}",error.failure().code);}},Err(error)=>eprintln!("korrid save push: {}",error.failure().code)}
                        pending_prunes.insert(game);
                    },None=>break},
                    _=prune_tick.tick(), if pending_baseline || !pending_prunes.is_empty()=> {
                        if pending_baseline && host.baseline_saves().await.unwrap_or(false) {
                            pending_baseline=false;
                            if let Ok(peers)=directory.snapshot(){for contact in peers {let _=host.save_contact(&peer,&contact.device_public_key).await;}}
                        }
                        let pruning=host.clone();let games:Vec<_>=pending_prunes.iter().cloned().collect();
                        let completed=tokio::task::spawn_blocking(move|| {
                            let mut completed=Vec::new();
                            for game in games {match pruning.prune_saves(&game) {Ok(true)=>completed.push(game),Ok(false)=>break,Err(error)=>{eprintln!("korrid save retention: {}",error.code);break;}}}
                            completed
                        }).await;
                        if let Ok(completed)=completed {for game in completed {pending_prunes.remove(&game);}}
                    },
                    contact=contacts.recv()=>match contact {Ok(device)=> {if let Err(error)=host.save_contact(&peer,&device).await {eprintln!("korrid save contact: {}",error.failure().code);}},Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>{if let Ok(peers)=directory.snapshot(){for contact in peers {let _=host.save_contact(&peer,&contact.device_public_key).await;}}},Err(_)=>break},
                }
            }
        });
    }
    pub async fn shutdown_sessions(&self) -> Result<(), RpcFailure> {
        let host = self.clone();
        tokio::task::spawn_blocking(move || {
            host.control()?.shutdown().map_err(|message| RpcFailure {
                code: "SaveShutdownFailed".into(),
                message,
            })
        })
        .await
        .map_err(host_worker_failure)??;
        if let (Some(peer), Ok(person)) = (self.save_service.get(), self.launch_person()) {
            let games = self.save_store().heads(&person).map_err(failure)?.games;
            let _ = tokio::time::timeout(std::time::Duration::from_secs(3), async {
                for game in games {
                    let _ = peer.after_snapshot(&game.game_id).await;
                }
            })
            .await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    const PERSON: &str = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";
    async fn rpc(
        router: &axum::Router,
        method: &str,
        payload: serde_json::Value,
    ) -> serde_json::Value {
        let response = router
            .clone()
            .oneshot(
                Request::post("/rpc")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"_tag":method,"payload":payload}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap()
    }
    fn test_host(root: &Path) -> HostRuntime {
        let config = root.join("host.toml");
        std::fs::write(&config,"label = \"save-test\"\n[[games]]\nid = \"one\"\ntitle = \"One\"\ncommand = [\"game\"]\n").unwrap();
        let host = HostRuntime::from_paths_with_backend(
            &config,
            None,
            root.to_owned(),
            Arc::new(super::super::systemd_unit::InMemoryLaunchUnitBackend::default()),
        );
        host.claim_save_owner();
        host
    }

    #[test]
    fn remembered_warning_survives_slow_object_publication() {
        // Isolate the deliberately held process-wide object lock from other
        // parallel tests, without replacing the real storage or check path.
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "host::saves::tests::slow_object_publication_child",
                "--ignored",
                "--nocapture",
            ])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }

    #[tokio::test]
    #[ignore = "subprocess helper; holds the object-publication lock longer than the check budget"]
    async fn slow_object_publication_child() {
        struct SlowPublication {
            entered: Option<std::sync::mpsc::Sender<()>>,
            release: std::sync::mpsc::Receiver<()>,
            bytes: std::io::Cursor<Vec<u8>>,
        }
        impl std::io::Read for SlowPublication {
            fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
                if let Some(entered) = self.entered.take() {
                    entered.send(()).unwrap();
                    std::thread::sleep(std::time::Duration::from_secs(3));
                    self.release
                        .recv_timeout(std::time::Duration::from_secs(10))
                        .unwrap();
                }
                std::io::Read::read(&mut self.bytes, output)
            }
        }
        let root = tempfile::tempdir().unwrap();
        let host = test_host(root.path());
        let store = host.save_store();
        let peer = "1".repeat(64);
        let other = "2".repeat(64);
        let hash = crate::saves::record::hash(b"new save");
        let record = SnapshotRecord {
            parents: vec![],
            game_id: "one".into(),
            release_id: None,
            runner_id: "runner".into(),
            device: peer.clone(),
            ended_at: "2026-10-21T00:00:00.000Z".into(),
            duration_seconds: 1,
            interrupted: false,
            locations: vec![SnapshotLocation {
                key: "saves".into(),
                kind: SaveKind::Save,
                files: vec![SaveFile {
                    path: "game.srm".into(),
                    sha256: hash.clone(),
                    bytes: 8,
                }],
            }],
        };
        let id = store.put_record(PERSON, &record).unwrap();
        let mut missing = record.clone();
        missing.device = other.clone();
        missing.locations[0].files[0].sha256 = crate::saves::record::hash(b"elsewhere");
        missing.locations[0].files[0].bytes = 9;
        store.put_record(PERSON, &missing).unwrap();
        store
            .remember_in_use(PERSON, "one", &peer, Some(42))
            .unwrap();
        let (entered, waiting) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let publishing = store.clone();
        let writer = std::thread::spawn(move || {
            publishing.put_object(
                PERSON,
                &hash,
                &mut SlowPublication {
                    entered: Some(entered),
                    release: released,
                    bytes: std::io::Cursor::new(b"new save".to_vec()),
                },
            )
        });
        waiting
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        let started = tokio::time::Instant::now();
        let checked = host
            .save_check(
                SaveCheckRequest {
                    game_id: "one".into(),
                    runner_id: None,
                },
                None,
                None,
                started,
            )
            .await;
        let elapsed = started.elapsed();
        release.send(()).unwrap();
        writer.join().unwrap().unwrap();
        let checked = checked.expect("object publication must not hide remembered warnings");
        assert!(
            elapsed < std::time::Duration::from_millis(2500),
            "warning read exceeded its budget: {elapsed:?}"
        );
        assert!(checked
            .in_use
            .iter()
            .any(|warning| warning.device == peer && warning.since == 42));
        assert!(
            checked.in_use.iter().any(|warning| warning.device == other),
            "missing newer-head metadata must also survive object contention"
        );
        assert!(store.complete(PERSON, "one", &id).unwrap());
    }

    #[test]
    fn pruning_cannot_interleave_with_capture_or_completion_publication() {
        use super::super::identity::{clear_active, persist_active, replace_active, ActiveSession};
        use std::{sync::mpsc, time::Duration};
        let root = tempfile::tempdir().unwrap();
        let host = test_host(root.path());
        let store = host.save_store();
        let device = host.save_device().unwrap().to_owned();
        let mut parent = None;
        let mut oldest = None;
        for n in 0..12 {
            let content = format!("progress {n}");
            let hash = crate::saves::record::hash(content.as_bytes());
            let record = SnapshotRecord {
                parents: parent.into_iter().collect(),
                game_id: "one".into(),
                release_id: None,
                runner_id: "mgba".into(),
                device: device.clone(),
                ended_at: format!("2026-10-21T00:00:{n:02}.000Z"),
                duration_seconds: 1,
                interrupted: false,
                locations: vec![SnapshotLocation {
                    key: "saves".into(),
                    kind: SaveKind::Save,
                    files: vec![SaveFile {
                        path: "game.srm".into(),
                        sha256: hash,
                        bytes: content.len() as u64,
                    }],
                }],
            };
            let id = store.put_record(PERSON, &record).unwrap();
            store
                .put_object(
                    PERSON,
                    &record.locations[0].files[0].sha256,
                    &mut content.as_bytes(),
                )
                .unwrap();
            store.confirm(PERSON, "one", &id, &"1".repeat(64)).unwrap();
            if oldest.is_none() {
                oldest = Some(record);
            }
            parent = Some(id);
        }
        let folder = root.path().join("game-saves");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("game.srm"), b"progress 0").unwrap();
        let locations = vec![SaveLocation {
            key: "saves".into(),
            kind: SaveKind::Save,
            path: folder.display().to_string(),
        }];
        let mut candidate = oldest.unwrap();
        candidate.parents = vec![parent.unwrap()];
        candidate.ended_at = "2026-10-21T01:00:00.000Z".into();
        let (started, waiting) = mpsc::channel();
        let (finished, result) = mpsc::channel();
        let mut thread = None;
        let captured = host
            .control()
            .unwrap()
            .with_idle_session(|| {
                persist_active(
                    &root.path().join("host-session"),
                    &ActiveSession::running(
                        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                        "one".into(),
                        Some(PERSON.into()),
                        1,
                    ),
                )
                .unwrap();
                let captured = store.capture(PERSON, &locations, candidate).unwrap();
                let pruning = host.clone();
                thread = Some(std::thread::spawn(move || {
                    started.send(()).unwrap();
                    finished.send(pruning.prune_saves("one")).unwrap();
                }));
                waiting.recv().unwrap();
                assert!(
                    result.recv_timeout(Duration::from_millis(100)).is_err(),
                    "prune must wait for the transition guard"
                );
                replace_active(
                    &root.path().join("host-session"),
                    &ActiveSession::CompletionPending {
                        launch_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                        game_id: "one".into(),
                        person_public_key: PERSON.into(),
                        started_at: 1,
                        entry: super::super::play_log::PlayEntry {
                            occurred_at: captured.ended_at.clone(),
                            duration_seconds: 1.0,
                            release_id: None,
                        },
                        save_snapshot: Some(captured.clone()),
                    },
                )
                .unwrap();
                Ok(captured)
            })
            .unwrap();
        assert!(
            !result
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .unwrap(),
            "completion journal defers pruning"
        );
        thread.unwrap().join().unwrap();
        assert!(!host.prune_saves("one").unwrap());
        let id = store.put_record(PERSON, &captured).unwrap();
        assert!(store.complete(PERSON, "one", &id).unwrap());
        clear_active(&root.path().join("host-session")).unwrap();
        assert!(host.prune_saves("one").unwrap());
        assert!(
            store.complete(PERSON, "one", &id).unwrap(),
            "new unconfirmed snapshot protects the shared object"
        );
    }

    struct MigratedDevice {
        root: tempfile::TempDir,
        host: HostRuntime,
        backend: Arc<super::super::systemd_unit::InMemoryLaunchUnitBackend>,
        service: Arc<SavePeerService>,
        federation: crate::federation::coordinator::FederationResources,
        base: String,
        server: tokio::task::JoinHandle<()>,
    }
    impl Drop for MigratedDevice {
        fn drop(&mut self) {
            self.server.abort();
        }
    }
    impl MigratedDevice {
        async fn new(bytes: &[u8]) -> Self {
            Self::with_missing_artifact(bytes, false).await
        }
        async fn with_missing_artifact(bytes: &[u8], missing: bool) -> Self {
            use crate::{plugin::PluginRegistry, plugin_installation::EnabledPackage};
            let root = tempfile::tempdir().unwrap();
            let package = root.path().join("plugin");
            std::fs::create_dir(&package).unwrap();
            std::fs::write(package.join("plugin.ts"),r#"
                export const name='mgba';
                export const systems={gba:{id:'gba',title:'Game Boy Advance',content:{extensions:['gba']}}};
                export const runners={mgba:{id:'@korri:mgba/mgba',program:'program',systems:['gba']}};
                export const handlers={'launch.prepare':input=>({command:input.program,args:[],saveLocations:[{key:'saves',kind:'save',path:input.saveRoot+'/saves'}]})};
            "#).unwrap();
            if missing {
                use std::io::Write;
                let mut source = std::fs::OpenOptions::new()
                    .append(true)
                    .open(package.join("plugin.ts"))
                    .unwrap();
                writeln!(source,"export const artifacts={{runtime:{{id:'@korri:mgba/runtime',version:'v1',files:[{{url:'https://example.com/runtime',sha256:'{}'}}]}}}}; runners.mgba.artifacts=['@korri:mgba/runtime'];",crate::saves::record::hash(b"runtime")).unwrap();
            }
            std::fs::write(
                package.join("manifest.json"),
                r#"{"entry":"plugin.ts","sources":["plugin.ts"]}"#,
            )
            .unwrap();
            let registry = PluginRegistry::from_installed(vec![EnabledPackage {
                id: "@korri:mgba".into(),
                package,
                entry: "plugin.ts".into(),
                sources: vec!["plugin.ts".into()],
                requires: vec![],
                hosts: vec![],
                files: std::collections::BTreeMap::from([(
                    "program".into(),
                    PathBuf::from("/bin/sh"),
                )]),
            }])
            .unwrap();
            crate::plugin_test_fixtures::write_gba_library(root.path());
            std::fs::create_dir(root.path().join("roms")).unwrap();
            std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
            let config = root.path().join("host.toml");
            std::fs::write(&config, "label = \"migrated\"\ngames = []\n").unwrap();
            let private = root.path().join("private");
            let backend =
                Arc::new(super::super::systemd_unit::InMemoryLaunchUnitBackend::default());
            let host = HostRuntime::from_paths_with_backend(
                &config,
                Some(root.path().to_owned()),
                private.clone(),
                backend.clone(),
            )
            .with_route_registry(root.path().into(), registry);
            host.claim_save_owner();
            let folder = crate::saves::account_root(root.path(), PERSON)
                .unwrap()
                .join("saves/wario-land-4/saves");
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("game.srm"), bytes).unwrap();
            let federation =
                crate::federation::coordinator::FederationResources::open(&private).unwrap();
            let sessions = host.clone();
            let service = Arc::new(
                SavePeerService::new(
                    Arc::new(NativeSaveStore::new(&private)),
                    federation.credentials.clone(),
                    federation.directory.clone(),
                    private.clone(),
                    Arc::new(move |person, game| sessions.save_in_use(person, game)),
                )
                .unwrap(),
            );
            let (mut state, _) = crate::app_states(host.clone());
            state.saves = Some(service.clone());
            let router = crate::peer_rpc::PeerRpcServer::with_shared_authority(
                state,
                &private,
                federation.credentials.clone(),
                federation.authorization.clone(),
            )
            .unwrap()
            .router(None);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                axum::serve(listener, router).await.unwrap();
            });
            Self {
                root,
                host,
                backend,
                service,
                federation,
                base,
                server,
            }
        }
        fn remember(&self, other: &Self) {
            let identity = other.federation.credentials.identity_snapshot().unwrap();
            let device = identity.device_public_key().unwrap().to_owned();
            let own = self.federation.credentials.public_key().unwrap();
            let now = crate::peer_rpc::unix_time();
            self.federation
                .directory
                .apply_membership(
                    &self.federation.directory.begin_work().unwrap(),
                    &[identity.owner_statement_json().unwrap()],
                )
                .unwrap();
            let endpoint = crate::relay::EndpointRecord {
                device_public_key: device,
                owner_public_key: PERSON.into(),
                generation: 1,
                candidates: vec![other.base.clone()],
                issued_at: now,
                expires_at: now + 3600,
                label: Some("Other handheld".into()),
                moonlight_address: None,
            };
            let event = identity
                .encrypt_tagged_event(
                    &own,
                    crate::relay::ENDPOINT_EVENT_KIND,
                    vec![
                        vec!["d".into(), format!("org.korri.endpoint:{own}")],
                        vec!["p".into(), own.clone()],
                        vec!["expiration".into(), endpoint.expires_at.to_string()],
                    ],
                    &serde_json::to_string(&endpoint).unwrap(),
                    now,
                )
                .unwrap();
            self.federation
                .directory
                .apply_endpoint_event(
                    &self.federation.directory.begin_work().unwrap(),
                    &event.json,
                )
                .unwrap();
        }
    }

    #[tokio::test]
    async fn prelaunch_check_returns_remembered_warning_before_portal_deadline() {
        let device = MigratedDevice::new(b"local").await;
        let mut slow = MigratedDevice::new(b"peer").await;
        slow.server.abort();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        slow.base = format!("http://{}", listener.local_addr().unwrap());
        slow.server = tokio::spawn(async move {
            axum::serve(
                listener,
                axum::Router::new().fallback(|| async {
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                    StatusCode::SERVICE_UNAVAILABLE
                }),
            )
            .await
            .unwrap();
        });
        device.remember(&slow);
        device
            .host
            .save_store()
            .remember_in_use(
                PERSON,
                "wario-land-4",
                slow.host.save_device().unwrap(),
                Some(123),
            )
            .unwrap();
        let started = tokio::time::Instant::now();
        let result = device
            .host
            .save_check(
                SaveCheckRequest {
                    game_id: "wario-land-4".into(),
                    runner_id: None,
                },
                Some(device.service.clone()),
                Some(device.federation.directory.clone()),
                started,
            )
            .await
            .unwrap();
        assert!(
            started.elapsed() < std::time::Duration::from_millis(2800),
            "daemon must leave transport margin before portal's 3 seconds: {:?}",
            started.elapsed()
        );
        assert_eq!(result.in_use.len(), 1);
        assert_eq!(result.in_use[0].device, slow.host.save_device().unwrap());
        assert_eq!(result.in_use[0].since, 123);
        assert_eq!(result.status, SaveMark::NotChecked);
        let started = tokio::time::Instant::now();
        let result = device
            .host
            .save_check(
                SaveCheckRequest {
                    game_id: "wario-land-4".into(),
                    runner_id: None,
                },
                Some(device.service.clone()),
                Some(device.federation.directory.clone()),
                started - std::time::Duration::from_secs(2),
            )
            .await
            .unwrap();
        assert!(
            started.elapsed() < std::time::Duration::from_millis(700),
            "time already spent by this request must reduce the peer budget"
        );
        assert_eq!(result.in_use[0].since, 123);
    }

    #[tokio::test]
    async fn release_mismatch_names_the_snapshot_device_without_an_identifier() {
        let device = MigratedDevice::new(b"local").await;
        let peer = MigratedDevice::new(b"peer").await;
        device.remember(&peer);
        device.host.baseline_saves().await.unwrap();
        let store = device.host.save_store();
        let mut previous = store.records(PERSON, "wario-land-4").unwrap().remove(0);
        previous.parents = vec![snapshot_id(&previous).unwrap()];
        previous.device = peer.host.save_device().unwrap().into();
        previous.release_id = Some("older-release".into());
        store.put_record(PERSON, &previous).unwrap();
        let path = device.root.path().join("catalog/library.yaml");
        let mut library: serde_yaml::Value =
            serde_yaml::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        library["library"]["wario-land-4"]["releases"][0]["id"] = "another-release".into();
        std::fs::write(path, serde_yaml::to_string(&library).unwrap()).unwrap();
        let result = device
            .host
            .save_check(
                SaveCheckRequest {
                    game_id: "wario-land-4".into(),
                    runner_id: None,
                },
                None,
                Some(device.federation.directory.clone()),
                tokio::time::Instant::now(),
            )
            .await
            .unwrap();
        assert!(result.warnings.iter().any(|warning|warning=="These saves came from a different release of Wario Land 4, played on Other handheld."),"{:?}",result.warnings);
        assert!(result.warnings.iter().all(|warning| !warning
            .contains(peer.host.save_device().unwrap())
            && !warning.contains("another-release")));
    }

    #[tokio::test]
    async fn startup_baselines_migrated_files_without_a_launch_or_contact() {
        let device = MigratedDevice::new(b"migrated progress").await;
        device
            .host
            .configure_save_peer(device.service.clone(), device.federation.directory.clone());
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while device
                .host
                .save_store()
                .records(PERSON, "wario-land-4")
                .unwrap()
                .is_empty()
            {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(device.backend.launch_count(), 0);
        assert_eq!(
            device
                .host
                .save_store()
                .records(PERSON, "wario-land-4")
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn unavailable_baseline_game_does_not_block_other_game_sync() {
        fn add_games(device: &MigratedDevice, side: &str) {
            let library_path = device.root.path().join("catalog/library.yaml");
            let device_path = device.root.path().join("device.yaml");
            let mut library: serde_yaml::Value =
                serde_yaml::from_slice(&std::fs::read(&library_path).unwrap()).unwrap();
            let mut config: serde_yaml::Value =
                serde_yaml::from_slice(&std::fs::read(&device_path).unwrap()).unwrap();
            let template = library["library"]["wario-land-4"].clone();
            for game in ["aaa-blocked", "second-game"] {
                let content = format!("rom for {game}");
                let hash = crate::saves::record::hash(content.as_bytes());
                let mut entry = template.clone();
                entry["title"] = game.into();
                entry["releases"][0]["identity"]["value"] = hash.clone().into();
                if game == "aaa-blocked" {
                    entry["runner"] = "@missing:runner/run".into();
                }
                library["library"][game] = entry;
                config["locations"][hash.as_str()] = serde_yaml::to_value(
                    serde_json::json!([{"storage":"roms","path":format!("{game}.gba")}]),
                )
                .unwrap();
                std::fs::write(
                    device.root.path().join("roms").join(format!("{game}.gba")),
                    content,
                )
                .unwrap();
            }
            std::fs::write(library_path, serde_yaml::to_string(&library).unwrap()).unwrap();
            std::fs::write(device_path, serde_yaml::to_string(&config).unwrap()).unwrap();
            for game in ["aaa-blocked", "second-game", "wario-land-4"] {
                let folder = crate::saves::account_root(device.root.path(), PERSON)
                    .unwrap()
                    .join("saves")
                    .join(game)
                    .join("saves");
                std::fs::create_dir_all(&folder).unwrap();
                std::fs::write(folder.join("game.srm"), format!("{side} {game}")).unwrap();
            }
        }
        let a = MigratedDevice::with_missing_artifact(b"left", true).await;
        let b = MigratedDevice::with_missing_artifact(b"right", true).await;
        add_games(&a, "left");
        add_games(&b, "right");
        a.remember(&b);
        b.remember(&a);
        for device in [&a, &b] {
            let error = device
                .host
                .prepare("wario-land-4", Some(PERSON))
                .await
                .unwrap_err();
            assert_eq!(
                error.code, "ArtifactsNotReady",
                "real starts still require artifacts; declaration reads do not"
            );
        }
        a.host
            .save_contact(&a.service, b.host.save_device().unwrap())
            .await
            .expect("one unresolved declaration must not fail the meet");
        for (device, side) in [(&a, "left"), (&b, "right")] {
            let store = device.host.save_store();
            let heads = store.heads(PERSON).unwrap();
            assert_eq!(heads.games.len(), 2);
            for game in ["second-game", "wario-land-4"] {
                assert_eq!(
                    heads
                        .games
                        .iter()
                        .find(|heads| heads.game_id == game)
                        .unwrap()
                        .heads
                        .len(),
                    2
                );
                let records = store.records(PERSON, game).unwrap();
                assert_eq!(records.len(), 2);
                for record in records {
                    assert!(store
                        .complete(PERSON, game, &snapshot_id(&record).unwrap())
                        .unwrap());
                }
                assert_eq!(store.reconcile(PERSON, game).unwrap(), SaveMark::Split);
            }
            assert!(store.records(PERSON, "aaa-blocked").unwrap().is_empty());
            for game in ["aaa-blocked", "second-game", "wario-land-4"] {
                let file = crate::saves::account_root(device.root.path(), PERSON)
                    .unwrap()
                    .join("saves")
                    .join(game)
                    .join("saves/game.srm");
                assert_eq!(
                    std::fs::read(file).unwrap(),
                    format!("{side} {game}").as_bytes()
                );
            }
            assert_eq!(device.backend.launch_count(), 0);
            let path = device.root.path().join("catalog/library.yaml");
            let mut library: serde_yaml::Value =
                serde_yaml::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            library["library"]["aaa-blocked"]["runner"] = "@korri:mgba/mgba".into();
            std::fs::write(path, serde_yaml::to_string(&library).unwrap()).unwrap();
        }
        a.host
            .save_contact(&a.service, b.host.save_device().unwrap())
            .await
            .unwrap();
        for device in [&a, &b] {
            assert_eq!(
                device
                    .host
                    .save_store()
                    .records(PERSON, "aaa-blocked")
                    .unwrap()
                    .len(),
                2,
                "next contact must retry the deferred game"
            );
            assert_eq!(
                device
                    .host
                    .save_store()
                    .reconcile(PERSON, "aaa-blocked")
                    .unwrap(),
                SaveMark::Split
            );
        }
    }

    #[tokio::test]
    async fn migrated_saves_split_on_first_contact_before_any_launch() {
        let a = MigratedDevice::new(b"left progress").await;
        let b = MigratedDevice::new(b"right progress").await;
        assert!(a
            .host
            .save_store()
            .records(PERSON, "wario-land-4")
            .unwrap()
            .is_empty());
        assert!(b
            .host
            .save_store()
            .records(PERSON, "wario-land-4")
            .unwrap()
            .is_empty());
        a.remember(&b);
        b.remember(&a);
        a.host
            .save_contact(&a.service, b.host.save_device().unwrap())
            .await
            .unwrap();
        assert_eq!(
            a.host
                .save_store()
                .reconcile(PERSON, "wario-land-4")
                .unwrap(),
            SaveMark::Split
        );
        assert_eq!(
            b.host
                .save_store()
                .reconcile(PERSON, "wario-land-4")
                .unwrap(),
            SaveMark::Split
        );
        assert_eq!(a.backend.launch_count(), 0);
        assert_eq!(b.backend.launch_count(), 0);
    }

    #[tokio::test]
    async fn baselines_are_deferred_for_live_and_frozen_sessions() {
        let device = MigratedDevice::new(b"migrated progress").await;
        // An untracked native unit is sufficient to prevent the idle proof.
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        use super::super::systemd_unit::LaunchUnitBackend;
        device
            .backend
            .launch(id, &["game".into()], &Default::default())
            .unwrap();
        assert!(!device.host.baseline_saves().await.unwrap());
        device.backend.freeze(id).unwrap();
        assert!(!device.host.baseline_saves().await.unwrap());
        assert!(device
            .host
            .save_store()
            .records(PERSON, "wario-land-4")
            .unwrap()
            .is_empty());
        device.backend.stop(id).unwrap();
        assert!(device.host.baseline_saves().await.unwrap());
        assert_eq!(
            device
                .host
                .save_store()
                .records(PERSON, "wario-land-4")
                .unwrap()
                .len(),
            1
        );
        // A local snapshot suppresses reevaluation, even after the source changes.
        std::fs::write(
            device.root.path().join("plugin/plugin.ts"),
            "throw new Error('must not reevaluate');",
        )
        .unwrap();
        assert!(device.host.baseline_saves().await.unwrap());
    }

    #[tokio::test]
    async fn local_rpc_shows_split_and_appends_choice_and_restore_but_refuses_live_mutation() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(
            &config,
            "label = \"saves\"\n[[games]]\nid = \"one\"\ntitle = \"One\"\ncommand = [\"game\"]\n",
        )
        .unwrap();
        let host = HostRuntime::from_paths_with_backend(
            &config,
            None,
            root.path().to_owned(),
            Arc::new(super::super::systemd_unit::InMemoryLaunchUnitBackend::default()),
        );
        host.claim_save_owner();
        let store = host.save_store();
        let base = SnapshotRecord {
            parents: vec![],
            game_id: "one".into(),
            release_id: None,
            runner_id: "mgba".into(),
            device: PERSON.into(),
            ended_at: "2026-10-21T00:00:00.000Z".into(),
            duration_seconds: 1,
            interrupted: false,
            locations: vec![],
        };
        let base_id = store.put_record(PERSON, &base).unwrap();
        let mut ids = vec![];
        for content in [b"a", b"b"] {
            let mut record = base.clone();
            record.parents = vec![base_id.clone()];
            let hash = crate::saves::record::hash(content);
            record.locations = vec![SnapshotLocation {
                key: "saves".into(),
                kind: SaveKind::Save,
                files: vec![SaveFile {
                    path: "game.srm".into(),
                    sha256: hash.clone(),
                    bytes: 1,
                }],
            }];
            ids.push(store.put_record(PERSON, &record).unwrap());
            store
                .put_object(PERSON, &hash, &mut content.as_slice())
                .unwrap();
        }
        let (router, _) = crate::plain_host_routers_for_tests(host.clone());
        let status = rpc(&router, "app.saves.status", serde_json::json!({})).await;
        assert_eq!(status["outcome"]["payload"]["games"][0]["status"], "Split");
        let choice = rpc(
            &router,
            "app.saves.choose",
            serde_json::json!({"gameId":"one","snapshotId":ids[0]}),
        )
        .await;
        assert_eq!(choice["outcome"]["_tag"], "Ok", "{choice}");
        assert_eq!(
            choice["outcome"]["payload"]["records"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
        let restored = rpc(
            &router,
            "app.saves.restore",
            serde_json::json!({"gameId":"one","snapshotId":base_id}),
        )
        .await;
        assert_eq!(
            restored["outcome"]["payload"]["records"]
                .as_array()
                .unwrap()
                .len(),
            5
        );
        host.prepare("one", Some(PERSON)).await.unwrap();
        let refused = rpc(
            &router,
            "app.saves.restore",
            serde_json::json!({"gameId":"one","snapshotId":base_id}),
        )
        .await;
        assert_eq!(
            refused["outcome"]["payload"]["code"],
            "ActiveSessionConflict"
        );
        assert_eq!(store.records(PERSON, "one").unwrap().len(), 5);
    }
}
