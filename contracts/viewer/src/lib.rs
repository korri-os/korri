//! LinuxStreamViewer's per-launch socket treaty (ADR 0060).
//!
//! One newline-delimited JSON request at a time. Events may precede a reply.
//! EOF is not a successful mutation. The plugin supplies a private socket path;
//! this treaty assigns no installed path, plugin metadata or version policy.
use korri_surface_treaty::{
    SurfaceGameplayControl, SurfaceGameplayControlValue, SurfaceStreamHealth,
};
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Read, Write};
use typeshare::typeshare;

/// Transport budget, not a stored schema. Reject before allocating unbounded input.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// PluginDisplay's logical output dimensions, also used by ViewerShapeSnapshot.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ViewerDisplay {
    pub width: u32,
    pub height: u32,
}

/// LinuxViewerStatus, with no inferred systemd or PID state.
#[typeshare]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum ViewerStatus {
    Running,
    Backgrounded,
    Stopped,
}

/// ViewerShapeSnapshot, verbatim fields. `ready` means live resize is available.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewerShape {
    pub connection: String,
    pub screen: ViewerDisplay,
    pub encoded: ViewerDisplay,
    pub ready: bool,
}

/// ViewerSnapshot's serializable observations. Instant stays local to korrid.
#[typeshare]
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewerSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connected: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<ViewerShape>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampled_at_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_health: Option<SurfaceStreamHealth>,
    pub controls: Vec<SurfaceGameplayControl>,
}

/// Every variant and argument comes from LinuxStreamViewer's matching method.
/// Focus commands report focus already confirmed by korrid's compositor effect;
/// the viewer does not acquire compositor authority (ADR 0060).
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", content = "value", deny_unknown_fields)]
pub enum ViewerCommand {
    Start {
        address: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        display: Option<ViewerDisplay>,
    },
    Status,
    Background,
    ResumeAndFocus,
    Stop,
    Resize {
        connection: String,
        display: ViewerDisplay,
    },
    Snapshot,
    Apply {
        control: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<SurfaceGameplayControlValue>,
    },
    Notice {
        message: String,
    },
}

/// Exact launch identity is mandatory for every operation, as in the trait.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewerRequest {
    pub launch_id: String,
    pub command: ViewerCommand,
}

/// Method return values, including LinuxViewerError's message.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", content = "value", deny_unknown_fields)]
pub enum ViewerReply {
    Ok,
    Status(ViewerStatus),
    Snapshot {
        #[serde(skip_serializing_if = "Option::is_none")]
        snapshot: Option<ViewerSnapshot>,
    },
    Error {
        message: String,
    },
}

/// The trait's change_signal observations. Connection/disconnection originate
/// in the same native callbacks as Moonlight patch 0023, not process existence.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", content = "value", deny_unknown_fields)]
pub enum ViewerEvent {
    Status(ViewerStatus),
    Snapshot(ViewerSnapshot),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", content = "value", deny_unknown_fields)]
pub enum ViewerMessage {
    Reply(ViewerReply),
    Event(ViewerEvent),
}

pub fn read_message<T: serde::de::DeserializeOwned>(
    reader: &mut impl BufRead,
) -> io::Result<Option<T>> {
    let mut bytes = Vec::new();
    let length = reader
        .take((MAX_MESSAGE_BYTES + 1) as u64)
        .read_until(b'\n', &mut bytes)?;
    if length == 0 {
        return Ok(None);
    }
    if length > MAX_MESSAGE_BYTES || bytes.last() != Some(&b'\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "oversized or incomplete viewer message",
        ));
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn write_message(writer: &mut impl Write, message: &impl Serialize) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(message).map_err(io::Error::other)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "oversized viewer message",
        ));
    }
    writer.write_all(&bytes)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn rejects_truncated_oversized_and_unknown_fields() {
        for data in [
            b"{}".to_vec(),
            vec![b'x'; MAX_MESSAGE_BYTES + 1],
            b"{\"launchId\":\"a\",\"command\":{\"kind\":\"Stop\"},\"extra\":true}\n".to_vec(),
        ] {
            assert!(read_message::<ViewerRequest>(&mut Cursor::new(data)).is_err());
        }
        assert!(
            read_message::<ViewerRequest>(&mut Cursor::new(Vec::<u8>::new()))
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn commands_and_nested_control_values_round_trip() {
        for command in [
            ViewerCommand::Start {
                address: "127.0.0.1".into(),
                display: None,
            },
            ViewerCommand::Apply {
                control: "fit".into(),
                value: Some(SurfaceGameplayControlValue::Choice {
                    value: "fit".into(),
                }),
            },
        ] {
            let request = ViewerRequest {
                launch_id: "a".into(),
                command,
            };
            let mut bytes = Vec::new();
            write_message(&mut bytes, &request).unwrap();
            let decoded: ViewerRequest = read_message(&mut Cursor::new(&bytes)).unwrap().unwrap();
            assert_eq!(
                serde_json::to_value(decoded).unwrap(),
                serde_json::to_value(request).unwrap()
            );
        }
    }
}
