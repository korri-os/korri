//! The unit is the permission request. Parse systemd.syntax(7), then accept
//! only the service features exercised by the packaged daemon. Never sanitize
//! unknown directives: systemd must see exactly the bytes that were approved.
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DevicePolicy {
    Auto,
    Closed,
}

impl DevicePolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Closed => "closed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NativeUnitKind {
    Service,
    Socket,
}

impl NativeUnitKind {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Service => ".service",
            Self::Socket => ".socket",
        }
    }
}

pub fn managed_unit_name(declared: &str, kind: NativeUnitKind) -> Result<String, String> {
    for candidate in [NativeUnitKind::Service, NativeUnitKind::Socket] {
        if declared.ends_with(candidate.suffix()) {
            if kind != candidate {
                return Err(format!(
                    "native unit name {declared} does not match its {} implementation",
                    kind.suffix()
                ));
            }
            let base = declared
                .strip_suffix(candidate.suffix())
                .unwrap_or_default();
            if crate::declaration::valid_name(base) {
                return Ok(declared.into());
            }
            return Err(format!("invalid native unit name: {declared}"));
        }
    }
    if !crate::declaration::valid_name(declared) {
        return Err(format!("invalid native unit name: {declared}"));
    }
    Ok(format!("{declared}{}", kind.suffix()))
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeUnit {
    pub source: String,
    pub kind: NativeUnitKind,
    pub user: Option<String>,
    pub capabilities: Vec<String>,
    pub devices: Vec<String>,
    pub credentials: Vec<String>,
    pub address_families: Option<Vec<String>>,
    pub runtime_directory: Option<String>,
    pub runtime_directory_mode: Option<String>,
    pub device_policy: Option<DevicePolicy>,
    pub privileged_directives: Vec<String>,
    pub executables: Vec<String>,
}

impl NativeUnit {
    pub fn parse(source: &str) -> Result<Self, String> {
        if source.len() > 128 * 1024
            || source
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        {
            return Err("unit contains unsupported control characters or exceeds 128 KiB".into());
        }
        // systemd strips ASCII whitespace, not Rust's Unicode White_Space.
        // Reject the unsupported language even in comments/values so it cannot
        // turn an ignored systemd assignment into an authority reset here.
        if source.chars().any(|c| c.is_whitespace() && !c.is_ascii()) {
            return Err("unit contains unsupported non-ASCII whitespace".into());
        }
        // systemd treats bare CR as a line boundary; str::lines does not.
        // Permit CRLF only, before skipping comments or parsing any values.
        if source
            .as_bytes()
            .iter()
            .enumerate()
            .any(|(i, &byte)| byte == b'\r' && source.as_bytes().get(i + 1) != Some(&b'\n'))
        {
            return Err("unit contains a bare carriage return; use LF or CRLF".into());
        }
        let mut unit = Self {
            source: source.into(),
            kind: NativeUnitKind::Service,
            user: None,
            capabilities: vec![],
            devices: vec![],
            credentials: vec![],
            address_families: None,
            runtime_directory: None,
            runtime_directory_mode: None,
            device_policy: None,
            privileged_directives: vec![],
            executables: vec![],
        };
        let mut section = "".to_owned();
        let mut logical = String::new();
        let mut start = None;
        let mut prepare = Vec::new();
        let mut cleanup = Vec::new();
        let mut service_type = None;
        let mut saw_service = false;
        let mut saw_socket = false;
        let mut socket_listener = false;
        let mut socket_service = false;
        for line in source.lines() {
            let line = line.trim();
            if line.is_empty() {
                if !logical.is_empty() {
                    return Err("unsupported blank line inside unit continuation".into());
                }
                continue;
            }
            if line.starts_with(['#', ';']) {
                continue;
            }
            if let Some(part) = line.strip_suffix('\\') {
                logical.push_str(part);
                logical.push(' ');
                continue;
            }
            logical.push_str(line);
            let line = logical.trim();
            if line.starts_with('[') {
                if !matches!(line, "[Unit]" | "[Service]" | "[Socket]" | "[Install]") {
                    return Err(format!("unsupported unit section: {line}"));
                }
                section = line.to_owned();
                saw_service |= line == "[Service]";
                saw_socket |= line == "[Socket]";
            } else {
                let (key, value) = line
                    .split_once('=')
                    .ok_or_else(|| format!("invalid unit assignment: {line}"))?;
                let key = key.trim();
                let value = value.trim();
                let invalid = || format!("unsupported {key} value: {value}");
                match (section.as_str(), key) {
                    ("[Unit]", "Description") if !value.contains('%') => {}
                    ("[Unit]", "Before" | "After" | "Wants" | "Requires" | "BindsTo")
                        if valid_unit_list(value) =>
                    {
                        unit.privileged_directives.push(format!("{key}={value}"));
                    }
                    ("[Service]", "Type") if matches!(value, "notify" | "exec" | "simple") => {
                        saw_service = true;
                        service_type = Some(value.to_owned())
                    }
                    ("[Service]", "ExecStart") => {
                        if value.is_empty() {
                            start = None;
                        } else if start.is_some() {
                            return Err("ExecStart must name exactly one command".into());
                        } else {
                            let executable =
                                command(value, false).map_err(|e| format!("ExecStart: {e}"))?;
                            if is_host_wrapper(&executable) {
                                unit.privileged_directives
                                    .push(format!("ExecStart={value}"));
                            }
                            start = Some(executable);
                        }
                    }
                    ("[Service]", "User")
                        if value.is_empty() || crate::declaration::valid_name(value) =>
                    {
                        unit.privileged_directives
                            .retain(|directive| !directive.starts_with("User="));
                        unit.user = (!value.is_empty()).then(|| value.to_owned());
                        if !value.is_empty() {
                            unit.privileged_directives.push(format!("User={value}"));
                        }
                    }
                    ("[Service]", "ExecStartPre") => {
                        if value.is_empty() {
                            prepare.clear();
                        } else {
                            prepare.push(
                                command(value, true).map_err(|e| format!("ExecStartPre: {e}"))?,
                            );
                            if has_privileged_prefix(value) {
                                unit.privileged_directives
                                    .push(format!("ExecStartPre={value}"));
                            }
                        }
                    }
                    ("[Service]", "ExecStopPost") => {
                        if value.is_empty() {
                            cleanup.clear();
                            unit.privileged_directives
                                .retain(|directive| !directive.starts_with("ExecStopPost="));
                        } else {
                            cleanup.push(
                                command(value, true).map_err(|e| format!("ExecStopPost: {e}"))?,
                            );
                            if has_privileged_prefix(value) {
                                unit.privileged_directives
                                    .push(format!("ExecStopPost={value}"));
                            }
                        }
                    }
                    ("[Service]", "Group") if crate::declaration::valid_name(value) => {
                        unit.privileged_directives.push(format!("{key}={value}"));
                    }
                    ("[Service]", "SupplementaryGroups") if valid_name_list(value) => {
                        unit.privileged_directives.push(format!("{key}={value}"));
                    }
                    ("[Service]", "WorkingDirectory") if valid_absolute_path(value) => {
                        unit.privileged_directives.push(format!("{key}={value}"));
                    }
                    ("[Service]", "Environment") if valid_environment(value) => {
                        unit.privileged_directives.push(format!("{key}={value}"));
                    }
                    ("[Service]", "Sockets") if valid_unit_list_for(value, ".socket") => {
                        unit.privileged_directives.push(format!("{key}={value}"));
                    }
                    ("[Service]", "ExecCondition") => {
                        let executable =
                            command(value, true).map_err(|e| format!("ExecCondition: {e}"))?;
                        prepare.push(executable);
                        unit.privileged_directives.push(format!("{key}={value}"));
                    }
                    ("[Service]", "ReadWritePaths" | "InaccessiblePaths")
                        if valid_path_list(value) =>
                    {
                        unit.privileged_directives.push(format!("{key}={value}"));
                    }
                    ("[Service]", "RuntimeDirectory") if value == "korri-input-seat" => {
                        if unit.runtime_directory.is_some() {
                            return Err("RuntimeDirectory must name exactly one directory".into());
                        }
                        unit.runtime_directory = Some(value.into());
                        unit.privileged_directives.push(format!("{key}={value}"));
                    }
                    ("[Service]", "RuntimeDirectoryMode") if valid_mode(value) => {
                        unit.runtime_directory_mode = Some(value.into());
                        unit.privileged_directives
                            .retain(|directive| !directive.starts_with("RuntimeDirectoryMode="));
                        unit.privileged_directives.push(format!("{key}={value}"));
                    }
                    ("[Service]", "UMask") if valid_mode(value) => {
                        unit.privileged_directives.push(format!("{key}={value}"));
                    }
                    ("[Service]", "Restart") if value == "on-failure" => {}
                    ("[Service]", "RestartSec") if valid_unsigned(value) => {}
                    ("[Service]", "RestrictAddressFamilies") if valid_address_families(value) => {
                        let families = unit.address_families.get_or_insert_with(Vec::new);
                        for family in value.split_ascii_whitespace() {
                            if !families.iter().any(|current| current == family) {
                                families.push(family.into());
                            }
                        }
                        unit.privileged_directives.push(format!("{key}={value}"));
                    }
                    ("[Service]", "DevicePolicy") if matches!(value, "auto" | "closed") => {
                        unit.device_policy = Some(if value == "auto" {
                            DevicePolicy::Auto
                        } else {
                            DevicePolicy::Closed
                        });
                        unit.privileged_directives
                            .retain(|directive| !directive.starts_with("DevicePolicy="));
                        unit.privileged_directives.push(format!("{key}={value}"));
                    }
                    ("[Service]", "ProtectSystem") if value == "strict" => {}
                    ("[Service]", "ProtectProc") if value == "invisible" => {}
                    ("[Service]", "ProcSubset") if value == "pid" => {}
                    ("[Service]", "SystemCallArchitectures") if value == "native" => {}
                    (
                        "[Service]",
                        "LockPersonality"
                        | "MemoryDenyWriteExecute"
                        | "ProtectClock"
                        | "ProtectControlGroups"
                        | "ProtectHostname"
                        | "ProtectKernelLogs"
                        | "ProtectKernelModules"
                        | "ProtectKernelTunables"
                        | "RestrictSUIDSGID",
                    ) if matches!(value, "true" | "false") => {
                        if value == "false" {
                            unit.privileged_directives.push(format!("{key}={value}"));
                        }
                    }
                    ("[Service]", "PrivateTmp") if matches!(value, "true" | "false") => {
                        unit.privileged_directives
                            .retain(|directive| !directive.starts_with("PrivateTmp="));
                        if value == "false" {
                            unit.privileged_directives.push(format!("{key}={value}"));
                        }
                    }
                    ("[Service]", "ProtectHome")
                        if matches!(value, "true" | "false" | "yes" | "no" | "read-only") =>
                    {
                        unit.privileged_directives
                            .retain(|directive| !directive.starts_with("ProtectHome="));
                        if matches!(value, "false" | "no" | "read-only") {
                            unit.privileged_directives.push(format!("{key}={value}"));
                        }
                    }
                    ("[Service]", "CapabilityBoundingSet") => {
                        // Permission directives use their literal native values.
                        // Do not apply ExecStart's C-unescaping rules to a
                        // different directive's parser and widen its request.
                        let words: Vec<_> = value.split_ascii_whitespace().collect();
                        if words.is_empty() {
                            unit.capabilities.clear();
                        }
                        for capability in words {
                            if !matches!(
                                capability,
                                "CAP_CHOWN"
                                    | "CAP_NET_ADMIN"
                                    | "CAP_NET_RAW"
                                    | "CAP_SETPCAP"
                                    | "CAP_SYS_ADMIN"
                            ) {
                                return Err(invalid());
                            }
                            if !unit.capabilities.iter().any(|c| c == capability) {
                                unit.capabilities.push(capability.into());
                            }
                        }
                    }
                    ("[Service]", "DeviceAllow") => {
                        if value.is_empty() {
                            unit.devices.clear();
                        } else {
                            let request = value.split_ascii_whitespace().collect::<Vec<_>>();
                            if request.len() != 2 || request[1] != "rw" || !valid_device(request[0])
                            {
                                return Err(invalid());
                            }
                            if !unit.devices.iter().any(|device| device == value) {
                                unit.devices.push(value.into());
                            }
                        }
                    }
                    ("[Service]", "NoNewPrivileges" | "PrivatePIDs" | "PrivateDevices")
                        if matches!(value, "true" | "false") =>
                    {
                        let request = format!("{key}={value}");
                        unit.privileged_directives
                            .retain(|directive| !directive.starts_with(&format!("{key}=")));
                        if value == "false" {
                            unit.privileged_directives.push(request);
                        }
                    }
                    ("[Service]", "AmbientCapabilities") if value.is_empty() => {}
                    ("[Socket]", "Accept" | "RemoveOnStop" | "NonBlocking")
                        if matches!(value, "true" | "false") =>
                    {
                        saw_socket = true;
                        unit.privileged_directives.push(format!("{key}={value}"));
                    }
                    ("[Socket]", "ListenSequentialPacket") if valid_runtime_path(value) => {
                        saw_socket = true;
                        socket_listener = true;
                        unit.privileged_directives.push(format!("{key}={value}"));
                    }
                    ("[Socket]", "FileDescriptorName") if crate::declaration::valid_name(value) => {
                        saw_socket = true;
                        unit.privileged_directives.push(format!("{key}={value}"));
                    }
                    ("[Socket]", "SocketUser") if value == "root" => {
                        saw_socket = true;
                        unit.privileged_directives.push(format!("{key}={value}"));
                    }
                    ("[Socket]", "SocketGroup") if crate::declaration::valid_name(value) => {
                        saw_socket = true;
                        unit.privileged_directives.push(format!("{key}={value}"));
                    }
                    ("[Socket]", "SocketMode" | "DirectoryMode") if valid_mode(value) => {
                        saw_socket = true;
                        unit.privileged_directives.push(format!("{key}={value}"));
                    }
                    ("[Socket]", "Service") if valid_unit_name(value, ".service") => {
                        saw_socket = true;
                        socket_service = true;
                        unit.privileged_directives.push(format!("{key}={value}"));
                    }
                    ("[Install]", "WantedBy") if valid_unit_list(value) => {}
                    ("[Service]", "LoadCredential") => {
                        if value.is_empty() {
                            unit.credentials.clear();
                        } else if value == "authkey:tailscale-authkey" {
                            // systemd owns named-credential lookup (inherited
                            // credentials and credstore). The host reads no secret
                            // and does not turn credential delivery into login.
                            unit.credentials = vec!["authkey:tailscale-authkey".into()];
                        } else {
                            return Err(invalid());
                        }
                    }
                    _ => {
                        return Err(format!(
                            "unsupported unit directive {section} {key}: {value}"
                        ))
                    }
                }
            }
            logical.clear();
        }
        if !logical.is_empty() {
            return Err("unterminated unit continuation".into());
        }
        if unit.runtime_directory_mode.is_some() && unit.runtime_directory.is_none() {
            return Err("RuntimeDirectoryMode requires an approved RuntimeDirectory".into());
        }
        if saw_service && saw_socket {
            return Err("one native unit cannot mix [Service] and [Socket]".into());
        }
        if saw_socket {
            if !socket_listener || !socket_service {
                return Err("socket unit requires ListenSequentialPacket and Service".into());
            }
            unit.kind = NativeUnitKind::Socket;
        } else {
            if service_type.is_none() {
                return Err("Type must be simple, exec or notify".into());
            }
            unit.executables.push(start.ok_or("ExecStart is required")?);
            unit.executables.extend(prepare);
            unit.executables.extend(cleanup);
        }
        Ok(unit)
    }

    pub fn effective_device_policy(&self) -> DevicePolicy {
        self.device_policy.unwrap_or_else(|| {
            if self.devices.is_empty()
                && self
                    .privileged_directives
                    .iter()
                    .any(|directive| directive == "PrivateDevices=false")
            {
                DevicePolicy::Auto
            } else {
                DevicePolicy::Closed
            }
        })
    }
}

fn valid_device(value: &str) -> bool {
    matches!(value, "/dev/net/tun" | "/dev/uinput" | "/dev/uhid")
        || value.strip_prefix("/dev/dri/card").is_some_and(|minor| {
            !minor.is_empty() && minor.bytes().all(|byte| byte.is_ascii_digit())
        })
        || value.strip_prefix("/dev/dri/renderD").is_some_and(|minor| {
            !minor.is_empty() && minor.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn valid_name_list(value: &str) -> bool {
    let names = value.split_ascii_whitespace().collect::<Vec<_>>();
    !names.is_empty()
        && names
            .iter()
            .all(|name| crate::declaration::valid_name(name))
}

fn valid_unit_list_for(value: &str, suffix: &str) -> bool {
    let names = value.split_ascii_whitespace().collect::<Vec<_>>();
    !names.is_empty() && names.iter().all(|name| valid_unit_name(name, suffix))
}

fn valid_unit_list(value: &str) -> bool {
    let names = value.split_ascii_whitespace().collect::<Vec<_>>();
    !names.is_empty()
        && names.iter().all(|name| {
            [".service", ".socket", ".target"]
                .iter()
                .any(|suffix| valid_unit_name(name, suffix))
        })
}

fn valid_unit_name(value: &str, suffix: &str) -> bool {
    value
        .strip_suffix(suffix)
        .is_some_and(crate::declaration::valid_name)
}

fn valid_environment(value: &str) -> bool {
    words(value).is_ok_and(|assignments| {
        !assignments.is_empty()
            && assignments.iter().all(|assignment| {
                let Some((name, value)) = assignment.split_once('=') else {
                    return false;
                };
                !name.is_empty()
                    && !name.as_bytes()[0].is_ascii_digit()
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                    && !value.contains(['%', '$'])
                    && !value.chars().any(char::is_control)
            })
    })
}

fn valid_path_list(value: &str) -> bool {
    words(value)
        .is_ok_and(|paths| !paths.is_empty() && paths.iter().all(|path| valid_absolute_path(path)))
}

fn valid_absolute_path(value: &str) -> bool {
    value.starts_with('/')
        && value.len() <= 4096
        && !value.contains(['%', '$'])
        && !value.chars().any(char::is_control)
        && !value
            .trim_start_matches('/')
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
}

fn valid_runtime_path(value: &str) -> bool {
    value.starts_with("/run/")
        && value.len() <= 4096
        && !value.contains(['%', '$'])
        && !value.chars().any(char::is_control)
        && !value
            .trim_start_matches('/')
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
}

fn valid_unsigned(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn valid_address_families(value: &str) -> bool {
    let families = value.split_ascii_whitespace().collect::<Vec<_>>();
    !families.is_empty()
        && families
            .iter()
            .all(|family| matches!(*family, "AF_UNIX" | "AF_INET" | "AF_INET6" | "AF_NETLINK"))
}

fn valid_mode(value: &str) -> bool {
    value.len() == 4
        && value.starts_with('0')
        && value.bytes().all(|byte| matches!(byte, b'0'..=b'7'))
}

fn has_privileged_prefix(value: &str) -> bool {
    words(value).is_ok_and(|args| args.first().is_some_and(|program| program.starts_with('+')))
}

fn command(value: &str, allow_privileged: bool) -> Result<String, String> {
    let args = words(value)?;
    if args.is_empty() || args.len() > 32 {
        return Err("command must contain 1 to 32 arguments".into());
    }
    let program = if allow_privileged {
        args[0].strip_prefix('+').unwrap_or(&args[0])
    } else {
        &args[0]
    };
    if args[0].starts_with('+') && !allow_privileged {
        return Err("privileged execution prefix is not admitted here".into());
    }
    if !is_host_wrapper(program) {
        immutable_path(program)?;
    }
    for arg in &args {
        let rest = arg
            .replace("${STATE_DIRECTORY}", "")
            .replace("${RUNTIME_DIRECTORY}", "");
        if arg.len() > 4096
            || arg.contains('%')
            || rest.contains('$')
            || arg == ";"
            || arg.chars().any(char::is_control)
        {
            return Err("unsupported command argument or expansion".into());
        }
    }
    Ok(program.to_owned())
}

pub fn is_host_wrapper(value: &str) -> bool {
    value
        .strip_prefix("/run/wrappers/bin/")
        .is_some_and(crate::declaration::valid_name)
}

pub fn immutable_path(value: &str) -> Result<&std::path::Path, String> {
    let path = std::path::Path::new(value);
    let rest = value
        .strip_prefix("/nix/store/")
        .ok_or("command and artifact paths must be immutable store paths")?;
    let (output, inside) = rest
        .split_once('/')
        .map_or((rest, None), |(output, inside)| (output, Some(inside)));
    crate::package::validate_store_path(std::path::Path::new(&format!("/nix/store/{output}")))?;
    if inside.is_some_and(|inside| {
        inside
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    }) {
        return Err("store artifact has path traversal".into());
    }
    Ok(path)
}

// systemd.syntax(7) quoting and C escapes, not shell parsing. Restrict the
// accepted language rather than disagreeing with systemd on malformed input.
fn words(value: &str) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    let mut chars = value.chars().peekable();
    while chars.peek().is_some() {
        while chars.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }
        let quote = chars.peek().copied().filter(|c| matches!(c, '\'' | '"'));
        if quote.is_some() {
            chars.next();
        }
        let mut word = String::new();
        let mut closed = quote.is_none();
        while let Some(c) = chars.next() {
            if Some(c) == quote {
                closed = true;
                if chars.peek().is_some_and(|c| !c.is_ascii_whitespace()) {
                    return Err("text after closing quote".into());
                }
                break;
            }
            if quote.is_none() && c.is_ascii_whitespace() {
                break;
            }
            if c == '\\' {
                let c = chars.next().ok_or("unterminated escape")?;
                word.push(match c {
                    '\\' | '"' | '\'' => c,
                    's' => ' ',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    'a' => '\x07',
                    'b' => '\x08',
                    'f' => '\x0c',
                    'v' => '\x0b',
                    'x' | 'u' | 'U' => {
                        let length = match c {
                            'x' => 2,
                            'u' => 4,
                            _ => 8,
                        };
                        let hex: String = chars.by_ref().take(length).collect();
                        if hex.len() != length {
                            return Err("short escape".into());
                        }
                        char::from_u32(u32::from_str_radix(&hex, 16).map_err(|_| "invalid escape")?)
                            .ok_or("invalid character")?
                    }
                    '0'..='7' => {
                        let octal: String =
                            std::iter::once(c).chain(chars.by_ref().take(2)).collect();
                        if octal.len() != 3 {
                            return Err("short octal escape".into());
                        }
                        char::from_u32(
                            u32::from_str_radix(&octal, 8).map_err(|_| "invalid octal escape")?,
                        )
                        .ok_or("invalid character")?
                    }
                    _ => return Err("unsupported escape".into()),
                });
            } else if quote.is_none() && matches!(c, '\'' | '"') {
                return Err("quotes must surround a complete word".into());
            } else {
                word.push(c);
            }
        }
        if !closed {
            return Err("unterminated quote".into());
        }
        result.push(word);
    }
    Ok(result)
}

pub fn validate_unit_selection(requested: &[String], packaged: &[String]) -> Result<(), String> {
    let requested = requested.iter().collect::<BTreeSet<_>>();
    let packaged = packaged.iter().collect::<BTreeSet<_>>();
    if let Some(name) = requested.difference(&packaged).next() {
        return Err(format!("requested unit {name} has no packaged native unit"));
    }
    if let Some(name) = packaged.difference(&requested).next() {
        return Err(format!(
            "packaged unit {name} was not requested by the plugin declaration"
        ));
    }
    Ok(())
}

pub fn validate_names(names: &[String]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for name in names {
        if !crate::declaration::valid_name(name) || !seen.insert(name) {
            return Err(format!("invalid or repeated service name: {name}"));
        }
    }
    Ok(())
}
