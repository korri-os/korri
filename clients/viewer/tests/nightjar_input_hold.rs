// SPDX-License-Identifier: GPL-3.0-or-later
//! #391 step E: nightjar-core's input queue while ENet has reliable data
//! waiting, against moonlight-common-c's.
//!
//! The Android proof's run 1 (clients/viewer/proof/android-nightjar/,
//! 2026-11-18) dropped 2 of 2060 touch moves for a full queue, which no C
//! core run did. Both queues hold 150 packets (`MAX_QUEUED_INPUT_PACKETS`).
//! The C core's input thread (`InputStream.c`, `inputSendThreadProc`) hands
//! each packet to ENet as it comes and waits only for a reliable one to
//! leave, at most 10 ms (`ControlStream.c`, `sendMessageEnet`); a touch move
//! is unreliable (`LiSendTouchEvent`), so it never waits, and its slot is
//! free again at once. nightjar-core's control thread leaves input in the
//! queue while ENet has reliable data it cannot send yet
//! (`control/driver.rs`, `transmit`, up to `INPUT_HOLD_LIMIT`), so that a
//! newer controller state can replace a queued one.
//!
//! The test fills a plain ENet host's smallest reliable window with keys and
//! lets the host acknowledge nothing, so reliable data waits. Then touch
//! moves come one by one, as Android delivers them, and then a burst that
//! comes faster than the control thread runs. As in the C core, no touch
//! move may stay in the queue, and the burst must find all 150 slots free.
#![cfg(feature = "nightjar")]

use nightjar_core::control::driver::{DriverConfig, connect};
use nightjar_core::control::negotiation::{encryption, feature};
use nightjar_core::control::{HostCapabilities, SessionOptions};
use nightjar_core::input::InputError;
use nightjar_core::input::packets::touch_event;
use rusty_enet::{Host, HostSettings};
use std::net::UdpSocket;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const KEY: [u8; 16] = *b"hold-test-key-16";
/// moonlight-common-c's `MAX_QUEUED_INPUT_PACKETS`.
const SLOTS: usize = 150;

#[test]
fn touch_moves_are_not_held_behind_waiting_reliable_input() {
    // The smallest window that a host grants (4096 bytes), as nightjar-core's
    // own test of the hold makes it (tests/control_loopback.rs).
    let mut host = Host::new(
        UdpSocket::bind("127.0.0.1:0").unwrap(),
        HostSettings {
            peer_limit: 4,
            channel_limit: 255,
            incoming_bandwidth_limit: Some(1),
            ..Default::default()
        },
    )
    .unwrap();
    let address = host.socket().local_addr().unwrap();
    let serving = Arc::new(AtomicBool::new(true));
    let stop = Arc::new(AtomicBool::new(false));
    let (host_serving, host_stop) = (serving.clone(), stop.clone());
    let host_thread = thread::spawn(move || {
        while !host_stop.load(Ordering::Relaxed) {
            if host_serving.load(Ordering::Relaxed) {
                while let Ok(Some(_)) = host.service() {}
            }
            thread::sleep(Duration::from_micros(200));
        }
    });

    let caps = HostCapabilities {
        app_version: [7, 1, 431, -1],
        feature_flags: feature::PEN_TOUCH_EVENTS,
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
    thread::sleep(Duration::from_millis(300));
    let queued = || handle.with_input(|queue| queue.len());
    assert_eq!(queued(), 0, "the start's input left");

    // The host stops acknowledging; keys fill the reliable window until ENet
    // has one it cannot send, and the control thread holds the next.
    serving.store(false, Ordering::Relaxed);
    thread::sleep(Duration::from_millis(20));
    let mut keys = 0;
    let held = loop {
        assert!(keys < 1000, "the window never filled");
        handle
            .with_input(|queue| queue.send_keyboard(0x41, keys % 2 == 0, 0, 0))
            .unwrap();
        keys += 1;
        thread::sleep(Duration::from_millis(2));
        if queued() > 0 {
            thread::sleep(Duration::from_millis(20));
            let held = queued();
            if held > 0 {
                break held;
            }
        }
    };
    assert!(held < 10, "{held} keys held");

    // Touch moves one by one, with time for the control thread between them.
    let mut x = 0.0f32;
    let mut touch = |queue: &mut nightjar_core::input::InputQueue| {
        x += 0.001;
        queue.send_touch(touch_event::MOVE, 0, x, 0.5, 1.0, 0.0, 0.0, 0xffff)
    };
    for _ in 0..60 {
        handle.with_input(&mut touch).unwrap();
        thread::sleep(Duration::from_millis(1));
    }
    thread::sleep(Duration::from_millis(50));
    let after_moves = queued();

    // A burst faster than the control thread: one lock for all of it.
    let burst = 100;
    let dropped = handle.with_input(|queue| {
        (0..burst)
            .filter(|_| matches!(touch(queue), Err(InputError::QueueFull)))
            .count()
    });
    let started = Instant::now();
    while queued() > 0 && started.elapsed() < Duration::from_secs(1) {
        thread::sleep(Duration::from_millis(5));
    }

    handle.stop();
    serving.store(true, Ordering::Relaxed);
    let _ = client_thread.join();
    stop.store(true, Ordering::Relaxed);
    host_thread.join().unwrap();

    println!(
        "{keys} keys filled the window, {held} held; after 60 touch moves {after_moves} \
         packets stayed queued; a burst of {burst} lost {dropped} to a full queue \
         ({SLOTS} slots)"
    );
    assert_eq!(
        after_moves, 0,
        "touch moves stayed in the queue behind waiting reliable input"
    );
    assert_eq!(dropped, 0, "the burst found fewer than {burst} free slots");
}
