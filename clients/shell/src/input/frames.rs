//! Complete live native samples cross the UI boundary as one job. Baselines
//! have completion controls instead of SYN, and remain silent in the mapper.
use super::native::NativeEvent;

use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Frames {
    pending: BTreeMap<String, Vec<NativeEvent>>,
}

impl Frames {
    pub(super) fn push(&mut self, event: NativeEvent) -> Result<Option<Vec<NativeEvent>>, ()> {
        match &event {
            NativeEvent::Disconnected => self.pending.clear(),
            NativeEvent::DeviceAdded { id, .. } => {
                if !self.pending.contains_key(id)
                    && self.pending.len() >= super::mapper::MAX_DEVICES
                {
                    return Err(());
                }
                self.pending.insert(id.clone(), Vec::new());
            }
            NativeEvent::DeviceRemoved(id) => {
                self.pending.remove(id);
            }
            NativeEvent::Initialized => {
                // Initial snapshots have no SYN; the global completion commits
                // all their baselines silently in the mapper in the same job.
                let mut batch: Vec<_> =
                    self.pending.values_mut().flat_map(std::mem::take).collect();
                batch.push(event);
                return Ok(Some(batch));
            }
            NativeEvent::DeviceInitialized(id) => {
                let pending = self.pending.get_mut(id).ok_or(())?;
                let mut batch = std::mem::take(pending);
                batch.push(event);
                return Ok(Some(batch));
            }
            NativeEvent::Input {
                device,
                kind,
                code,
                value,
                ..
            } => {
                let pending = self.pending.get_mut(device).ok_or(())?;
                if (*kind, *code, *value) == (0, 0, 0) {
                    let mut batch = std::mem::take(pending);
                    batch.push(event);
                    return Ok(Some(batch));
                }
                // Linux's 768 keys + 64 axes, as bounded by publish_frame.
                // Malformed producers cannot grow an unfinished frame forever.
                if pending.len() >= 832 {
                    return Err(());
                }
                pending.push(event);
                return Ok(None);
            }
        }
        Ok(Some(vec![event]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{native::NativeEvent, Press, Router};
    use crate::messages::Button;

    fn added(id: &str) -> NativeEvent {
        NativeEvent::DeviceAdded {
            id: id.into(),
            gamepad: true,
            axes: vec![],
        }
    }
    fn input(id: &str, kind: u32, code: u16, value: i64) -> NativeEvent {
        NativeEvent::Input {
            device: id.into(),
            gamepad: true,
            kind,
            code,
            value,
        }
    }
    fn ready() -> Frames {
        let mut frames = Frames::default();
        frames.push(added("pad")).unwrap();
        frames.push(NativeEvent::Initialized).unwrap();
        frames
    }

    #[test]
    fn live_fields_and_syn_are_one_ui_job_with_correct_semantics() {
        let mut frames = ready();
        let mut router = Router::new(true);
        router.native(added("pad"), 0.0);
        router.native(NativeEvent::Initialized, 0.0);
        let axis = input("pad", 3, 16, 1);
        let key = input("pad", 1, 0x130, 1);
        let syn = input("pad", 0, 0, 0);
        assert_eq!(frames.push(axis.clone()).unwrap(), None);
        assert_eq!(frames.push(key.clone()).unwrap(), None);
        let batch = frames.push(syn.clone()).unwrap().unwrap();
        assert_eq!(batch, vec![axis, key, syn]);
        let presses: Vec<_> = batch
            .into_iter()
            .flat_map(|event| router.native(event, 1.0))
            .collect();
        assert_eq!(
            presses,
            vec![Press::Button(Button::Right), Press::Button(Button::Confirm)]
        );
        assert_eq!(frames.push(input("pad", 3, 16, 0)).unwrap(), None);
        assert_eq!(
            frames.push(input("pad", 0, 0, 0)).unwrap().unwrap().len(),
            2
        );
    }

    #[test]
    fn baselines_and_lifecycle_remain_ordered_and_silent() {
        let mut frames = Frames::default();
        frames.push(added("pad")).unwrap();
        let held = input("pad", 1, 0x130, 1);
        assert_eq!(frames.push(held.clone()).unwrap(), None);
        assert_eq!(
            frames.push(NativeEvent::Initialized).unwrap(),
            Some(vec![held, NativeEvent::Initialized])
        );
        assert!(frames.push(input("pad", 3, 16, 1)).unwrap().is_none());
        frames
            .push(NativeEvent::DeviceRemoved("pad".into()))
            .unwrap();
        frames.push(added("pad")).unwrap();
        let baseline = input("pad", 3, 16, 0);
        assert_eq!(frames.push(baseline.clone()).unwrap(), None);
        assert_eq!(
            frames
                .push(NativeEvent::DeviceInitialized("pad".into()))
                .unwrap(),
            Some(vec![baseline, NativeEvent::DeviceInitialized("pad".into())])
        );
        assert_eq!(
            frames.push(input("pad", 0, 0, 0)).unwrap().unwrap().len(),
            1
        );
        frames.push(input("pad", 1, 0x130, 1)).unwrap();
        frames.push(NativeEvent::Disconnected).unwrap();
        frames.push(added("pad")).unwrap();
        frames.push(NativeEvent::Initialized).unwrap();
        assert_eq!(
            frames.push(input("pad", 0, 0, 0)).unwrap().unwrap().len(),
            1
        );
    }

    #[test]
    fn interleaved_devices_never_share_a_frame() {
        let mut frames = ready();
        frames.push(added("other")).unwrap();
        frames
            .push(NativeEvent::DeviceInitialized("other".into()))
            .unwrap();
        let pad = input("pad", 3, 16, 1);
        let other = input("other", 3, 17, -1);
        assert!(frames.push(pad.clone()).unwrap().is_none());
        assert!(frames.push(other.clone()).unwrap().is_none());
        let syn = input("other", 0, 0, 0);
        assert_eq!(frames.push(syn.clone()).unwrap(), Some(vec![other, syn]));
        let syn = input("pad", 0, 0, 0);
        assert_eq!(frames.push(syn.clone()).unwrap(), Some(vec![pad, syn]));
    }

    #[test]
    fn known_device_count_is_bounded() {
        let mut frames = Frames::default();
        for n in 0..super::super::mapper::MAX_DEVICES {
            frames.push(added(&n.to_string())).unwrap();
        }
        assert!(frames.push(added("over-limit")).is_err());
    }

    #[test]
    fn incomplete_frames_and_unknown_devices_are_bounded() {
        let mut frames = ready();
        assert!(frames.push(input("unknown", 3, 16, 1)).is_err());
        for _ in 0..832 {
            assert!(frames.push(input("pad", 3, 16, 1)).unwrap().is_none());
        }
        assert!(frames.push(input("pad", 3, 16, 1)).is_err());
    }
}
