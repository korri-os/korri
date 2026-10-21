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
        let registry = self.route_registry()?;
        let launch = match runner {
            Some(runner) => crate::game_routes::selected_launch(
                self.route_root()?,
                &registry,
                &crate::game_routes::SelectedGameLaunchRequest {
                    game_id: game.into(),
                    runner_id: runner.into(),
                    overrides: None,
                },
                None,
                &self.launch_person()?,
            )?,
            None => crate::game_routes::default_launch(
                self.route_root()?,
                &registry,
                game,
                None,
                &self.launch_person()?,
            )?,
        };
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
    pub(crate) async fn save_check(
        &self,
        request: SaveCheckRequest,
        peer: Option<Arc<SavePeerService>>,
    ) -> Result<SaveCheck, RpcFailure> {
        let preparing = self.clone();
        let preparing_game = request.game_id.clone();
        let runner = request.runner_id.clone();
        tokio::task::spawn_blocking(move || {
            preparing.control()?.with_idle_session(|| {
                if let Ok(Some((input, prepared))) =
                    preparing.save_declaration(&preparing_game, runner.as_deref())
                {
                    if !prepared.plan.save_locations.is_empty() {
                        crate::saves::capture_initial(
                            &preparing.save_store(),
                            &preparing.launch_person()?,
                            &prepared.plan.save_locations,
                            SnapshotRecord {
                                parents: vec![],
                                game_id: preparing_game,
                                release_id: prepared.release_id,
                                runner_id: input.runner_id,
                                device: preparing.save_device()?.into(),
                                ended_at: now(),
                                duration_seconds: 0,
                                interrupted: false,
                                locations: vec![],
                            },
                        )
                        .map_err(failure)?;
                    }
                }
                Ok(())
            })
        })
        .await
        .map_err(host_worker_failure)??;
        let person = self.launch_person()?;
        let mut answered = false;
        let mut failed = false;
        let mut observations = Vec::new();
        if let Some(peer) = peer {
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
            match peer.pull_before_launch(&request.game_id, deadline).await {
                Ok(checks) => {
                    for check in checks {
                        if let Some(in_use) = check.in_use {
                            answered = true;
                            let since = match in_use {
                                SaveInUse::Active(value) => Some(value.since),
                                SaveInUse::Idle(_) => None,
                            };
                            observations.push((check.device, since));
                        }
                        failed |= check.transfer.is_err();
                    }
                }
                Err(_) => failed = true,
            }
        }
        let host = self.clone();
        tokio::task::spawn_blocking(move || {
            if host.launch_person()? != person {
                return Err(RpcFailure {
                    code: "SaveIdentityChanged".into(),
                    message: "The device owner changed. Check saves again.".into(),
                });
            }
            let store = host.save_store();
            for (device, since) in observations {
                store
                    .remember_in_use(&person, &request.game_id, &device, since)
                    .map_err(failure)?;
            }
            let mut status = host.save_mark(&request.game_id)?;
            if status == SaveMark::NotChecked && answered && !failed {
                status = SaveMark::Synced;
            }
            if status == SaveMark::Synced && (!answered || failed) {
                status = SaveMark::NotChecked;
            }
            let history = host.save_history_blocking(&request.game_id)?;
            let latest = history
                .current
                .as_ref()
                .and_then(|id| history.records.iter().find(|r| &r.snapshot_id == id));
            let mut warnings = Vec::new();
            if let Some(latest) = latest {
                if let Ok(Some((input, prepared))) =
                    host.save_declaration(&request.game_id, request.runner_id.as_deref())
                {
                    if latest.record.release_id.is_some()
                        && latest.record.release_id != prepared.release_id
                    {
                        let title = host
                            .catalog_snapshot_blocking(Some(&person))
                            .ok()
                            .and_then(|c| c.games.into_iter().find(|g| g.id == request.game_id))
                            .map(|g| g.title)
                            .unwrap_or_else(|| "this game".into());
                        warnings.push(format!(
                            "These saves came from a different release of {title}."
                        ));
                    }
                    if latest.record.runner_id != input.runner_id
                        && latest
                            .record
                            .locations
                            .iter()
                            .any(|l| l.kind == SaveKind::Savestate && !l.files.is_empty())
                    {
                        warnings.push(
                            "These savestates came from a different runner and may not load."
                                .into(),
                        );
                    }
                }
            }
            Ok(SaveCheck {
                warnings,
                status,
                in_use: store
                    .in_use_warnings(&person, &request.game_id)
                    .map_err(failure)?,
                release_id: latest.and_then(|r| r.record.release_id.clone()),
                runner_id: latest
                    .filter(|r| {
                        r.record
                            .locations
                            .iter()
                            .any(|l| l.kind == SaveKind::Savestate && !l.files.is_empty())
                    })
                    .map(|r| r.record.runner_id.clone()),
            })
        })
        .await
        .map_err(host_worker_failure)?
    }
    pub(crate) fn save_in_use(&self, person: &str, game: &str) -> Result<Option<u64>, SaveError> {
        let live = self
            .control()
            .map_err(|e| SaveError::Storage(e.message))?
            .save_in_use(person, game)
            .map_err(SaveError::Storage)?;
        Ok(live.or_else(|| self.launch_reservations.in_use(person, game)))
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
            if let Ok(peers) = directory.snapshot() {
                for contact in peers {
                    if let Err(error) = peer.on_contact(&contact.device_public_key).await {
                        eprintln!("korrid save contact: {}", error.failure().code);
                    }
                }
            }
            loop {
                tokio::select! {
                    game=snapshots.recv()=>match game {Some(game)=> {
                        match peer.after_snapshot(&game).await {Ok(results)=>for (_,result) in results {if let Err(error)=result {eprintln!("korrid save push: {}",error.failure().code);}},Err(error)=>eprintln!("korrid save push: {}",error.failure().code)}
                        if let (Ok(person),Ok(device))=(host.launch_person(),host.save_device()) {if let Err(error)=host.save_store().prune(&person,&game,device) {eprintln!("korrid save retention: {error}");}}
                    },None=>break},
                    contact=contacts.recv()=>match contact {Ok(device)=> {if let Err(error)=peer.on_contact(&device).await {eprintln!("korrid save contact: {}",error.failure().code);}},Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>{if let Ok(peers)=directory.snapshot(){for contact in peers {let _=peer.on_contact(&contact.device_public_key).await;}}},Err(_)=>break},
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
