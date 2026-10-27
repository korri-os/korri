//! Holder-side shared-play operations. No viewer or peer effects run here.
use super::*;

/// The title of one launch's game, as the catalog stated it when the launch was
/// first described. Session status runs on every portal and peer poll while
/// holding the session lock; building the catalog evaluates every installed
/// plugin, so doing that per poll queued status reads for tens of seconds on
/// a busy handheld. A launch's game does not change, so one lookup serves it.
pub(super) struct ActiveTitle {
    launch_id: String,
    game_id: String,
    person: Option<String>,
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
        self.control()?.describe_stream_players(active)?;
        if let Some(game_id) = active.game_id.as_deref() {
            let person = active.person_public_key.as_deref();
            if let Some(known) = self.active_title.lock().ok().and_then(|known| {
                known
                    .as_ref()
                    .filter(|known| {
                        known.launch_id == active.launch_id
                            && known.game_id == game_id
                            && known.person.as_deref() == person
                    })
                    .map(|known| known.title.clone())
            }) {
                active.title = Some(known);
                return Ok(());
            }
            let catalog = self.catalog_snapshot_blocking(person)?;
            active.title = catalog
                .games
                .into_iter()
                .find(|game| game.id == game_id)
                .map(|game| game.title);
            // A game the catalog does not list yet is looked up again next time.
            if let (Some(title), Ok(mut known)) = (&active.title, self.active_title.lock()) {
                *known = Some(ActiveTitle {
                    launch_id: active.launch_id.clone(),
                    game_id: game_id.to_owned(),
                    person: person.map(str::to_owned),
                    title: title.clone(),
                });
            }
        }
        Ok(())
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
