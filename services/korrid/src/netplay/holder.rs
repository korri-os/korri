use super::*;
use crate::{
    authorization::{AuthorizationContext, Principal},
    *,
};

#[derive(Default)]
pub(crate) struct Holder {
    reservation: tokio::sync::Mutex<Option<(String, String, u8, bool)>>,
}
impl Holder {
    pub(crate) async fn stream_play(
        &self,
        host: &host::HostRuntime,
        launch: &str,
        player: StreamPlayer,
    ) -> Result<ActiveSession, RpcFailure> {
        let reservation = self.reservation.lock().await;
        if reservation
            .as_ref()
            .is_some_and(|(known_launch, device, _, _)| {
                known_launch == launch && device != &player.device_public_key
            })
        {
            return Err(failure(
                "NetplayReceiverPresent",
                "This session already has a receiving device.",
            ));
        }
        host.stream_play(launch, player).await
    }
}
pub(crate) async fn register(
    state: &AppState,
    context: &AuthorizationContext,
    request: &SessionNetplayRequest,
) -> SessionNetplayOutcome {
    let result = async {
        let (
            ServerMode::Host(host),
            AuthorizationContext::Peer(Principal::OwnerDevice {
                device_public_key, ..
            }),
        ) = (&state.mode, context)
        else {
            return Err(failure(
                "NoNetplayRoute",
                "Netplay requires a device of the same owner.",
            ));
        };
        if request.device_label.trim().is_empty()
            || request.device_label.len() > 256
            || request.device_label.chars().any(char::is_control)
        {
            return Err(failure(
                "InvalidRequest",
                "A published device label is required.",
            ));
        }
        let mut reservation = state.netplay_holder.reservation.lock().await;
        let host = host.clone();
        let expected = request.launch_id.clone();
        let local = tokio::task::spawn_blocking(move || host.netplay_local(&expected))
            .await
            .map_err(|_| failure("NoNetplayRoute", "Netplay is unavailable."))??;
        let already = reservation
            .as_ref()
            .filter(|(launch, _, _, _)| launch == &request.launch_id);
        if already.is_some_and(|(_, device, _, _)| device != device_public_key) {
            return Err(failure(
                "NetplayReceiverPresent",
                "This session already has a receiving device.",
            ));
        }
        let seat = if let Some((_, _, seat, _)) = already {
            *seat
        } else {
            let ServerMode::Host(host) = &state.mode else {
                unreachable!()
            };
            let host = host.clone();
            let expected = request.launch_id.clone();
            let device = device_public_key.clone();
            let seat =
                tokio::task::spawn_blocking(move || host.netplay_reserve_seat(&expected, &device))
                    .await
                    .map_err(|_| failure("NoNetplayRoute", "No controller seat is available."))??;
            // Keep the claim even when the toggle/readback fails. Retrying
            // cannot admit another device or consume another seat.
            *reservation = Some((
                request.launch_id.clone(),
                device_public_key.clone(),
                seat,
                false,
            ));
            seat
        };
        if !reservation.as_ref().expect("reserved seat").3 {
            let ServerMode::Host(host) = &state.mode else {
                unreachable!()
            };
            let host = host.clone();
            let expected = request.launch_id.clone();
            tokio::task::spawn_blocking(move || host.netplay_host(&expected))
                .await
                .map_err(|_| failure("NoNetplayRoute", "Hosting is unavailable."))??;
            reservation.as_mut().unwrap().3 = true;
        }
        let password = password::read(&local.config)
            .map_err(|_| failure("NoNetplayRoute", "The session credential is unavailable."))?;
        Ok(SessionNetplayPrepared {
            seat,
            port: 55435,
            password,
            release_key: local.facts.release_key,
            release_bytes: local.facts.release_bytes,
            runner: local.facts.runner,
        })
    }
    .await;
    result
        .map(SessionNetplayOutcome::Ok)
        .unwrap_or_else(SessionNetplayOutcome::Err)
}
pub(crate) async fn object(
    state: &AppState,
    context: &AuthorizationContext,
    key: &str,
) -> Result<std::fs::File, RpcFailure> {
    let (
        ServerMode::Host(host),
        AuthorizationContext::Peer(Principal::OwnerDevice {
            device_public_key, ..
        }),
    ) = (&state.mode, context)
    else {
        return Err(failure("NoNetplayRoute", "No release is available."));
    };
    let reservation = state.netplay_holder.reservation.lock().await;
    let (launch, device, _, _) = reservation
        .as_ref()
        .filter(|(_, device, _, _)| device == device_public_key)
        .ok_or_else(|| failure("NoNetplayRoute", "No netplay seat is reserved."))?;
    let _ = device;
    let host = host.clone();
    let launch = launch.clone();
    let key = key.to_owned();
    tokio::task::spawn_blocking(move || {
        let local = host.netplay_local(&launch)?;
        if local.facts.release_key != key {
            return Err(failure("NoNetplayRoute", "This is not the live release."));
        }
        std::fs::File::open(local.content)
            .map_err(|_| failure("CopyFailed", "The release could not be read."))
    })
    .await
    .map_err(|_| failure("CopyFailed", "The release could not be read."))?
}
