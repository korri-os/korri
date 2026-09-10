//! The unit is the permission request. Parse systemd.syntax(7), then accept
//! only the service features exercised by the packaged daemon. Never sanitize
//! unknown directives: systemd must see exactly the bytes that were approved.
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize)]
pub struct NativeUnit {
    pub source: String,
    pub capabilities: Vec<String>,
    pub devices: Vec<String>,
    pub credentials: Vec<String>,
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
            capabilities: vec![],
            devices: vec![],
            credentials: vec![],
            executables: vec![],
        };
        let mut section = "".to_owned();
        let mut logical = String::new();
        let mut start = None;
        let mut cleanup = Vec::new();
        let mut service_type = None;
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
                if !matches!(line, "[Unit]" | "[Service]") {
                    return Err(format!("unsupported unit section: {line}"));
                }
                section = line.to_owned();
            } else {
                let (key, value) = line
                    .split_once('=')
                    .ok_or_else(|| format!("invalid unit assignment: {line}"))?;
                let key = key.trim();
                let value = value.trim();
                let invalid = || format!("unsupported {key} value: {value}");
                match (section.as_str(), key) {
                    ("[Unit]", "Description") if !value.contains('%') => {}
                    ("[Service]", "Type") if matches!(value, "notify" | "exec") => {
                        service_type = Some(value.to_owned())
                    }
                    ("[Service]", "ExecStart") => {
                        if value.is_empty() {
                            start = None;
                        } else if start.is_some() {
                            return Err("ExecStart must name exactly one command".into());
                        } else {
                            start = Some(command(value).map_err(|e| format!("ExecStart: {e}"))?);
                        }
                    }
                    ("[Service]", "ExecStopPost") => {
                        if value.is_empty() {
                            cleanup.clear();
                        } else {
                            cleanup.push(command(value).map_err(|e| format!("ExecStopPost: {e}"))?);
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
                            if !matches!(capability, "CAP_NET_ADMIN" | "CAP_NET_RAW") {
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
                        } else if value.split_ascii_whitespace().collect::<Vec<_>>()
                            == ["/dev/net/tun", "rw"]
                        {
                            if unit.devices.is_empty() {
                                unit.devices.push("/dev/net/tun rw".into());
                            }
                        } else {
                            return Err(invalid());
                        }
                    }
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
        if service_type.is_none() {
            return Err("Type must be exec or notify".into());
        }
        unit.executables.push(start.ok_or("ExecStart is required")?);
        unit.executables.extend(cleanup);
        Ok(unit)
    }
}

fn command(value: &str) -> Result<String, String> {
    let args = words(value)?;
    if args.is_empty() || args.len() > 32 {
        return Err("command must contain 1 to 32 arguments".into());
    }
    immutable_path(&args[0])?;
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
    Ok(args[0].clone())
}

pub fn immutable_path(value: &str) -> Result<&std::path::Path, String> {
    let path = std::path::Path::new(value);
    let rest = value
        .strip_prefix("/nix/store/")
        .ok_or("command and artifact paths must be immutable store paths")?;
    let (output, inside) = rest
        .split_once('/')
        .ok_or("store artifact must name a file")?;
    crate::package::validate_store_path(std::path::Path::new(&format!("/nix/store/{output}")))?;
    if inside
        .split('/')
        .any(|s| s.is_empty() || s == "." || s == "..")
    {
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

pub fn validate_names(names: &[String]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for name in names {
        if !crate::declaration::valid_name(name) || !seen.insert(name) {
            return Err(format!("invalid or repeated service name: {name}"));
        }
    }
    Ok(())
}
