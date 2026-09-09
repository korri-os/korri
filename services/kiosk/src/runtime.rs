use crate::Error;
use serde_json::{Value, json};
use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub(crate) const MAX_CONFIG: usize = 4096;

// Producer: services/korrid/src/main.rs::publish_browser_runtime.
// Consumer: clients/portal/src/runtime-config.ts. surfaceId is a launch-time
// override, not a new field written to korrid's file.
pub(crate) fn load(path: &Path, surface: Option<&str>) -> Result<Value, Error> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(Error::Configuration);
    }
    let mut bytes = Vec::new();
    file.take((MAX_CONFIG + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_CONFIG {
        return Err(Error::Configuration);
    }
    let value = serde_json::from_slice(&bytes).map_err(|_| Error::Configuration)?;
    validate(value, surface)
}

pub(crate) fn validate(value: Value, surface: Option<&str>) -> Result<Value, Error> {
    let port = value["korridPort"]
        .as_u64()
        .filter(|n| (1..=65535).contains(n))
        .ok_or(Error::Configuration)?;
    let capability = value["korridCapability"]
        .as_str()
        .filter(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .ok_or(Error::Configuration)?;
    if surface.is_some_and(|s| s.is_empty() || s.len() > MAX_CONFIG) {
        return Err(Error::Configuration);
    }
    let mut result = json!({"korridPort": port, "korridCapability": capability});
    if let Some(surface) = surface {
        result["surfaceId"] = json!(surface);
    }
    Ok(result)
}

// CDP Fetch.fulfillRequest specifies base64, not a JSON string body.
pub(crate) fn base64(input: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let bits = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        output.push(ALPHABET[((bits >> 18) & 63) as usize] as char);
        output.push(ALPHABET[((bits >> 12) & 63) as usize] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[((bits >> 6) & 63) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            ALPHABET[(bits & 63) as usize] as char
        } else {
            '='
        });
    }
    output
}
