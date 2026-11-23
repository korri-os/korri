//! Fixed address-only reads shared by the observer and the role policy.
use std::fs;
use std::path::Path;
use std::process::Command;

pub fn bus_for(sysfs: &Path, controller: &str) -> Result<String, String> {
    let adapters = sysfs.join("bus/i2c/devices");
    let mut matches = Vec::new();
    for entry in fs::read_dir(adapters).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path.file_name().unwrap().to_string_lossy();
        if let Some(number) = name.strip_prefix("i2c-") {
            if number.parse::<u32>().is_ok()
                && fs::canonicalize(path.join("of_node"))
                    .is_ok_and(|p| p.file_name().is_some_and(|n| n == controller))
            {
                matches.push(number.to_owned());
            }
        }
    }
    match matches.as_slice() {
        [bus] => Ok(bus.clone()),
        _ => Err(format!(
            "expected one adapter for {controller}, got {matches:?}"
        )),
    }
}

pub fn parse_read(output: &[u8], count: usize) -> Result<Vec<u8>, String> {
    let text = std::str::from_utf8(output).map_err(|e| e.to_string())?;
    let bytes: Result<Vec<_>, _> = text
        .split_whitespace()
        .map(|s| {
            let hex = s.strip_prefix("0x").ok_or("missing hex prefix")?;
            if hex.len() != 2 {
                return Err("expected one byte");
            }
            u8::from_str_radix(hex, 16).map_err(|_| "invalid hex byte")
        })
        .collect();
    let bytes = bytes?;
    if bytes.len() != count {
        return Err(format!(
            "short/long read: expected {count}, got {}",
            bytes.len()
        ));
    }
    Ok(bytes)
}

// i2ctransfer uses a single I2C_RDWR: address-only write, repeated START, read.
// The kernel refuses an address already claimed by a driver; never use -f.
pub fn read(bus: &str, address: u8, register: &[u8], count: usize) -> Result<Vec<u8>, String> {
    let mut command = Command::new("timeout");
    command.args(["--signal=KILL", "0.2s", "i2ctransfer"]);
    command.args(["-y", bus, &format!("w{}@0x{address:02x}", register.len())]);
    for byte in register {
        command.arg(format!("0x{byte:02x}"));
    }
    let result = command
        .arg(format!("r{count}"))
        .output()
        .map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(format!(
            "bus={bus} address={address:#04x} register={register:02x?}: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    parse_read(&result.stdout, count)
}
