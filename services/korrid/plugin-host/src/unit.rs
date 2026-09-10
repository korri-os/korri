use crate::{
    declaration::ServiceType,
    package::{self, Report},
    process, storage,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub struct Units {
    pub systemctl: PathBuf,
}

// org.freedesktop.systemd1.Service.ExecStopPost, from systemd's D-Bus treaty.
type ExecStatus = (String, Vec<String>, bool, u64, u64, u64, u64, u32, i32, i32);

impl Units {
    pub fn start(&self, report: &Report) -> Result<(), String> {
        let path = self.path(&report.id);
        storage::write_atomic(&path, render(report)?.as_bytes())?;
        self.checked(["daemon-reload"])?;
        let connection = zbus::blocking::connection::Builder::system()
            .map_err(|e| e.to_string())?
            .method_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| e.to_string())?;
        let manager = zbus::blocking::Proxy::new(
            &connection,
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
        )
        .map_err(|e| e.to_string())?;
        let _: zbus::zvariant::OwnedObjectPath = manager
            .call("LoadUnit", &report.unit)
            .map_err(|e| e.to_string())?;
        manager
            .call::<_, _, ()>("RefUnit", &report.unit)
            .map_err(|e| e.to_string())?;
        manager
            .call::<_, _, ()>("ResetFailedUnit", &report.unit)
            .map_err(|e| e.to_string())?;
        self.checked(["start", &report.unit])?;
        self.checked(["is-active", "--quiet", &report.unit])?;
        Ok(())
    }

    pub fn matches_running(&self, report: &Report) -> Result<bool, String> {
        let expected = render(report)?;
        match fs::read_to_string(self.path(&report.id)) {
            Ok(unit) if unit == expected => {}
            Ok(_) => return Ok(false),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.to_string()),
        }
        Ok(process::run(
            &self.systemctl,
            ["is-active", "--quiet", &report.unit],
            Duration::from_secs(10),
        )?
        .success)
    }

    pub fn stop(&self, id: &str, purge: bool) -> Result<(), String> {
        let path = self.path(id);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("cannot inspect managed unit: {error}")),
        };
        if !metadata.is_file() {
            return Err("managed unit path must be a regular file".into());
        }
        let unit = format!("{}.service", package::unit_name(id));
        // Keep a manager reference while stopping. Otherwise systemd can
        // unload an inactive unit before its cleanup exit status is read.
        let connection = zbus::blocking::connection::Builder::system()
            .map_err(|e| e.to_string())?
            .method_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| e.to_string())?;
        let manager = zbus::blocking::Proxy::new(
            &connection,
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
        )
        .map_err(|e| e.to_string())?;
        manager
            .call::<_, _, ()>("RefUnit", &unit)
            .map_err(|e| e.to_string())?;
        let object: zbus::zvariant::OwnedObjectPath =
            manager.call("GetUnit", &unit).map_err(|e| e.to_string())?;
        self.checked(["stop", &unit])?;
        let service = zbus::blocking::Proxy::new(
            &connection,
            "org.freedesktop.systemd1",
            object.as_str(),
            "org.freedesktop.systemd1.Service",
        )
        .map_err(|e| e.to_string())?;
        let cleanup: Vec<ExecStatus> = service
            .get_property("ExecStopPost")
            .map_err(|e| e.to_string())?;
        let main_started: u64 = service
            .get_property("ExecMainStartTimestampMonotonic")
            .map_err(|e| e.to_string())?;
        // A staged unit may never have started. Only that case can omit
        // cleanup. A daemon that ran must have a successful cleanup result.
        if cleanup.iter().any(|command| {
            let never_ran = main_started == 0 && command.3 == 0 && command.4 == 0 && command.7 == 0;
            !never_ran && (command.8 != libc::CLD_EXITED || command.9 != 0)
        }) {
            return Err(format!(
                "{unit} cleanup did not succeed; keep the package installed"
            ));
        }
        if purge {
            self.checked(["clean", "--what=state", &unit])?;
        }
        storage::remove(&path)?;
        self.checked(["daemon-reload"])?;
        Ok(())
    }

    pub fn purge_inactive(&self, report: &Report) -> Result<(), String> {
        let path = self.path(&report.id);
        storage::write_atomic(&path, render(report)?.as_bytes())?;
        self.checked(["daemon-reload"])?;
        self.checked(["clean", "--what=state", &report.unit])?;
        storage::remove(&path)?;
        self.checked(["daemon-reload"])?;
        Ok(())
    }

    pub fn path(&self, id: &str) -> PathBuf {
        Path::new("/run/systemd/system").join(format!("{}.service", package::unit_name(id)))
    }

    fn checked<I, S>(&self, args: I) -> Result<String, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        process::checked(&self.systemctl, args, Duration::from_secs(75))
    }
}

pub fn render(report: &Report) -> Result<String, String> {
    let daemon = &report.declaration.daemons[0];
    let name = package::unit_name(&report.id);
    let service_type = match daemon.service_type {
        ServiceType::Notify => "notify",
        ServiceType::Exec => "exec",
    };
    let mut unit = format!("[Unit]\nDescription=Korri managed plugin {}\nAfter=network.target\n\n[Service]\nType={service_type}\nDynamicUser=yes\nStateDirectory={name}\nStateDirectoryMode=0700\nRuntimeDirectory={name}\nRuntimeDirectoryMode=0700\nUMask=0077\nRestart=on-failure\nRestartSec=1\nTimeoutStartSec=30\nTimeoutStopSec=30\nKillMode=control-group\nNoNewPrivileges=yes\nProtectSystem=strict\nProtectHome=yes\nPrivateTmp=yes\nProtectKernelTunables=yes\nProtectKernelModules=yes\nProtectKernelLogs=yes\nProtectControlGroups=yes\nProtectProc=invisible\nRestrictSUIDSGID=yes\nRestrictRealtime=yes\nRestrictNamespaces=yes\nLockPersonality=yes\nRestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK\nDevicePolicy=closed\nCapabilityBoundingSet={}\nAmbientCapabilities={}\n", report.id, daemon.capabilities.join(" "), daemon.capabilities.join(" "));
    if report.host_network_admin() {
        unit.push_str("DeviceAllow=/dev/net/tun rw\n");
    }
    unit.push_str(&format!("ExecStart={}\n", command(report, &daemon.start)?));
    if let Some(cleanup) = &daemon.cleanup {
        unit.push_str(&format!("ExecStopPost={}\n", command(report, cleanup)?));
    }
    Ok(unit)
}

fn command(report: &Report, args: &[String]) -> Result<String, String> {
    let executable = package::resolve_executable(&report.package, &args[0])?;
    let mut command = vec![quote(executable.to_str().ok_or("invalid executable path")?)];
    for arg in &args[1..] {
        command.push(quote(
            &arg.replace("${STATE_DIRECTORY}", &report.state_directory)
                .replace("${RUNTIME_DIRECTORY}", &report.runtime_directory),
        ));
    }
    Ok(command.join(" "))
}

fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

impl Report {
    fn host_network_admin(&self) -> bool {
        self.declaration.host_network_admin()
    }
}
