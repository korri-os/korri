//! Phase A only: fixed, vendor-documented status reads. No register data writes,
//! interrupt acknowledgement, role changes, firmware paths or arbitrary register CLI.
//! Sources and field definitions: typec.md beside this file.
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

mod typec;
#[cfg(test)]
use typec::parse_read;
use typec::{bus_for, read};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn pd_fields(bytes: &[u8]) -> Result<String, String> {
    let [state, cc, next, flags] = *bytes else {
        return Err("FUNC_STATE must have four bytes".into());
    };
    Ok(format!(
        "state={state} cc1={} cc2={} next={next} attached={} source={} dfp={} short={} reset={}",
        cc & 7,
        (cc >> 4) & 7,
        flags & 1,
        (flags >> 1) & 1,
        (flags >> 2) & 1,
        (flags >> 5) & 3,
        flags >> 7,
    ))
}

fn sample(sysfs: &Path, pd_bus: &str, sm_bus: &str) -> Result<String, String> {
    // Samsung wakes the ordinary status window with a one-byte dummy read.
    // This is a status read too, not an MCU command or firmware access.
    read(pd_bus, 0x33, &[0x00, 0x08], 1)?;
    let func = read(pd_bus, 0x33, &[0x00, 0x20], 4)?;
    let interrupts = read(pd_bus, 0x33, &[0x00, 0x30], 32)?;
    let low_power = read(pd_bus, 0x33, &[0x00, 0x60], 4)?;
    let muic_irq = read(sm_bus, 0x25, &[0x03], 3)?;
    let muic_devices = read(sm_bus, 0x25, &[0x09], 4)?;
    let muic_vbus = read(sm_bus, 0x25, &[0x15], 1)?;
    let muic_charger = read(sm_bus, 0x25, &[0x17], 1)?;
    let charger_status = read(sm_bus, 0x49, &[0x08], 1)?;
    let charger_mode = read(sm_bus, 0x49, &[0x0c], 1)?;
    let boost = read(sm_bus, 0x49, &[0x20], 1)?;
    let gauge = fs::read_to_string(sysfs.join("class/power_supply/sm5705-fuelgauge/status"))
        .map_err(|e| e.to_string())?;
    let current = fs::read_to_string(sysfs.join("class/power_supply/sm5705-fuelgauge/current_now"))
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "func={} {} lp={} irq={} muic_irq={} muic_adc_dev={} muic_vbus={} muic_chg={} chg_status={} chg_mode={} boost={} gauge={:?} current_ua={}",
        hex(&func), pd_fields(&func)?, hex(&low_power), hex(&interrupts),
        hex(&muic_irq), hex(&muic_devices), hex(&muic_vbus), hex(&muic_charger),
        hex(&charger_status), hex(&charger_mode), hex(&boost), gauge.trim(), current.trim(),
    ))
}

fn gpio_chip(sysfs: &Path) -> Result<PathBuf, String> {
    let mut matches = Vec::new();
    for entry in fs::read_dir(sysfs.join("bus/gpio/devices")).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if fs::canonicalize(path.join("of_node"))
            .is_ok_and(|p| p.file_name().is_some_and(|n| n == "pinctrl@3100000"))
        {
            matches.push(Path::new("/dev").join(path.file_name().unwrap()));
        }
    }
    match matches.as_slice() {
        [chip] => Ok(chip.clone()),
        _ => Err(format!("expected one TLMM gpiochip, got {matches:?}")),
    }
}

fn run() -> Result<(), String> {
    if std::env::args().len() != 1 {
        return Err("usage: tabs6-typec-status (read-only; no arguments)".into());
    }
    let sysfs = Path::new("/sys");
    let pd_bus = bus_for(sysfs, "i2c@c94000")?;
    let sm_bus = bus_for(sysfs, "i2c@890000")?;
    let chip = gpio_chip(sysfs)?;
    // Stock DT: s2mm005=123, MUIC=122, charger=41. Input, no bias change.
    let mut irq = Command::new("stdbuf")
        .args(["-oL", "gpiomon", "--chip"])
        .arg(chip)
        .args([
            "--edges=both",
            "--banner",
            "--consumer=tabs6-typec-status",
            "--format=irq timestamp=%S line=%o edge=%e",
            "123",
            "122",
            "41",
        ])
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    println!("read-only pd_bus={pd_bus} sm_bus={sm_bus}; no IRQ acknowledgement");
    loop {
        if let Some(status) = irq.try_wait().map_err(|e| e.to_string())? {
            return Err(format!("gpiomon exited: {status}"));
        }
        let uptime = fs::read_to_string("/proc/uptime").map_err(|e| e.to_string())?;
        match sample(sysfs, &pd_bus, &sm_bus) {
            Ok(status) => println!(
                "uptime={} {status}",
                uptime.split_whitespace().next().unwrap()
            ),
            Err(error) => println!(
                "uptime={} read_error={error:?}",
                uptime.split_whitespace().next().unwrap()
            ),
        }
        io::stdout().flush().map_err(|e| e.to_string())?;
        thread::sleep(Duration::from_millis(250));
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("tabs6-typec-status: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_require_exactly_the_requested_bytes() {
        assert_eq!(
            parse_read(b"0x03 0x42 0x03 0x07\n", 4).unwrap(),
            [3, 66, 3, 7]
        );
        for data in [
            b"".as_slice(),
            b"0x03",
            b"0x03 0x42 0x03 0x07 0x00",
            b"0x003 0x42 0x03 0x07",
            b"03 42 03 07",
            b"0xgg 0x42 0x03 0x07",
        ] {
            assert!(parse_read(data, 4).is_err(), "{data:?}");
        }
    }

    #[test]
    fn vendor_bitfields_decode_without_native_layout_assumptions() {
        assert_eq!(
            pd_fields(&[3, 0x42, 6, 7]).unwrap(),
            "state=3 cc1=2 cc2=4 next=6 attached=1 source=1 dfp=1 short=0 reset=0"
        );
        assert_eq!(
            pd_fields(&[0, 0, 0, 0]).unwrap(),
            "state=0 cc1=0 cc2=0 next=0 attached=0 source=0 dfp=0 short=0 reset=0"
        );
        assert!(pd_fields(&[0]).is_err());
    }

    #[test]
    fn real_charger_baseline_has_dfp_set_without_source_permission() {
        // Actual Phase A capture on the tablet with its direct charger.
        assert_eq!(
            pd_fields(&[0x15, 0x21, 0xff, 0x9d]).unwrap(),
            "state=21 cc1=1 cc2=2 next=255 attached=1 source=0 dfp=1 short=0 reset=1"
        );
    }

    #[test]
    fn sampling_only_issues_the_documented_address_only_reads() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("tabs6-typec-reads-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let program = root.join("i2ctransfer");
        fs::write(
            &program,
            r#"#!/usr/bin/env bash
set -eu
printf '%s\n' "$*" >> "$TABS6_READ_LOG"
if [ "${TABS6_READ_ERROR:-}" = 1 ]; then echo 'Read failed' >&2; exit 1; fi
if [ "${TABS6_SHORT_READ:-}" = 1 ] && [ "$*" = '-y 7 w2@0x33 0x00 0x20 r4' ]; then echo 0x03; exit 0; fi
case "$*" in
'-y 7 w2@0x33 0x00 0x08 r1') echo 0x01 ;;
'-y 7 w2@0x33 0x00 0x20 r4') echo '0x03 0x42 0x03 0x07' ;;
'-y 7 w2@0x33 0x00 0x30 r32') for i in {1..32}; do printf '0x00 '; done; echo ;;
'-y 7 w2@0x33 0x00 0x60 r4') echo '0xa0 0x00 0x00 0x00' ;;
'-y 8 w1@0x25 0x03 r3') echo '0x00 0x00 0x00' ;;
'-y 8 w1@0x25 0x09 r4') echo '0x1f 0x00 0x00 0x00' ;;
'-y 8 w1@0x25 0x15 r1') echo 0x01 ;;
'-y 8 w1@0x25 0x17 r1') echo 0x01 ;;
'-y 8 w1@0x49 0x08 r1') echo 0x21 ;;
'-y 8 w1@0x49 0x0c r1') echo 0x05 ;;
'-y 8 w1@0x49 0x20 r1') echo 0x05 ;;
*) echo 'forbidden read or register DATA write' >&2; exit 1 ;;
esac
"#,
        )
        .unwrap();
        // /usr/bin/env is intentionally absent in a Nix build sandbox.
        let bash = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|p| p.join("bash"))
            .find(|p| p.is_file())
            .unwrap();
        let script = fs::read_to_string(&program).unwrap();
        fs::write(
            &program,
            script.replacen("#!/usr/bin/env bash", &format!("#!{}", bash.display()), 1),
        )
        .unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        let gauge = root.join("class/power_supply/sm5705-fuelgauge");
        fs::create_dir_all(&gauge).unwrap();
        fs::write(gauge.join("status"), "Charging\n").unwrap();
        fs::write(gauge.join("current_now"), "1000000\n").unwrap();
        let path = std::env::var_os("PATH").unwrap();
        std::env::set_var(
            "PATH",
            format!("{}:{}", root.display(), path.to_string_lossy()),
        );
        std::env::set_var("TABS6_READ_LOG", root.join("reads"));
        let result = sample(&root, "7", "8").unwrap();
        // Prepare the fake-charger input for Phase B: a negative battery
        // current must not hide incoming VBUS even if the MCU says source.
        fs::write(gauge.join("status"), "Discharging\n").unwrap();
        fs::write(gauge.join("current_now"), "-500000\n").unwrap();
        let conflict = sample(&root, "7", "8").unwrap();
        std::env::set_var("TABS6_READ_ERROR", "1");
        assert!(sample(&root, "7", "8").unwrap_err().contains("Read failed"));
        std::env::remove_var("TABS6_READ_ERROR");
        std::env::set_var("TABS6_SHORT_READ", "1");
        assert!(sample(&root, "7", "8")
            .unwrap_err()
            .contains("short/long read"));
        std::env::remove_var("TABS6_SHORT_READ");
        std::env::set_var("PATH", path);
        assert!(conflict.contains("gauge=\"Discharging\""));
        assert!(conflict.contains("attached=1 source=1"));
        assert!(conflict.contains("muic_vbus=01"));
        assert!(conflict.contains("chg_status=21"));
        assert!(result.contains("gauge=\"Charging\""));
        assert!(result.contains("attached=1 source=1"));
        assert!(result.contains("muic_vbus=01"));
        assert_eq!(
            fs::read_to_string(root.join("reads"))
                .unwrap()
                .lines()
                .count(),
            25
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn adapter_resolution_uses_of_node_not_probe_order() {
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join(format!("tabs6-typec-{}", std::process::id()));
        let device = root.join("firmware/devicetree/base/soc/i2c@890000");
        fs::create_dir_all(&device).unwrap();
        let adapter = root.join("bus/i2c/devices/i2c-7");
        fs::create_dir_all(&adapter).unwrap();
        symlink(&device, adapter.join("of_node")).unwrap();
        assert_eq!(bus_for(&root, "i2c@890000").unwrap(), "7");
        assert!(bus_for(&root, "i2c@c94000").is_err());
        let duplicate = root.join("bus/i2c/devices/i2c-8");
        fs::create_dir_all(&duplicate).unwrap();
        symlink(&device, duplicate.join("of_node")).unwrap();
        assert!(bus_for(&root, "i2c@890000").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
