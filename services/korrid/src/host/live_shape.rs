//! Ephemeral viewer ordering for one exact launch. Authentication and native
//! session admission precede this module; it never selects a launch by itself.
use super::shape_owner::{Connection, ShapeOwners};
use crate::launcher::plugin_launch::PluginDisplay;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Pins native hook authority to the exact launched plan, as AfterHooks does.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DisplayChangeContext {
    pub boot_id: String,
    pub command: Vec<String>,
    pub environment: std::collections::BTreeMap<String, String>,
    pub sandbox: super::systemd_unit::LaunchSandbox,
    pub programs: Vec<String>,
    #[serde(default)]
    pub portrait: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<super::live_shape_effect::Snapshot>,
}

struct Viewer {
    instance: String,
    connection: Connection,
    screen: PluginDisplay,
    seen: Instant,
}

#[derive(Clone)]
pub(super) struct ShapeAttempt {
    pub revision: String,
    pub peer: String,
    pub connection: String,
    pub previous: super::live_shape_effect::Snapshot,
    pub target: PluginDisplay,
    pub previous_portrait: bool,
    pub deadline: Instant,
}

#[derive(Default)]
pub(super) struct LiveShape {
    pub pending: Option<ShapeAttempt>,
    pub failed_target: Option<(String, String, PluginDisplay)>,
    notice_owner: Option<(String, String)>,
    order: ShapeOwners<String>,
    viewers: HashMap<String, Viewer>,
    retired: HashSet<(String, String)>,
    pub portrait: bool,
}

impl LiveShape {
    pub(super) fn queue_notice(&mut self) {
        self.notice_owner = self.owner_identity();
    }
    pub(super) fn take_notice(&mut self, peer: &str, connection: &str) -> Option<String> {
        if self.notice_owner != self.owner_identity() {
            self.notice_owner = None;
        }
        if self
            .notice_owner
            .as_ref()
            .is_some_and(|(p, c)| p == peer && c == connection)
        {
            self.notice_owner = None;
            Some("Couldn't reshape".into())
        } else {
            None
        }
    }
    pub(super) fn with_portrait(portrait: bool) -> Self {
        Self {
            portrait,
            ..Default::default()
        }
    }
    /// A repeated report is a heartbeat, not a new connection. The opaque
    /// instance is the viewer's exact connection generation, not its name.
    pub(super) fn report(
        &mut self,
        peer: &str,
        instance: &str,
        screen: PluginDisplay,
        now: Instant,
    ) -> Result<(), String> {
        if instance.is_empty() || instance.len() > 128 {
            return Err("invalid viewer connection identity".into());
        }
        if self.retired.contains(&(peer.into(), instance.into())) {
            return Err("viewer connection has ended".into());
        }
        let screen = super::display_placement::client_shape(&PluginDisplay {
            width: screen.width.max(screen.height),
            height: screen.width.min(screen.height),
        })?;
        if let Some(viewer) = self.viewers.get_mut(peer) {
            if viewer.instance == instance {
                viewer.screen = screen;
                viewer.seen = now;
                return Ok(());
            }
        }
        if let Some(previous) = self.viewers.remove(peer) {
            self.order.disconnect(&peer.to_owned(), previous.connection);
            self.retired.insert((peer.into(), previous.instance));
        }
        let connection = self.order.connect(peer.to_owned());
        self.viewers.insert(
            peer.into(),
            Viewer {
                instance: instance.into(),
                connection,
                screen,
                seen: now,
            },
        );
        Ok(())
    }

    pub(super) fn leave(&mut self, peer: &str, instance: &str) -> bool {
        if !self
            .viewers
            .get(peer)
            .is_some_and(|v| v.instance == instance)
        {
            return false;
        }
        let viewer = self.viewers.remove(peer).expect("checked viewer");
        self.order.disconnect(&peer.to_owned(), viewer.connection);
        self.retired.insert((peer.into(), viewer.instance));
        true
    }

    pub(super) fn expire(&mut self, now: Instant, timeout: Duration) {
        let stale: Vec<_> = self
            .viewers
            .iter()
            .filter(|(_, v)| now.saturating_duration_since(v.seen) >= timeout)
            .map(|(peer, v)| (peer.clone(), v.instance.clone()))
            .collect();
        for (peer, instance) in stale {
            self.leave(&peer, &instance);
        }
    }

    pub(super) fn is_owner(&self, peer: &str, instance: &str) -> bool {
        self.viewers.get(peer).is_some_and(|v| {
            v.instance == instance && self.order.is_owner(&peer.to_owned(), v.connection)
        })
    }

    pub(super) fn portrait(
        &mut self,
        peer: &str,
        instance: &str,
        enabled: bool,
    ) -> Result<(), String> {
        if !self.is_owner(peer, instance) {
            return Err("only the shape owner can change orientation".into());
        }
        self.portrait = enabled;
        Ok(())
    }

    pub(super) fn owner_identity(&self) -> Option<(String, String)> {
        let peer = self.order.owner()?;
        Some((peer.clone(), self.viewers.get(peer)?.instance.clone()))
    }

    pub(super) fn desired(&self) -> Option<PluginDisplay> {
        let screen = &self.viewers.get(self.order.owner()?)?.screen;
        Some(if self.portrait {
            PluginDisplay {
                width: screen.height,
                height: screen.width,
            }
        } else {
            screen.clone()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn screen(w: u32, h: u32) -> PluginDisplay {
        PluginDisplay {
            width: w,
            height: h,
        }
    }
    #[test]
    fn heartbeats_and_rotation_do_not_change_ownership_or_orientation() {
        let mut shape = LiveShape::default();
        let now = Instant::now();
        shape
            .report("phone", "one", screen(2448, 1848), now)
            .unwrap();
        shape.report("tv", "two", screen(1920, 1080), now).unwrap();
        shape
            .report("phone", "one", screen(1848, 2448), now)
            .unwrap();
        assert_eq!(shape.desired(), Some(screen(2448, 1848)));
        assert!(shape.is_owner("phone", "one"));
        assert!(shape.portrait("tv", "two", true).is_err());
    }
    #[test]
    fn portrait_survives_handoff_and_no_viewers_but_not_a_new_session() {
        let mut shape = LiveShape::default();
        let now = Instant::now();
        shape
            .report("phone", "one", screen(2448, 1848), now)
            .unwrap();
        shape.portrait("phone", "one", true).unwrap();
        shape.report("tv", "two", screen(1920, 1080), now).unwrap();
        shape.leave("phone", "one");
        assert_eq!(shape.desired(), Some(screen(1080, 1920)));
        shape.leave("tv", "two");
        assert_eq!(shape.desired(), None);
        shape
            .report("tablet", "three", screen(1600, 1200), now)
            .unwrap();
        assert_eq!(shape.desired(), Some(screen(1200, 1600)));
        assert!(shape
            .report("phone", "one", screen(2448, 1848), now)
            .is_err());
        let mut next = LiveShape::default();
        next.report("tablet", "four", screen(1600, 1200), now)
            .unwrap();
        assert_eq!(next.desired(), Some(screen(1600, 1200)));
    }
    #[test]
    fn failure_notice_waits_for_its_owner_and_is_delivered_once() {
        let mut shape = LiveShape::default();
        let now = Instant::now();
        shape.report("a", "one", screen(1920, 1080), now).unwrap();
        shape.report("b", "two", screen(1280, 720), now).unwrap();
        shape.queue_notice();
        assert!(shape.take_notice("b", "two").is_none());
        assert_eq!(
            shape.take_notice("a", "one").as_deref(),
            Some("Couldn't reshape")
        );
        assert!(shape.take_notice("a", "one").is_none());
        shape.queue_notice();
        shape.leave("a", "one");
        assert!(shape.take_notice("b", "two").is_none());
        shape.queue_notice();
        assert!(shape.take_notice("b", "two").is_some());
    }

    #[test]
    fn loss_expires_only_the_dead_viewer_and_hands_off_to_the_oldest_remaining() {
        let mut shape = LiveShape::default();
        let now = Instant::now();
        shape
            .report("phone", "one", screen(2448, 1848), now)
            .unwrap();
        shape.report("tv", "two", screen(1920, 1080), now).unwrap();
        let later = now + Duration::from_secs(4);
        shape
            .report("tv", "two", screen(1920, 1080), later)
            .unwrap();
        shape.expire(now + Duration::from_secs(5), Duration::from_secs(5));
        assert!(shape.is_owner("tv", "two"));
        assert_eq!(shape.desired(), Some(screen(1920, 1080)));
        assert!(shape
            .report("phone", "one", screen(2448, 1848), later)
            .is_err());
    }
}
