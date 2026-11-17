//! A game's Neural Rendering switches (native `GET /neural` and `POST /neural/settings`),
//! offered while the host lists `neural.frame_generation`:
//! [`Feature::NeuralFrameGeneration`](super::Feature::NeuralFrameGeneration). The pinned native
//! API types predate them, so they live here, read as tolerantly as the host's own.
//!
//! The host downloads and installs on its own after a write; a client reads the state again
//! while the job runs. A device never names a program on the PC: when the host can't tell which
//! is the game's, the switch says to choose it in the console.

use super::native_api::radeon::ControlState;
use serde::{Deserialize, Serialize};

/// The frame generation switch's id.
pub const FRAME_GENERATION: &str = "frame_generation";

/// `GET /neural?app=<id>`, and the answer to a write.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Neural {
    pub app: String,
    /// The one-time notice was accepted on the host; until it is, turning a switch on carries
    /// `accept_notice` once the user accepted it here.
    #[serde(default)]
    pub accepted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu: Option<String>,
    /// When `gpu` is not the GPU the host streams with: that frame generation runs only when the
    /// game renders on it, in the host's words (absent from an older host).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_note: Option<String>,
    #[serde(default)]
    pub switches: Vec<NeuralSwitch>,
    /// Rubylight 3.0's neural upscaling is in the game's folder: turning a switch on replaces it.
    #[serde(default)]
    pub replaces_earlier_upscaling: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<NeuralJob>,
    /// How it is in the game's folder once it is on (absent from an older host).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<NeuralMode>,
    /// The release in the game's folder once it is on (absent from an older host).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NeuralMode {
    /// The host's own package.
    Full,
    /// Added to an upscaler mod the user installed.
    Addon,
    #[serde(other)]
    Unknown,
}

/// Why a switch can't change, as a code the app says in its own words; an older host sends
/// none, and a code this app doesn't know reads as `Unknown`, which shows the host's `reason`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NeuralWhy {
    Busy,
    Streaming,
    NoHostControl,
    Gpu,
    /// The PC lacks the Visual C++ runtime frame generation needs.
    Runtime,
    AntiCheat,
    NoFrameGeneration,
    ChooseProgram,
    Game,
    #[serde(other)]
    Unknown,
}

impl Neural {
    /// The frame generation switch, when the host offers it for the game.
    pub fn frame_generation(&self) -> Option<&NeuralSwitch> {
        self.switches.iter().find(|s| s.id == FRAME_GENERATION)
    }

    /// The host is downloading, installing or removing: read again in a moment.
    pub fn working(&self) -> bool {
        self.job.as_ref().is_some_and(|job| {
            matches!(
                job.phase,
                NeuralPhase::Download | NeuralPhase::Place | NeuralPhase::Remove
            )
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeuralSwitch {
    pub id: String,
    pub state: ControlState,
    #[serde(default)]
    pub on: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<NeuralWhy>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NeuralAction {
    Install,
    Remove,
    Update,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NeuralPhase {
    Download,
    Place,
    Remove,
    Done,
    Failed,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeuralJob {
    pub action: NeuralAction,
    pub phase: NeuralPhase,
    #[serde(default)]
    pub done: u64,
    #[serde(default)]
    pub total: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// `POST /neural/settings`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeuralWrite {
    pub app: String,
    pub switch: String,
    pub on: bool,
    /// The user accepted the notice on this device just now.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub accept_notice: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The host's documented answer (`docs/native-api.md`, "Neural Rendering").
    const DOCUMENTED: &str = r#"{
      "app": "6F1D2C3B-4A59-4E2B-9C1D-2B3A4C5D6E7F",
      "accepted": true,
      "gpu": "AMD Radeon RX 7900 XT",
      "switches": [
        { "id": "frame_generation", "state": "needs_external_setup", "on": false,
          "why": "choose_program",
          "reason": "Choose the game's program in the Rubylight console on the PC." }
      ],
      "replaces_earlier_upscaling": true,
      "job": { "action": "install", "phase": "failed", "done": 0, "total": 0,
               "error": "Rubylight doesn't know where this game is installed." }
    }"#;

    #[test]
    fn the_documented_answer_reads_and_a_newer_one_degrades() {
        let state: Neural = serde_json::from_str(DOCUMENTED).unwrap();
        let switch = state.frame_generation().unwrap();
        assert_eq!(switch.state, ControlState::NeedsExternalSetup);
        assert!(state.replaces_earlier_upscaling && !state.working());
        let newer: Neural = serde_json::from_str(
            r#"{"app":"a","switches":[{"id":"upscaling","state":"later"}],
                "job":{"action":"tune","phase":"download","done":1,"total":2},"extra":1}"#,
        )
        .unwrap();
        assert!(newer.frame_generation().is_none());
        assert_eq!(newer.switches[0].state, ControlState::Unknown);
        assert!(newer.working());
        assert_eq!(newer.job.unwrap().action, NeuralAction::Unknown);
    }

    /// The codes, mode and release a newer host sends read, survive the Android bridge's
    /// re-serialisation, and the old answer above (none of them) still reads as before.
    #[test]
    fn the_why_mode_and_release_read_and_round_trip() {
        assert_eq!(
            serde_json::from_str::<Neural>(DOCUMENTED).unwrap().frame_generation().unwrap().why,
            Some(NeuralWhy::ChooseProgram)
        );
        // A host before the codes: no `why`, `mode` or `release`.
        let old: Neural = serde_json::from_str(&DOCUMENTED.replace(r#""why": "choose_program","#, "")).unwrap();
        assert_eq!(old.frame_generation().unwrap().why, None);
        assert_eq!((old.mode, old.release.as_deref()), (None, None));
        let state: Neural = serde_json::from_str(
            r#"{"app":"a","accepted":true,"switches":[{"id":"frame_generation","state":"read_only",
                "on":true,"why":"no_host_control","reason":"Turn on Host control."}],
                "mode":"addon","release":"FG-V.B16"}"#,
        )
        .unwrap();
        assert_eq!(state.frame_generation().unwrap().why, Some(NeuralWhy::NoHostControl));
        assert_eq!(state.mode, Some(NeuralMode::Addon));
        assert_eq!(state.release.as_deref(), Some("FG-V.B16"));
        let again: serde_json::Value = serde_json::to_value(&state).unwrap();
        assert_eq!(again["switches"][0]["why"], "no_host_control");
        assert_eq!((again["mode"].as_str(), again["release"].as_str()), (Some("addon"), Some("FG-V.B16")));
        for (why, read) in [
            ("busy", NeuralWhy::Busy),
            ("streaming", NeuralWhy::Streaming),
            ("gpu", NeuralWhy::Gpu),
            ("runtime", NeuralWhy::Runtime),
            ("anti_cheat", NeuralWhy::AntiCheat),
            ("no_frame_generation", NeuralWhy::NoFrameGeneration),
            ("choose_program", NeuralWhy::ChooseProgram),
            ("game", NeuralWhy::Game),
            ("later", NeuralWhy::Unknown),
        ] {
            assert_eq!(serde_json::from_value::<NeuralWhy>(why.into()).unwrap(), read);
        }
        assert_eq!(serde_json::from_value::<NeuralMode>("later".into()).unwrap(), NeuralMode::Unknown);
    }

    #[test]
    fn a_write_carries_the_notice_only_when_accepted() {
        let write = NeuralWrite {
            app: "a".into(),
            switch: FRAME_GENERATION.into(),
            on: true,
            accept_notice: false,
        };
        assert_eq!(
            serde_json::to_string(&write).unwrap(),
            r#"{"app":"a","switch":"frame_generation","on":true}"#
        );
        let accepted = NeuralWrite {
            accept_notice: true,
            ..write
        };
        assert!(serde_json::to_string(&accepted)
            .unwrap()
            .ends_with(r#""accept_notice":true}"#));
    }
}
