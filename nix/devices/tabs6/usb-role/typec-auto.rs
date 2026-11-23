//! The measured Kishi/pass-through policy. All effects go through tabs6-usb-role.
mod typec;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use typec::{bus_for, read};

const INTERVAL: Duration = Duration::from_millis(300);

#[derive(Debug)]
struct Sample {
    state: u8,
    attached: bool,
    source: bool,
    muic_vbus: u8,
    charger: u8,
    gauge: String,
    sourcing: bool,
}

impl Sample {
    fn permits_host(&self) -> bool {
        self.state == 14
            && self.attached
            && self.source
            && self.gauge != "Charging"
            // Our own boost raises both VBUS indicators. Only the measured
            // s2mm005 source role can distinguish this from incoming power.
            && (self.sourcing || (self.muic_vbus == 0 && self.charger & 1 == 0))
    }
}

fn sample(sysfs: &Path) -> Result<Sample, String> {
    let pd = bus_for(sysfs, "i2c@c94000")?;
    let sm = bus_for(sysfs, "i2c@890000")?;
    read(&pd, 0x33, &[0x00, 0x08], 1)?;
    let func = read(&pd, 0x33, &[0x00, 0x20], 4)?;
    let muic_vbus = read(&sm, 0x25, &[0x15], 1)?[0];
    let charger = read(&sm, 0x49, &[0x08], 1)?[0];
    let mode = read(&sm, 0x49, &[0x0c], 1)?[0] & 7;
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
    })
}

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Device,
    Wait,
    Host,
}

#[derive(Default)]
enum State {
    #[default]
    Device,
    Qualifying(u8),
    Host,
}

#[derive(Default)]
struct Policy {
    state: State,
}

impl Policy {
    fn update(&mut self, sample: Result<&Sample, &str>) -> Action {
        let Ok(sample) = sample else {
            self.state = State::Device;
            return Action::Device;
        };
        if !sample.permits_host() || (sample.sourcing && !matches!(self.state, State::Host)) {
            self.state = State::Device;
            return Action::Device;
        }
        if matches!(self.state, State::Host) && sample.sourcing {
            return Action::Wait;
        }
        let consecutive = match self.state {
            State::Qualifying(count) => count + 1,
            _ => 1,
        };
        if consecutive >= 3 {
            self.state = State::Host;
            Action::Host
        } else {
            self.state = State::Qualifying(consecutive);
            Action::Wait
        }
    }
}

fn apply(role: &str) -> Result<(), String> {
    let helper = std::env::var("TABS6_USB_ROLE_HELPER").unwrap_or("tabs6-usb-role".into());
    let status = Command::new(helper)
        .arg(role)
        .env("TABS6_AUTOMATIC", "1")
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{role} failed: {status}"))
    }
}

fn run() -> Result<(), String> {
    let root = std::env::var("TABS6_SYSFS").unwrap_or("/sys".into());
    let sysfs = Path::new(&root);
    match std::env::args().nth(1).as_deref() {
        Some("--sm-bus") if std::env::args().len() == 2 => {
            println!("{}", bus_for(sysfs, "i2c@890000")?);
            return Ok(());
        }
        Some("--check-host") if std::env::args().len() == 2 => {
            let s = sample(sysfs)?;
            return if !s.sourcing && s.permits_host() {
                Ok(())
            } else {
                Err(format!("host refused: {s:?}"))
            };
        }
        None => (),
        _ => return Err("usage: tabs6-typec-auto [--check-host|--sm-bus]".into()),
    }
    // Never inherit permission or boost across restart. ExecStopPost also
    // requests off/device after SIGTERM, SIGKILL, or any failure here.
    apply("device")?;
    let mut policy = Policy::default();
    let start = Instant::now();
    loop {
        let tick = Instant::now();
        let current = sample(sysfs);
        let was_host = matches!(policy.state, State::Host);
        let action = policy.update(current.as_ref().map_err(String::as_str));
        println!(
            "elapsed_ms={} sample={current:?} action={action:?}",
            start.elapsed().as_millis()
        );
        match action {
            Action::Device if was_host || current.as_ref().map_or(true, |s| s.sourcing) => {
                apply("device")?;
            }
            Action::Host => {
                // The helper takes the same fresh snapshot again, including
                // one immediately before the boost write. Never retry enable.
                apply("host")?;
            }
            _ => (),
        }
        thread::sleep(INTERVAL.saturating_sub(tick.elapsed()));
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("tabs6-typec-auto: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kishi() -> Sample {
        Sample {
            state: 14,
            attached: true,
            source: true,
            muic_vbus: 0,
            charger: 0x22,
            gauge: "Discharging".into(),
            sourcing: false,
        }
    }

    #[test]
    fn three_consecutive_samples_then_own_vbus_is_not_incoming_power() {
        let mut policy = Policy::default();
        let mut s = kishi();
        assert_eq!(policy.update(Ok(&s)), Action::Wait);
        assert_eq!(policy.update(Ok(&s)), Action::Wait);
        assert_eq!(policy.update(Ok(&s)), Action::Host);
        s.sourcing = true;
        s.muic_vbus = 1;
        s.charger = 0x21;
        for _ in 0..100 {
            assert_eq!(policy.update(Ok(&s)), Action::Wait);
        }
        s.state = 0;
        s.source = false;
        assert_eq!(policy.update(Ok(&s)), Action::Device);
    }

    #[test]
    fn every_state_and_attach_source_combination_is_fail_closed() {
        for state in 0..=255 {
            for flags in 0..4 {
                let s = Sample {
                    state,
                    attached: flags & 1 != 0,
                    source: flags & 2 != 0,
                    ..kishi()
                };
                assert_eq!(s.permits_host(), state == 14 && flags == 3);
            }
        }
    }

    #[test]
    fn incoming_power_and_read_errors_revoke_and_reset_qualification() {
        for failure in 0..5 {
            let mut policy = Policy::default();
            let s = kishi();
            policy.update(Ok(&s));
            policy.update(Ok(&s));
            let mut bad = kishi();
            match failure {
                0 => bad.muic_vbus = 1,
                1 => bad.charger = 0x21,
                2 => bad.gauge = "Charging".into(),
                3 => bad.sourcing = true,
                _ => (),
            }
            assert_eq!(
                policy.update(if failure == 4 {
                    Err("read failed")
                } else {
                    Ok(&bad)
                }),
                Action::Device
            );
            assert_eq!(policy.update(Ok(&s)), Action::Wait);
            assert_eq!(policy.update(Ok(&s)), Action::Wait);
            assert_eq!(policy.update(Ok(&s)), Action::Host);
            assert_eq!(policy.update(Err("read failed")), Action::Device);
        }
    }

    #[test]
    fn replay_real_phase_a_charger_transitions() {
        let expected = [
            Action::Device,
            Action::Device,
            Action::Wait,
            Action::Wait,
            Action::Host,
            Action::Device,
            Action::Device,
            Action::Device,
            Action::Device,
            Action::Device,
            Action::Device,
            Action::Device,
            Action::Wait,
            Action::Wait,
            Action::Host,
        ];
        let mut policy = Policy::default();
        let lines: Vec<_> = include_str!("phase-a-transition.txt").lines().collect();
        assert_eq!(lines.len(), expected.len());
        for (line, expected) in lines.into_iter().zip(expected) {
            let field = |key| {
                line.split_whitespace()
                    .find_map(|part| part.strip_prefix(key))
                    .unwrap()
            };
            let s = Sample {
                state: field("state=").parse().unwrap(),
                attached: field("attached=") == "1",
                source: field("source=") == "1",
                muic_vbus: u8::from_str_radix(field("muic_vbus="), 16).unwrap(),
                charger: u8::from_str_radix(field("chg_status="), 16).unwrap(),
                gauge: field("gauge=").trim_matches('"').into(),
                sourcing: false, // The real capture had boost off throughout.
            };
            assert_eq!(policy.update(Ok(&s)), expected, "{line}");
        }
    }
}
