//! Read-only role observer. The kernel owns every automatic role decision.
//! This workaround only restarts the gadget and input providers after detach.
use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

fn role(sysfs: &Path) -> Result<String, String> {
    let mut roles = fs::read_dir(sysfs.join("class/usb_role"))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if roles.len() != 1 {
        return Err("expected one USB role switch".into());
    }
    let role =
        fs::read_to_string(roles.remove(0).path().join("role")).map_err(|e| e.to_string())?;
    match role.trim() {
        "host" | "device" => Ok(role.trim().into()),
        _ => Err(format!("unexpected role {role:?}")),
    }
}

fn run() -> Result<(), String> {
    if std::env::args().len() != 1 {
        return Err("usage: tabs6-typec-role-watch".into());
    }
    let root = std::env::var("TABS6_SYSFS").unwrap_or("/sys".into());
    let sysfs = Path::new(&root);
    let helper = std::env::var("TABS6_USB_ROLE_HELPER").unwrap_or("tabs6-usb-role".into());
    let mut previous = role(sysfs)?;
    loop {
        thread::sleep(Duration::from_millis(50));
        let current = match role(sysfs) {
            Ok(role) => role,
            Err(error) => {
                // Preserve the last observed host across transient read errors,
                // so the next device reading still triggers provider cleanup.
                eprintln!("tabs6-typec-role-watch: role read failed: {error}");
                continue;
            }
        };
        if previous == "host" && current == "device" {
            println!("kernel host -> device; restarting detached input providers");
            let status = Command::new(&helper)
                .arg("detached")
                .status()
                .map_err(|e| e.to_string())?;
            if !status.success() {
                return Err(format!("detach workaround failed: {status}"));
            }
        }
        previous = current;
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("tabs6-typec-role-watch: {error}");
        std::process::exit(1);
    }
}
