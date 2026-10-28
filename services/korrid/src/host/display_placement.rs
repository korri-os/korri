//! ADR 0039: pure layout choice. Runner ids and emulator settings are opaque.
use crate::plugin::{RunnerRecord, SystemRecord};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Display {
    pub name: String,
    pub main: bool,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Hardware path, not a compositor input identifier.
    pub touch: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub layout: Option<String>,
    pub global: bool,
    pub width: u32,
    pub height: u32,
    pub outputs: Vec<String>,
}

pub fn configured_displays() -> Result<Vec<Display>, String> {
    let Some(value) = std::env::var_os("KORRID_DISPLAYS") else {
        return Ok(Vec::new());
    };
    let displays: Vec<Display> = serde_json::from_str(&value.to_string_lossy())
        .map_err(|e| format!("invalid display facts: {e}"))?;
    if displays.iter().filter(|d| d.main).count() != 1
        || displays.iter().any(|d| {
            d.width == 0
                || d.height == 0
                || d.name.is_empty()
                || !d
                    .name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        })
    {
        return Err("invalid display geometry or main display".into());
    }
    Ok(displays)
}

pub fn choose(
    system: Option<&SystemRecord>,
    runner: &RunnerRecord,
    displays: &[Display],
) -> Result<Placement, String> {
    let main = displays.iter().find(|d| d.main).ok_or("no main display")?;
    let mut plan = Placement {
        layout: None,
        global: false,
        width: main.width,
        height: main.height,
        outputs: vec![main.name.clone()],
    };
    if runner.layouts.is_empty() {
        return Ok(plan);
    }
    let panes = system.and_then(|s| s.panes.as_deref()).unwrap_or(&[]);
    if panes.len() <= 1 {
        return Ok(plan);
    }
    // The first real relation is upper-above-lower. Match the declaration's
    // geometry, never its emulator-specific layout id.
    if panes.len() != 2 {
        return Err("unsupported pane arrangement".into());
    }
    let (id, _) = runner
        .layouts
        .iter()
        .find(|(_, layout)| {
            if layout.panes.len() != 2 {
                return false;
            }
            let Some(upper) = layout.panes.get(&panes[0]) else {
                return false;
            };
            let Some(lower) = layout.panes.get(&panes[1]) else {
                return false;
            };
            [
                upper.x.value(),
                upper.y.value(),
                upper.width.value(),
                upper.height.value(),
                lower.x.value(),
                lower.y.value(),
                lower.width.value(),
                lower.height.value(),
            ] == [0.0, 0.0, 1.0, 0.5, 0.0, 0.5, 1.0, 0.5]
        })
        .ok_or("runner has no layout for declared pane arrangement")?;
    plan.layout = Some(id.clone());
    if displays.len() == 2
        && system.and_then(|s| s.arrangement.as_deref()) == Some("upper-above-lower")
    {
        let other = displays.iter().find(|d| !d.main).unwrap();
        if main.x == other.x
            && i64::from(main.y) + i64::from(main.height) == i64::from(other.y)
            && main.width == other.width
            && main.height == other.height
        {
            plan.global = true;
            plan.height = main
                .height
                .checked_add(other.height)
                .ok_or("display extent overflow")?;
            plan.outputs.push(other.name.clone());
        }
    }
    Ok(plan)
}

pub fn overlay_display<'a>(displays: &'a [Display], placement: &Placement) -> Option<&'a Display> {
    displays
        .iter()
        .find(|d| !placement.outputs.contains(&d.name))
        .or_else(|| displays.iter().find(|d| d.main))
}

/// Every output power transition uses the same content assignment.
pub(crate) fn power_commands(displays: &[Display], occupied: &[String]) -> Vec<String> {
    displays
        .iter()
        .map(|d| {
            format!(
                "output {} power {}",
                d.name,
                if occupied.contains(&d.name) {
                    "on"
                } else {
                    "off"
                }
            )
        })
        .collect()
}

pub(crate) fn game_commands(
    displays: &[Display],
    placement: &Placement,
    node: i64,
    focus: bool,
) -> Result<Vec<String>, String> {
    let main = displays.iter().find(|d| d.main).ok_or("no main display")?;
    let power = power_commands(displays, &placement.outputs);
    let mut commands: Vec<_> = power
        .iter()
        .filter(|command| !command.ends_with(" off"))
        .cloned()
        .collect();
    commands.push(format!("[con_id={node}] fullscreen disable"));
    commands.push(format!("[con_id={node}] floating disable"));
    commands.push(format!(
        "[con_id={node}] move container to output {}",
        main.name
    ));
    commands.push(format!(
        "[con_id={node}] fullscreen enable{}",
        if placement.global { " global" } else { "" }
    ));
    // Placement is a handoff: focus the exact game only after its output and
    // fullscreen recipe, before powering off unused displays.
    commands.push(format!("[con_id={node}] opacity 1"));
    if focus {
        commands.push(format!("[con_id={node}] focus"));
    }
    // Focus first. DRM disable is a blocking tail of this same IPC batch,
    // never a prerequisite for the game receiving activation.
    commands.extend(
        power
            .into_iter()
            .filter(|command| command.ends_with(" off")),
    );
    Ok(commands)
}

pub(crate) fn apply(
    control: &dyn super::compositor_focus::CompositorControl,
    commands: &[String],
) -> Result<(), String> {
    for command in commands {
        control.command(command)?;
    }
    Ok(())
}

/// A launched process may not have mapped a window yet. That is progress,
/// not a compositor failure, and must remain retryable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GamePlacementStatus {
    AwaitingWindow,
    Placed(i64),
}

pub(crate) fn place_game(
    control: &dyn super::compositor_focus::CompositorControl,
    displays: &[Display],
    placement: &Placement,
    pids: &std::collections::BTreeSet<i32>,
    excluded: &std::collections::BTreeSet<i32>,
    focus: bool,
) -> Result<GamePlacementStatus, String> {
    use super::compositor_focus::{select_focus_target, FocusTarget};
    let tree = control.tree()?;
    let candidate = match select_focus_target(&tree, pids, excluded) {
        FocusTarget::Window(candidate) => candidate,
        FocusTarget::NoWindow => return Ok(GamePlacementStatus::AwaitingWindow),
        FocusTarget::Ambiguous(_) => return Err("several windows found for exact launch".into()),
        FocusTarget::Unreadable(reason) => return Err(reason),
    };
    if let Err(reason) =
        control.command(&game_commands(displays, placement, candidate.node_id, focus)?.join("; "))
    {
        // Same runner layout, restored to the main display. Never choose a
        // different layout because an effect failed.
        let main = displays.iter().find(|d| d.main).ok_or("no main display")?;
        let fallback = Placement {
            layout: placement.layout.clone(),
            global: false,
            width: main.width,
            height: main.height,
            outputs: vec![main.name.clone()],
        };
        let _ = control
            .command(&game_commands(displays, &fallback, candidate.node_id, focus)?.join("; "));
        return Err(reason);
    }
    Ok(GamePlacementStatus::Placed(candidate.node_id))
}

#[typeshare::typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayWakeRequest {}

#[typeshare::typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum DisplayWakeOutcome {
    Ok,
    Err(crate::RpcFailure),
}

#[typeshare::typeshare]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SurfaceDisplayScope {
    Main,
    All,
}

#[typeshare::typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurfaceDisplaysRequest {
    pub displays: SurfaceDisplayScope,
    pub overlay: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_id: Option<String>,
}

#[typeshare::typeshare]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SurfaceDisplay {
    pub id: String,
    pub main: bool,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub touch: bool,
}

#[typeshare::typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SurfaceDisplaysOutcome {
    Ok(Vec<SurfaceDisplay>),
    Err(crate::RpcFailure),
}

pub(crate) fn surface_rectangles(
    displays: &[Display],
    game: Option<&Placement>,
) -> Vec<SurfaceDisplay> {
    let selected: Vec<&Display> = if let Some(game) = game {
        overlay_display(displays, game).into_iter().collect()
    } else {
        displays.iter().filter(|d| d.main).collect()
    };
    let x = selected.iter().map(|d| d.x).min().unwrap_or(0);
    let y = selected.iter().map(|d| d.y).min().unwrap_or(0);
    selected
        .into_iter()
        .map(|d| SurfaceDisplay {
            id: d.name.clone(),
            main: d.main,
            x: d.x - x,
            y: d.y - y,
            width: d.width,
            height: d.height,
            touch: d.touch.is_some(),
        })
        .collect()
}

pub(crate) fn place_surface(
    control: &dyn super::compositor_focus::CompositorControl,
    displays: &[Display],
    game: Option<&Placement>,
    pids: &std::collections::BTreeSet<i32>,
    portal_pids: &std::collections::BTreeSet<i32>,
) -> Result<Vec<SurfaceDisplay>, String> {
    let started = std::time::Instant::now();
    let mut timings = SurfacePlacementTimings::default();
    let result = place_surface_timed(control, displays, game, pids, portal_pids, &mut timings);
    if displays.len() > 1 && game.is_none() {
        let outcome = if result.is_ok() { "Changed" } else { "Failed" };
        eprintln!("<6>korrid: PlacementTransition {{ target: Catalog, outcome: {outcome}, total_ms: {}, phases_ms: {{\"portal_tree\": {}, \"portal_power\": {}, \"portal_geometry\": {}}} }}", started.elapsed().as_millis(), timings.tree_ms, timings.power_ms, timings.geometry_ms);
    }
    result
}

#[derive(Default)]
pub(crate) struct SurfacePlacementTimings {
    pub tree_ms: u128,
    pub power_ms: u128,
    pub geometry_ms: u128,
}

pub(crate) fn place_surface_timed(
    control: &dyn super::compositor_focus::CompositorControl,
    displays: &[Display],
    game: Option<&Placement>,
    pids: &std::collections::BTreeSet<i32>,
    portal_pids: &std::collections::BTreeSet<i32>,
    timings: &mut SurfacePlacementTimings,
) -> Result<Vec<SurfaceDisplay>, String> {
    use super::compositor_focus::{select_focus_target, FocusTarget};
    let rectangles = surface_rectangles(displays, game);
    if displays.len() <= 1 {
        return Ok(rectangles);
    }
    let start = std::time::Instant::now();
    let tree = control.tree();
    timings.tree_ms = start.elapsed().as_millis();
    let tree = tree?;
    let portal = super::compositor_focus::portal_node(&tree, portal_pids)?;
    let mut occupied = game.map(|p| p.outputs.clone()).unwrap_or_default();
    occupied.extend(rectangles.iter().map(|d| d.id.clone()));
    let mut commands = Vec::new();
    if let Some(game) = game.filter(|p| p.global) {
        let candidate = match select_focus_target(&tree, pids, portal_pids) {
            FocusTarget::Window(c) => c,
            _ => return Err("no unique game window for overlay".into()),
        };
        let main = displays.iter().find(|d| d.main).ok_or("no main display")?;
        // A global fullscreen container hides all other containers. Preserve
        // the same game rectangle while allowing an overlay above its upper pane.
        commands.extend([
            format!("[con_id={}] fullscreen disable", candidate.node_id),
            format!("[con_id={}] floating enable", candidate.node_id),
            format!(
                "[con_id={}] resize set width {} px height {} px",
                candidate.node_id, game.width, game.height
            ),
            format!(
                "[con_id={}] move absolute position {} px {} px",
                candidate.node_id, main.x, main.y
            ),
        ]);
    }
    let first = rectangles.first().ok_or("surface has no display")?;
    let origin = displays
        .iter()
        .find(|d| d.name == first.id)
        .ok_or("display missing")?;
    let power = power_commands(displays, &occupied);
    if game.is_none() {
        // Surfaces never span outputs. Normal fullscreen selects the main
        // output even when the previous portal presentation was a free-display
        // overlay. Focus before the blocking unused-output power-off tail.
        commands.extend([
            format!("[con_id={portal}] fullscreen disable"),
            format!("[con_id={portal}] floating disable"),
            format!("[con_id={portal}] move container to output {}", origin.name),
            format!("[con_id={portal}] fullscreen enable"),
            format!("[con_id={portal}] focus"),
        ]);
        commands.extend(
            power
                .iter()
                .filter(|command| command.ends_with(" off"))
                .cloned(),
        );
    } else {
        commands.extend([
            format!("[con_id={portal}] fullscreen disable"),
            format!("[con_id={portal}] floating enable"),
            format!(
                "[con_id={portal}] resize set width {} px height {} px",
                first.width, first.height
            ),
            format!(
                "[con_id={portal}] move absolute position {} px {} px",
                origin.x, origin.y
            ),
            format!("[con_id={portal}] focus"),
        ]);
    }
    let start = std::time::Instant::now();
    let power = if game.is_none() {
        power
            .into_iter()
            .filter(|command| command.ends_with(" on"))
            .collect::<Vec<_>>()
    } else {
        power
    };
    let result = control.command(&power.join("; "));
    timings.power_ms = start.elapsed().as_millis();
    result?;
    let start = std::time::Instant::now();
    let result = control.command(&commands.join("; "));
    timings.geometry_ms = start.elapsed().as_millis();
    result?;
    Ok(rectangles)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn runner() -> RunnerRecord {
        serde_json::from_value(serde_json::json!({"layouts":{"Top/Bottom":{"panes":{"upper":{"x":0,"y":0,"width":1,"height":0.5},"lower":{"x":0,"y":0.5,"width":1,"height":0.5}}}}})).unwrap()
    }
    fn system() -> SystemRecord {
        serde_json::from_str(r#"{"panes":["upper","lower"],"arrangement":"upper-above-lower"}"#)
            .unwrap()
    }
    fn displays() -> Vec<Display> {
        vec![
            Display {
                name: "DSI-2".into(),
                main: true,
                x: 0,
                y: 0,
                width: 640,
                height: 480,
                touch: None,
            },
            Display {
                name: "DSI-1".into(),
                main: false,
                x: 0,
                y: 480,
                width: 640,
                height: 480,
                touch: None,
            },
        ]
    }
    #[test]
    fn stacked_panes_fill_matching_displays_and_overlay_uses_main() {
        let ds = displays();
        let p = choose(Some(&system()), &runner(), &ds).unwrap();
        assert_eq!(p.layout.as_deref(), Some("Top/Bottom"));
        assert!(p.global);
        assert_eq!((p.width, p.height), (640, 960));
        assert_eq!(p.outputs, vec!["DSI-2", "DSI-1"]);
        assert!(overlay_display(&ds, &p).unwrap().main);
    }
    #[test]
    fn one_display_or_different_arrangement_keeps_panes_on_main() {
        let mut ds = displays();
        ds[1].x = 640;
        ds[1].y = 0;
        for ds in [&ds[..], &ds[..1]] {
            let p = choose(Some(&system()), &runner(), ds).unwrap();
            assert!(!p.global);
            assert_eq!(p.layout.as_deref(), Some("Top/Bottom"));
            assert_eq!(p.height, 480);
        }
    }
    #[test]
    fn no_layouts_is_one_pane_and_overlay_uses_empty_display() {
        let ds = displays();
        let mut r = runner();
        r.layouts.clear();
        let p = choose(Some(&system()), &r, &ds).unwrap();
        assert_eq!(p.layout, None);
        assert_eq!(p.outputs, vec!["DSI-2"]);
        assert_eq!(overlay_display(&ds, &p).unwrap().name, "DSI-1");
    }
    #[test]
    fn missing_layout_is_an_error_not_a_different_choice() {
        let mut r = runner();
        r.layouts
            .get_mut("Top/Bottom")
            .unwrap()
            .panes
            .remove("lower");
        assert!(choose(Some(&system()), &r, &displays()).is_err());
    }
    #[test]
    fn catalog_uses_main_and_overlay_still_uses_the_free_display() {
        let ds = displays();
        assert_eq!(surface_rectangles(&ds, None).len(), 1);
        let all = surface_rectangles(&ds, None);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, "DSI-2");
        assert_eq!(
            (all[0].x, all[0].y, all[0].width, all[0].height),
            (0, 0, 640, 480)
        );
        let mut r = runner();
        r.layouts.clear();
        let single = choose(None, &r, &ds).unwrap();
        let overlay = surface_rectangles(&ds, Some(&single));
        assert_eq!(overlay.len(), 1);
        assert_eq!(overlay[0].id, "DSI-1");
        assert_eq!(overlay[0].y, 0);
    }

    fn sway_program(
        directory: &std::path::Path,
        reply: &str,
    ) -> super::super::compositor_focus::SwaymsgCompositorControl {
        use std::os::unix::fs::PermissionsExt;
        let program = directory.join("swaymsg");
        std::fs::write(directory.join("tree"),r#"{"nodes":[{"id":10,"pid":100,"app_id":"portal"},{"id":20,"pid":200,"app_id":"game"}]}"#).unwrap();
        std::fs::write(directory.join("reply"), reply).unwrap();
        std::fs::write(
            &program,
            format!(
                r#"#!/bin/sh
cd '{}' || exit 1
for argument do command="$argument"; done
case "$command" in get_tree)
 printf 'get_tree\n' >> events
 cat tree
 exit;;
esac
printf '%s\n' "$command" >> events
printf '%s\n' "$command" >> batches
old_ifs="$IFS"
IFS=';'
for part in $command; do
 part="${{part# }}"
 printf '%s\n' "$part" >> commands
 if test -f validate-portal-restore; then
  mode=$(cat portal-fullscreen)
  case "$part" in
   '[con_id=10] fullscreen disable') mode=0;;
   '[con_id=10] floating disable')
    if test "$mode" != 0; then printf '[{{"success":false,"error":"floating changed while fullscreen"}}]'; exit; fi;;
   '[con_id=10] fullscreen enable')
    if test "$mode" != 0; then printf '[{{"success":false,"error":"enable on already-fullscreen con"}}]'; exit; fi
    mode=1;;
  esac
  printf '%s' "$mode" > portal-fullscreen
 fi
done
IFS="$old_ifs"
if test -f reject && case "$command" in *"$(cat reject)"*) true;; *) false;; esac; then
    printf '[{{"success":false,"error":"refused"}}]'
else
    cat reply
fi
"#,
                directory.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        super::super::compositor_focus::SwaymsgCompositorControl::new(
            program,
            directory.join("socket"),
            std::time::Duration::from_secs(1),
        )
        .unwrap()
    }

    #[test]
    fn catalog_moves_the_owned_portal_to_main_then_powers_off_unused_displays() {
        let dir = tempfile::tempdir().unwrap();
        let control = sway_program(dir.path(), r#"[{"success":true}]"#);
        std::fs::write(dir.path().join("validate-portal-restore"), "").unwrap();
        std::fs::write(dir.path().join("portal-fullscreen"), "1").unwrap();
        let facts = place_surface(
            &control,
            &displays(),
            None,
            &Default::default(),
            &[100].into_iter().collect(),
        )
        .unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].id, "DSI-2");
        let batches = std::fs::read_to_string(dir.path().join("batches")).unwrap();
        assert_eq!(batches.lines().count(), 2, "{batches}");
        assert_eq!(batches.lines().next().unwrap(), "output DSI-2 power on");
        assert_eq!(batches.lines().nth(1).unwrap(), "[con_id=10] fullscreen disable; [con_id=10] floating disable; [con_id=10] move container to output DSI-2; [con_id=10] fullscreen enable; [con_id=10] focus; output DSI-1 power off");
        assert!(!batches.contains(" global"), "{batches}");
        assert!(!batches.contains("resize set"), "{batches}");
    }

    #[test]
    fn catalog_handles_irregular_display_arrangements_without_a_bounding_viewport() {
        let dir = tempfile::tempdir().unwrap();
        let control = sway_program(dir.path(), r#"[{"success":true}]"#);
        let mut ds = displays();
        ds[1].x = 640;
        ds[1].y = -200;
        ds.push(Display {
            name: "SIDE".into(),
            main: false,
            x: 1280,
            y: 300,
            width: 1920,
            height: 1080,
            touch: None,
        });
        let facts = place_surface(
            &control,
            &ds,
            None,
            &Default::default(),
            &[100].into_iter().collect(),
        )
        .unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!((facts[0].width, facts[0].height), (640, 480));
        let batches = std::fs::read_to_string(dir.path().join("batches")).unwrap();
        assert!(
            batches.ends_with("output DSI-1 power off; output SIDE power off\n"),
            "{batches}"
        );
        assert!(!batches.contains(" global"), "{batches}");
    }

    #[test]
    fn refused_catalog_move_is_an_error_not_a_mutation_retry() {
        let dir = tempfile::tempdir().unwrap();
        let control = sway_program(dir.path(), r#"[{"success":true}]"#);
        std::fs::write(dir.path().join("reject"), "move container to output DSI-2").unwrap();
        assert!(place_surface(
            &control,
            &displays(),
            None,
            &Default::default(),
            &[100].into_iter().collect()
        )
        .is_err());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("batches"))
                .unwrap()
                .lines()
                .count(),
            2
        );
    }

    #[test]
    fn overlay_geometry_and_focus_share_one_batch_after_output_power() {
        let dir = tempfile::tempdir().unwrap();
        let control = sway_program(dir.path(), r#"[{"success":true}]"#);
        let single = choose(None, &runner(), &displays()).unwrap();
        let mut timings = SurfacePlacementTimings::default();
        place_surface_timed(
            &control,
            &displays(),
            Some(&single),
            &[200].into_iter().collect(),
            &[100].into_iter().collect(),
            &mut timings,
        )
        .unwrap();
        let batches = std::fs::read_to_string(dir.path().join("batches")).unwrap();
        assert_eq!(batches.lines().count(), 2, "{batches}");
        assert!(batches
            .lines()
            .last()
            .unwrap()
            .ends_with("[con_id=10] move absolute position 0 px 480 px; [con_id=10] focus"));
    }

    #[test]
    fn single_display_game_placement_focuses_the_exact_window_in_the_placement_batch() {
        let dir = tempfile::tempdir().unwrap();
        let control = sway_program(dir.path(), r#"[{"success":true}]"#);
        std::fs::write(dir.path().join("tree"), r#"{"nodes":[{"id":10,"pid":100,"app_id":"portal","focused":true,"fullscreen_mode":2},{"id":20,"pid":200,"app_id":"retroarch","focused":false}]}"#).unwrap();
        let single = choose(None, &runner(), &displays()).unwrap();
        place_game(
            &control,
            &displays(),
            &single,
            &[200].into_iter().collect(),
            &[100].into_iter().collect(),
            true,
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(dir.path().join("batches")).unwrap(),
            "output DSI-2 power on; [con_id=20] fullscreen disable; [con_id=20] floating disable; [con_id=20] move container to output DSI-2; [con_id=20] fullscreen enable; [con_id=20] opacity 1; [con_id=20] focus; output DSI-1 power off\n");
    }

    #[test]
    fn real_swaymsg_path_places_game_and_overlay_and_restores_game_power() {
        let dir = tempfile::tempdir().unwrap();
        let control = sway_program(dir.path(), r#"[{"success":true}]"#);
        let ds = displays();
        let p = choose(Some(&system()), &runner(), &ds).unwrap();
        let pids = [200].into_iter().collect();
        place_game(&control, &ds, &p, &pids, &[100].into_iter().collect(), true).unwrap();
        let commands = std::fs::read_to_string(dir.path().join("commands")).unwrap();
        assert!(commands.contains("[con_id=20] fullscreen enable global"));
        assert!(commands.contains("output DSI-1 power on"));
        assert!(commands.ends_with(
            "[con_id=20] fullscreen enable global\n[con_id=20] opacity 1\n[con_id=20] focus\n"
        ));
        std::fs::write(dir.path().join("commands"), "").unwrap();
        place_surface(&control, &ds, Some(&p), &pids, &[100].into_iter().collect()).unwrap();
        let commands = std::fs::read_to_string(dir.path().join("commands")).unwrap();
        assert!(commands.contains("[con_id=20] resize set width 640 px height 960 px"));
        assert!(
            commands.ends_with("[con_id=10] move absolute position 0 px 0 px\n[con_id=10] focus\n")
        );
        // Closing the overlay restores the game's global placement and focus.
        place_game(&control, &ds, &p, &pids, &[100].into_iter().collect(), true).unwrap();
        let batches = std::fs::read_to_string(dir.path().join("batches")).unwrap();
        assert!(batches.lines().last().unwrap().ends_with(
            "[con_id=20] fullscreen enable global; [con_id=20] opacity 1; [con_id=20] focus"
        ));
        let mut r = runner();
        r.layouts.clear();
        let single = choose(None, &r, &ds).unwrap();
        place_surface(
            &control,
            &ds,
            Some(&single),
            &pids,
            &[100].into_iter().collect(),
        )
        .unwrap();
        let commands = std::fs::read_to_string(dir.path().join("commands")).unwrap();
        assert!(commands
            .ends_with("[con_id=10] move absolute position 0 px 480 px\n[con_id=10] focus\n"));
        place_game(
            &control,
            &ds,
            &single,
            &pids,
            &[100].into_iter().collect(),
            true,
        )
        .unwrap();
        let commands = std::fs::read_to_string(dir.path().join("commands")).unwrap();
        assert!(commands.contains("output DSI-1 power off"));
        let batches = std::fs::read_to_string(dir.path().join("batches")).unwrap();
        assert!(batches
            .lines()
            .last()
            .unwrap()
            .ends_with("[con_id=20] fullscreen enable; [con_id=20] opacity 1; [con_id=20] focus; output DSI-1 power off"));
        // End returns from the free-display overlay to the main-only catalog.
        place_surface(
            &control,
            &ds,
            None,
            &std::collections::BTreeSet::new(),
            &[100].into_iter().collect(),
        )
        .unwrap();
        let batches = std::fs::read_to_string(dir.path().join("batches")).unwrap();
        assert!(batches.lines().last().unwrap().ends_with("[con_id=10] move container to output DSI-2; [con_id=10] fullscreen enable; [con_id=10] focus; output DSI-1 power off"), "{batches}");
        let commands = std::fs::read_to_string(dir.path().join("commands")).unwrap();
        assert!(commands.ends_with(
            "[con_id=10] fullscreen enable\n[con_id=10] focus\noutput DSI-1 power off\n"
        ));
    }

    #[test]
    fn surface_placement_refuses_zero_or_multiple_windows_owned_by_the_kiosk_unit() {
        let dir = tempfile::tempdir().unwrap();
        let control = sway_program(dir.path(), r#"[{"success":true}]"#);
        let portal_pids = [2603].into_iter().collect();
        for tree in [
            r#"{"nodes":[{"id":10,"pid":999,"app_id":"chromium-browser"}]}"#,
            r#"{"nodes":[{"id":10,"pid":2603,"app_id":"chrome-_text_html,%3Ctitle%3EKorri%3C_title%3E-Default"},{"id":11,"pid":2603,"app_id":"second-window"}]}"#,
        ] {
            std::fs::write(dir.path().join("tree"), tree).unwrap();
            assert_eq!(
                place_surface(
                    &control,
                    &displays(),
                    None,
                    &std::collections::BTreeSet::new(),
                    &portal_pids
                )
                .unwrap_err(),
                "no unique portal window"
            );
            assert!(!dir.path().join("commands").exists());
        }
    }

    #[test]
    fn unmapped_window_waits_and_command_rejection_recovers_on_main_display() {
        let dir = tempfile::tempdir().unwrap();
        let control = sway_program(dir.path(), r#"[{"success":true}]"#);
        std::fs::write(
            dir.path().join("reject"),
            "[con_id=20] fullscreen enable global",
        )
        .unwrap();
        let ds = displays();
        let p = choose(Some(&system()), &runner(), &ds).unwrap();
        assert_eq!(
            place_game(
                &control,
                &ds,
                &p,
                &[999].into_iter().collect(),
                &[100].into_iter().collect(),
                true
            )
            .unwrap(),
            GamePlacementStatus::AwaitingWindow
        );
        assert!(!dir.path().join("commands").exists());
        assert_eq!(
            place_game(
                &control,
                &ds,
                &p,
                &[200].into_iter().collect(),
                &[100].into_iter().collect(),
                true
            )
            .unwrap_err(),
            "refused"
        );
        let commands = std::fs::read_to_string(dir.path().join("commands")).unwrap();
        assert_eq!(
            commands.lines().collect::<Vec<_>>(),
            vec![
                "output DSI-2 power on",
                "output DSI-1 power on",
                "[con_id=20] fullscreen disable",
                "[con_id=20] floating disable",
                "[con_id=20] move container to output DSI-2",
                "[con_id=20] fullscreen enable global",
                "[con_id=20] opacity 1",
                "[con_id=20] focus",
                "output DSI-2 power on",
                "[con_id=20] fullscreen disable",
                "[con_id=20] floating disable",
                "[con_id=20] move container to output DSI-2",
                "[con_id=20] fullscreen enable",
                "[con_id=20] opacity 1",
                "[con_id=20] focus",
                "output DSI-1 power off",
            ]
        );
    }

    #[test]
    fn parser_rejects_unknown_keys_and_invalid_fractions() {
        assert!(serde_json::from_str::<SystemRecord>(r#"{"unknown":true}"#).is_err());
        assert!(serde_json::from_str::<RunnerRecord>(
            r#"{"layouts":{"x":{"panes":{},"unknown":true}}}"#
        )
        .is_err());
        assert!(serde_json::from_str::<crate::plugin::PaneRect>(
            r#"{"x":0,"y":0,"width":1,"height":1,"unknown":0}"#
        )
        .is_err());
        assert!(serde_json::from_str::<crate::plugin::PaneRect>(
            r#"{"x":-1,"y":0,"width":1,"height":1}"#
        )
        .is_err());
    }
}
