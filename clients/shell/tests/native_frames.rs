//! The authenticated native socket must never post a partial sample to UI.
#![cfg(feature = "fixture")]
use korri_shell::{
    fixture::{Behaviour, Fixture},
    input::native::{NativeEvent, NativeInput},
    verify,
};
use serde_json::json;
use std::{sync::mpsc, time::Duration};

#[test]
fn socket_delivers_one_job_per_frame_and_discards_removed_partial_samples() {
    let (model, _) = verify::fixture();
    let fixture = Fixture::start(model, vec![], Behaviour::default()).unwrap();
    let (sender, receiver) = mpsc::channel();
    let _input = NativeInput::start(
        fixture.connection(),
        true,
        Box::new(move |batch| {
            sender.send(batch).unwrap();
        }),
    );
    let next = || {
        receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("native delivery")
    };
    assert_eq!(next(), vec![NativeEvent::Initialized]);
    fixture.send_input(&Fixture::gamepad_frames("pad"));
    assert!(matches!(&next()[..], [NativeEvent::DeviceAdded { id, .. }] if id == "pad"));
    assert_eq!(next(), vec![NativeEvent::DeviceInitialized("pad".into())]);
    let field = |value| json!({"kind":"input","deviceId":"pad","class":"gamepad","type":3,"code":16,"value":value,"timestamp":0});
    let syn = json!({"kind":"input","deviceId":"pad","class":"gamepad","type":0,"code":0,"value":0,"timestamp":0});
    // A later device metadata callback is an ordered reader fence, not a sleep:
    // receiving it proves the earlier field was read without reaching the UI.
    fixture.send_input(&[field(1), Fixture::gamepad_frames("fence")[0].clone()]);
    assert!(matches!(&next()[..], [NativeEvent::DeviceAdded { id, .. }] if id == "fence"));
    assert!(receiver.try_recv().is_err());
    fixture.send_input(std::slice::from_ref(&syn));
    let frame = next();
    assert!(matches!(
        &frame[..],
        [
            NativeEvent::Input {
                kind: 3,
                code: 16,
                value: 1,
                ..
            },
            NativeEvent::Input {
                kind: 0,
                code: 0,
                value: 0,
                ..
            }
        ]
    ));
    fixture.send_input(&[field(-1), json!({"kind":"device-removed","deviceId":"pad"})]);
    assert_eq!(next(), vec![NativeEvent::DeviceRemoved("pad".into())]);
    fixture.send_input(&Fixture::gamepad_frames("pad"));
    assert!(matches!(&next()[..], [NativeEvent::DeviceAdded { .. }]));
    assert_eq!(next(), vec![NativeEvent::DeviceInitialized("pad".into())]);
    fixture.send_input(&[syn]);
    assert!(matches!(
        &next()[..],
        [NativeEvent::Input {
            kind: 0,
            code: 0,
            value: 0,
            ..
        }]
    ));
}

#[test]
fn suspend_and_resume_never_replay_an_incomplete_frame() {
    let (model, _) = verify::fixture();
    let fixture = Fixture::start(model, vec![], Behaviour::default()).unwrap();
    let (sender, receiver) = mpsc::channel();
    let _input = NativeInput::start(
        fixture.connection(),
        true,
        Box::new(move |batch| {
            sender.send(batch).unwrap();
        }),
    );
    let next = || {
        receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("native delivery")
    };
    assert_eq!(next(), vec![NativeEvent::Initialized]);
    fixture.send_input(&Fixture::gamepad_frames("pad"));
    assert!(matches!(&next()[..], [NativeEvent::DeviceAdded { .. }]));
    assert_eq!(next(), vec![NativeEvent::DeviceInitialized("pad".into())]);
    fixture.send_input(&[
        json!({"kind":"input","deviceId":"pad","class":"gamepad","type":3,"code":16,"value":1,"timestamp":0}),
        json!({"kind":"suspend","generation":"1","requestId":"1"}),
    ]);
    assert_eq!(next(), vec![NativeEvent::Disconnected]);
    fixture.send_input(&[json!({"kind":"resume","generation":"1","requestId":"1"})]);
    assert!(matches!(&next()[..], [NativeEvent::DeviceAdded { .. }]));
    assert_eq!(next(), vec![NativeEvent::DeviceInitialized("pad".into())]);
    assert_eq!(next(), vec![NativeEvent::Initialized]);
    fixture.wait(Duration::from_secs(5), |fixture| {
        fixture.input_subscribers() == 1
    });
    fixture.send_input(&[json!({"kind":"input","deviceId":"pad","class":"gamepad","type":0,"code":0,"value":0,"timestamp":0})]);
    assert!(matches!(
        &next()[..],
        [NativeEvent::Input {
            kind: 0,
            code: 0,
            value: 0,
            ..
        }]
    ));
}
