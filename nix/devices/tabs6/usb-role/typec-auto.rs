//! USB-C data-role policy and bounded VBUS candidate observer. Automatic
//! sourcing remains blocked; sink-host never powers VBUS. Effects use tabs6-usb-role.
mod typec;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use typec::{bus_for, read};

const INTERVAL: Duration = Duration::from_millis(300);
const TRACE_INTERVAL: Duration = Duration::from_millis(100);
const TRACE_WINDOW: Duration = Duration::from_secs(5);
const SETTLE_WINDOW: Duration = Duration::from_secs(3);
const MAX_BACKOFF: Duration = Duration::from_secs(60);

#[derive(Debug)]
struct Sample {
    state: u8,
    attached: bool,
    source: bool,
    dfp: bool,
    muic_vbus: u8,
    charger: u8,
    gauge: String,
    sourcing: bool,
    cntl: u8,
    boost: u8,
    muic_devices: [u8; 3],
    role: String,
}

impl Sample {
    fn permits_sink_host(&self) -> bool {
        // Measured on this board with the Kishi pass-through charger:
        // SNK_Ready + DFP is a data role, not permission to source VBUS.
        self.state == 21
            && self.attached
            && !self.source
            && self.dfp
            && self.muic_vbus == 1
            && self.gauge == "Charging"
            && self.cntl & 7 == 5
            && !self.sourcing
    }

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
        dfp: func[3] & 4 != 0,
        muic_vbus,
        charger,
        gauge: gauge.trim().into(),
        sourcing: mode == 7,
        cntl,
        boost,
        muic_devices,
        role: role.trim().into(),
    })
}

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Device,
    Wait,
    Host,
    SinkHost,
}

#[derive(Default)]
enum State {
    #[default]
    Device,
    Qualifying(u8),
    Settling {
        since: Duration,
    },
    Host {
        since: Duration,
    },
}

impl State {
    fn sourcing(&self) -> bool {
        matches!(self, Self::Settling { .. } | Self::Host { .. })
    }
}

#[derive(Default)]
struct Policy {
    state: State,
    next_host: Duration,
    backoff: Duration,
}

impl Policy {
    fn host_started(&mut self, now: Duration) {
        self.state = State::Settling { since: now };
    }

    fn device(&mut self, now: Duration) -> Action {
        // Every unsuccessful start backs off, including an early unsafe
        // signature or failed read. Off is immediate; only re-enable waits.
        if matches!(self.state, State::Settling { .. }) {
            self.backoff = if self.backoff.is_zero() {
                SETTLE_WINDOW
            } else {
                (self.backoff * 2).min(MAX_BACKOFF)
            };
            self.next_host = now + self.backoff;
        }
        self.state = State::Device;
        Action::Device
    }

    fn update(&mut self, now: Duration, sample: Result<&Sample, &str>) -> Action {
        let Ok(sample) = sample else {
            return self.device(now);
        };
        if !sample.safe_power() {
            return self.device(now);
        }
        if let State::Settling { since } = self.state {
            let elapsed = now.saturating_sub(since);
            if sample.sourcing && sample.source_attached() {
                if elapsed >= SETTLE_WINDOW {
                    self.state = State::Host { since };
                }
                return Action::Wait;
            }
            if elapsed >= SETTLE_WINDOW {
                return self.device(now);
            }
            // This is the ONLY exception. Sink signatures (17/20/21),
            // lost boost and unknown states revoke now. State 0 also briefly
            // carries attached=1 during local-VBUS redetection, then
            // state 14 appears with both flags clear. Only source=1 recovery
            // before the deadline could permit a sustained host.
            if sample.sourcing
                && !sample.source
                && (sample.state == 0 || (sample.state == 14 && !sample.attached))
            {
                return Action::Wait;
            }
            return self.device(now);
        }
        if !sample.permits_host() || (sample.sourcing && !self.state.sourcing()) {
            return self.device(now);
        }
        if let State::Host { since } = self.state {
            if sample.sourcing {
                if now.saturating_sub(since) >= MAX_BACKOFF {
                    self.backoff = Duration::ZERO;
                }
                return Action::Wait;
            }
            return self.device(now);
        }
        if now < self.next_host {
            return self.device(now);
        }
        let consecutive = match self.state {
            State::Qualifying(count) => count + 1,
            _ => 1,
        };
        if consecutive >= 3 {
            self.host_started(now);
            Action::Host
        } else {
            self.state = State::Qualifying(consecutive);
            Action::Wait
        }
    }
}

// Independent from the blocked VBUS-on policy. Each update consumes a new
// hardware read; one bad read revokes immediately and clears qualification.
#[derive(Default)]
struct SinkHostPolicy {
    consecutive: u8,
}

impl SinkHostPolicy {
    fn update(&mut self, sample: Result<&Sample, &str>) -> Action {
        if !sample.is_ok_and(Sample::permits_sink_host) {
            self.consecutive = 0;
            return Action::Device;
        }
        self.consecutive = (self.consecutive + 1).min(3);
        if self.consecutive == 3 {
            Action::SinkHost
        } else {
            Action::Device
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
        Some("--check-sink-host") if std::env::args().len() == 2 => {
            let s = sample(sysfs)?;
            return if s.permits_sink_host() {
                Ok(())
            } else {
                Err(format!("sink-host refused: {s:?}"))
            };
        }
        None => (),
        _ => {
            return Err("usage: tabs6-typec-auto [--check-host|--check-sink-host|--sm-bus]".into())
        }
    }
    // Never inherit permission or boost across restart. ExecStopPost also
    // requests off/device after SIGTERM, SIGKILL, or any failure here.
    apply("device")?;
    println!(
        "mode=observe automatic_host=blocked reason=no_verified_power_direction sink_host=enabled"
    );
    let mut policy = Policy::default();
    let mut sink_host = SinkHostPolicy::default();
    let start = Instant::now();
    let mut host_trace = None;
    loop {
        let tick = Instant::now();
        let current = sample(sysfs);
        let was_host = policy.state.sourcing();
        let was_sink_host = sink_host.consecutive == 3;
        let requested = policy.update(start.elapsed(), current.as_ref().map_err(String::as_str));
        // Live trials never recovered source=1 with boost on. STATUS1 and
        // MUIC cannot distinguish our output from charger input. No runtime
        // opt-in may bypass this block. --check-host remains the manual
        // helper's fresh interlock, not automatic sourcing permission.
        let sourcing_action = if requested == Action::Host {
            policy.state = State::Device;
            println!(
                "automatic_host=blocked reason=no_verified_power_direction staying_device=true"
            );
            Action::Device
        } else {
            requested
        };
        let sink_action = sink_host.update(current.as_ref().map_err(String::as_str));
        let action = if sink_action == Action::SinkHost {
            sink_action
        } else if sourcing_action == Action::Wait {
            // Qualification of a blocked sourcing candidate grants no role.
            Action::Device
        } else {
            sourcing_action
        };
        if current.as_ref().is_ok_and(|s| s.sourcing) && host_trace.is_none() {
            host_trace = Some(Instant::now());
        }
        if let Ok(s) = &current {
            println!(
                "hardware role={} cntl={:#04x} boost={:#04x}",
                s.role, s.cntl, s.boost
            );
        }
        let host_ms = host_trace.map(|since: Instant| since.elapsed().as_millis());
        println!(
            "elapsed_ms={} host_ms={host_ms:?} sample_ms={} sample={current:?} action={action:?} retry_after_ms={}",
            start.elapsed().as_millis(),
            tick.elapsed().as_millis(),
            policy.next_host.saturating_sub(start.elapsed()).as_millis()
        );
        match action {
            Action::SinkHost if current.as_ref().is_ok_and(|s| s.role != "host") => {
                if let Err(error) = apply("sink-host") {
                    eprintln!("tabs6-typec-auto: sink-host failed, returning to device: {error}");
                    sink_host.consecutive = 0;
                    apply("sink-device")?;
                }
            }
            Action::Device
                if was_host
                    || was_sink_host
                    || current
                        .as_ref()
                        .map_or(true, |s| s.sourcing || s.role != "device") =>
            {
                // Finish provider teardown while the host controller is absent.
                // A failed teardown stops policy; it must not re-enable host.
                apply(if was_sink_host {
                    "sink-device"
                } else {
                    "device"
                })?;
            }
            _ => (),
        }
        let interval = if host_trace.is_some_and(|since| since.elapsed() < TRACE_WINDOW) {
            TRACE_INTERVAL
        } else {
            INTERVAL
        };
        thread::sleep(interval.saturating_sub(tick.elapsed()));
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
            dfp: true,
            muic_vbus: 0,
            charger: 0x22,
            gauge: "Discharging".into(),
            sourcing: false,
            cntl: 5,
            boost: 5,
            muic_devices: [0; 3],
            role: "device".into(),
        }
    }

    #[test]
    fn three_consecutive_samples_then_own_vbus_is_not_incoming_power() {
        let mut policy = Policy::default();
        let mut s = kishi();
        assert_eq!(policy.update(Duration::ZERO, Ok(&s)), Action::Wait);
        assert_eq!(policy.update(Duration::ZERO, Ok(&s)), Action::Wait);
        assert_eq!(policy.update(Duration::ZERO, Ok(&s)), Action::Host);
        s.sourcing = true;
        s.boost = 0x0b;
        s.muic_vbus = 1;
        s.charger = 0x21;
        for _ in 0..100 {
            assert_eq!(policy.update(SETTLE_WINDOW, Ok(&s)), Action::Wait);
        }
        s.state = 0;
        s.source = false;
        assert_eq!(policy.update(SETTLE_WINDOW, Ok(&s)), Action::Device);
    }

    #[test]
    fn host_start_does_not_revoke_for_the_measured_local_vbus_detach() {
        let mut policy = Policy::default();
        let mut s = kishi();
        policy.update(Duration::ZERO, Ok(&s));
        policy.update(Duration::ZERO, Ok(&s));
        assert_eq!(policy.update(Duration::ZERO, Ok(&s)), Action::Host);
        s.sourcing = true;
        s.boost = 0x0b;
        s.muic_vbus = 1;
        s.charger = 0x21;
        s.state = 0;
        s.attached = false;
        s.source = false;
        assert_eq!(policy.update(Duration::ZERO, Ok(&s)), Action::Wait);
    }

    fn detached_on_our_boost() -> Sample {
        Sample {
            state: 0,
            attached: false,
            source: false,
            sourcing: true,
            cntl: 7,
            boost: 0x0b,
            muic_vbus: 1,
            charger: 0x21,
            role: "host".into(),
            ..kishi()
        }
    }

    #[test]
    fn settle_deadline_is_bounded_and_retries_back_off_to_sixty_seconds() {
        let mut policy = Policy::default();
        let mut now = Duration::ZERO;
        for delay in [3, 6, 12, 24, 48, 60, 60] {
            assert_eq!(policy.update(now, Ok(&kishi())), Action::Wait);
            assert_eq!(policy.update(now, Ok(&kishi())), Action::Wait);
            assert_eq!(policy.update(now, Ok(&kishi())), Action::Host);
            // A sample must not restart or extend the deadline.
            for ms in (0..3000).step_by(100) {
                assert_eq!(
                    policy.update(
                        now + Duration::from_millis(ms),
                        Ok(&detached_on_our_boost())
                    ),
                    Action::Wait
                );
            }
            now += SETTLE_WINDOW;
            assert_eq!(
                policy.update(now, Ok(&detached_on_our_boost())),
                Action::Device
            );
            let retry = now + Duration::from_secs(delay);
            // Neither a read failure nor device samples can erase backoff.
            assert_eq!(policy.update(now, Err("read failed")), Action::Device);
            assert_eq!(
                policy.update(retry - Duration::from_millis(1), Ok(&kishi())),
                Action::Device
            );
            now = retry;
        }
    }

    #[test]
    fn recovered_source_holds_but_detach_after_settle_revokes() {
        let mut policy = Policy::default();
        policy.host_started(Duration::ZERO);
        let mut s = detached_on_our_boost();
        assert_eq!(
            policy.update(Duration::from_millis(2999), Ok(&s)),
            Action::Wait
        );
        s.state = 14;
        s.attached = true;
        s.source = true;
        assert_eq!(policy.update(SETTLE_WINDOW, Ok(&s)), Action::Wait);
        assert_eq!(policy.update(Duration::from_secs(10), Ok(&s)), Action::Wait);
        assert_eq!(
            policy.update(Duration::from_secs(11), Ok(&detached_on_our_boost())),
            Action::Device
        );
    }

    #[test]
    fn safety_revocations_are_immediate_even_during_settle() {
        for ms in [0, 100, 2999, 3000] {
            for failure in 0..12 {
                let mut policy = Policy::default();
                policy.host_started(Duration::ZERO);
                let mut bad = detached_on_our_boost();
                match failure {
                    0..=2 => {
                        bad.state = [17, 20, 21][failure];
                        bad.attached = true;
                    }
                    3 => bad.gauge = "Charging".into(),
                    4 => bad.charger = 0x29, // VBUSLIMIT, not an OTG indicator.
                    5 => bad.charger = 0x25, // VBUSOVP.
                    6 => bad.charger = 0x31, // Wireless input POK.
                    7 => bad.muic_devices = [0x40, 0, 0], // Dedicated charger.
                    8 => bad.sourcing = false,
                    9 => bad.state = 42,
                    10 => bad.boost = 5,
                    _ => (),
                }
                assert_eq!(
                    policy.update(
                        Duration::from_millis(ms),
                        if failure == 11 {
                            Err("read failed")
                        } else {
                            Ok(&bad)
                        }
                    ),
                    Action::Device,
                    "ms={ms} failure={failure}"
                );
            }
        }
    }

    #[test]
    fn no_restart_can_inherit_boost_or_the_settle_exception() {
        let mut policy = Policy::default();
        assert_eq!(
            policy.update(Duration::ZERO, Ok(&detached_on_our_boost())),
            Action::Device
        );
    }

    fn logged_sample(line: &str) -> Sample {
        let value = |key: &str| {
            line.split_once(key)
                .unwrap()
                .1
                .split([',', ' ', '}'])
                .next()
                .unwrap()
        };
        let sourcing = value("sourcing: ") == "true";
        Sample {
            state: value("state: ").parse().unwrap(),
            attached: value("attached: ") == "true",
            source: value("source: ") == "true",
            muic_vbus: value("muic_vbus: ").parse().unwrap(),
            charger: value("charger: ").parse().unwrap(),
            gauge: value("gauge: ").trim_matches('"').into(),
            sourcing,
            // The old flapping daemon did not log boost. The later live
            // trace measured 0x0b with our boost on; use that for its replay.
            boost: if sourcing { 0x0b } else { 5 },
            ..kishi()
        }
    }

    #[test]
    fn replay_every_live_host_sample_through_the_bounded_shutdown() {
        let mut policy = Policy::default();
        let mut started = false;
        let mut samples = 0;
        for line in include_str!("phase-b-host-transition.txt").lines() {
            let field = |key: &str| {
                line.split_once(key)
                    .unwrap()
                    .1
                    .split_whitespace()
                    .next()
                    .unwrap()
            };
            let now = Duration::from_millis(field("elapsed_ms=").parse().unwrap());
            if let Some((_, host)) = line.split_once("host_ms=Some(") {
                if !started {
                    let ms = host.split(')').next().unwrap().parse().unwrap();
                    policy.host_started(now - Duration::from_millis(ms));
                    started = true;
                }
            }
            let expected = match field("action=") {
                "Host" => Action::Host,
                "Wait" => Action::Wait,
                "Device" => Action::Device,
                other => panic!("unexpected action {other}"),
            };
            assert_eq!(
                policy.update(now, Ok(&logged_sample(line))),
                expected,
                "{line}"
            );
            samples += 1;
        }
        assert_eq!(samples, 51);
        assert!(policy.next_host > SETTLE_WINDOW);
    }

    #[test]
    fn original_live_flap_is_not_a_reason_to_reenable_or_shut_down_early() {
        let mut policy = Policy::default();
        let mut qualified = 0;
        let mut detach_checked = false;
        for line in include_str!("phase-b-flap.txt")
            .lines()
            .filter(|l| l.contains("sample=Ok("))
        {
            let s = logged_sample(line);
            if s.source && s.attached {
                qualified += 1;
                let expected = if qualified == 3 {
                    Action::Host
                } else {
                    Action::Wait
                };
                assert_eq!(policy.update(Duration::ZERO, Ok(&s)), expected);
            } else if qualified == 3 && s.sourcing {
                // The old daemon logged Device for this exact sample and
                // restarted the cycle. The bounded policy must wait instead.
                assert!(line.contains("action=Device"));
                assert_eq!(
                    policy.update(Duration::from_millis(100), Ok(&s)),
                    Action::Wait
                );
                detach_checked = true;
                break;
            }
        }
        assert!(detach_checked);
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
            policy.update(Duration::ZERO, Ok(&s));
            policy.update(Duration::ZERO, Ok(&s));
            let mut bad = kishi();
            match failure {
                0 => bad.muic_vbus = 1,
                1 => bad.charger = 0x21,
                2 => bad.gauge = "Charging".into(),
                3 => bad.sourcing = true,
                _ => (),
            }
            assert_eq!(
                policy.update(
                    Duration::ZERO,
                    if failure == 4 {
                        Err("read failed")
                    } else {
                        Ok(&bad)
                    }
                ),
                Action::Device
            );
            assert_eq!(policy.update(Duration::ZERO, Ok(&s)), Action::Wait);
            assert_eq!(policy.update(Duration::ZERO, Ok(&s)), Action::Wait);
            assert_eq!(policy.update(Duration::ZERO, Ok(&s)), Action::Host);
            assert_eq!(
                policy.update(Duration::ZERO, Err("read failed")),
                Action::Device
            );
        }
    }

    fn phase_a_sample(line: &str) -> Sample {
        let field = |key| {
            line.split_whitespace()
                .find_map(|part| part.strip_prefix(key))
                .unwrap()
        };
        let cntl = u8::from_str_radix(field("chg_mode="), 16).unwrap();
        Sample {
            state: field("state=").parse().unwrap(),
            attached: field("attached=") == "1",
            source: field("source=") == "1",
            dfp: field("dfp=") == "1",
            muic_vbus: u8::from_str_radix(field("muic_vbus="), 16).unwrap(),
            charger: u8::from_str_radix(field("chg_status="), 16).unwrap(),
            gauge: field("gauge=").trim_matches('"').into(),
            sourcing: cntl & 7 == 7,
            cntl,
            ..kishi()
        }
    }

    fn charging_kishi() -> Sample {
        phase_a_sample(
            include_str!("sink-host-charging.txt")
                .lines()
                .next()
                .unwrap(),
        )
    }

    #[test]
    fn replay_charging_kishi_then_unplug_and_pc_preserves_device_lifeline() {
        let mut policy = SinkHostPolicy::default();
        // Phase A includes charging before the data-role swap, including
        // SNK_Ready/UFP and state 42/DFP: neither permits host.
        for line in include_str!("phase-a-transition.txt").lines() {
            assert_eq!(policy.update(Ok(&phase_a_sample(line))), Action::Device);
        }
        for (i, line) in include_str!("sink-host-charging.txt").lines().enumerate() {
            assert_eq!(
                policy.update(Ok(&phase_a_sample(line))),
                if i < 2 {
                    Action::Device
                } else {
                    Action::SinkHost
                }
            );
        }
        let unplugged: Vec<_> = include_str!("phase-a-transition.txt")
            .lines()
            .skip(10)
            .map(phase_a_sample)
            .collect();
        // First detach revokes, including a stale Charging gauge. Continuing
        // Kishi source-ready samples cannot enable boost or host for 60 s.
        for sample in &unplugged {
            assert_eq!(sample.cntl & 7, 5);
            assert_eq!(policy.update(Ok(sample)), Action::Device);
        }
        for _ in 0..200 {
            assert_eq!(policy.update(Ok(unplugged.last().unwrap())), Action::Device);
        }
        // Conventional PC: same sink-ready signature but UFP, as recorded
        // before the data-role swap in phase A. Gadget stays available.
        let pc = phase_a_sample(
            include_str!("phase-a-transition.txt")
                .lines()
                .nth(8)
                .unwrap(),
        );
        for _ in 0..3 {
            assert_eq!(policy.update(Ok(&pc)), Action::Device);
        }
        for expected in [Action::Device, Action::Device, Action::SinkHost] {
            assert_eq!(policy.update(Ok(&charging_kishi())), expected);
        }
    }

    #[test]
    fn sink_host_each_interlock_and_read_error_revokes_and_requalifies() {
        for failure in 0..10 {
            let mut policy = SinkHostPolicy::default();
            let good = charging_kishi();
            for _ in 0..3 {
                policy.update(Ok(&good));
            }
            let mut bad = charging_kishi();
            match failure {
                0 => bad.state = 20,
                1 => bad.attached = false,
                2 => bad.source = true,
                3 => bad.dfp = false,
                4 => bad.muic_vbus = 0,
                5 => bad.muic_vbus = 2,
                6 => bad.gauge = "Discharging".into(),
                7 => bad.cntl = 7,
                8 => bad.sourcing = true,
                _ => (),
            }
            assert_eq!(
                policy.update(if failure == 9 {
                    Err("read failed")
                } else {
                    Ok(&bad)
                }),
                Action::Device
            );
            for expected in [Action::Device, Action::Device, Action::SinkHost] {
                assert_eq!(policy.update(Ok(&good)), expected);
            }
            // A read error part way through qualification resets it too.
            policy.update(Err("read failed"));
            policy.update(Ok(&good));
            policy.update(Ok(&good));
            assert_eq!(policy.update(Err("read failed")), Action::Device);
            assert_eq!(policy.update(Ok(&good)), Action::Device);
            assert_eq!(policy.update(Ok(&good)), Action::Device);
        }
    }

    #[test]
    fn sink_host_accepts_only_sink_ready_attached_dfp() {
        for state in 0..=255 {
            for flags in 0..8 {
                let sample = Sample {
                    state,
                    attached: flags & 1 != 0,
                    source: flags & 2 != 0,
                    dfp: flags & 4 != 0,
                    ..charging_kishi()
                };
                assert_eq!(sample.permits_sink_host(), state == 21 && flags == 5);
            }
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
                ..kishi()
            };
            let now = Duration::from_secs_f64(field("uptime=").parse().unwrap());
            assert_eq!(policy.update(now, Ok(&s)), expected, "{line}");
        }
    }
}
