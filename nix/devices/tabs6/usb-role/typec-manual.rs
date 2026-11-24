//! Fresh read-only interlocks for the operator-only VBUS sourcing helper.
mod typec;
use std::fs;
use std::path::Path;
use typec::{bus_for, read};

#[derive(Debug)]
struct Sample {
    state: u8,
    attached: bool,
    source: bool,
    muic_vbus: u8,
    charger: u8,
    gauge: String,
    sourcing: bool,
    boost: u8,
    muic_devices: [u8; 3],
}

impl Sample {
    fn safe_power(&self) -> bool {
        // STATUS1 does NOT distinguish OTG output from external VBUS.
        // Only the measured no-input/own-output values are accepted. This
        // rejects VBUSOVP/LIMIT and every wireless-input/fault indication.
        self.gauge != "Charging"
            && matches!(self.charger, 0x21 | 0x22)
            && self.muic_vbus <= 1
            && (!self.sourcing || self.boost & 0x0f == 0x0b)
            // MUIC DEVT1 bit7 = OTG, bit3 = passive UART (also measured
            // with the unplugged Kishi after reboot). Neither proves power
            // direction. Charger/PC and unknown signatures still deny.
            && matches!(self.muic_devices, [0 | 0x08 | 0x80, 0, 0])
    }

    fn source_attached(&self) -> bool {
        self.state == 14 && self.attached && self.source
    }

    fn permits_host(&self) -> bool {
        self.safe_power()
            && self.source_attached()
            && (self.sourcing || (self.muic_vbus == 0 && self.charger == 0x22))
    }
}

fn sample(sysfs: &Path) -> Result<Sample, String> {
    let pd = bus_for(sysfs, "i2c@c94000")?;
    let sm = bus_for(sysfs, "i2c@890000")?;
    read(&pd, 0x33, &[0x00, 0x08], 1)?;
    let func = read(&pd, 0x33, &[0x00, 0x20], 4)?;
    let muic_vbus = read(&sm, 0x25, &[0x15], 1)?[0];
    let charger = read(&sm, 0x49, &[0x08], 1)?[0];
    let cntl = read(&sm, 0x49, &[0x0c], 1)?[0];
    let mode = cntl & 7;
    let boost = read(&sm, 0x49, &[0x20], 1)?[0];
    let devices = read(&sm, 0x25, &[0x0a], 3)?;
    let muic_devices = [devices[0], devices[1], devices[2]];
    let roles: Vec<_> = fs::read_dir(sysfs.join("class/usb_role"))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    if roles.len() != 1 {
        return Err("expected exactly one USB role switch".into());
    }
    let role = fs::read_to_string(roles[0].path().join("role")).map_err(|e| e.to_string())?;
    if !matches!(role.trim(), "host" | "device") {
        return Err(format!("unexpected USB role {role:?}"));
    }
    if mode != 5 && mode != 7 {
        return Err(format!("unexpected charger mode {mode}"));
    }
    let gauge = fs::read_to_string(sysfs.join("class/power_supply/sm5705-fuelgauge/status"))
        .map_err(|e| e.to_string())?;
    if gauge.trim().is_empty() {
        return Err("empty gauge status".into());
    }
    Ok(Sample {
        state: func[0],
        attached: func[3] & 1 != 0,
        source: func[3] & 2 != 0,
        muic_vbus,
        charger,
        gauge: gauge.trim().into(),
        sourcing: mode == 7,
        boost,
        muic_devices,
    })
}

fn run() -> Result<(), String> {
    let root = std::env::var("TABS6_SYSFS").unwrap_or("/sys".into());
    let sysfs = Path::new(&root);
    match std::env::args().nth(1).as_deref() {
        Some("--sm-bus") if std::env::args().len() == 2 => {
            println!("{}", bus_for(sysfs, "i2c@890000")?);
            Ok(())
        }
        Some("--check-host") if std::env::args().len() == 2 => {
            let s = sample(sysfs)?;
            if !s.sourcing && s.permits_host() {
                Ok(())
            } else {
                Err(format!("host refused: {s:?}"))
            }
        }
        _ => Err("usage: tabs6-typec-manual [--check-host|--sm-bus]".into()),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("tabs6-typec-manual: {error}");
        std::process::exit(1);
    }
}
