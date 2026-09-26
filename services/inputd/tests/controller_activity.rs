use korri_input_core::controls::{Control, ControlTransition};
use korri_inputd::{activity::ControllerActivity, dbus::SemanticInput};

#[test]
fn every_normalized_button_hat_stick_and_trigger_counts_but_not_sync_or_noise() {
    for code in [
        0x130, 0x131, 0x133, 0x134, 0x136, 0x137, 0x13a, 0x13b, 0x13c, 0x13d, 0x13e, 0x116, 0x72,
        0x73, 0x2c0, 0x2c1, 0x2c2, 0x2c3,
    ] {
        let mut activity = ControllerActivity::default();
        activity.evdev(1, code, 1);
        assert!(activity.take_due(0), "button {code}");
        activity.evdev(1, code, 0);
        assert!(activity.take_due(1000));
        assert!(!activity.take_due(2000));
    }
    for (axis, value) in [
        (0, 20000),
        (1, -20000),
        (3, 20000),
        (4, -20000),
        (2, 200),
        (5, 200),
        (16, -1),
        (17, 1),
    ] {
        let mut activity = ControllerActivity::default();
        activity.evdev(3, axis, value);
        assert!(activity.take_due(0), "axis {axis}");
    }
    let mut activity = ControllerActivity::default();
    for t in 0..10000 {
        activity.evdev(0, 0, 0);
        activity.evdev(3, 0, (t % 1000) - 500);
        activity.evdev(3, 2, t % 8);
        activity.evdev(1, 0x130, 2);
        assert!(!activity.take_due(t as u64));
    }
}

#[test]
fn hysteresis_keeps_threshold_noise_quiet_after_release() {
    let mut activity = ControllerActivity::default();
    activity.evdev(3, 0, 20000);
    assert!(activity.take_due(0));
    activity.evdev(3, 0, 0);
    assert!(activity.take_due(1000));
    for t in 2000..10000 {
        activity.evdev(3, 0, 1200 + t % 2800);
        assert!(!activity.take_due(t as u64));
    }
}

#[test]
fn held_controls_refresh_but_high_rate_events_coalesce_and_reset_clears_holds() {
    let mut activity = ControllerActivity::default();
    let mut notifications = 0;
    for t in 0..100000 {
        activity.evdev(3, 0, 20000 + t % 2000);
        notifications += usize::from(activity.take_due(t as u64 / 10));
    }
    assert_eq!(
        notifications, 10,
        "at most one dispatch per second, not per event"
    );
    assert!(
        activity.take_due(10000),
        "a deliberate held stick keeps navigation awake"
    );
    activity.reset();
    assert!(!activity.take_due(10001));
    activity.evdev(1, 0x130, 1);
    assert!(
        !activity.take_due(10001),
        "hotplug cannot bypass the rate limit"
    );
    assert!(activity.take_due(11000));
}

#[test]
fn partial_gameplay_travel_and_hysteresis_are_distinct_from_portal_navigation() {
    for (code, enter, leave) in [(0, 4096, 3072), (1, 4096, 3072), (2, 8, 4), (5, 8, 4)] {
        let mut activity = ControllerActivity::default();
        activity.evdev(3, code, enter);
        assert!(!activity.take_due(0));
        activity.evdev(3, code, enter + 1);
        assert!(activity.take_due(0));
        activity.evdev(3, code, enter - 1);
        assert!(
            activity.take_due(1000),
            "hysteresis retains a deliberate hold"
        );
        activity.evdev(3, code, leave);
        assert!(activity.take_due(2000));
        assert!(!activity.take_due(3000), "release ends the hold");
        activity.evdev(3, code, enter - 1);
        assert!(!activity.take_due(4000));
        activity.evdev(3, code, i32::MAX);
        assert!(
            !activity.take_due(5000),
            "invalid normalized values are not activity"
        );
    }
}

#[test]
fn semantic_direct_actions_and_home_count_without_evdev() {
    for control in [Control::VolumeUp, Control::VolumeDown, Control::Home] {
        let mut activity = ControllerActivity::default();
        activity.semantic(SemanticInput::Control(control, ControlTransition::Pressed));
        assert!(activity.take_due(0));
        assert!(activity.take_due(1000));
        activity.semantic(SemanticInput::Control(control, ControlTransition::Released));
        assert!(activity.take_due(2000));
        assert!(!activity.take_due(3000));
    }
}
