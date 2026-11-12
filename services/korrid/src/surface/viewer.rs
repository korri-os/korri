//! Presentation projection of the exact local viewer, never streamed-session controls.
use std::time::{Duration, Instant};

use korri_surface_treaty::{
    SurfaceGameplayControlGroup, SurfaceGameplayControlInteraction as Interaction,
    SurfaceGameplayControlValue as Value, SurfaceGameplayOverlayPresentation,
};

use crate::linux_viewer::ViewerSnapshot;

pub const STALE_AFTER: Duration = Duration::from_millis(3000);
pub const GROUP_ID: &str = "viewer:stream";

pub fn project(
    presentation: &mut SurfaceGameplayOverlayPresentation,
    snapshot: Option<&(Instant, ViewerSnapshot)>,
    now: Instant,
) {
    presentation.battery_percent = None;
    presentation.stream_health = None;
    presentation.groups.retain(|group| group.id != GROUP_ID);
    let Some((received_at, snapshot)) =
        snapshot.filter(|(at, _)| now.duration_since(*at) <= STALE_AFTER)
    else {
        return;
    };
    presentation.battery_percent = snapshot
        .battery_percent
        .filter(|v| v.is_finite() && (0.0..=100.0).contains(v) && v.fract() == 0.0);
    presentation.stream_health = snapshot
        .stream_health
        .clone()
        .filter(|_| now.duration_since(snapshot.sampled_at.unwrap_or(*received_at)) <= STALE_AFTER)
        .and_then(|mut health| {
            health.rtt_ms = health.rtt_ms.filter(|v| {
                v.is_finite() && (0.0..=u32::MAX as f64).contains(v) && v.fract() == 0.0
            });
            health.delivered_bitrate_kbps = health
                .delivered_bitrate_kbps
                .filter(|v| v.is_finite() && *v >= 0.0);
            health.frames_dropped = health
                .frames_dropped
                .filter(|v| v.is_finite() && *v >= 0.0 && v.fract() == 0.0);
            (health.rtt_ms.is_some()
                || health.delivered_bitrate_kbps.is_some()
                || health.frames_dropped.is_some())
            .then_some(health)
        });
    if !snapshot.controls.is_empty() {
        presentation.groups.push(SurfaceGameplayControlGroup {
            id: GROUP_ID.into(),
            label: "Stream controls".into(),
            controls: snapshot.controls.clone(),
        });
    }
}

pub fn valid(interaction: &Interaction, value: &Option<Value>) -> bool {
    match (interaction, value) {
        (Interaction::Command, None) | (Interaction::Toggle { .. }, Some(Value::Toggle { .. })) => {
            true
        }
        (Interaction::Choice { options, .. }, Some(Value::Choice { value })) => {
            options.iter().any(|o| o.value == *value)
        }
        (Interaction::Range { min, max, step, .. }, Some(Value::Range { value })) => {
            crate::valid_range_value(*value, *min, *max, *step)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use korri_surface_treaty::{SurfaceGameplayOverlayKind, SurfaceStreamHealth};

    #[test]
    fn viewer_projection_clears_expired_and_invalid_measurements() {
        let now = Instant::now();
        let snapshot = (
            now,
            ViewerSnapshot {
                battery_percent: Some(53.0),
                stream_health: Some(SurfaceStreamHealth {
                    rtt_ms: Some(8.0),
                    delivered_bitrate_kbps: Some(789.0),
                    frames_dropped: Some(0.0),
                }),
                controls: vec![serde_json::from_value(serde_json::json!({
                    "id": "viewer:launch:1:1:rotate-screen", "label": "Rotate screen",
                    "enabled": true, "destructive": false, "dismissOnSuccess": true,
                    "interaction": { "kind": "command" }
                }))
                .unwrap()],
                ..Default::default()
            },
        );
        let mut presentation = SurfaceGameplayOverlayPresentation {
            kind: SurfaceGameplayOverlayKind,
            battery_percent: None,
            stream_health: None,
            title: Some("Game".into()),
            controls: Vec::new(),
            groups: Vec::new(),
            stream_players: None,
        };
        project(&mut presentation, Some(&snapshot), now + STALE_AFTER);
        assert_eq!(presentation.battery_percent, Some(53.0));
        assert_eq!(presentation.groups.len(), 1);
        assert_eq!(presentation.groups[0].label, "Stream controls");
        assert_eq!(
            presentation.stream_health.as_ref().unwrap().frames_dropped,
            Some(0.0)
        );
        project(
            &mut presentation,
            Some(&snapshot),
            now + STALE_AFTER + Duration::from_millis(1),
        );
        assert!(presentation.battery_percent.is_none() && presentation.stream_health.is_none());
        assert!(presentation.groups.is_empty());
        assert_eq!(presentation.title.as_deref(), Some("Game"));
        let mut old_sample = snapshot.clone();
        old_sample.1.sampled_at = Some(now - STALE_AFTER - Duration::from_millis(1));
        project(&mut presentation, Some(&old_sample), now);
        assert_eq!(presentation.battery_percent, Some(53.0));
        assert!(
            presentation.stream_health.is_none(),
            "a fresh JNI receipt must not refresh an old measurement"
        );
        let invalid = (
            now,
            ViewerSnapshot {
                battery_percent: Some(-1.0),
                stream_health: Some(SurfaceStreamHealth {
                    rtt_ms: Some(-1.0),
                    delivered_bitrate_kbps: Some(f64::NAN),
                    frames_dropped: Some(0.5),
                }),
                controls: Vec::new(),
                ..Default::default()
            },
        );
        project(&mut presentation, Some(&invalid), now);
        assert!(presentation.battery_percent.is_none() && presentation.stream_health.is_none());
    }
}
