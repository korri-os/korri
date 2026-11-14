//! Drives the daemon's Linux viewer outside a federation, for the NixOS VM
//! proof of `@korri:viewer` (services/korrid/viewer-vm-test.nix).
//!
//! The probe owns only what a peer would supply: the certificate broker
//! returns a fixture Sunshine's certificate for the one client certificate
//! paired with it, no controller seats are occupied, and settings resolve
//! from an empty game library with the installed plugins. Selection, units,
//! focus and the lifecycle socket are the daemon's own `installed_viewer`.
//!
//! usage: viewer_probe ADDRESS SERVER_CERTIFICATE PAIRED_CLIENT_CERTIFICATE LIBRARY_ROOT
//! stdin: `ACTION LAUNCH` per line; ACTION is start, status, snapshot,
//! background, resume or stop. stdout: one JSON object per line.
use korrid::linux_viewer::{
    installed_viewer, render_settings, CertificateProvision, LinuxStreamViewer, LinuxViewerError,
    LocalSeats, ViewerSettings,
};
use std::{io::BufRead, path::PathBuf, sync::Arc};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [address, server, paired, library] = args.as_slice() else {
        eprintln!(
            "usage: viewer_probe ADDRESS SERVER_CERTIFICATE PAIRED_CLIENT_CERTIFICATE LIBRARY_ROOT"
        );
        std::process::exit(2);
    };
    let server = std::fs::read_to_string(server).expect("read the fixture server certificate");
    let paired = std::fs::read_to_string(paired).expect("read the paired client certificate");
    let library = PathBuf::from(library);
    let provision: CertificateProvision = Arc::new(move |host, client| {
        let server = server.clone();
        let paired = paired.clone();
        Box::pin(async move {
            eprintln!("viewer_probe: broker asked for host {host}");
            if client.trim() != paired.trim() {
                return Err(LinuxViewerError::new(
                    "the viewer's bootstrap certificate is not the paired one",
                ));
            }
            Ok(korrid::MoonlightCertificateProvisioned {
                server_certificate: server,
            })
        })
    });
    let seats: LocalSeats = Arc::new(|| Box::pin(async { Ok(Vec::new()) }));
    let settings: ViewerSettings = Arc::new(move |plugin, display| {
        let plugins = korrid::plugin_policy::installed_registry()
            .map_err(|error| LinuxViewerError::new(error.to_string()))?;
        render_settings(&library, &plugins, plugin.runner(), None, display)
    });
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let viewer = runtime.block_on(async { installed_viewer(provision, seats, settings) });
    for line in std::io::stdin().lock().lines() {
        let line = line.expect("read a probe command");
        let Some((action, launch)) = line.trim().split_once(' ') else {
            continue;
        };
        let result = runtime.block_on(run(viewer.as_ref(), action, launch, address));
        let reply = match result {
            Ok(value) => {
                serde_json::json!({"action": action, "launch": launch, "ok": true, "result": value})
            }
            Err(error) => {
                serde_json::json!({"action": action, "launch": launch, "ok": false, "error": error.to_string()})
            }
        };
        println!("{reply}");
    }
}

async fn run(
    viewer: &dyn LinuxStreamViewer,
    action: &str,
    launch: &str,
    address: &str,
) -> Result<serde_json::Value, LinuxViewerError> {
    Ok(match action {
        "start" => {
            viewer.start(launch, address, None).await?;
            serde_json::Value::Null
        }
        "status" => serde_json::json!(format!("{:?}", viewer.status(launch).await?)),
        "snapshot" => serde_json::to_value(viewer.snapshot(launch).await?)
            .map_err(|error| LinuxViewerError::new(error.to_string()))?,
        "background" => {
            viewer.background(launch).await?;
            serde_json::Value::Null
        }
        "resume" => {
            viewer.resume_and_focus(launch).await?;
            serde_json::Value::Null
        }
        "stop" => {
            viewer.stop(launch).await?;
            serde_json::Value::Null
        }
        other => return Err(LinuxViewerError::new(format!("unknown action {other}"))),
    })
}
