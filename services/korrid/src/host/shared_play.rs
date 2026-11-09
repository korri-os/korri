//! Holder-side shared-play operations. No viewer or peer effects run here.
use super::*;

/// The title of one launch's game, found once per launch. Session status runs
/// on every shell and peer poll, so the title is read from the launched
/// game's own entry, never from a full catalog: on the RG353M a catalog took
/// 2.3 to 3.6 s, and status polls that missed this cache each built one.
/// A launch's game does not change, so one lookup serves it.
pub(super) struct ActiveTitle {
    launch_id: String,
    game_id: String,
    title: String,
}

impl HostRuntime {
    pub(crate) fn local_label(&self) -> Option<&str> {
        self.config
            .as_ref()
            .ok()
            .map(|config| config.label.as_str())
    }

    pub(crate) fn describe_active_session(
        &self,
        active: &mut crate::ActiveSession,
    ) -> Result<(), RpcFailure> {
        self.describe_active_session_players(active)?;
        self.describe_active_session_catalog(active)
    }

    /// Launch-scoped journal facts belong to the native status snapshot.
    pub(crate) fn describe_active_session_players(
        &self,
        active: &mut crate::ActiveSession,
    ) -> Result<(), RpcFailure> {
        self.control()?.describe_stream_players(active)?;
        self.control()?.describe_runner_steps(active);
        active.netplay = self
            .describe_netplay(&active.launch_id)
            .ok()
            .map(|local| Box::new(local.facts));
        Ok(())
    }

    /// Catalog loading can read plugins and per-game play logs. It must not
    /// hold native transition authority merely to label an observed launch.
    pub(crate) fn describe_active_session_catalog(
        &self,
        active: &mut crate::ActiveSession,
    ) -> Result<(), RpcFailure> {
        let start = std::time::Instant::now();
        let result = (|| {
            if let Some(game_id) = active.game_id.as_deref() {
                #[cfg(test)]
                self.session_rpc_probe("app.session.status.catalog");
                let known = || {
                    self.active_title.lock().ok().and_then(|known| {
                        known
                            .as_ref()
                            .filter(|known| {
                                known.launch_id == active.launch_id && known.game_id == game_id
                            })
                            .map(|known| known.title.clone())
                    })
                };
                if let Some(title) = known() {
                    active.title = Some(title);
                    return Ok(());
                }
                // One lookup in flight: concurrent polls wait for it, then
                // read its answer.
                let _lookup = self
                    .title_lookup
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some(title) = known() {
                    active.title = Some(title);
                    return Ok(());
                }
                active.title = self.game_title(game_id)?;
                // A game without a title yet is looked up again next time.
                if let (Some(title), Ok(mut known)) = (&active.title, self.active_title.lock()) {
                    *known = Some(ActiveTitle {
                        launch_id: active.launch_id.clone(),
                        game_id: game_id.to_owned(),
                        title: title.clone(),
                    });
                }
            }
            Ok(())
        })();
        if start.elapsed() >= std::time::Duration::from_millis(100) {
            eprintln!(
                "<6>korrid: SessionCatalogDescription {{ launch_id: {}, elapsed_ms: {} }}",
                active.launch_id,
                start.elapsed().as_millis()
            );
        }
        result
    }

    /// One game's title, from its own entry: the host.toml title, or the
    /// library title. A library game without a title is named by its first
    /// identified release, as the catalog names a game by its release.
    pub(super) fn game_title(&self, game_id: &str) -> Result<Option<String>, RpcFailure> {
        #[cfg(test)]
        self.session_rpc_probe("host.game_title");
        let config = self.config.as_ref().map_err(config_failure)?;
        if let Some(game) = config.games.iter().find(|game| game.id == game_id) {
            return Ok(Some(game.title.clone()));
        }
        let Ok(root) = self.route_root() else {
            return Ok(None);
        };
        let (snapshot, _) = crate::config::settings::runner_choice_snapshot(root)
            .map_err(crate::game_routes::settings_failure)?;
        Ok(snapshot.library.get(game_id).and_then(|game| {
            game.title.clone().or_else(|| {
                game.releases
                    .iter()
                    .find_map(|release| release.identity.as_ref().map(|i| i.value().to_owned()))
            })
        }))
    }

    pub(crate) async fn stream_play(
        &self,
        launch_id: &str,
        player: crate::stream_play::StreamPlayer,
    ) -> Result<crate::ActiveSession, RpcFailure> {
        let runtime = self.clone();
        let launch_id = launch_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let mut active = runtime.control()?.stream_play(&launch_id, player)?;
            runtime.describe_active_session(&mut active)?;
            Ok(active)
        })
        .await
        .map_err(host_worker_failure)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn concurrent_status_reads_share_one_title_lookup() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(
            &config,
            "label = \"zao\"\n[[games]]\nid = \"static\"\ntitle = \"Static game\"\ncommand = [\"game\"]\n",
        )
        .unwrap();
        let lookups = Arc::new(AtomicUsize::new(0));
        let counted = lookups.clone();
        let host = HostRuntime::from_paths_with_backend(
            &config,
            None,
            root.path().join("private"),
            Arc::new(systemd_unit::InMemoryLaunchUnitBackend::default()),
        )
        .with_session_rpc_probe(Arc::new(move |phase| {
            if phase == "host.game_title" {
                counted.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }));
        let readers: Vec<_> = (0..5)
            .map(|_| {
                let host = host.clone();
                std::thread::spawn(move || {
                    let mut active: crate::ActiveSession =
                        serde_json::from_value(serde_json::json!({
                            "launchId": "launch-1",
                            "gameId": "static",
                        }))
                        .unwrap();
                    host.describe_active_session_catalog(&mut active).unwrap();
                    active.title
                })
            })
            .collect();
        for reader in readers {
            assert_eq!(reader.join().unwrap().as_deref(), Some("Static game"));
        }
        assert_eq!(lookups.load(Ordering::SeqCst), 1);
    }
}
