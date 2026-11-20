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

/// One unit the host writes for a plugin: its source and the host's policy
/// drop-in, which starts with the owner marker.
#[derive(Clone)]
struct ManagedUnit {
    name: String,
    source: String,
    policy: String,
    /// A service template (`name@.service`) runs only as instances that its
    /// socket starts: it is written and removed with the plugin, but never
    /// loaded, started, upheld or checked for activity by name.
    template: bool,
}

impl ManagedUnit {
    fn upheld(&self) -> bool {
        !self.template
    }
}

// org.freedesktop.systemd1.Service.ExecStopPost, from systemd's D-Bus treaty.
type ExecStatus = (String, Vec<String>, bool, u64, u64, u64, u64, u32, i32, i32);

impl Units {
    pub fn start(&self, report: &Report) -> Result<(), String> {
        self.start_all(&[report]).pop().unwrap_or(Ok(()))
    }

    /// Start several plugins' units with one systemd reload. Each reload cost
    /// about 1.5 s on the RG353M at boot. The result at each index belongs to
    /// the report at that index; a plugin that fails is cleaned up alone.
    pub fn start_all(&self, reports: &[&Report]) -> Vec<Result<(), String>> {
        let mut results = Vec::with_capacity(reports.len());
        let mut staged = Vec::new();
        for (index, report) in reports.iter().enumerate() {
            if report.native_units.is_empty() {
                results.push(Ok(()));
                continue;
            }
            let units = match managed_units(report) {
                Ok(units) => units,
                Err(error) => {
                    results.push(Err(self.cleanup_failed(report, error)));
                    continue;
                }
            };
            if let Err(error) = self.write(report, &units) {
                let error = self.discard_after(report, &units, error);
                results.push(Err(self.cleanup_failed(report, error)));
                continue;
            }
            results.push(Ok(()));
            staged.push((index, units));
        }
        if staged.is_empty() {
            return results;
        }
        if let Err(error) = self.checked(["daemon-reload"]) {
            for (index, _) in &staged {
                let report = reports[*index];
                results[*index] = Err(self.cleanup_failed(report, error.clone()));
            }
            return results;
        }
        for (index, units) in &staged {
            let report = reports[*index];
            if let Err(error) = self.activate(report, units) {
                results[*index] = Err(self.cleanup_failed(report, error));
            }
        }
        results
    }

    fn discard_after(&self, report: &Report, units: &[ManagedUnit], error: String) -> String {
        match self.discard_written(&report.id, units) {
            Ok(()) => error,
            Err(cleanup) => format!("{error}; staged-unit cleanup failed: {cleanup}"),
        }
    }

    fn cleanup_failed(&self, report: &Report, error: String) -> String {
        match self.stop(&report.id, false) {
            Ok(()) => error,
            Err(cleanup) => format!("{error}; activation cleanup failed: {cleanup}"),
        }
    }

    fn activate(&self, report: &Report, units: &[ManagedUnit]) -> Result<(), String> {
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
        for unit in units.iter().filter(|unit| unit.upheld()) {
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
        start.extend(
            units
                .iter()
                .filter(|unit| unit.upheld())
                .map(|unit| unit.name.clone()),
        );
        self.checked(start)?;
        for unit in units.iter().filter(|unit| unit.upheld()) {
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
            if unit.upheld()
                && fs::read_link(self.uphold(&unit.name)).ok() != Some(uphold_target(&unit.name))
            {
                return Ok(false);
            }
            match fs::read_to_string(self.unit_path(&unit.name)) {
                Ok(source) if source == unit.source => {}
                Ok(_) => return Ok(false),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                Err(e) => return Err(e.to_string()),
            }
            if fs::read_to_string(self.drop_in(&unit.name)).ok().as_deref()
                != Some(unit.policy.as_str())
            {
                return Ok(false);
            }
            if unit.template {
                continue;
            }
            let fragment =
                self.checked(["show", "--property=FragmentPath", "--value", &unit.name])?;
            if fragment.trim() != self.unit_path(&unit.name).to_string_lossy().as_ref() {
                return Err(format!(
                    "native unit {} is shadowed by a host unit",
                    unit.name
                ));
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
        for unit in units.iter().filter(|unit| !is_template(unit)) {
            manager
                .call::<_, _, ()>("RefUnit", unit)
                .map_err(|e| e.to_string())?;
            let object: zbus::zvariant::OwnedObjectPath =
                manager.call("GetUnit", unit).map_err(|e| e.to_string())?;
            self.require_loaded_fragment(&connection, object.as_str(), unit)?;
            objects.push((unit, object));
        }
        // Block concurrent settings-change restarts as well as automatic
        // starts. The barrier stays loaded until the stopped unit is removed;
        // enabled staging replaces the policy and clears it.
        for unit in &units {
            self.refuse_manual_start(unit)?;
            self.remove_uphold(unit)?;
        }
        self.checked(["daemon-reload"])?;
        // A template stops as the pattern of its running instances.
        let mut stop = vec!["stop".to_owned()];
        stop.extend(
            units
                .iter()
                .map(|unit| match unit.strip_suffix("@.service") {
                    Some(base) => format!("{base}@*.service"),
                    None => unit.clone(),
                }),
        );
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
            for unit in units
                .iter()
                .filter(|unit| unit.ends_with(".service") && !is_template(unit))
            {
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

    fn write(&self, report: &Report, units: &[ManagedUnit]) -> Result<(), String> {
        storage::directory(&self.ownership_directory)?;
        for unit in units {
            let runtime_present = path_present(&self.unit_path(&unit.name))?
                || path_present(&self.drop_in(&unit.name))?
                || path_present(&self.uphold(&unit.name))?;
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
            storage::write_atomic(&drop_in, unit.policy.as_bytes())?;
            storage::write_atomic(&self.unit_path(&unit.name), unit.source.as_bytes())?;
            if unit.upheld() {
                self.write_uphold(&unit.name)?;
            }
        }
        Ok(())
    }

    fn discard_written(&self, id: &str, units: &[ManagedUnit]) -> Result<(), String> {
        let mut errors = Vec::new();
        for unit in units {
            if self.owner_matches(id, &unit.name) {
                if let Err(error) = self.remove_uphold(&unit.name) {
                    errors.push(error);
                }
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

    // systemd's native .upholds directory expresses Enabled intent without
    // modifying either the approved unit source or korrid's NixOS fragment.
    fn uphold(&self, unit: &str) -> PathBuf {
        self.unit_directory
            .join("korrid.service.upholds")
            .join(unit)
    }

    fn write_uphold(&self, unit: &str) -> Result<(), String> {
        let path = self.uphold(unit);
        let parent = path.parent().ok_or("invalid uphold path")?;
        match fs::create_dir(parent) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if !fs::symlink_metadata(parent).is_ok_and(|metadata| metadata.is_dir()) {
                    return Err("plugin uphold directory must be a real directory".into());
                }
            }
            Err(error) => return Err(error.to_string()),
        }
        let target = uphold_target(unit);
        if path_present(&path)? {
            if fs::read_link(&path).ok().as_ref() != Some(&target) {
                return Err(format!(
                    "plugin uphold relation for {unit} conflicts with a host entry"
                ));
            }
            return Ok(());
        }
        storage::root_link(&path, &target)
    }

    fn remove_uphold(&self, unit: &str) -> Result<(), String> {
        let path = self.uphold(unit);
        if !path_present(&path)? {
            return Ok(());
        }
        if fs::read_link(&path).ok() != Some(uphold_target(unit)) {
            return Err(format!(
                "plugin uphold relation for {unit} conflicts with a host entry"
            ));
        }
        storage::remove(&path)
    }

    fn refuse_manual_start(&self, unit: &str) -> Result<(), String> {
        const BARRIER: &str = "\n[Unit]\nRefuseManualStart=yes\n";
        let path = self.drop_in(unit);
        let mut policy = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        if !policy.ends_with(BARRIER) {
            policy.push_str(BARRIER);
            storage::write_atomic(&path, policy.as_bytes())?;
        }
        Ok(())
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
        let fragment: String = loaded
            .get_property("FragmentPath")
            .map_err(|e| e.to_string())?;
        if fragment != self.unit_path(unit).to_string_lossy().as_ref() {
            return Err(format!(
                "native unit {unit} is shadowed by a host unit: {fragment}"
            ));
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

fn managed_units(report: &Report) -> Result<Vec<ManagedUnit>, String> {
    let mut names = BTreeSet::new();
    let mut units = report
        .native_units
        .iter()
        .map(|(declared, native)| {
            let name = crate::native_unit::managed_unit_name(declared, native.kind)?;
            if !names.insert(name.clone()) {
                return Err(format!("native unit name is duplicated: {name}"));
            }
            Ok(ManagedUnit {
                name,
                source: native.source.clone(),
                policy: hardening_for(&report.id, native),
                template: false,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    for unit in account_units(report)? {
        if !names.insert(unit.name.clone()) {
            return Err(format!("native unit name is duplicated: {}", unit.name));
        }
        units.push(unit);
    }
    Ok(units)
}

fn is_template(unit: &str) -> bool {
    unit.ends_with("@.service")
}

fn valid_owned_unit_name(unit: &str) -> bool {
    if let Some(base) = unit.strip_suffix("@.service") {
        return crate::declaration::valid_name(base);
    }
    [".service", ".socket"].iter().any(|suffix| {
        unit.strip_suffix(suffix)
            .is_some_and(crate::declaration::valid_name)
    })
}

/// The user and group korrid's service runs as on every product device
/// (`services/korrid/nixos-module.nix`, `serviceUser` and `serviceGroup`).
/// Only it may connect to a plugin's account socket.
pub const KORRID_USER: &str = "korrid";
pub const KORRID_GROUP: &str = "korrid";
/// The longest one account job runs: a QR sign-in waits up to 15 minutes in
/// korrid, and this bounds a program that does not end with its input.
pub const ACCOUNT_RUNTIME_MAX: &str = "20min";
const ACCOUNT_POLICY: &str = "account-policy-v1: host-generated account sign-in socket for korrid only; the declared program as the plugin's first service's user, reading its runtime directory and with its approved credentials; no capabilities; two connections; bounded runtime";

/// ADR 0089: for a plugin that declares `accounts`, a socket that only
/// korrid can connect to, and the service template systemd starts for each
/// connection, with the connection as the program's input and output. The
/// program runs as the plugin's first declared service's user, beside that
/// service (`Requisite=`), in its runtime directory with its approved
/// credentials, and with no capabilities. Every property is the host's; the
/// package supplies only the program.
fn account_units(report: &Report) -> Result<Vec<ManagedUnit>, String> {
    let declaration = serde_json::to_value(&report.declaration).map_err(|e| e.to_string())?;
    let accounts = crate::plugin_references::accounts(&declaration, &report.files)?;
    let Some(account) = accounts.values().next() else {
        return Ok(Vec::new());
    };
    let (declared, service) = report
        .declaration
        .services
        .iter()
        .find_map(|declared| {
            report
                .native_units
                .get(declared)
                .filter(|native| native.kind == crate::native_unit::NativeUnitKind::Service)
                .map(|native| (declared, native))
        })
        .ok_or("accounts require a native service for the account program to run beside")?;
    let service_name = crate::native_unit::managed_unit_name(declared, service.kind)?;
    let identity = match service.user.as_deref() {
        Some("root") => return Err("an account program may not run as root".into()),
        Some(user) => format!("User={user}\nDynamicUser=no\n"),
        // systemd names a dynamic user after its unit when User= is unset, so
        // this shares the running service's uid and its runtime directory.
        None => format!(
            "DynamicUser=yes\nUser={}\n",
            service_name
                .strip_suffix(".service")
                .unwrap_or(&service_name)
        ),
    };
    let program = report.files[&account.program]
        .to_str()
        .ok_or("account program path must be UTF-8")?;
    if !program.starts_with("/nix/store/")
        || program.contains(|c: char| c.is_whitespace() || "%\\\"'$;".contains(c))
    {
        return Err("account program must be a plain store path".into());
    }
    // The service's runtime directory, by path. The account unit does not
    // declare it: a shared `RuntimeDirectory=` with a dynamic user would move
    // the service's folder under /run/private (`RuntimeDirectoryPreserve=`)
    // or remove it when the account program ends. Its user reads it; the
    // sandbox keeps it read-only.
    let (runtime_directory, _) = runtime_directory_policy(&package::unit_name(&report.id), service);
    let runtime_directory = runtime_directory
        .split_whitespace()
        .next()
        .filter(|name| {
            !name.starts_with('.')
                && name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        })
        .ok_or("the service's runtime directory must be one plain name")?;
    let base = crate::plugin_references::account_unit(&report.id);
    let owner = owner_marker(&report.id);
    let policy = format!("{owner}# {ACCOUNT_POLICY}\n");
    let socket = format!(
        "[Unit]\nDescription=Account sign-in for {id}\n\n[Socket]\nListenStream={path}\nSocketUser={KORRID_USER}\nSocketGroup={KORRID_GROUP}\nSocketMode=0600\nAccept=yes\nMaxConnections=2\nRemoveOnStop=yes\n",
        id = report.id,
        path = crate::plugin_references::account_socket(&report.id).display(),
    );
    let mut template = format!(
        "[Unit]\nDescription=Account sign-in for {id}\nRequisite={service_name}\nAfter={service_name}\n\n[Service]\nType=simple\nExecStart={program}\nStandardInput=socket\nStandardOutput=socket\nStandardError=journal\nRuntimeMaxSec={ACCOUNT_RUNTIME_MAX}\nRestart=no\nKillMode=control-group\nTimeoutStopSec=10\n{identity}Environment=RUNTIME_DIRECTORY=/run/{runtime_directory}\nUMask=0077\nNoNewPrivileges=yes\nProtectSystem=strict\nProtectHome=yes\nPrivateTmp=yes\nPrivateDevices=yes\nDevicePolicy=closed\nProtectKernelTunables=yes\nProtectKernelModules=yes\nProtectKernelLogs=yes\nProtectControlGroups=yes\nProtectProc=invisible\nRestrictSUIDSGID=yes\nRestrictRealtime=yes\nRestrictNamespaces=yes\nLockPersonality=yes\nRestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK\nCapabilityBoundingSet=\nAmbientCapabilities=\n",
        id = report.id,
    );
    for credential in &service.credentials {
        template.push_str(&format!("LoadCredential={credential}\n"));
    }
    Ok(vec![
        ManagedUnit {
            name: format!("{base}.socket"),
            source: socket,
            policy: policy.clone(),
            template: false,
        },
        ManagedUnit {
            name: format!("{base}@.service"),
            source: template,
            policy,
            template: true,
        },
    ])
}

fn uphold_target(unit: &str) -> PathBuf {
    PathBuf::from("..").join(unit)
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
        .chain(account_units(report)?.into_iter().map(|unit| {
            format!(
                "# account unit: {}\n{}\n{}",
                unit.name, unit.source, unit.policy
            )
        }))
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
    // The native limit is the explicit permission; leaving the seccomp RT
    // filter enabled would silently nullify that approved request.
    let restrict_realtime = if native
        .privileged_directives
        .iter()
        .any(|directive| directive.starts_with("LimitRTPRIO="))
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
        "{owner}# {}\n[Unit]\nAfter=network.target\n\n[Service]\n{user_policy}DynamicUser={dynamic_user}\nStateDirectory=\nStateDirectory={name}\nStateDirectoryMode=0700\nRuntimeDirectory=\nRuntimeDirectory={runtime_directory}\nRuntimeDirectoryMode={runtime_directory_mode}\nUMask=0077\nRestart=on-failure\nRestartSec=1\nTimeoutStartSec=30\nTimeoutStopSec=30\nKillMode=control-group\nNoNewPrivileges={no_new_privileges}\nProtectSystem=strict\nProtectHome={protect_home}\nPrivateTmp={private_tmp}\nProtectKernelTunables=yes\nProtectKernelModules=yes\nProtectKernelLogs=yes\nProtectControlGroups=yes\nProtectProc=invisible\nRestrictSUIDSGID=yes\nRestrictRealtime={restrict_realtime}\nRestrictNamespaces=yes\nLockPersonality=yes\nRestrictAddressFamilies=\nRestrictAddressFamilies={address_families}\nDevicePolicy={device_policy}\nDeviceAllow=\nCapabilityBoundingSet=\nCapabilityBoundingSet={capabilities}\nAmbientCapabilities=\nAmbientCapabilities={capabilities}\nLoadCredential=\n",
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

#[cfg(test)]
mod tests {
    use super::*;

    fn units(root: &std::path::Path) -> Units {
        Units {
            systemctl: "/bin/false".into(),
            unit_directory: root.join("systemd"),
            ownership_directory: root.join("ownership"),
            firewall: crate::firewall::Firewall {
                ipv4: "/bin/false".into(),
                ipv6: "/bin/false".into(),
            },
        }
    }

    fn report() -> Report {
        let source = "[Service]\nType=exec\nExecStart=/nix/store/00000000000000000000000000000000-coreutils/bin/sleep infinity\n";
        Report {
            publisher: None,
            id: "@korri:sunshine".into(),
            package: "/nix/store/00000000000000000000000000000000-plugin".into(),
            provenance: crate::provenance::Provenance::RawCache {
                cache_url: "file:///cache".into(),
            },
            approval: String::new(),
            policy: package::BASE_POLICY,
            warning: String::new(),
            unit: Some("korri-sunshine.service".into()),
            state_directory: String::new(),
            runtime_directory: String::new(),
            unit_configuration: String::new(),
            declaration: crate::declaration::Declaration::evaluate(
                "@korri",
                "export const name = 'sunshine'; export const services = ['korri-sunshine'];",
            )
            .unwrap(),
            native_units: std::collections::BTreeMap::from([(
                "korri-sunshine".into(),
                crate::native_unit::NativeUnit::parse(source).unwrap(),
            )]),
            ports: Default::default(),
            hosts: Vec::new(),
            credential_host: None,
            credential_token_url: None,
            packages: Default::default(),
            files: Default::default(),
            entry: "plugin.ts".into(),
            sources: vec!["plugin.ts".into()],
            requires: Vec::new(),
            brings: Vec::new(),
        }
    }

    #[test]
    fn managed_unit_write_restores_its_uphold_relation_and_discard_withdraws_it() {
        let root = tempfile::tempdir().unwrap();
        let units = units(root.path());
        fs::create_dir(&units.unit_directory).unwrap();
        let name = "korri-sunshine.service";
        let report = report();
        let managed = managed_units(&report).unwrap();
        // An owned runtime unit with a lost relation must be repaired on
        // restore, without depending on the machine's systemd or D-Bus.
        fs::write(units.unit_path(name), &managed[0].source).unwrap();
        storage::directory(units.drop_in(name).parent().unwrap()).unwrap();
        fs::write(units.drop_in(name), owner_marker(&report.id)).unwrap();
        units.write(&report, &managed).unwrap();
        assert_eq!(
            fs::read_link(units.uphold(name)).unwrap(),
            PathBuf::from("../korri-sunshine.service")
        );
        let enabled_policy = fs::read_to_string(units.drop_in(name)).unwrap();
        units.refuse_manual_start(name).unwrap();
        let stopping_policy = fs::read_to_string(units.drop_in(name)).unwrap();
        assert_eq!(
            stopping_policy,
            format!("{enabled_policy}\n[Unit]\nRefuseManualStart=yes\n")
        );
        assert!(units.owner_matches(&report.id, name));
        units.refuse_manual_start(name).unwrap();
        assert_eq!(
            fs::read_to_string(units.drop_in(name)).unwrap(),
            stopping_policy
        );
        // Enabled staging clears the transient stop barrier and does not
        // create a second relation or replace a peer's file.
        units.write(&report, &managed).unwrap();
        assert_eq!(
            fs::read_to_string(units.drop_in(name)).unwrap(),
            enabled_policy
        );
        units.discard_written(&report.id, &managed).unwrap();
        assert!(!path_present(&units.uphold(name)).unwrap());
        units.remove_uphold(name).unwrap();
    }

    const PROGRAM: &str =
        "/nix/store/00000000000000000000000000000000-tailscale-account/bin/korri-tailscale-account";

    /// The Tailscale plugin's shape: one service with the one admitted
    /// credential, and one account.
    fn account_report(service: &str) -> Report {
        let mut report = report();
        report.id = "@korri:tailscale".into();
        report.declaration = crate::declaration::Declaration::evaluate(
            "@korri",
            "export const name = 'tailscale'; export const services = ['tailscaled']; \
             export const accounts = { tailscale: { title: 'Tailscale', program: 'account', offer: 'Private network: reach your devices from anywhere' } };",
        )
        .unwrap();
        report.native_units = std::collections::BTreeMap::from([(
            "tailscaled".into(),
            crate::native_unit::NativeUnit::parse(service).unwrap(),
        )]);
        report.files = std::collections::BTreeMap::from([("account".into(), PROGRAM.into())]);
        report
    }

    const TAILSCALED: &str = "[Service]\nType=notify\nExecStart=/nix/store/00000000000000000000000000000000-tailscale/bin/tailscaled\nLoadCredential=authkey:tailscale-authkey\n";

    #[test]
    fn an_account_gets_a_korrid_only_socket_and_a_template_with_the_services_identity() {
        let report = account_report(TAILSCALED);
        let managed = managed_units(&report).unwrap();
        let base = crate::plugin_references::account_unit("@korri:tailscale");
        assert_eq!(
            managed.iter().map(|u| u.name.clone()).collect::<Vec<_>>(),
            [
                "tailscaled.service".to_owned(),
                format!("{base}.socket"),
                format!("{base}@.service")
            ]
        );
        let (socket, template) = (&managed[1], &managed[2]);
        assert!(socket.upheld() && !template.upheld());
        let lines = |source: &str| source.lines().map(str::to_owned).collect::<Vec<_>>();
        let socket_lines = lines(&socket.source);
        for expected in [
            format!("ListenStream=/run/{base}.sock"),
            "SocketUser=korrid".into(),
            "SocketGroup=korrid".into(),
            "SocketMode=0600".into(),
            "Accept=yes".into(),
            "MaxConnections=2".into(),
        ] {
            assert!(
                socket_lines.contains(&expected),
                "{expected} in {socket_lines:?}"
            );
        }
        let template_lines = lines(&template.source);
        let runtime = package::unit_name("@korri:tailscale");
        for expected in [
            "Requisite=tailscaled.service".to_owned(),
            format!("ExecStart={PROGRAM}"),
            "StandardInput=socket".into(),
            "StandardOutput=socket".into(),
            "RuntimeMaxSec=20min".into(),
            "DynamicUser=yes".into(),
            "User=tailscaled".into(),
            format!("Environment=RUNTIME_DIRECTORY=/run/{runtime}"),
            "NoNewPrivileges=yes".into(),
            "CapabilityBoundingSet=".into(),
            "AmbientCapabilities=".into(),
            "LoadCredential=authkey:tailscale-authkey".into(),
        ] {
            assert!(
                template_lines.contains(&expected),
                "{expected} in {template_lines:?}"
            );
        }
        // No capability is granted anywhere, and nothing else names a user.
        // It never declares the service's runtime directory as its own.
        assert!(!template_lines
            .iter()
            .any(|l| l.starts_with("RuntimeDirectory")));
        for prefix in ["CapabilityBoundingSet=", "AmbientCapabilities=", "User="] {
            assert_eq!(
                template_lines
                    .iter()
                    .filter(|l| l.starts_with(prefix))
                    .count(),
                1,
                "{prefix}"
            );
        }
        for unit in [socket, template] {
            assert!(unit.policy.starts_with(&owner_marker("@korri:tailscale")));
        }
        // Approval binds the generated units: they are in the rendered set.
        let rendered = render(&report).unwrap();
        assert!(rendered.contains(&format!("# account unit: {base}@.service")));
        assert!(rendered.contains("RuntimeMaxSec=20min"));
        assert!(valid_owned_unit_name(&template.name));
        assert!(valid_owned_unit_name(&socket.name));
        assert!(!valid_owned_unit_name("x@y.service"));
        assert_eq!(base.len(), "korri-account-".len() + 32);
    }

    #[test]
    fn an_account_needs_an_unprivileged_service_and_a_plugin_without_one_gets_no_units() {
        assert_eq!(
            managed_units(&account_report(
                "[Service]\nType=exec\nUser=root\nExecStart=/nix/store/00000000000000000000000000000000-x/bin/x\n"
            ))
            .err()
            .as_deref(),
            Some("an account program may not run as root")
        );
        let mut none = account_report(TAILSCALED);
        none.native_units.clear();
        assert_eq!(
            managed_units(&none).err().as_deref(),
            Some("accounts require a native service for the account program to run beside")
        );
        assert_eq!(managed_units(&report()).unwrap().len(), 1);
    }

    /// The generated units are written, owned and removed with the plugin's
    /// own; the template has no uphold relation.
    #[test]
    fn account_units_are_written_owned_and_removed_with_the_plugin() {
        let root = tempfile::tempdir().unwrap();
        let mut units = units(root.path());
        // `write` asks systemctl whether each unit already exists: no.
        let systemctl = root.path().join("systemctl");
        fs::write(&systemctl, "#!/bin/sh\nexit 1\n").unwrap();
        fs::set_permissions(
            &systemctl,
            <fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o755),
        )
        .unwrap();
        units.systemctl = systemctl;
        fs::create_dir(&units.unit_directory).unwrap();
        let report = account_report(TAILSCALED);
        let managed = managed_units(&report).unwrap();
        units.write(&report, &managed).unwrap();
        assert_eq!(
            units.read_owned_units(&report.id).unwrap(),
            managed.iter().map(|u| u.name.clone()).collect::<Vec<_>>()
        );
        for unit in &managed {
            assert_eq!(
                fs::read_to_string(units.unit_path(&unit.name)).unwrap(),
                unit.source
            );
            assert!(units.owner_matches(&report.id, &unit.name));
            assert_eq!(
                path_present(&units.uphold(&unit.name)).unwrap(),
                unit.upheld()
            );
        }
        units.discard_written(&report.id, &managed).unwrap();
        for unit in &managed {
            assert!(!path_present(&units.unit_path(&unit.name)).unwrap());
            assert!(!path_present(&units.drop_in(&unit.name)).unwrap());
            assert!(!path_present(&units.uphold(&unit.name)).unwrap());
        }
        assert!(units.read_owned_units(&report.id).unwrap().is_empty());
    }

    #[test]
    fn lifecycle_relation_does_not_replace_an_unowned_link_or_file() {
        let root = tempfile::tempdir().unwrap();
        let units = units(root.path());
        fs::create_dir(&units.unit_directory).unwrap();
        fs::create_dir(units.uphold("daemon.service").parent().unwrap()).unwrap();
        fs::write(units.uphold("daemon.service"), "host-owned").unwrap();
        assert!(units.write_uphold("daemon.service").is_err());
        assert!(units.remove_uphold("daemon.service").is_err());
        assert_eq!(
            fs::read_to_string(units.uphold("daemon.service")).unwrap(),
            "host-owned"
        );
    }
}
