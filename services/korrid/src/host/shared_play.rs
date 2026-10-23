//! Holder-side shared-play operations. No viewer or peer effects run here.
use super::*;

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
            let catalog = self.catalog_snapshot_blocking(active.person_public_key.as_deref())?;
            active.title = catalog
                .games
                .into_iter()
                .find(|game| game.id == game_id)
                .map(|game| game.title);
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
