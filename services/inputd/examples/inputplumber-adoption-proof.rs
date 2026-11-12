//! VM-only driver for inputd's production profile reconciler. It needs no
//! korrid transport, and does not replace InputPlumber or a DBus implementation.
use korri_inputd::dbus::DbusSignalSource;
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
    let source = DbusSignalSource::system().await.unwrap();
    let mut last = String::new();
    loop {
        let status = format!(
            "{:?}",
            source.ensure_capture_profiles(Path::new(&profile)).await
        );
        if status != last {
            println!("{status}");
            last = status;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
