//! The exact native mode and each hook's own geometry form the rollback
//! snapshot. A game can render at a different size from its capture output.
use super::{
    compositor_focus::CompositorControl,
    display_placement::{Display, Placement},
};
use crate::launcher::plugin_launch::PluginDisplay;

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub(super) struct Mode {
    pub width: u32,
    pub height: u32,
    pub refresh: u32,
}
impl Mode {
    fn display(&self) -> PluginDisplay {
        PluginDisplay {
            width: self.width,
            height: self.height,
        }
    }
    fn command(&self, name: &str) -> String {
        format!(
            "output {name} mode --custom {}x{}@{}.{:03}Hz",
            self.width,
            self.height,
            self.refresh / 1000,
            self.refresh % 1000
        )
    }
}
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Snapshot {
    pub output: String,
    pub mode: Mode,
    pub hooks: Vec<PluginDisplay>,
}

fn observed(control: &dyn CompositorControl, name: &str) -> Result<Mode, String> {
    #[derive(serde::Deserialize)]
    struct Output {
        name: String,
        current_mode: Option<Mode>,
    }
    let outputs: Vec<Output> = serde_json::from_str(&control.outputs()?)
        .map_err(|_| "invalid native display observation")?;
    let mode = outputs
        .into_iter()
        .find(|o| o.name == name)
        .and_then(|o| o.current_mode)
        .ok_or("display mode unavailable")?;
    if mode.width == 0 || mode.height == 0 || mode.refresh == 0 {
        return Err("invalid native display mode".into());
    }
    Ok(mode)
}
fn apply(control: &dyn CompositorControl, name: &str, mode: &Mode) -> Result<(), String> {
    control.command(&mode.command(name))?;
    let actual = observed(control, name)?;
    if actual.width != mode.width
        || actual.height != mode.height
        || actual.refresh.abs_diff(mode.refresh) > 100
    {
        return Err("compositor did not confirm display mode".into());
    }
    Ok(())
}
pub(super) fn current(
    control: &dyn CompositorControl,
    name: &str,
) -> Result<PluginDisplay, String> {
    observed(control, name).map(|m| m.display())
}

pub(super) fn eligible(displays: &[Display], placement: &Placement) -> Result<String, String> {
    let main = displays
        .iter()
        .find(|d| d.main)
        .ok_or("main display unavailable")?;
    if !main.unwatched
        || placement.global
        || placement.layout.is_some()
        || placement.outputs != [main.name.clone()]
    {
        return Err("this placement cannot reshape live".into());
    }
    Ok(main.name.clone())
}
pub(super) fn capture(
    control: &dyn CompositorControl,
    displays: &[Display],
    placement: &Placement,
    hooks: Vec<PluginDisplay>,
) -> Result<Snapshot, String> {
    let output = eligible(displays, placement)?;
    Ok(Snapshot {
        mode: observed(control, &output)?,
        output,
        hooks,
    })
}
pub(super) fn restore(
    control: &dyn CompositorControl,
    displays: &[Display],
    placement: &Placement,
    snapshot: &Snapshot,
    mut hook: impl FnMut(usize, &PluginDisplay) -> Result<(), String>,
) -> Result<(), String> {
    if eligible(displays, placement)? != snapshot.output {
        return Err("display authority changed".into());
    }
    // Always reapply hook state, even if Sway already has the old size. A
    // previous rollback can have stopped between these two effects.
    let mode_result = apply(control, &snapshot.output, &snapshot.mode);
    let mut hooks_ok = true;
    for (index, previous) in snapshot.hooks.iter().enumerate().rev() {
        hooks_ok &= hook(index, previous).is_ok();
    }
    if mode_result.is_err() || !hooks_ok {
        return Err("previous display state could not be confirmed".into());
    }
    Ok(())
}
#[derive(Debug, thiserror::Error)]
pub(super) enum ShapeFailure {
    #[error("{0}")]
    Refused(String),
    #[error("Couldn't reshape; previous display state could not be confirmed")]
    RecoveryRequired,
}
impl From<String> for ShapeFailure {
    fn from(value: String) -> Self {
        Self::Refused(value)
    }
}
impl From<&str> for ShapeFailure {
    fn from(value: &str) -> Self {
        Self::Refused(value.into())
    }
}

pub(super) fn reshape(
    control: &dyn CompositorControl,
    displays: &[Display],
    placement: &Placement,
    requested: &PluginDisplay,
    previous: &Snapshot,
    mut hook: impl FnMut(usize, &PluginDisplay) -> Result<(), String>,
) -> Result<PluginDisplay, ShapeFailure> {
    if eligible(displays, placement)? != previous.output {
        return Err("display authority changed".into());
    }
    let desired = super::display_placement::client_shape(requested)?;
    let next = Mode {
        width: desired.width,
        height: desired.height,
        refresh: previous.mode.refresh,
    };
    let result = apply(control, &previous.output, &next).and_then(|_| {
        for index in 0..previous.hooks.len() {
            hook(index, &desired)?;
        }
        Ok(())
    });
    if let Err(error) = result {
        return match restore(control, displays, placement, previous, hook) {
            Ok(()) => Err(ShapeFailure::Refused(format!("Couldn't reshape: {error}"))),
            Err(_) => Err(ShapeFailure::RecoveryRequired),
        };
    }
    Ok(desired)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    #[derive(Debug)]
    struct NativeDisplay {
        mode: Mutex<(u32, u32)>,
    }
    impl CompositorControl for NativeDisplay {
        fn outputs(&self) -> Result<String, String> {
            let (w, h) = *self.mode.lock().unwrap();
            Ok(format!(
                r#"[{{"name":"HDMI-1","current_mode":{{"width":{w},"height":{h},"refresh":60000}}}}]"#
            ))
        }
        fn command(&self, c: &str) -> Result<(), String> {
            let m = c
                .strip_prefix("output HDMI-1 mode --custom ")
                .unwrap()
                .split('@')
                .next()
                .unwrap();
            let (w, h) = m.split_once('x').unwrap();
            *self.mode.lock().unwrap() = (w.parse().unwrap(), h.parse().unwrap());
            Ok(())
        }
        fn tree(&self) -> Result<String, String> {
            Err("not used".into())
        }
        fn focus(&self, _: i64) -> Result<(), String> {
            Err("not used".into())
        }
    }
    fn facts() -> (Vec<Display>, Placement) {
        (
            vec![Display {
                name: "HDMI-1".into(),
                main: true,
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
                touch: None,
                unwatched: true,
                mode: "1920x1080@60Hz".into(),
            }],
            Placement {
                layout: None,
                global: false,
                width: 1920,
                height: 1080,
                outputs: vec!["HDMI-1".into()],
            },
        )
    }
    #[test]
    fn hook_failure_restores_each_original_size_not_the_capture_size() {
        let native = NativeDisplay {
            mode: Mutex::new((1600, 1200)),
        };
        let (displays, placement) = facts();
        let old = capture(
            &native,
            &displays,
            &placement,
            vec![PluginDisplay {
                width: 1280,
                height: 720,
            }],
        )
        .unwrap();
        let mut calls = Vec::new();
        assert!(reshape(
            &native,
            &displays,
            &placement,
            &PluginDisplay {
                width: 2448,
                height: 1848
            },
            &old,
            |index, size| {
                calls.push((index, size.clone()));
                if size.width == 2448 {
                    Err("refused".into())
                } else {
                    Ok(())
                }
            }
        )
        .is_err());
        assert_eq!(*native.mode.lock().unwrap(), (1600, 1200));
        assert_eq!(
            calls[1].1,
            PluginDisplay {
                width: 1280,
                height: 720
            }
        );
    }
    #[test]
    fn retry_restores_hooks_even_when_capture_already_has_its_old_mode() {
        let native = NativeDisplay {
            mode: Mutex::new((1920, 1080)),
        };
        let (displays, placement) = facts();
        let old = capture(
            &native,
            &displays,
            &placement,
            vec![PluginDisplay {
                width: 1280,
                height: 720,
            }],
        )
        .unwrap();
        let mut calls = 0;
        restore(&native, &displays, &placement, &old, |_, size| {
            calls += 1;
            assert_eq!(size.width, 1280);
            Ok(())
        })
        .unwrap();
        assert_eq!(calls, 1);
    }
    #[test]
    fn watched_display_is_never_changed() {
        let native = NativeDisplay {
            mode: Mutex::new((1920, 1080)),
        };
        let (mut displays, placement) = facts();
        displays[0].unwatched = false;
        assert!(capture(&native, &displays, &placement, vec![]).is_err());
        assert_eq!(*native.mode.lock().unwrap(), (1920, 1080));
    }
}
