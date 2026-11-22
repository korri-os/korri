//! Android edge: the Korri app with no Korri Java (`app`, #434). Its own
//! library calls `app::start` in its process; no Java calls into korrid.
//! The Artemis app's JNI bridge (`KorridServer`) went with that app on
//! 2026-11-18 (ADR 0087; the tag `artemis-final` keeps it).

use crate::{
    MoonlightCertificateProvisionOutcome, MoonlightCertificateProvisionRequest, RpcRequest,
    RpcResponse,
};
pub mod app;

/// This device's broker provisions a client certificate at the Sunshine with
/// this host UUID, through korrid's own authenticated localhost RPC, for the
/// Android app's in-process viewer (`app.rs`). Returns Sunshine's server
/// certificate, or a code that names the failure.
pub(crate) async fn provision_locally(
    host_uuid: String,
    client_certificate: String,
) -> Result<String, &'static str> {
    if crate::host::moonlight_certificate::validate_host_uuid(&host_uuid).is_err()
        || crate::host::moonlight_certificate::validate_single_pem(&client_certificate).is_err()
    {
        return Err("InvalidRequest");
    }
    let port = crate::local_server_port().ok_or("ServerUnavailable")?;
    let capability = crate::local_server_capability().ok_or("ServerUnavailable")?;
    let request = RpcRequest::MoonlightCertificateProvision(MoonlightCertificateProvisionRequest {
        host_uuid,
        client_certificate,
    });
    // The broker owns one deadline across peer selection and provision.
    // Keep the localhost caller alive longer than that complete operation.
    let client = reqwest::Client::builder()
        .timeout(crate::upstreams::MOONLIGHT_CERTIFICATE_CALLER_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "ServerUnavailable")?;
    let response = client
        .post(format!("http://127.0.0.1:{port}/rpc"))
        .bearer_auth(capability)
        .json(&request)
        .send()
        .await
        .map_err(|_| "PeerUnavailable")?;
    if !response.status().is_success() {
        return Err("PeerUnavailable");
    }
    let response = response
        .json::<RpcResponse>()
        .await
        .map_err(|_| "InvalidResponse")?;
    match response {
        RpcResponse::MoonlightCertificateProvision(MoonlightCertificateProvisionOutcome::Ok(
            provisioned,
        )) => Ok(provisioned.server_certificate),
        RpcResponse::MoonlightCertificateProvision(MoonlightCertificateProvisionOutcome::Err(
            failure,
        )) => Err(match failure.code.as_str() {
            "MoonlightHostNotFound" => "HostNotFound",
            "MoonlightHostAmbiguous" => "HostAmbiguous",
            "MoonlightCertificatePeerUnavailable" => "PeerUnavailable",
            "MoonlightCertificateInvalid" => "InvalidCertificate",
            _ => "ProvisioningFailed",
        }),
        _ => Err("InvalidResponse"),
    }
}

/// The emulator proof's resize request, through korrid's local RPC (#476).
#[cfg(feature = "proof")]
pub(crate) fn resize_locally(
    launch: &str,
    display: crate::launcher::plugin_launch::PluginDisplay,
) -> Result<crate::live_display::ViewerResize, String> {
    use crate::live_display::{SessionViewerResizeOutcome, SessionViewerResizeRequest};
    let (Some(port), Some(capability)) =
        (crate::local_server_port(), crate::local_server_capability())
    else {
        return Err("korrid is not running".into());
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|cause| cause.to_string())?;
    runtime.block_on(async move {
        let response = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|cause| cause.to_string())?
            .post(format!("http://127.0.0.1:{port}/rpc"))
            .bearer_auth(capability)
            .json(&RpcRequest::SessionViewerResize(
                SessionViewerResizeRequest {
                    expected_launch_id: launch.to_owned(),
                    display,
                },
            ))
            .send()
            .await
            .map_err(|cause| cause.to_string())?
            .error_for_status()
            .map_err(|cause| cause.to_string())?;
        match response
            .json::<RpcResponse>()
            .await
            .map_err(|cause| cause.to_string())?
        {
            RpcResponse::SessionViewerResize(SessionViewerResizeOutcome::Ok(answer)) => Ok(answer),
            RpcResponse::SessionViewerResize(SessionViewerResizeOutcome::Err(failure)) => {
                Err(format!("{}: {}", failure.code, failure.message))
            }
            _ => Err("korrid answered another request".into()),
        }
    })
}

/// The person left the stream of the exact `launch` for the gameplay
/// overlay: korrid's `session.leave` through its own authenticated
/// localhost RPC, which checks the selection and publishes Home without
/// freezing the streamed game. True when korrid answered that this launch
/// runs on. The Android app's viewer asks through `app::leave_viewer`. It
/// blocks on a runtime of its own: never call it on a runtime's thread.
pub(crate) fn leave_locally(launch: &str) -> bool {
    if launch.len() != 32
        || !launch
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return false;
    }
    let (Some(port), Some(capability)) =
        (crate::local_server_port(), crate::local_server_capability())
    else {
        return false;
    };
    let result = (|| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()?;
        runtime.block_on(async move {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .ok()?;
            let response = client
                .post(format!("http://127.0.0.1:{port}/rpc"))
                .bearer_auth(capability)
                .json(&RpcRequest::SessionLeave(crate::SessionLeaveRequest {
                    expected_launch_id: Some(launch.to_owned()),
                }))
                .send()
                .await
                .ok()?
                .error_for_status()
                .ok()?;
            match response.json::<RpcResponse>().await.ok()? {
                RpcResponse::SessionLeave(crate::SessionTransitionOutcome::Ok(result))
                    if result.launch_id == launch
                        && result.state == crate::SessionFreezerState::Running =>
                {
                    Some(())
                }
                _ => None,
            }
        })
    })();
    result.is_some()
}
