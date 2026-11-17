// SPDX-License-Identifier: GPL-3.0-or-later
//! #391 step E: nightjar-core's input under a delay on the loopback.
//!
//! The private pair of stream-cadence streams over one loopback, and its
//! `--net 'delay 30ms'` delays every UDP datagram on it. In that set
//! nightjar-core's key presses left 4 to 6 ms later than the C core's
//! (`clients/viewer/proof/nightjar/README.md`, part 4). This test plays the
//! same condition: a network namespace of its own whose loopback delays
//! every packet by 30 ms (netem), nightjar-core's control client, and a
//! plain ENet host that notes when each keyboard packet arrives. Each key
//! must arrive the delay after it was queued, and no later than 3 ms after
//! that.
//!
//! It needs an unprivileged user namespace (or root), `unshare`, `ip` and
//! `tc` on the PATH, and the kernel's netem, so it is ignored by default:
//!
//!     cargo test --features nightjar --test nightjar_input_wake -- --ignored
#![cfg(feature = "nightjar")]

use nightjar_core::control::driver::{DriverConfig, connect};
use nightjar_core::control::negotiation::encryption;
use nightjar_core::control::{HostCapabilities, SessionOptions, channel};
use rusty_enet::{Event, Host, HostSettings};
use std::net::UdpSocket;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

const DELAY: Duration = Duration::from_millis(30);
const CHILD: &str = "KORRI_NIGHTJAR_WAKE_IN_NAMESPACE";
const KEY: [u8; 16] = *b"wake-test-key-16";
/// What a key may take beyond the delay: ENet's send and the host's service.
const ALLOWED_EXTRA: Duration = Duration::from_millis(3);
const PRESSES: usize = 20;

#[test]
#[ignore = "needs a network namespace with netem; see the module's comment"]
fn a_key_leaves_when_it_is_queued_under_a_delay_on_the_loopback() {
    if std::env::var_os(CHILD).is_some() {
        in_the_namespace();
        return;
    }
    let status = Command::new("unshare")
        .args(["--user", "--map-root-user", "--net"])
        .arg(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "a_key_leaves_when_it_is_queued_under_a_delay_on_the_loopback",
            "--ignored",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .status()
        .expect("unshare runs");
    assert!(
        status.success(),
        "the test in its namespace failed: {status}"
    );
}

fn run(program: &str, args: &[&str]) {
    let status = Command::new(program)
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("{program}: {e}"));
    assert!(status.success(), "{program} {args:?}: {status}");
}

fn in_the_namespace() {
    run("ip", &["link", "set", "lo", "up"]);
    run(
        "tc",
        &[
            "qdisc", "add", "dev", "lo", "root", "netem", "delay", "30ms",
        ],
    );

    // A plain ENet host on the loopback, as Sunshine's control stream: it
    // notes when each packet on the keyboard channel arrives.
    let mut host = Host::new(
        UdpSocket::bind("127.0.0.1:0").unwrap(),
        HostSettings {
            peer_limit: 4,
            channel_limit: 255,
            ..Default::default()
        },
    )
    .unwrap();
    let address = host.socket().local_addr().unwrap();
    let (arrived, arrivals) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let host_stop = stop.clone();
    let host_thread = thread::spawn(move || {
        while !host_stop.load(Ordering::Relaxed) {
            while let Ok(Some(event)) = host.service() {
                if let Event::Receive { channel_id, .. } = event
                    && channel_id == channel::KEYBOARD
                {
                    let _ = arrived.send(Instant::now());
                }
            }
            thread::sleep(Duration::from_micros(100));
        }
    });

    let caps = HostCapabilities {
        app_version: [7, 1, 431, -1],
        feature_flags: 0,
        encryption_enabled: encryption::CONTROL_V2,
        control_messages: Vec::new(),
        client_roaming: false,
    };
    let (client, handle) = connect(
        DriverConfig::new(address),
        caps,
        &KEY,
        SessionOptions::default(),
    )
    .expect("the control stream connects");
    let client_thread = thread::spawn(move || client.run(|_| {}));
    // The session's own start messages and first pings settle.
    thread::sleep(Duration::from_secs(1));

    let mut extras = Vec::new();
    for press in 0..PRESSES {
        // Presses at no fixed phase of the control thread's timers.
        thread::sleep(Duration::from_millis(150 + (press as u64 * 37) % 90));
        while arrivals.try_recv().is_ok() {}
        let queued = Instant::now();
        handle
            .with_input(|queue| queue.send_keyboard(0x41, press % 2 == 0, 0, 0))
            .expect("the key is queued");
        let at = arrivals
            .recv_timeout(Duration::from_secs(2))
            .expect("the key reaches the host");
        extras.push((at - queued).saturating_sub(DELAY));
    }
    handle.stop();
    let _ = client_thread.join();
    stop.store(true, Ordering::Relaxed);
    host_thread.join().unwrap();

    let mut sorted = extras.clone();
    sorted.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    println!(
        "time beyond the 30 ms delay, ms: median {:.2}, max {:.2}, each {:?}",
        ms(sorted[PRESSES / 2]),
        ms(sorted[PRESSES - 1]),
        extras
            .iter()
            .map(|d| (ms(*d) * 100.0).round() / 100.0)
            .collect::<Vec<_>>()
    );
    assert!(
        sorted[PRESSES - 1] <= ALLOWED_EXTRA,
        "a key waited {:.2} ms beyond the delay (median {:.2} ms)",
        ms(sorted[PRESSES - 1]),
        ms(sorted[PRESSES / 2])
    );
}
