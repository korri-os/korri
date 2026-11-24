//! VM-only driver for inputd's production profile reconciler. It needs no
//! korrid transport, and does not replace InputPlumber or a DBus implementation.
use korri_inputd::{
    action_catalog::ActionRoutes,
    capture::CaptureRuntime,
    dbus::{DbusSignalSource, ProfileStatus},
    devices::{CaptureProvider, EvdevProvider},
};
use std::{path::Path, time::Duration};

#[tokio::main]
async fn main() {
    let name = "korri-inputplumber-adoption";
    assert_eq!(
        std::fs::read_to_string("/etc/hostname").unwrap().trim(),
        name
    );
    assert_eq!(
        std::fs::read_to_string(format!("/etc/{name}-vm")).unwrap(),
        "isolated-kernel-proof-only\n"
    );
    assert_eq!(
        std::fs::read_to_string("/sys/class/dmi/id/product_name")
            .unwrap()
            .trim(),
        name
    );
    let profile = std::env::args().nth(1).expect("immutable profile path");
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    let capture = std::env::args().nth(2).as_deref() == Some("--capture");
    let source = DbusSignalSource::system().await.unwrap();
    let mut runtime = CaptureRuntime::with_action_routes(ActionRoutes::default());
    let mut provider = EvdevProvider::default();
    let mut reconcile = tokio::time::interval(Duration::from_millis(100));
    let mut last = String::new();
    loop {
        tokio::select! {
            _ = reconcile.tick() => {
                if capture && provider.enumerate_capture().is_ok_and(|devices| runtime.topology_settled(&devices)) {
                    continue;
                }
                let result = source.ensure_capture_profiles(Path::new(&profile)).await;
                let status = format!("{result:?}");
                if status != last {
                    println!("{status}");
                    last = status;
                }
                if capture {
                    runtime.set_dbus_owner(source.current_owner().await.unwrap().as_deref());
                    match result {
                        Ok(ProfileStatus::Ready | ProfileStatus::Applied) => {
                            runtime.reconcile(&mut provider, &source).await;
                        }
                        Ok(ProfileStatus::MissingSource) => runtime.source_missing(),
                        Ok(ProfileStatus::AmbiguousSources) => runtime.source_ambiguous(),
                        _ => runtime.set_dbus_owner(None),
                    }
                }
            }
            result = runtime.next_evdev_actions(), if capture && runtime.has_open_target() => {
                result.unwrap();
            }
        }
        match runtime.take_updates() {
            Ok(updates) => {
                for update in updates {
                    println!("{}", serde_json::to_string(&update).unwrap());
                }
            }
            Err(_) => runtime.transport_lost(),
        }
    }
}
