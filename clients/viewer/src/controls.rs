// SPDX-License-Identifier: GPL-3.0-or-later
//! The stream controls that the viewer reports in its snapshot and takes in
//! `Apply` (#434, slice D11), and the status that goes with them. Pure.
//!
//! Every field is the one that Artemis reports to korrid today
//! (`clients/android`, `KorriViewerControls.java` and
//! `korri/moonlight/KorriMoonlightActionExecutor.java`): a control's id is
//! `viewer:<launch>:<generation>:<lease>:<effect>`, with the effect's wire
//! name; its label, its interaction and whether it dismisses the overlay are
//! Artemis's; it is always enabled and never destructive. A control is
//! listed only when the viewer can perform it, as Artemis lists only the
//! effects that its `Game` supports. The values belong to one stream: they
//! start at Artemis's defaults, live in that runtime's lifecycle while it
//! runs, and are never saved, as Artemis keeps them in its session's
//! `KorriStreamControlState` and not in its preferences.
//!
//! The controls that this viewer can perform, of the thirteen that Artemis
//! can list (`KorriViewerControls.included`):
//!
//! | Effect | Label | Interaction |
//! | --- | --- | --- |
//! | `toggle-keyboard` | Toggle keyboard | command (`Game.toggleKeyboard`); listed only on a device with a touch screen, as Artemis lists its touch controls (`touchOnly`); closes the overlay |
//! | `set-fill-mode` | Crop to fill | toggle (`Game.toggleFillMode`: fit or fill) |
//! | `set-face-button-flip` | Flip A/B and X/Y | toggle (`ControllerHandler.handleFlipFaceButtons`) |
//! | `set-rumble` | Rumble | toggle (`Game.setRumble`), listed only while a controller that can rumble is in the stream (`ControllerHandler.hasRumble`) |
//!
//! The others need what this viewer does not have: Artemis's on-screen
//! full keyboard and controller, its pan and zoom, its mouse modes and touch
//! sensitivity, its pointer capture (the local cursor), its GL renderer
//! (SGSR) and picture-in-picture (`clients/viewer/README.md`, slice D11).
use anyhow::{Result, anyhow, bail};
use korri_viewer_treaty::{
    SurfaceGameplayControl, SurfaceGameplayControlInteraction as Interaction,
    SurfaceGameplayControlValue as Value, SurfaceStreamHealth,
};

/// One control: an effect of Artemis's executor (`Effect`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    /// `TOGGLE_KEYBOARD`: show Android's soft keyboard over the stream, or
    /// hide it.
    ToggleKeyboard,
    /// `SET_FILL_MODE`: crop the picture to fill the window, or fit it.
    Fill,
    /// `SET_FACE_BUTTON_FLIP`: a controller's A is B and X is Y, and back.
    FlipFaceButtons,
    /// `SET_RUMBLE`: the host's rumble reaches the controllers, or not.
    Rumble,
}

impl Control {
    /// In Artemis's order (`Effect.values()`).
    pub const ALL: [Control; 4] = [
        Control::ToggleKeyboard,
        Control::Fill,
        Control::FlipFaceButtons,
        Control::Rumble,
    ];

    /// The effect's wire name (`Effect.wire()`).
    pub fn wire(self) -> &'static str {
        match self {
            Control::ToggleKeyboard => "toggle-keyboard",
            Control::Fill => "set-fill-mode",
            Control::FlipFaceButtons => "set-face-button-flip",
            Control::Rumble => "set-rumble",
        }
    }

    /// `KorriViewerControls.label`.
    pub fn label(self) -> &'static str {
        match self {
            Control::ToggleKeyboard => "Toggle keyboard",
            Control::Fill => "Crop to fill",
            Control::FlipFaceButtons => "Flip A/B and X/Y",
            Control::Rumble => "Rumble",
        }
    }

    /// `KorriViewerControls.dismiss`: Toggle keyboard closes the overlay,
    /// so that the keyboard shows over the stream.
    pub fn dismiss(self) -> bool {
        matches!(self, Control::ToggleKeyboard)
    }

    /// `KorriViewerControls.supported`: whether the stream can perform it
    /// now.
    fn offered(self, offer: Offer) -> bool {
        match self {
            Control::ToggleKeyboard => offer.touch,
            Control::Fill | Control::FlipFaceButtons => true,
            Control::Rumble => offer.rumble,
        }
    }

    /// `KorriViewerControls.interaction`: a toggle with its value, or a
    /// command, which has none.
    fn interaction(self, values: Values) -> Interaction {
        // `KorriViewerControls.toggle`.
        let toggle = |value| Interaction::Toggle {
            value,
            true_label: "On".into(),
            false_label: "Off".into(),
        };
        match self {
            Control::ToggleKeyboard => Interaction::Command,
            Control::Fill => toggle(values.fill),
            Control::FlipFaceButtons => toggle(values.flip_face_buttons),
            Control::Rumble => toggle(values.rumble),
        }
    }

    /// The change that `value` asks for, checked as
    /// `KorriMoonlightActionExecutor.valid` checks it: a command takes no
    /// value, a toggle takes a toggle.
    fn change(self, value: Option<&Value>) -> Result<Change> {
        let toggle = |make: fn(bool) -> Change| match value {
            Some(Value::Toggle { value }) => Ok(make(*value)),
            _ => Err(anyhow!("{} takes a toggle", self.label())),
        };
        match self {
            Control::ToggleKeyboard => match value {
                None => Ok(Change::ToggleKeyboard),
                Some(_) => bail!("{} takes no value", self.label()),
            },
            Control::Fill => toggle(Change::Fill),
            Control::FlipFaceButtons => toggle(Change::FlipFaceButtons),
            Control::Rumble => toggle(Change::Rumble),
        }
    }
}

/// What the stream can perform that depends on its devices: rumble needs a
/// controller that can rumble (`ControllerHandler.hasRumble`), and the touch
/// controls a touch screen (`KorriViewerControls.included`, its
/// `FEATURE_TOUCHSCREEN`). The thread that owns the stream says it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Offer {
    pub rumble: bool,
    pub touch: bool,
}

/// The stream's control values. The defaults are Artemis's: the picture
/// fits (`ScaleMode.FIT`), the face buttons are not flipped, and rumble is
/// on (`checkbox_enable_rumble`). A command has no value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Values {
    pub fill: bool,
    pub flip_face_buttons: bool,
    pub rumble: bool,
}

impl Default for Values {
    fn default() -> Self {
        Self {
            fill: false,
            flip_face_buttons: false,
            rumble: true,
        }
    }
}

impl Values {
    /// These values after `change`.
    pub fn with(mut self, change: Change) -> Self {
        match change {
            Change::Fill(value) => self.fill = value,
            Change::FlipFaceButtons(value) => self.flip_face_buttons = value,
            Change::Rumble(value) => self.rumble = value,
            // The keyboard's visibility belongs to Android's window
            // (`KorriViewerControls.apply` remembers nothing for it).
            Change::ToggleKeyboard => {}
        }
        self
    }
}

/// A change that `Apply` asked for and `parse` accepted: a toggle's new
/// value, or a command to run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    ToggleKeyboard,
    Fill(bool),
    FlipFaceButtons(bool),
    Rumble(bool),
}

impl Change {
    pub fn control(self) -> Control {
        match self {
            Change::ToggleKeyboard => Control::ToggleKeyboard,
            Change::Fill(_) => Control::Fill,
            Change::FlipFaceButtons(_) => Control::FlipFaceButtons,
            Change::Rumble(_) => Control::Rumble,
        }
    }
}

/// The ids' common part (`KorriViewerControls.prefix`): the launch, the
/// runtime's generation (each start of the launch) and the lease (the
/// activity that shows it). A control of another runtime or activity does
/// not match.
pub fn prefix(launch: &str, generation: u64, lease: u64) -> String {
    format!("viewer:{launch}:{generation}:{lease}:")
}

/// The controls that the stream can perform now, with their values, as
/// `KorriViewerControls.snapshot` lists them.
pub fn list(prefix: &str, values: Values, offer: Offer) -> Vec<SurfaceGameplayControl> {
    Control::ALL
        .into_iter()
        .filter(|control| control.offered(offer))
        .map(|control| SurfaceGameplayControl {
            id: format!("{prefix}{}", control.wire()),
            label: control.label().into(),
            description: None,
            enabled: true,
            disabled_reason: None,
            destructive: false,
            dismiss_on_success: control.dismiss(),
            interaction: control.interaction(values),
        })
        .collect()
}

/// The change that `Apply` asks for, checked as `KorriViewerControls.apply`
/// checks it: this prefix, an effect that the stream can perform now, and a
/// value of the effect's kind.
pub fn parse(prefix: &str, id: &str, value: Option<&Value>, offer: Offer) -> Result<Change> {
    let Some(wire) = id.strip_prefix(prefix) else {
        bail!("Unknown or stale stream control");
    };
    let Some(control) = Control::ALL
        .into_iter()
        .find(|c| c.wire() == wire && c.offered(offer))
    else {
        bail!("Unknown or stale stream control");
    };
    control.change(value)
}

/// The stream's status besides its controls, as Artemis's snapshot reads
/// it: the device's battery (`BatteryManager.BATTERY_PROPERTY_CAPACITY`) and
/// the stream's health (`MediaCodecDecoderRenderer.viewerHealth`). Each is
/// the treaty's field of the same name; a reading that is missing is absent,
/// never zero.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Readings {
    pub battery_percent: Option<f64>,
    /// `ViewerVideoStats.Sample.atMs`: when the health was measured, in
    /// CLOCK_MONOTONIC milliseconds.
    pub sampled_at_ms: Option<f64>,
    pub stream_health: Option<SurfaceStreamHealth>,
}

impl Readings {
    /// A battery reading only within 0 to 100, as Artemis reports it.
    pub fn battery(capacity: i32) -> Option<f64> {
        (0..=100).contains(&capacity).then_some(f64::from(capacity))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAUNCH: &str = "0123456789abcdef0123456789abcdef";

    /// The treaty's JSON of the controls, as the viewer writes it.
    fn json(value: &Vec<SurfaceGameplayControl>) -> String {
        let mut bytes = Vec::new();
        korri_viewer_treaty::write_message(&mut bytes, value).unwrap();
        String::from_utf8(bytes).unwrap().trim_end().to_owned()
    }

    /// Each listed control's wire name, in the list's order.
    fn wires(prefix: &str, offer: Offer) -> Vec<String> {
        list(prefix, Values::default(), offer)
            .into_iter()
            .map(|c| c.id.trim_start_matches(prefix).to_owned())
            .collect()
    }

    #[test]
    fn ids_are_artemis_prefix_and_wire_name() {
        assert_eq!(
            prefix(LAUNCH, 3, 7),
            "viewer:0123456789abcdef0123456789abcdef:3:7:"
        );
    }

    const RUMBLE: Offer = Offer {
        rumble: true,
        touch: false,
    };
    const TOUCH: Offer = Offer {
        rumble: false,
        touch: true,
    };

    #[test]
    fn the_list_is_artemis_json_for_each_control_with_its_value() {
        let prefix = prefix(LAUNCH, 1, 1);
        let listed = list(
            &prefix,
            Values {
                fill: true,
                flip_face_buttons: false,
                rumble: true,
            },
            Offer {
                rumble: true,
                touch: true,
            },
        );
        // KorriViewerControls.snapshot's object for each, in its key order,
        // and in Artemis's order (`Effect.values()`): TOGGLE_KEYBOARD comes
        // before SET_FILL_MODE.
        assert_eq!(
            json(&listed),
            format!(
                concat!(
                    r#"[{{"id":"{0}toggle-keyboard","label":"Toggle keyboard","enabled":true,"#,
                    r#""destructive":false,"dismissOnSuccess":true,"#,
                    r#""interaction":{{"kind":"command"}}}},"#,
                    r#"{{"id":"{0}set-fill-mode","label":"Crop to fill","enabled":true,"#,
                    r#""destructive":false,"dismissOnSuccess":false,"#,
                    r#""interaction":{{"kind":"toggle","value":true,"trueLabel":"On","falseLabel":"Off"}}}},"#,
                    r#"{{"id":"{0}set-face-button-flip","label":"Flip A/B and X/Y","enabled":true,"#,
                    r#""destructive":false,"dismissOnSuccess":false,"#,
                    r#""interaction":{{"kind":"toggle","value":false,"trueLabel":"On","falseLabel":"Off"}}}},"#,
                    r#"{{"id":"{0}set-rumble","label":"Rumble","enabled":true,"#,
                    r#""destructive":false,"dismissOnSuccess":false,"#,
                    r#""interaction":{{"kind":"toggle","value":true,"trueLabel":"On","falseLabel":"Off"}}}}]"#,
                ),
                prefix
            )
        );
    }

    // KorriViewerControls.supported: SET_RUMBLE needs viewerHasRumble.
    #[test]
    fn rumble_is_listed_and_taken_only_while_a_controller_can_rumble() {
        let prefix = prefix(LAUNCH, 1, 1);
        assert_eq!(
            wires(&prefix, Offer::default()),
            ["set-fill-mode", "set-face-button-flip"]
        );
        assert_eq!(
            wires(&prefix, RUMBLE),
            ["set-fill-mode", "set-face-button-flip", "set-rumble"]
        );
        let id = format!("{prefix}set-rumble");
        let off = Value::Toggle { value: false };
        assert!(parse(&prefix, &id, Some(&off), Offer::default()).is_err());
        assert_eq!(
            parse(&prefix, &id, Some(&off), RUMBLE).unwrap(),
            Change::Rumble(false)
        );
        // Artemis's default: rumble on.
        assert!(Values::default().rumble);
    }

    // KorriViewerControls.included: TOGGLE_KEYBOARD is one of the effects
    // that Artemis lists only on a device with a touch screen (`touchOnly`).
    #[test]
    fn toggle_keyboard_is_listed_and_taken_only_on_a_touch_screen() {
        let prefix = prefix(LAUNCH, 1, 1);
        assert_eq!(
            wires(&prefix, TOUCH),
            ["toggle-keyboard", "set-fill-mode", "set-face-button-flip"]
        );
        let id = format!("{prefix}toggle-keyboard");
        assert!(parse(&prefix, &id, None, Offer::default()).is_err());
        assert!(parse(&prefix, &id, None, RUMBLE).is_err());
        assert_eq!(
            parse(&prefix, &id, None, TOUCH).unwrap(),
            Change::ToggleKeyboard
        );
    }

    // KorriMoonlightActionExecutor.valid: a COMMAND takes no value, and a
    // TOGGLE takes a toggle.
    #[test]
    fn a_command_takes_no_value_and_a_toggle_takes_a_toggle() {
        let prefix = prefix(LAUNCH, 1, 1);
        let keyboard = format!("{prefix}toggle-keyboard");
        for value in [
            Value::Toggle { value: true },
            Value::Choice { value: "0".into() },
            Value::Range { value: 1.0 },
        ] {
            assert!(
                parse(&prefix, &keyboard, Some(&value), TOUCH).is_err(),
                "{value:?}"
            );
        }
        assert!(parse(&prefix, &format!("{prefix}set-fill-mode"), None, TOUCH).is_err());
    }

    // KorriViewerControls.apply: TOGGLE_KEYBOARD remembers nothing, because
    // the keyboard's visibility belongs to Android's window, not to the
    // stream's values.
    #[test]
    fn toggling_the_keyboard_changes_no_value_and_closes_the_overlay() {
        let changed = Values {
            fill: true,
            flip_face_buttons: true,
            rumble: false,
        };
        assert_eq!(changed.with(Change::ToggleKeyboard), changed);
        assert_eq!(Change::ToggleKeyboard.control(), Control::ToggleKeyboard);
        // KorriViewerControls.dismiss.
        assert!(Control::ToggleKeyboard.dismiss());
        assert!(!Control::Fill.dismiss() && !Control::Rumble.dismiss());
    }

    #[test]
    fn a_toggle_of_a_listed_control_is_a_change() {
        let prefix = prefix(LAUNCH, 1, 1);
        let all = Offer {
            rumble: true,
            touch: true,
        };
        for (wire, change) in [
            ("set-fill-mode", Change::Fill as fn(bool) -> Change),
            ("set-face-button-flip", Change::FlipFaceButtons),
            ("set-rumble", Change::Rumble),
        ] {
            for value in [true, false] {
                assert_eq!(
                    parse(
                        &prefix,
                        &format!("{prefix}{wire}"),
                        Some(&Value::Toggle { value }),
                        all
                    )
                    .unwrap(),
                    change(value)
                );
                assert_eq!(change(value).control().wire(), wire);
            }
        }
        assert_eq!(
            Values::default().with(Change::FlipFaceButtons(true)),
            Values {
                fill: false,
                flip_face_buttons: true,
                rumble: true
            }
        );
    }

    // KorriViewerControlsTest.staleIdentityAndInvalidRangesHaveNoEffect.
    #[test]
    fn another_runtime_activity_effect_or_kind_of_value_is_refused() {
        let prefix = prefix(LAUNCH, 1, 1);
        let toggle = Value::Toggle { value: true };
        for stale in [
            "viewer:other:set-fill-mode".to_owned(),
            format!("{}set-fill-mode", super::prefix(LAUNCH, 2, 1)),
            format!("{}set-fill-mode", super::prefix(LAUNCH, 1, 2)),
            // Artemis lists it, but this viewer cannot perform it.
            format!("{prefix}set-sgsr-sharpness"),
            format!("{prefix}rotate-screen"),
            "set-fill-mode".to_owned(),
        ] {
            assert!(
                parse(&prefix, &stale, Some(&toggle), RUMBLE).is_err(),
                "{stale}"
            );
        }
        // Another runtime's or activity's command.
        for stale in [
            format!("{}toggle-keyboard", super::prefix(LAUNCH, 2, 1)),
            format!("{}toggle-keyboard", super::prefix(LAUNCH, 1, 2)),
        ] {
            assert!(parse(&prefix, &stale, None, TOUCH).is_err(), "{stale}");
        }
        let id = format!("{prefix}set-fill-mode");
        for wrong in [
            None,
            Some(Value::Choice { value: "1".into() }),
            Some(Value::Range { value: 1.0 }),
        ] {
            assert!(
                parse(&prefix, &id, wrong.as_ref(), RUMBLE).is_err(),
                "{wrong:?}"
            );
        }
    }

    #[test]
    fn a_battery_outside_zero_to_a_hundred_is_no_reading() {
        assert_eq!(Readings::battery(53), Some(53.0));
        assert_eq!(Readings::battery(0), Some(0.0));
        assert_eq!(Readings::battery(100), Some(100.0));
        // BatteryManager gives Integer.MIN_VALUE when it cannot tell.
        for absent in [i32::MIN, -1, 101] {
            assert_eq!(Readings::battery(absent), None);
        }
    }
}
