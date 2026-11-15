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
//! start at Artemis's defaults and nothing stores them
//! (`KorriStreamControlState`).
//!
//! The controls that this viewer can perform, of the thirteen that Artemis
//! can list (`KorriViewerControls.included`):
//!
//! | Effect | Label | Interaction |
//! | --- | --- | --- |
//! | `set-fill-mode` | Crop to fill | toggle (`Game.toggleFillMode`: fit or fill) |
//! | `set-face-button-flip` | Flip A/B and X/Y | toggle (`ControllerHandler.handleFlipFaceButtons`) |
//!
//! The others need what this viewer does not have: Artemis's on-screen
//! keyboards and controller, its pan and zoom, its mouse modes, its GL
//! renderer (SGSR, the local cursor), picture-in-picture and rumble
//! (`clients/viewer/README.md`, slice D11).
use anyhow::{Result, bail};
use korri_viewer_treaty::{
    SurfaceGameplayControl, SurfaceGameplayControlInteraction as Interaction,
    SurfaceGameplayControlValue as Value, SurfaceStreamHealth,
};

/// One control: an effect of Artemis's executor (`Effect`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    /// `SET_FILL_MODE`: crop the picture to fill the window, or fit it.
    Fill,
    /// `SET_FACE_BUTTON_FLIP`: a controller's A is B and X is Y, and back.
    FlipFaceButtons,
}

impl Control {
    /// In Artemis's order (`Effect.values()`).
    pub const ALL: [Control; 2] = [Control::Fill, Control::FlipFaceButtons];

    /// The effect's wire name (`Effect.wire()`).
    pub fn wire(self) -> &'static str {
        match self {
            Control::Fill => "set-fill-mode",
            Control::FlipFaceButtons => "set-face-button-flip",
        }
    }

    /// `KorriViewerControls.label`.
    pub fn label(self) -> &'static str {
        match self {
            Control::Fill => "Crop to fill",
            Control::FlipFaceButtons => "Flip A/B and X/Y",
        }
    }

    /// `KorriViewerControls.dismiss`: neither closes the overlay.
    pub fn dismiss(self) -> bool {
        false
    }
}

/// The stream's control values. The defaults are Artemis's: the picture
/// fits (`ScaleMode.FIT`), and the face buttons are not flipped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Values {
    pub fill: bool,
    pub flip_face_buttons: bool,
}

impl Values {
    pub fn get(self, control: Control) -> bool {
        match control {
            Control::Fill => self.fill,
            Control::FlipFaceButtons => self.flip_face_buttons,
        }
    }

    /// These values after `change`.
    pub fn with(mut self, change: Change) -> Self {
        match change.control {
            Control::Fill => self.fill = change.value,
            Control::FlipFaceButtons => self.flip_face_buttons = change.value,
        }
        self
    }
}

/// A change that `Apply` asked for and `parse` accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Change {
    pub control: Control,
    pub value: bool,
}

/// The ids' common part (`KorriViewerControls.prefix`): the launch, the
/// runtime's generation (each start of the launch) and the lease (the
/// activity that shows it). A control of another runtime or activity does
/// not match.
pub fn prefix(launch: &str, generation: u64, lease: u64) -> String {
    format!("viewer:{launch}:{generation}:{lease}:")
}

/// The controls with their values, as `KorriViewerControls.snapshot` lists
/// them.
pub fn list(prefix: &str, values: Values) -> Vec<SurfaceGameplayControl> {
    Control::ALL
        .into_iter()
        .map(|control| SurfaceGameplayControl {
            id: format!("{prefix}{}", control.wire()),
            label: control.label().into(),
            description: None,
            enabled: true,
            disabled_reason: None,
            destructive: false,
            dismiss_on_success: control.dismiss(),
            // `KorriViewerControls.toggle`.
            interaction: Interaction::Toggle {
                value: values.get(control),
                true_label: "On".into(),
                false_label: "Off".into(),
            },
        })
        .collect()
}

/// The change that `Apply` asks for, checked as `KorriViewerControls.apply`
/// checks it: this prefix, a known effect, and a value of the effect's kind.
pub fn parse(prefix: &str, id: &str, value: Option<&Value>) -> Result<Change> {
    let Some(wire) = id.strip_prefix(prefix) else {
        bail!("Unknown or stale stream control");
    };
    let Some(control) = Control::ALL.into_iter().find(|c| c.wire() == wire) else {
        bail!("Unknown or stale stream control");
    };
    let value = match value {
        Some(Value::Toggle { value }) => *value,
        _ => bail!("{} takes a toggle", control.label()),
    };
    Ok(Change { control, value })
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

    #[test]
    fn ids_are_artemis_prefix_and_wire_name() {
        assert_eq!(
            prefix(LAUNCH, 3, 7),
            "viewer:0123456789abcdef0123456789abcdef:3:7:"
        );
    }

    #[test]
    fn the_list_is_artemis_json_for_each_control_with_its_value() {
        let prefix = prefix(LAUNCH, 1, 1);
        let listed = list(
            &prefix,
            Values {
                fill: true,
                flip_face_buttons: false,
            },
        );
        // KorriViewerControls.snapshot's object for each, in its key order.
        assert_eq!(
            json(&listed),
            format!(
                concat!(
                    r#"[{{"id":"{0}set-fill-mode","label":"Crop to fill","enabled":true,"#,
                    r#""destructive":false,"dismissOnSuccess":false,"#,
                    r#""interaction":{{"kind":"toggle","value":true,"trueLabel":"On","falseLabel":"Off"}}}},"#,
                    r#"{{"id":"{0}set-face-button-flip","label":"Flip A/B and X/Y","enabled":true,"#,
                    r#""destructive":false,"dismissOnSuccess":false,"#,
                    r#""interaction":{{"kind":"toggle","value":false,"trueLabel":"On","falseLabel":"Off"}}}}]"#,
                ),
                prefix
            )
        );
    }

    #[test]
    fn a_toggle_of_a_listed_control_is_a_change() {
        let prefix = prefix(LAUNCH, 1, 1);
        for (wire, control) in [
            ("set-fill-mode", Control::Fill),
            ("set-face-button-flip", Control::FlipFaceButtons),
        ] {
            for value in [true, false] {
                assert_eq!(
                    parse(
                        &prefix,
                        &format!("{prefix}{wire}"),
                        Some(&Value::Toggle { value })
                    )
                    .unwrap(),
                    Change { control, value }
                );
            }
        }
        let change = Change {
            control: Control::FlipFaceButtons,
            value: true,
        };
        assert_eq!(
            Values::default().with(change),
            Values {
                fill: false,
                flip_face_buttons: true
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
            assert!(parse(&prefix, &stale, Some(&toggle)).is_err(), "{stale}");
        }
        let id = format!("{prefix}set-fill-mode");
        for wrong in [
            None,
            Some(Value::Choice { value: "1".into() }),
            Some(Value::Range { value: 1.0 }),
        ] {
            assert!(parse(&prefix, &id, wrong.as_ref()).is_err(), "{wrong:?}");
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
