use crate::{
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
    pub firewall: crate::firewall::Firewall,
}

// org.freedesktop.systemd1.Service.ExecStopPost, from systemd's D-Bus treaty.
type ExecStatus = (String, Vec<String>, bool, u64, u64, u64, u64, u32, i32, i32);

impl Units {
    pub fn start(&self, report: &Report) -> Result<(), String> {
        if report.native_unit.is_none() {
            return Ok(());
        }
        self.write(report)?;
        self.checked(["daemon-reload"])?;
        self.firewall.apply(&report.id, &report.ports)?;
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
        // A network-administration daemon can add its own INPUT jumps while
        // starting. Finish this operation with the host chain first.
        self.firewall.apply(&report.id, &report.ports)?;
        Ok(())
    }

    pub fn matches_running(&self, report: &Report) -> Result<bool, String> {
        let Some(native) = &report.native_unit else {
            return Ok(true);
        };
        match fs::read_to_string(self.path(&report.id)) {
            Ok(unit) if unit == native.source => {}
            Ok(_) => return Ok(false),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.to_string()),
        }
        if fs::read_to_string(self.drop_in(&report.id)).ok().as_deref()
            != Some(hardening(report).as_str())
        {
            return Ok(false);
        }
        Ok(process::run(
            &self.systemctl,
            ["is-active", "--quiet", &report.unit],
            Duration::from_secs(10),
        )?
        .success)
    }

    pub fn stop(&self, id: &str, purge: bool) -> Result<(), String> {
        let firewall = self.firewall.remove(id);
        let service = self.stop_service(id, purge);
        match (firewall, service) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(a), Err(b)) => Err(format!("{a}; {b}")),
            (Err(error), _) | (_, Err(error)) => Err(error),
        }
    }

    fn stop_service(&self, id: &str, purge: bool) -> Result<(), String> {
        let path = self.path(id);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return self.remove_drop_in(id)
            }
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
        self.remove_drop_in(id)?;
        self.checked(["daemon-reload"])?;
        Ok(())
    }

    pub fn purge_inactive(&self, report: &Report) -> Result<(), String> {
        self.firewall.remove(&report.id)?;
        // State belongs to the plugin ID, not its current services. An update
        // can remove every service while retaining data from an older unit.
        // Load only host-owned cleanup metadata; never start plugin code just
        // to give systemd the StateDirectory cleanup contract. systemd 258's
        // exec_context_get_clean_directories removes both public and private
        // state paths unconditionally, including an earlier User=root build.
        let executable = self
            .systemctl
            .to_str()
            .ok_or("systemctl path must be UTF-8")?
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%");
        let cleanup = format!(
            "[Unit]\nRefuseManualStart=yes\n[Service]\nType=oneshot\nExecStart=\"{executable}\" --version\nDynamicUser=yes\nStateDirectory={}\n",
            package::unit_name(&report.id)
        );
        self.remove_drop_in(&report.id)?;
        storage::write_atomic(&self.path(&report.id), cleanup.as_bytes())?;
        self.checked(["daemon-reload"])?;
        self.checked(["clean", "--what=state", &report.unit])?;
        storage::remove(&self.path(&report.id))?;
        self.remove_drop_in(&report.id)?;
        self.checked(["daemon-reload"])?;
        Ok(())
    }

    fn write(&self, report: &Report) -> Result<(), String> {
        let native = report.native_unit.as_ref().ok_or("no native service")?;
        let drop_in = self.drop_in(&report.id);
        storage::directory(drop_in.parent().ok_or("invalid drop-in path")?)?;
        // The host writes the policy before exposing the native unit. Plugin
        // packages cannot ship drop-ins into this host-owned directory.
        storage::write_atomic(&drop_in, hardening(report).as_bytes())?;
        storage::write_atomic(&self.path(&report.id), native.source.as_bytes())
    }

    fn drop_in(&self, id: &str) -> PathBuf {
        self.path(id)
            .with_extension("service.d")
            .join("zzzz-korri-policy.conf")
    }

    fn remove_drop_in(&self, id: &str) -> Result<(), String> {
        let path = self.drop_in(id);
        storage::remove(&path)?;
        match fs::remove_dir(path.parent().ok_or("invalid drop-in path")?) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
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
    Ok(report
        .native_unit
        .as_ref()
        .map(|unit| format!("{}\n{}", unit.source, hardening(report)))
        .unwrap_or_default())
}

pub fn hardening(report: &Report) -> String {
    let Some(native) = &report.native_unit else {
        return String::new();
    };
    let name = package::unit_name(&report.id);
    if native.user.as_deref() == Some("root") {
        // User=root requests device-wide authority, not another sandbox profile.
        // Administrative login sessions must retain account switching, writable
        // homes, PTYs and normal device access. Never grant this by plugin ID.
        let policy = package::ROOT_POLICY;
        return format!("# {policy}\n[Unit]\nAfter=network.target\n\n[Service]\nUser=root\nDynamicUser=no\nStateDirectory=\nStateDirectory={name}\nStateDirectoryMode=0700\nRuntimeDirectory=\nRuntimeDirectory={name}\nRuntimeDirectoryMode=0700\nUMask=0077\nRestart=on-failure\nRestartSec=1\nTimeoutStartSec=30\nTimeoutStopSec=30\nKillMode=control-group\n");
    }
    let capabilities = native.capabilities.join(" ");
    // List directives merge across fragments. Reset them explicitly before
    // installing the approved request. The native unit owns commands; the
    // host owns execution policy and never translates service templates.
    let mut policy = format!("[Unit]\nAfter=network.target\n\n[Service]\nUser=\nDynamicUser=yes\nStateDirectory=\nStateDirectory={name}\nStateDirectoryMode=0700\nRuntimeDirectory=\nRuntimeDirectory={name}\nRuntimeDirectoryMode=0700\nUMask=0077\nRestart=on-failure\nRestartSec=1\nTimeoutStartSec=30\nTimeoutStopSec=30\nKillMode=control-group\nNoNewPrivileges=yes\nProtectSystem=strict\nProtectHome=yes\nPrivateTmp=yes\nProtectKernelTunables=yes\nProtectKernelModules=yes\nProtectKernelLogs=yes\nProtectControlGroups=yes\nProtectProc=invisible\nRestrictSUIDSGID=yes\nRestrictRealtime=yes\nRestrictNamespaces=yes\nLockPersonality=yes\nRestrictAddressFamilies=\nRestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK\nDevicePolicy=closed\nDeviceAllow=\nCapabilityBoundingSet=\nCapabilityBoundingSet={capabilities}\nAmbientCapabilities=\nAmbientCapabilities={capabilities}\nLoadCredential=\n");
    for device in &native.devices {
        policy.push_str(&format!("DeviceAllow={device}\n"));
    }
    for credential in &native.credentials {
        policy.push_str(&format!("LoadCredential={credential}\n"));
    }
    policy
}
