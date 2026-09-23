use crate::{
    package::{self, Report},
    process, storage,
};
use std::{collections::BTreeSet, fs, path::PathBuf, time::Duration};

pub struct Units {
    pub systemctl: PathBuf,
    pub unit_directory: PathBuf,
    pub ownership_directory: PathBuf,
    pub firewall: crate::firewall::Firewall,
}

#[derive(Clone)]
struct ManagedUnit<'a> {
    name: String,
    native: &'a crate::native_unit::NativeUnit,
}

// org.freedesktop.systemd1.Service.ExecStopPost, from systemd's D-Bus treaty.
type ExecStatus = (String, Vec<String>, bool, u64, u64, u64, u64, u32, i32, i32);

impl Units {
    pub fn start(&self, report: &Report) -> Result<(), String> {
        if report.native_units.is_empty() {
            return Ok(());
        }
        match self.start_units(report) {
            Ok(()) => Ok(()),
            Err(error) => match self.stop(&report.id, false) {
                Ok(()) => Err(error),
                Err(cleanup) => Err(format!("{error}; activation cleanup failed: {cleanup}")),
            },
        }
    }

    fn start_units(&self, report: &Report) -> Result<(), String> {
        let units = managed_units(report)?;
        if let Err(error) = self.write(report, &units) {
            return match self.discard_written(&report.id, &units) {
                Ok(()) => Err(error),
                Err(cleanup) => Err(format!("{error}; staged-unit cleanup failed: {cleanup}")),
            };
        }
        if let Err(error) = self.checked(["daemon-reload"]) {
            return match self.discard_written(&report.id, &units) {
                Ok(()) => Err(error),
                Err(cleanup) => Err(format!("{error}; staged-unit cleanup failed: {cleanup}")),
            };
        }
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
        for unit in &units {
            let object: zbus::zvariant::OwnedObjectPath = manager
                .call("LoadUnit", &unit.name)
                .map_err(|e| e.to_string())?;
            self.require_loaded_fragment(&connection, object.as_str(), &unit.name)?;
            manager
                .call::<_, _, ()>("RefUnit", &unit.name)
                .map_err(|e| e.to_string())?;
            manager
                .call::<_, _, ()>("ResetFailedUnit", &unit.name)
                .map_err(|e| e.to_string())?;
        }
        let mut start = vec!["start".to_owned()];
        start.extend(units.iter().map(|unit| unit.name.clone()));
        self.checked(start)?;
        for unit in &units {
            self.checked(["is-active", "--quiet", unit.name.as_str()])?;
        }
        // Do not expose a competing listener while activation is pending.
        // Every approved unit must be active before the host opens ports.
        self.firewall.apply(&report.id, &report.ports)?;
        Ok(())
    }

    pub fn matches_running(&self, report: &Report) -> Result<bool, String> {
        let units = managed_units(report)?;
        if units.is_empty() {
            return Ok(true);
        }
        if self.read_owned_units(&report.id)?
            != units
                .iter()
                .map(|unit| unit.name.clone())
                .collect::<Vec<_>>()
        {
            return Ok(false);
        }
        for unit in &units {
            match fs::read_to_string(self.unit_path(&unit.name)) {
                Ok(source) if source == unit.native.source => {}
                Ok(_) => return Ok(false),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                Err(e) => return Err(e.to_string()),
            }
            if fs::read_to_string(self.drop_in(&unit.name)).ok().as_deref()
                != Some(hardening_for(&report.id, unit.native).as_str())
            {
                return Ok(false);
            }
            let fragment = self.checked(["show", "--property=FragmentPath", "--value", &unit.name])?;
            if fragment.trim() != self.unit_path(&unit.name).to_string_lossy().as_ref() {
                return Err(format!("native unit {} is shadowed by a host unit", unit.name));
            }
            if !process::run(
                &self.systemctl,
                ["is-active", "--quiet", unit.name.as_str()],
                Duration::from_secs(10),
            )?
            .success
            {
                return Ok(false);
            }
        }
        Ok(true)
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
        let units = self.read_owned_units(id)?;
        if units.is_empty() {
            return Ok(());
        }
        for unit in &units {
            self.require_owned_unit(id, unit)?;
        }
        // Keep manager references while stopping. Otherwise systemd can unload
        // an inactive service before its cleanup exit status is read.
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
        let mut objects = Vec::new();
        for unit in &units {
            manager
                .call::<_, _, ()>("RefUnit", unit)
                .map_err(|e| e.to_string())?;
            let object: zbus::zvariant::OwnedObjectPath =
                manager.call("GetUnit", unit).map_err(|e| e.to_string())?;
            self.require_loaded_fragment(&connection, object.as_str(), unit)?;
            objects.push((unit, object));
        }
        let mut stop = vec!["stop".to_owned()];
        stop.extend(units.iter().cloned());
        self.checked(stop)?;
        for (unit, object) in &objects {
            if !unit.ends_with(".service") {
                continue;
            }
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
                let never_ran =
                    main_started == 0 && command.3 == 0 && command.4 == 0 && command.7 == 0;
                !never_ran && (command.8 != libc::CLD_EXITED || command.9 != 0)
            }) {
                return Err(format!(
                    "{unit} cleanup did not succeed; keep the package installed"
                ));
            }
        }
        if purge {
            for unit in units.iter().filter(|unit| unit.ends_with(".service")) {
                self.checked(["clean", "--what=state", unit.as_str()])?;
            }
        }
        for unit in &units {
            storage::remove(&self.unit_path(unit))?;
            self.remove_drop_in(unit)?;
        }
        storage::remove(&self.ownership_path(id))?;
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
        let cleanup_unit = purge_unit_name(report);
        self.remove_drop_in(&cleanup_unit)?;
        storage::write_atomic(&self.path(&report.id), cleanup.as_bytes())?;
        self.checked(["daemon-reload"])?;
        self.checked(["clean", "--what=state", &cleanup_unit])?;
        storage::remove(&self.path(&report.id))?;
        self.remove_drop_in(&cleanup_unit)?;
        self.checked(["daemon-reload"])?;
        Ok(())
    }

    fn write(&self, report: &Report, units: &[ManagedUnit<'_>]) -> Result<(), String> {
        storage::directory(&self.ownership_directory)?;
        for unit in units {
            let runtime_present = path_present(&self.unit_path(&unit.name))?
                || path_present(&self.drop_in(&unit.name))?;
            if runtime_present {
                self.require_owned_unit(&report.id, &unit.name)
                    .map_err(|_| {
                        format!(
                            "native unit {} already exists and is not owned by {}",
                            unit.name, report.id
                        )
                    })?;
            } else if process::run(
                &self.systemctl,
                ["cat", unit.name.as_str()],
                Duration::from_secs(10),
            )?
            .success
            {
                return Err(format!(
                    "native unit {} conflicts with an existing host unit",
                    unit.name
                ));
            }
        }
        let mut ownership = units
            .iter()
            .map(|unit| unit.name.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        ownership.push('\n');
        storage::write_atomic(&self.ownership_path(&report.id), ownership.as_bytes())?;
        for unit in units {
            let drop_in = self.drop_in(&unit.name);
            storage::directory(drop_in.parent().ok_or("invalid drop-in path")?)?;
            // The host writes policy and ownership before exposing source.
            // Plugin packages cannot ship drop-ins into this directory.
            storage::write_atomic(&drop_in, hardening_for(&report.id, unit.native).as_bytes())?;
            storage::write_atomic(&self.unit_path(&unit.name), unit.native.source.as_bytes())?;
        }
        Ok(())
    }

    fn discard_written(&self, id: &str, units: &[ManagedUnit<'_>]) -> Result<(), String> {
        let mut errors = Vec::new();
        for unit in units {
            if self.owner_matches(id, &unit.name) {
                if let Err(error) = storage::remove(&self.unit_path(&unit.name)) {
                    errors.push(error);
                }
                if let Err(error) = self.remove_drop_in(&unit.name) {
                    errors.push(error);
                }
            }
        }
        if let Err(error) = storage::remove(&self.ownership_path(id)) {
            errors.push(error);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn require_owned_unit(&self, id: &str, unit: &str) -> Result<(), String> {
        let metadata = fs::symlink_metadata(self.unit_path(unit))
            .map_err(|error| format!("cannot inspect managed unit {unit}: {error}"))?;
        if !metadata.is_file() {
            return Err(format!("managed unit {unit} must be a regular file"));
        }
        if !self.owner_matches(id, unit) {
            return Err(format!("managed unit {unit} has a different owner"));
        }
        Ok(())
    }

    fn owner_matches(&self, id: &str, unit: &str) -> bool {
        fs::read_to_string(self.drop_in(unit))
            .is_ok_and(|policy| policy.starts_with(&owner_marker(id)))
    }

    fn read_owned_units(&self, id: &str) -> Result<Vec<String>, String> {
        let path = self.ownership_path(id);
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("cannot read native unit ownership: {error}")),
        };
        let units = source.lines().map(str::to_owned).collect::<Vec<_>>();
        let unique = units.iter().collect::<BTreeSet<_>>();
        if units.is_empty()
            || units.len() > 64
            || unique.len() != units.len()
            || units.iter().any(|unit| !valid_owned_unit_name(unit))
        {
            return Err("native unit ownership metadata is invalid".into());
        }
        Ok(units)
    }

    fn unit_path(&self, unit: &str) -> PathBuf {
        self.unit_directory.join(unit)
    }

    fn drop_in(&self, unit: &str) -> PathBuf {
        self.unit_directory
            .join(format!("{unit}.d"))
            .join("zzzz-korri-policy.conf")
    }

    fn remove_drop_in(&self, unit: &str) -> Result<(), String> {
        let path = self.drop_in(unit);
        storage::remove(&path)?;
        match fs::remove_dir(path.parent().ok_or("invalid drop-in path")?) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    fn ownership_path(&self, id: &str) -> PathBuf {
        self.ownership_directory
            .join(format!("{}.units", package::unit_name(id)))
    }

    pub fn path(&self, id: &str) -> PathBuf {
        self.unit_directory
            .join(format!("{}.service", package::unit_name(id)))
    }

    pub fn has_owned_units(&self, id: &str) -> Result<bool, String> {
        Ok(!self.read_owned_units(id)?.is_empty())
    }

    fn require_loaded_fragment(
        &self,
        connection: &zbus::blocking::Connection,
        object: &str,
        unit: &str,
    ) -> Result<(), String> {
        let loaded = zbus::blocking::Proxy::new(
            connection,
            "org.freedesktop.systemd1",
            object,
            "org.freedesktop.systemd1.Unit",
        )
        .map_err(|e| e.to_string())?;
        let fragment: String = loaded.get_property("FragmentPath").map_err(|e| e.to_string())?;
        if fragment != self.unit_path(unit).to_string_lossy().as_ref() {
            return Err(format!("native unit {unit} is shadowed by a host unit: {fragment}"));
        }
        Ok(())
    }

    fn checked<I, S>(&self, args: I) -> Result<String, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        process::checked(&self.systemctl, args, Duration::from_secs(75))
    }
}

pub fn purge_unit_name(report: &Report) -> String {
    format!("{}.service", package::unit_name(&report.id))
}

fn managed_units(report: &Report) -> Result<Vec<ManagedUnit<'_>>, String> {
    let mut names = BTreeSet::new();
    report
        .native_units
        .iter()
        .map(|(declared, native)| {
            let name = crate::native_unit::managed_unit_name(declared, native.kind)?;
            if !names.insert(name.clone()) {
                return Err(format!("native unit name is duplicated: {name}"));
            }
            Ok(ManagedUnit { name, native })
        })
        .collect()
}

fn valid_owned_unit_name(unit: &str) -> bool {
    [".service", ".socket"].iter().any(|suffix| {
        unit.strip_suffix(suffix)
            .is_some_and(crate::declaration::valid_name)
    })
}

fn owner_marker(id: &str) -> String {
    format!("# korri-plugin-owner: {id}\n")
}

fn path_present(path: &std::path::Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

pub fn render(report: &Report) -> Result<String, String> {
    Ok(report
        .native_units
        .iter()
        .map(|(unit_name, unit)| {
            format!(
                "# native unit: {unit_name}\n{}\n{}",
                unit.source,
                hardening_for(&report.id, unit)
            )
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

pub fn hardening(report: &Report) -> String {
    report
        .native_units
        .values()
        .next()
        .map(|native| hardening_for(&report.id, native))
        .unwrap_or_default()
}

fn hardening_for(id: &str, native: &crate::native_unit::NativeUnit) -> String {
    let owner = owner_marker(id);
    if native.executables.is_empty() {
        return format!("{owner}# {}\n", package::BASE_POLICY);
    }
    let name = package::unit_name(id);
    let (runtime_directory, runtime_directory_mode) = runtime_directory_policy(&name, native);
    if native.user.as_deref() == Some("root") {
        // User=root requests device-wide authority, not another sandbox profile.
        // Administrative login sessions must retain account switching, writable
        // homes, PTYs and normal device access. Never grant this by plugin ID.
        let policy = package::ROOT_POLICY;
        return format!("{owner}# {policy}\n[Unit]\nAfter=network.target\n\n[Service]\nUser=root\nDynamicUser=no\nStateDirectory=\nStateDirectory={name}\nStateDirectoryMode=0700\nRuntimeDirectory=\nRuntimeDirectory={runtime_directory}\nRuntimeDirectoryMode={runtime_directory_mode}\nUMask=0077\nRestart=on-failure\nRestartSec=1\nTimeoutStartSec=30\nTimeoutStopSec=30\nKillMode=control-group\n");
    }
    let capabilities = native.capabilities.join(" ");
    let address_families = native
        .address_families
        .as_ref()
        .map(|families| families.join(" "))
        .unwrap_or_else(|| "AF_UNIX AF_INET AF_INET6 AF_NETLINK".into());
    let (user_policy, dynamic_user) = match native.user.as_deref() {
        Some(user) => (format!("User=\nUser={user}\n"), "no"),
        None => ("User=\n".into(), "yes"),
    };
    let no_new_privileges = if native
        .privileged_directives
        .iter()
        .any(|directive| directive == "NoNewPrivileges=false")
    {
        "no"
    } else {
        "yes"
    };
    let private_tmp = if native
        .privileged_directives
        .iter()
        .any(|directive| directive == "PrivateTmp=false")
    {
        "no"
    } else {
        "yes"
    };
    let protect_home = native
        .privileged_directives
        .iter()
        .find_map(|directive| directive.strip_prefix("ProtectHome="))
        .unwrap_or("yes");
    let device_policy = native.effective_device_policy().as_str();
    // List directives merge across fragments. Reset them explicitly before
    // installing the approved request. The native unit owns commands; the
    // host owns execution policy and never translates service templates.
    let mut policy = format!(
        "{owner}# {}\n[Unit]\nAfter=network.target\n\n[Service]\n{user_policy}DynamicUser={dynamic_user}\nStateDirectory=\nStateDirectory={name}\nStateDirectoryMode=0700\nRuntimeDirectory=\nRuntimeDirectory={runtime_directory}\nRuntimeDirectoryMode={runtime_directory_mode}\nUMask=0077\nRestart=on-failure\nRestartSec=1\nTimeoutStartSec=30\nTimeoutStopSec=30\nKillMode=control-group\nNoNewPrivileges={no_new_privileges}\nProtectSystem=strict\nProtectHome={protect_home}\nPrivateTmp={private_tmp}\nProtectKernelTunables=yes\nProtectKernelModules=yes\nProtectKernelLogs=yes\nProtectControlGroups=yes\nProtectProc=invisible\nRestrictSUIDSGID=yes\nRestrictRealtime=yes\nRestrictNamespaces=yes\nLockPersonality=yes\nRestrictAddressFamilies=\nRestrictAddressFamilies={address_families}\nDevicePolicy={device_policy}\nDeviceAllow=\nCapabilityBoundingSet=\nCapabilityBoundingSet={capabilities}\nAmbientCapabilities=\nAmbientCapabilities={capabilities}\nLoadCredential=\n",
        package::BASE_POLICY
    );
    for device in &native.devices {
        policy.push_str(&format!("DeviceAllow={device}\n"));
    }
    for credential in &native.credentials {
        policy.push_str(&format!("LoadCredential={credential}\n"));
    }
    policy
}

fn runtime_directory_policy(
    host_name: &str,
    native: &crate::native_unit::NativeUnit,
) -> (String, String) {
    match native.runtime_directory.as_deref() {
        Some(directory) => (
            directory.into(),
            native
                .runtime_directory_mode
                .clone()
                .unwrap_or_else(|| "0755".into()),
        ),
        None => (host_name.into(), "0700".into()),
    }
}
