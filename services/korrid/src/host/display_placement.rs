//! ADR 0039: pure layout choice. Runner ids and emulator settings are opaque.
use crate::plugin::{RunnerRecord, SystemRecord};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Display {
    pub name: String,
    pub main: bool,
    pub unwatched: bool,
    pub mode: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Hardware path, not a compositor input identifier.
    pub touch: Option<String>,
    /// Sway's output transform. After `configured_displays`, `width` and
    /// `height` are the logical (post-transform) size every placement uses;
    /// `mode` stays the panel's native mode.
    pub transform: Transform,
}

/// Sway's own transform vocabulary (`output <name> transform <t>`); flipped
/// transforms have no device yet.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Transform {
    #[serde(rename = "normal")]
    Normal,
    #[serde(rename = "90")]
    Rotate90,
    #[serde(rename = "180")]
    Rotate180,
    #[serde(rename = "270")]
    Rotate270,
}

impl Transform {
    fn swaps_axes(self) -> bool {
        matches!(self, Transform::Rotate90 | Transform::Rotate270)
    }
}

/// The panel's native size, which its mode names. A quarter turn swaps the
/// logical size back.
fn native_size(display: &Display) -> (u32, u32) {
    if display.transform.swaps_axes() {
        (display.height, display.width)
    } else {
        (display.width, display.height)
    }
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
    parse_displays(&value.to_string_lossy())
}

/// The published facts carry the native mode size. Every placement works in
/// Sway's layout space, so turn a quarter-turned display's size here, once.
pub(crate) fn parse_displays(value: &str) -> Result<Vec<Display>, String> {
    let mut displays: Vec<Display> =
        serde_json::from_str(value).map_err(|e| format!("invalid display facts: {e}"))?;
    for display in &mut displays {
        if display.transform.swaps_axes() {
            std::mem::swap(&mut display.width, &mut display.height);
        }
    }
    if displays.iter().filter(|d| d.main).count() != 1
        || displays.iter().any(|d| {
            d.width == 0
                || d.height == 0
                || configured_refresh(d).is_err()
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

// Mode is the native Sway value authored in the Nix display option. Validate
// it before inserting it in an IPC command, without maintaining another mode.
fn configured_refresh(display: &Display) -> Result<&str, String> {
    let (width, height) = native_size(display);
    let prefix = format!("{width}x{height}@");
    let refresh = display
        .mode
        .strip_prefix(&prefix)
        .ok_or("configured mode disagrees with display geometry")?;
    let number = refresh.strip_suffix("Hz").unwrap_or(refresh);
    if number.is_empty()
        || !number.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        || !number
            .parse::<f64>()
            .is_ok_and(|n| n.is_finite() && n > 0.0)
    {
        return Err("invalid configured display refresh".into());
    }
    Ok(refresh)
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

/// The viewer's own size: an unwatched display has no fixed resolution, so
/// the stream needs no scaling (owner, 2026-11-09). Only the 4096-per-side
/// encoder limit scales it down, keeping the aspect ratio.
pub(crate) fn client_shape(
    viewer: &crate::launcher::plugin_launch::PluginDisplay,
) -> Result<crate::launcher::plugin_launch::PluginDisplay, String> {
    use crate::launcher::plugin_launch::PluginDisplay;
    if viewer.width < 8 || viewer.height < 8 {
        return Err("viewer display must have at least eight pixels per side".into());
    }
    let width = f64::from(viewer.width);
    let height = f64::from(viewer.height);
    let scale = 1.0_f64.min(4096.0 / width).min(4096.0 / height);
    let width = (width * scale) as u32 & !7;
    let height = (height * scale) as u32 & !7;
    if width == 0 || height == 0 {
        return Err("viewer aspect ratio cannot fit an eight-pixel-aligned output mode".into());
    }
    Ok(PluginDisplay { width, height })
}

// Sway's mode fields, unlike the launch treaty. refresh is in mHz.
#[derive(Deserialize)]
struct SwayMode {
    width: u32,
    height: u32,
    #[serde(default)]
    refresh: u32,
}

#[derive(Deserialize)]
struct SwayOutput {
    name: String,
    current_mode: Option<SwayMode>,
    #[serde(default)]
    modes: Vec<SwayMode>,
}

fn sway_output(
    control: &dyn super::compositor_focus::CompositorControl,
    name: &str,
) -> Result<Option<SwayOutput>, String> {
    let outputs: Vec<SwayOutput> = serde_json::from_str(&control.outputs()?)
        .map_err(|e| format!("invalid Sway outputs: {e}"))?;
    Ok(outputs.into_iter().find(|o| o.name == name))
}

fn output_size(
    control: &dyn super::compositor_focus::CompositorControl,
    name: &str,
) -> Result<crate::launcher::plugin_launch::PluginDisplay, String> {
    use crate::launcher::plugin_launch::PluginDisplay;
    sway_output(control, name)?
        .and_then(|o| o.current_mode)
        .map(|m| PluginDisplay {
            width: m.width,
            height: m.height,
        })
        .ok_or_else(|| format!("output {name} has no current mode"))
}

/// The output's own mode for this size, at the refresh nearest the configured
/// one, as a Sway mode value. A custom mode builds its own CVT timing, which
/// some sinks refuse even for a size they list (aka's dummy plug at 4K60).
/// Sway can accept a plain `mode` for an unlisted size without changing
/// anything, so only a listed mode qualifies.
fn listed_mode(
    output: &SwayOutput,
    size: &crate::launcher::plugin_launch::PluginDisplay,
    refresh: &str,
) -> Option<String> {
    let wanted = refresh
        .strip_suffix("Hz")
        .unwrap_or(refresh)
        .parse::<f64>()
        .ok()?
        * 1000.0;
    let mode = output
        .modes
        .iter()
        .filter(|m| m.width == size.width && m.height == size.height && m.refresh > 0)
        .min_by(|a, b| {
            (f64::from(a.refresh) - wanted)
                .abs()
                .total_cmp(&(f64::from(b.refresh) - wanted).abs())
        })?;
    // Sway matches a listed mode by rounding the rate to mHz, so print it exactly.
    let rate = format!("{}.{:03}", mode.refresh / 1000, mode.refresh % 1000);
    let rate = rate.trim_end_matches('0').trim_end_matches('.');
    Some(format!("{}x{}@{rate}Hz", mode.width, mode.height))
}

/// None means this placement is outside client shaping. Errors are launch warnings.
pub(crate) fn apply_client_shape(
    control: Option<&dyn super::compositor_focus::CompositorControl>,
    displays: &[Display],
    placement: &mut Placement,
    viewer: Option<&crate::launcher::plugin_launch::PluginDisplay>,
) -> Result<Option<crate::launcher::plugin_launch::PluginDisplay>, String> {
    let Some(viewer) = viewer else {
        return Ok(None);
    };
    let main = displays.iter().find(|d| d.main).ok_or("no main display")?;
    if !main.unwatched
        || placement.layout.is_some()
        || placement.global
        || placement.outputs != [main.name.clone()]
    {
        return Ok(None);
    }
    let size = client_shape(viewer)?;
    let refresh = configured_refresh(main)?;
    let control = control.ok_or("compositor unavailable for viewer display")?;
    let set = |mode: &str| -> Result<(), String> {
        control.command(&format!("output {} mode {mode}", main.name))?;
        if output_size(control, &main.name)? != size {
            return Err("compositor did not confirm viewer display size".into());
        }
        Ok(())
    };
    let result = (|| {
        let listed = sway_output(control, &main.name)?
            .and_then(|output| listed_mode(&output, &size, refresh));
        let custom = format!("--custom {}x{}@{}", size.width, size.height, refresh);
        match listed {
            None => set(&custom),
            Some(listed) => set(&listed).or_else(|first| {
                set(&custom).map_err(|reason| format!("listed mode {listed}: {first}; {reason}"))
            }),
        }
    })();
    if let Err(reason) = result {
        // A refused/mismatched mode may have changed the output. Restore
        // the configured mode before composing the configured-size launch.
        return match control.command(&configured_mode_command(main)) {
            Ok(()) => Err(reason),
            Err(restore) => Err(format!(
                "{reason}; configured mode restore failed: {restore}"
            )),
        };
    }
    placement.width = size.width;
    placement.height = size.height;
    Ok(Some(size))
}

fn configured_mode_command(display: &Display) -> String {
    format!("output {} mode {}", display.name, display.mode)
}

fn restore_commands(
    control: &dyn super::compositor_focus::CompositorControl,
    displays: &[Display],
    game: Option<&Placement>,
) -> Result<Vec<String>, String> {
    let mut commands = Vec::new();
    if game.is_none() {
        for display in displays.iter().filter(|d| d.main && d.unwatched) {
            let current = output_size(control, &display.name)?;
            if (current.width, current.height) != native_size(display) {
                configured_refresh(display)?;
                commands.push(configured_mode_command(display));
            }
        }
    }
    Ok(commands)
}

/// The configured displays Sway reports active now, and always the main one.
/// A configured display can be absent: the Thor boots with its bottom panel
/// unplugged (`video=DSI-1:d`) until the compositor runs. A surface placed on
/// an absent output would hold focus where nobody can see it. When Sway's
/// outputs cannot be read, only the main display counts.
pub(crate) fn present_displays(displays: &[Display], outputs: Option<&str>) -> Vec<Display> {
    #[derive(Deserialize)]
    struct Output {
        name: String,
        #[serde(default)]
        active: bool,
    }
    let outputs: Vec<Output> = outputs
        .and_then(|outputs| serde_json::from_str(outputs).ok())
        .unwrap_or_default();
    displays
        .iter()
        .filter(|d| d.main || outputs.iter().any(|o| o.name == d.name && o.active))
        .cloned()
        .collect()
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
    commands.push(format!(
        "[con_id={node}] move container to workspace current"
    ));
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
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayIdleRequest {}

#[typeshare::typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum DisplayIdleOutcome {
    Ok,
    Err(crate::RpcFailure),
}

#[typeshare::typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurfaceDisplaysRequest {
    /// The shown package's panes (ADR 0086); absent for a package with one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panes: Option<korri_surface_treaty::SurfacePackagePanes>,
    pub overlay: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_id: Option<String>,
    /// Only answer which windows the panes would get now; no effect.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub check: bool,
}

/// Where a surface placement left the shell: the main-display facts the host
/// keeps for itself, and the windows a package with panes gets (ADR 0086).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfacePlaced {
    pub rectangles: Vec<SurfaceDisplay>,
    pub windows: Option<Vec<korri_surface_treaty::SurfaceWindow>>,
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

/// The windows a package's panes get: absent for a package without panes.
#[typeshare::typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SurfaceDisplaysOutcome {
    Ok(Option<Vec<korri_surface_treaty::SurfaceWindow>>),
    Err(crate::RpcFailure),
}

/// ADR 0086: the output a surface's lower pane gets. A configured display
/// that is not the main one, that Sway reports active (connected and
/// enabled), and that sits directly under the main display with some
/// horizontal overlap. The main display must be active too. Power is not
/// asked: korrid itself powers an unused display off.
pub(crate) fn lower_output(displays: &[Display], outputs: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Output {
        name: String,
        #[serde(default)]
        active: bool,
    }
    let outputs: Vec<Output> = serde_json::from_str(outputs).ok()?;
    let active = |name: &str| outputs.iter().any(|o| o.name == name && o.active);
    let main = displays.iter().find(|d| d.main)?;
    if !active(&main.name) {
        return None;
    }
    let (left, right) = (i64::from(main.x), i64::from(main.x) + i64::from(main.width));
    let bottom = i64::from(main.y) + i64::from(main.height);
    displays
        .iter()
        .filter(|d| !d.main && !d.unwatched && active(&d.name))
        .find(|d| {
            i64::from(d.y) == bottom
                && i64::from(d.x) < right
                && i64::from(d.x) + i64::from(d.width) > left
        })
        .map(|d| d.name.clone())
}

/// ADR 0086: which panes each window holds. Two windows when the lower
/// output exists, else both panes in window 0.
pub(crate) fn surface_windows(
    panes: Option<&korri_surface_treaty::SurfacePackagePanes>,
    lower: Option<&str>,
) -> Option<Vec<korri_surface_treaty::SurfaceWindow>> {
    use korri_surface_treaty::SurfaceWindow;
    let panes = panes?;
    Some(match lower {
        Some(_) => panes
            .panes
            .iter()
            .map(|pane| SurfaceWindow {
                panes: vec![pane.clone()],
            })
            .collect(),
        None => vec![SurfaceWindow {
            panes: panes.panes.clone(),
        }],
    })
}

/// Fullscreen each further pane window on the lower output. A pane window
/// that has not mapped yet gets no command: the shell declares again once it
/// has drawn, and that declare places it.
pub(crate) fn pane_window_commands(
    tree: &str,
    shell_pids: &std::collections::BTreeSet<i32>,
    panes: &korri_surface_treaty::SurfacePackagePanes,
    lower: &str,
) -> Result<Vec<String>, String> {
    let windows = super::compositor_focus::pane_windows(tree, shell_pids)?;
    let mut commands = Vec::new();
    for (pane, node) in windows {
        if panes.panes.iter().skip(1).any(|p| *p == pane) {
            commands.extend([
                format!("[con_id={node}] move container to workspace current"),
                format!("[con_id={node}] fullscreen disable"),
                format!("[con_id={node}] floating disable"),
                format!("[con_id={node}] move container to output {lower}"),
                format!("[con_id={node}] fullscreen enable"),
            ]);
        }
    }
    Ok(commands)
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

/// A surface's lower pane and the output it goes on (ADR 0086).
pub(crate) type LowerPane<'a> = (&'a korri_surface_treaty::SurfacePackagePanes, &'a str);

pub(crate) fn place_surface(
    control: &dyn super::compositor_focus::CompositorControl,
    displays: &[Display],
    game: Option<&Placement>,
    pids: &std::collections::BTreeSet<i32>,
    shell_pids: &std::collections::BTreeSet<i32>,
    lower: Option<LowerPane<'_>>,
) -> Result<Vec<SurfaceDisplay>, String> {
    let started = std::time::Instant::now();
    let mut timings = SurfacePlacementTimings::default();
    let result = place_surface_timed(
        control,
        displays,
        game,
        pids,
        shell_pids,
        lower,
        &mut timings,
    );
    if displays.len() > 1 && game.is_none() {
        let outcome = if result.is_ok() { "Changed" } else { "Failed" };
        eprintln!("<6>korrid: PlacementTransition {{ target: Catalog, outcome: {outcome}, total_ms: {}, phases_ms: {{\"shell_tree\": {}, \"shell_power\": {}, \"shell_geometry\": {}}} }}", started.elapsed().as_millis(), timings.tree_ms, timings.power_ms, timings.geometry_ms);
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
    shell_pids: &std::collections::BTreeSet<i32>,
    lower: Option<LowerPane<'_>>,
    timings: &mut SurfacePlacementTimings,
) -> Result<Vec<SurfaceDisplay>, String> {
    // A lower pane belongs to the catalog only; a game holds the displays then.
    let lower = lower.filter(|_| game.is_none());
    use super::compositor_focus::{select_focus_target, FocusTarget};
    // A game's overlay takes a free display only if Sway has that output now.
    let rectangles = match game {
        Some(_) if displays.len() > 1 => surface_rectangles(
            &present_displays(displays, control.outputs().ok().as_deref()),
            game,
        ),
        _ => surface_rectangles(displays, game),
    };
    let restore = restore_commands(control, displays, game)?;
    if displays.len() <= 1 {
        if !restore.is_empty() {
            control.command(&restore.join("; "))?;
        }
        return Ok(rectangles);
    }
    let start = std::time::Instant::now();
    let tree = control.tree();
    timings.tree_ms = start.elapsed().as_millis();
    let tree = tree?;
    let shell = super::compositor_focus::shell_node(&tree, shell_pids)?;
    let mut occupied = game.map(|p| p.outputs.clone()).unwrap_or_default();
    occupied.extend(rectangles.iter().map(|d| d.id.clone()));
    occupied.extend(lower.map(|(_, output)| output.to_owned()));
    let pane_commands = match lower {
        Some((panes, output)) => pane_window_commands(&tree, shell_pids, panes, output)?,
        None => Vec::new(),
    };
    let mut commands = restore;
    if let Some(game) = game.filter(|p| p.global) {
        let candidate = match select_focus_target(&tree, pids, shell_pids) {
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
        // output even when the previous shell presentation was a free-display
        // overlay. Focus before the blocking unused-output power-off tail.
        commands.extend([
            format!("[con_id={shell}] move container to workspace current"),
            format!("[con_id={shell}] fullscreen disable"),
            format!("[con_id={shell}] floating disable"),
            format!("[con_id={shell}] move container to output {}", origin.name),
            format!("[con_id={shell}] fullscreen enable"),
        ]);
        // The lower pane's window, then focus back on the shell's own window:
        // the shell takes controller input while one of its windows has focus.
        commands.extend(pane_commands);
        commands.push(format!("[con_id={shell}] focus"));
        commands.extend(
            power
                .iter()
                .filter(|command| command.ends_with(" off"))
                .cloned(),
        );
    } else {
        commands.extend([
            format!("[con_id={shell}] move container to workspace current"),
            format!("[con_id={shell}] fullscreen disable"),
            format!("[con_id={shell}] floating enable"),
            format!(
                "[con_id={shell}] resize set width {} px height {} px",
                first.width, first.height
            ),
            format!(
                "[con_id={shell}] move absolute position {} px {} px",
                origin.x, origin.y
            ),
            format!("[con_id={shell}] focus"),
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
                unwatched: false,
                mode: "640x480@60Hz".into(),
                x: 0,
                y: 0,
                width: 640,
                height: 480,
                touch: None,
                transform: Transform::Normal,
            },
            Display {
                name: "DSI-1".into(),
                main: false,
                unwatched: false,
                mode: "640x480@60Hz".into(),
                x: 0,
                y: 480,
                width: 640,
                height: 480,
                touch: None,
                transform: Transform::Normal,
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

    #[test]
    fn configured_mode_is_native_and_safe_for_sway_commands() {
        let mut display = unwatched_main().remove(0);
        display.mode = "1920x1080@59.94Hz".into();
        assert_eq!(configured_refresh(&display).unwrap(), "59.94Hz");
        assert_eq!(
            configured_mode_command(&display),
            "output DSI-2 mode 1920x1080@59.94Hz"
        );
        for mode in [
            "1920x1080@0Hz",
            "1920x1080@NaNHz",
            "1920x1080@60Hz; exec bad",
            "640x480@60Hz",
        ] {
            display.mode = mode.into();
            assert!(configured_refresh(&display).is_err(), "{mode}");
        }
    }

    #[test]
    fn client_shape_keeps_the_viewer_size_within_eight_pixel_side_limits() {
        use crate::launcher::plugin_launch::PluginDisplay;
        for (width, height, expected) in [
            (2176, 1812, (2176, 1808)),
            (2448, 1848, (2448, 1848)),
            (5120, 1440, (4096, 1152)),
            (1280, 720, (1280, 720)),
            (641, 481, (640, 480)),
            (647, 487, (640, 480)),
            (15, 15, (8, 8)),
            (8, 8, (8, 8)),
            (1920, 1080, (1920, 1080)),
            (10000, 100, (4096, 40)),
            (100, 10000, (40, 4096)),
            (u32::MAX, u32::MAX, (4096, 4096)),
        ] {
            let size = client_shape(&PluginDisplay { width, height }).unwrap();
            assert_eq!((size.width, size.height), expected);
            assert!(size.width <= 4096 && size.height <= 4096);
            assert_eq!((size.width % 8, size.height % 8), (0, 0));
        }
        assert!(client_shape(&PluginDisplay {
            width: 0,
            height: 1080
        },)
        .is_err());
        assert!(client_shape(&PluginDisplay {
            width: 1,
            height: 1080
        },)
        .is_err());
        assert!(client_shape(&PluginDisplay {
            width: 7,
            height: 1080
        },)
        .is_err());
        assert!(client_shape(&PluginDisplay {
            width: u32::MAX,
            height: 8
        },)
        .is_err());
    }

    fn unwatched_main() -> Vec<Display> {
        let mut ds = displays();
        ds.truncate(1);
        ds[0].unwatched = true;
        ds[0].mode = "1920x1080@60Hz".into();
        ds[0].width = 1920;
        ds[0].height = 1080;
        ds
    }

    #[test]
    fn custom_mode_is_confirmed_before_changing_placement() {
        use crate::launcher::plugin_launch::PluginDisplay;
        let dir = tempfile::tempdir().unwrap();
        let control = sway_program(dir.path(), r#"[{"success":true}]"#);
        std::fs::write(
            dir.path().join("outputs"),
            r#"[{"name":"DSI-2","current_mode":{"width":2176,"height":1808,"refresh":60000}}]"#,
        )
        .unwrap();
        let mut ds = unwatched_main();
        ds[0].mode = "1920x1080@120Hz".into();
        let mut p = choose(None, &runner(), &ds).unwrap();
        let size = apply_client_shape(
            Some(&control),
            &ds,
            &mut p,
            Some(&PluginDisplay {
                width: 2176,
                height: 1812,
            }),
        )
        .unwrap()
        .unwrap();
        assert_eq!((size.width, size.height), (2176, 1808));
        assert_eq!((p.width, p.height), (2176, 1808));
        let events = std::fs::read_to_string(dir.path().join("events")).unwrap();
        assert_eq!(
            events,
            "get_outputs\noutput DSI-2 mode --custom 2176x1808@120Hz\nget_outputs\n"
        );
    }

    // aka's HDMI dummy plug lists 4K at several rates and refuses a custom 4K60.
    const LISTED_4K: &str = r#"[{"width":3840,"height":2160,"refresh":30000},{"width":3840,"height":2160,"refresh":59940},{"width":3840,"height":2160,"refresh":60000},{"width":1920,"height":1080,"refresh":60000}]"#;

    fn shape_4k(
        dir: &std::path::Path,
        configured: &str,
        current: (u32, u32),
        viewer: (u32, u32),
    ) -> (
        Result<Option<crate::launcher::plugin_launch::PluginDisplay>, String>,
        Placement,
    ) {
        use crate::launcher::plugin_launch::PluginDisplay;
        let control = sway_program(dir, r#"[{"success":true}]"#);
        std::fs::write(
            dir.join("outputs"),
            format!(
                r#"[{{"name":"DSI-2","current_mode":{{"width":{},"height":{},"refresh":60000}},"modes":{LISTED_4K}}}]"#,
                current.0, current.1
            ),
        )
        .unwrap();
        let mut ds = unwatched_main();
        ds[0].mode = configured.into();
        let mut p = choose(None, &runner(), &ds).unwrap();
        let result = apply_client_shape(
            Some(&control),
            &ds,
            &mut p,
            Some(&PluginDisplay {
                width: viewer.0,
                height: viewer.1,
            }),
        );
        (result, p)
    }

    #[test]
    fn listed_mode_is_chosen_without_custom_at_the_nearest_refresh() {
        for (configured, sent) in [
            ("1920x1080@60Hz", "3840x2160@60Hz"),
            ("1920x1080@120Hz", "3840x2160@60Hz"),
            ("1920x1080@59.94Hz", "3840x2160@59.94Hz"),
            ("1920x1080@24", "3840x2160@30Hz"),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let (result, p) = shape_4k(dir.path(), configured, (3840, 2160), (3840, 2160));
            let size = result.unwrap().unwrap();
            assert_eq!((size.width, size.height), (3840, 2160));
            assert_eq!((p.width, p.height), (3840, 2160));
            let events = std::fs::read_to_string(dir.path().join("events")).unwrap();
            assert_eq!(
                events,
                format!("get_outputs\noutput DSI-2 mode {sent}\nget_outputs\n")
            );
        }
    }

    #[test]
    fn unlisted_size_still_uses_a_custom_mode() {
        let dir = tempfile::tempdir().unwrap();
        let (result, p) = shape_4k(dir.path(), "1920x1080@60Hz", (2560, 1440), (2560, 1440));
        result.unwrap().unwrap();
        assert_eq!((p.width, p.height), (2560, 1440));
        let events = std::fs::read_to_string(dir.path().join("events")).unwrap();
        assert_eq!(
            events,
            "get_outputs\noutput DSI-2 mode --custom 2560x1440@60Hz\nget_outputs\n"
        );
    }

    #[test]
    fn refused_or_unconfirmed_listed_mode_falls_back_to_custom_then_restores() {
        for refusal in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            if refusal {
                std::fs::write(dir.path().join("reject"), "mode 3840x2160@60Hz").unwrap();
            }
            let (result, p) = shape_4k(dir.path(), "1920x1080@60Hz", (1920, 1080), (3840, 2160));
            let warning = result.unwrap_err();
            assert!(
                warning.starts_with("listed mode 3840x2160@60Hz: "),
                "{warning}"
            );
            assert_eq!(warning.contains("refused"), refusal, "{warning}");
            assert!(warning.ends_with("compositor did not confirm viewer display size"));
            assert_eq!((p.width, p.height), (1920, 1080));
            let batches = std::fs::read_to_string(dir.path().join("batches")).unwrap();
            assert_eq!(
                batches,
                "output DSI-2 mode 3840x2160@60Hz\noutput DSI-2 mode --custom 3840x2160@60Hz\noutput DSI-2 mode 1920x1080@60Hz\n"
            );
        }
    }

    #[test]
    fn watched_dual_pane_and_absent_viewer_never_request_a_mode() {
        use crate::launcher::plugin_launch::PluginDisplay;
        let ds = unwatched_main();
        let viewer = PluginDisplay {
            width: 2176,
            height: 1812,
        };
        let mut p = choose(None, &runner(), &ds).unwrap();
        assert_eq!(apply_client_shape(None, &ds, &mut p, None).unwrap(), None);
        let mut watched = ds.clone();
        watched[0].unwatched = false;
        assert_eq!(
            apply_client_shape(None, &watched, &mut p, Some(&viewer)).unwrap(),
            None
        );
        let mut dual = choose(Some(&system()), &runner(), &ds).unwrap();
        assert_eq!(
            apply_client_shape(None, &ds, &mut dual, Some(&viewer)).unwrap(),
            None
        );
        assert_eq!((p.width, p.height), (1920, 1080));
    }

    #[test]
    fn refused_or_unconfirmed_custom_mode_keeps_configured_placement_with_a_diagnostic() {
        use crate::launcher::plugin_launch::PluginDisplay;
        for refusal in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let control = sway_program(dir.path(), r#"[{"success":true}]"#);
            if refusal {
                std::fs::write(dir.path().join("reject"), "--custom").unwrap();
            }
            std::fs::write(
                dir.path().join("outputs"),
                r#"[{"name":"DSI-2","current_mode":{"width":1920,"height":1080}}]"#,
            )
            .unwrap();
            let ds = unwatched_main();
            let mut p = choose(None, &runner(), &ds).unwrap();
            let warning = apply_client_shape(
                Some(&control),
                &ds,
                &mut p,
                Some(&PluginDisplay {
                    width: 2176,
                    height: 1812,
                }),
            )
            .unwrap_err();
            assert!(warning.contains(if refusal {
                "refused"
            } else {
                "did not confirm"
            }));
            assert_eq!((p.width, p.height), (1920, 1080));
            let batches = std::fs::read_to_string(dir.path().join("batches")).unwrap();
            assert!(batches.ends_with("output DSI-2 mode 1920x1080@60Hz\n"));
        }
    }

    #[test]
    fn shell_restores_only_unwatched_main_without_a_game_and_only_if_different() {
        for count in [1, 2] {
            for (unwatched, game, width, restores) in [
                (true, false, 2176, true),
                (true, false, 1920, false),
                (false, false, 2176, false),
                (true, true, 2176, false),
            ] {
                let dir = tempfile::tempdir().unwrap();
                let control = sway_program(dir.path(), r#"[{"success":true}]"#);
                let mut ds = unwatched_main();
                ds[0].unwatched = unwatched;
                if count == 2 {
                    ds.push(displays().remove(1));
                }
                std::fs::write(
                    dir.path().join("outputs"),
                    format!(
                        r#"[{{"name":"DSI-2","current_mode":{{"width":{width},"height":1080}}}}]"#
                    ),
                )
                .unwrap();
                let p = choose(None, &runner(), &ds).unwrap();
                place_surface(
                    &control,
                    &ds,
                    game.then_some(&p),
                    &Default::default(),
                    &[100].into_iter().collect(),
                    None,
                )
                .unwrap();
                let batches =
                    std::fs::read_to_string(dir.path().join("batches")).unwrap_or_default();
                assert_eq!(
                    batches.contains("output DSI-2 mode 1920x1080@60Hz"),
                    restores,
                    "{batches}"
                );
                if count == 2 && restores {
                    assert!(batches.lines().nth(1).unwrap().starts_with(
                        "output DSI-2 mode 1920x1080@60Hz; [con_id=10] move container to workspace current; [con_id=10] fullscreen disable"
                    ));
                }
            }
        }
    }

    fn sway_program(
        directory: &std::path::Path,
        reply: &str,
    ) -> super::super::sway_test_peer::ScriptedSwayControl {
        use std::os::unix::fs::PermissionsExt;
        let program = directory.join("swaymsg");
        std::fs::write(directory.join("tree"),r#"{"nodes":[{"id":10,"pid":100,"app_id":"shell"},{"id":20,"pid":200,"app_id":"game"}]}"#).unwrap();
        std::fs::write(directory.join("reply"), reply).unwrap();
        // Both configured outputs are present unless a test says otherwise.
        std::fs::write(directory.join("outputs"), outputs(&["DSI-2", "DSI-1"])).unwrap();
        std::fs::write(
            &program,
            format!(
                r#"#!/bin/sh
cd '{}' || exit 1
for argument do command="$argument"; done
case "$command" in get_outputs)
 printf 'get_outputs\n' >> events
 cat outputs
 exit;;
get_tree)
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
 if test -f validate-shell-restore; then
  mode=$(cat shell-fullscreen)
  case "$part" in
   '[con_id=10] fullscreen disable') mode=0;;
   '[con_id=10] floating disable')
    if test "$mode" != 0; then printf '[{{"success":false,"error":"floating changed while fullscreen"}}]'; exit; fi;;
   '[con_id=10] fullscreen enable')
    if test "$mode" != 0; then printf '[{{"success":false,"error":"enable on already-fullscreen con"}}]'; exit; fi
    mode=1;;
  esac
  printf '%s' "$mode" > shell-fullscreen
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
        super::super::sway_test_peer::ScriptedSwayControl::new(
            program,
            directory.join("socket"),
            std::time::Duration::from_secs(1),
        )
        .unwrap()
    }

    fn stage_and_floor() -> korri_surface_treaty::SurfacePackagePanes {
        korri_surface_treaty::SurfacePackagePanes {
            panes: vec!["stage".into(), "floor".into()],
            arrangement: "upper-above-lower".into(),
        }
    }

    fn outputs(active: &[&str]) -> String {
        serde_json::to_string(
            &["DSI-2", "DSI-1"]
                .iter()
                .map(|name| serde_json::json!({ "name": name, "active": active.contains(name) }))
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }

    /// ADR 0086, live: korrid's own pane placement against a running Sway.
    /// The headless two-output proof runs it once per step
    /// (`clients/shell/proof/panes.sh`), with `KORRI_LIVE_SWAYSOCK`,
    /// `KORRI_LIVE_SHELL_PID` and `KORRI_LIVE_DISPLAYS` (display facts as
    /// `KORRID_DISPLAYS` holds them). It prints the windows it gave.
    #[test]
    #[ignore = "needs a running Sway and shell; run by clients/shell/proof/panes.sh"]
    fn live_pane_placement() {
        use super::super::compositor_focus::{CompositorControl, SwayCompositorControl};
        let env = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name}"));
        let control = SwayCompositorControl::new(
            env("KORRI_LIVE_SWAYSOCK").into(),
            std::time::Duration::from_secs(2),
        )
        .expect("Sway IPC");
        let displays: Vec<Display> = serde_json::from_str(&env("KORRI_LIVE_DISPLAYS")).unwrap();
        let pid: i32 = env("KORRI_LIVE_SHELL_PID").parse().unwrap();
        let panes = stage_and_floor();
        let lower = lower_output(&displays, &control.outputs().unwrap());
        let windows = surface_windows(Some(&panes), lower.as_deref());
        place_surface(
            &control,
            &displays,
            None,
            &Default::default(),
            &[pid].into_iter().collect(),
            lower.as_deref().map(|output| (&panes, output)),
        )
        .unwrap();
        println!("WINDOWS {}", serde_json::to_string(&windows).unwrap());
    }

    /// ADR 0086, the Thor's arrangement: a narrower display centred under the
    /// main one is the lower pane's output, while Sway has it active.
    #[test]
    fn the_lower_pane_goes_on_an_active_display_under_the_main_one() {
        let mut ds = displays();
        ds[0].width = 1920;
        ds[0].height = 1080;
        ds[1].x = 340;
        ds[1].y = 1080;
        ds[1].width = 1240;
        ds[1].height = 1080;
        assert_eq!(
            lower_output(&ds, &outputs(&["DSI-2", "DSI-1"])).as_deref(),
            Some("DSI-1")
        );
        // Disabled or unplugged: no lower pane. Main gone: nothing.
        assert_eq!(lower_output(&ds, &outputs(&["DSI-2"])), None);
        assert_eq!(lower_output(&ds, &outputs(&["DSI-1"])), None);
        // Beside the main display, or not touching it: not under it.
        let mut beside = ds.clone();
        beside[1].x = 1920;
        beside[1].y = 0;
        assert_eq!(lower_output(&beside, &outputs(&["DSI-2", "DSI-1"])), None);
        let mut apart = ds.clone();
        apart[1].y = 1100;
        assert_eq!(lower_output(&apart, &outputs(&["DSI-2", "DSI-1"])), None);
        // An unwatched display is no place for a pane.
        let mut unwatched = ds;
        unwatched[1].unwatched = true;
        assert_eq!(
            lower_output(&unwatched, &outputs(&["DSI-2", "DSI-1"])),
            None
        );
    }

    #[test]
    fn panes_get_two_windows_only_with_a_lower_output() {
        use korri_surface_treaty::SurfaceWindow;
        let window = |panes: &[&str]| SurfaceWindow {
            panes: panes.iter().map(|p| p.to_string()).collect(),
        };
        assert_eq!(surface_windows(None, Some("DSI-1")), None);
        assert_eq!(
            surface_windows(Some(&stage_and_floor()), None),
            Some(vec![window(&["stage", "floor"])])
        );
        assert_eq!(
            surface_windows(Some(&stage_and_floor()), Some("DSI-1")),
            Some(vec![window(&["stage"]), window(&["floor"])])
        );
    }

    /// ADR 0086: the shell's own window goes on the main display, the lower
    /// pane's window on the display under it, both stay powered, and focus
    /// ends on the shell's own window.
    #[test]
    fn catalog_places_a_lower_pane_window_under_the_main_display() {
        let dir = tempfile::tempdir().unwrap();
        let control = sway_program(dir.path(), r#"[{"success":true}]"#);
        std::fs::write(
            dir.path().join("tree"),
            r#"{"nodes":[{"id":10,"pid":100,"name":"Velvet"},{"id":11,"pid":100,"name":"korri-pane:floor"}]}"#,
        )
        .unwrap();
        let panes = stage_and_floor();
        place_surface(
            &control,
            &displays(),
            None,
            &Default::default(),
            &[100].into_iter().collect(),
            Some((&panes, "DSI-1")),
        )
        .unwrap();
        let batches = std::fs::read_to_string(dir.path().join("batches")).unwrap();
        assert_eq!(
            batches.lines().next().unwrap(),
            "output DSI-2 power on; output DSI-1 power on"
        );
        assert_eq!(batches.lines().nth(1).unwrap(), "[con_id=10] move container to workspace current; [con_id=10] fullscreen disable; [con_id=10] floating disable; [con_id=10] move container to output DSI-2; [con_id=10] fullscreen enable; [con_id=11] move container to workspace current; [con_id=11] fullscreen disable; [con_id=11] floating disable; [con_id=11] move container to output DSI-1; [con_id=11] fullscreen enable; [con_id=10] focus");
        assert!(!batches.contains("power off"), "{batches}");
        // The pane window does not count as the shell's own window.
        assert_eq!(
            super::super::compositor_focus::shell_node(
                &std::fs::read_to_string(dir.path().join("tree")).unwrap(),
                &[100].into_iter().collect()
            ),
            Ok(10)
        );
    }

    #[test]
    fn catalog_moves_the_owned_shell_to_main_then_powers_off_unused_displays() {
        let dir = tempfile::tempdir().unwrap();
        let control = sway_program(dir.path(), r#"[{"success":true}]"#);
        std::fs::write(dir.path().join("validate-shell-restore"), "").unwrap();
        std::fs::write(dir.path().join("shell-fullscreen"), "1").unwrap();
        let facts = place_surface(
            &control,
            &displays(),
            None,
            &Default::default(),
            &[100].into_iter().collect(),
            None,
        )
        .unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].id, "DSI-2");
        let batches = std::fs::read_to_string(dir.path().join("batches")).unwrap();
        assert_eq!(batches.lines().count(), 2, "{batches}");
        assert_eq!(batches.lines().next().unwrap(), "output DSI-2 power on");
        assert_eq!(batches.lines().nth(1).unwrap(), "[con_id=10] move container to workspace current; [con_id=10] fullscreen disable; [con_id=10] floating disable; [con_id=10] move container to output DSI-2; [con_id=10] fullscreen enable; [con_id=10] focus; output DSI-1 power off");
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
            unwatched: false,
            mode: "1920x1080@60Hz".into(),
            x: 1280,
            y: 300,
            width: 1920,
            height: 1080,
            touch: None,
            transform: Transform::Normal,
        });
        let facts = place_surface(
            &control,
            &ds,
            None,
            &Default::default(),
            &[100].into_iter().collect(),
            None,
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
            &[100].into_iter().collect(),
            None,
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
            None,
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
        std::fs::write(dir.path().join("tree"), r#"{"nodes":[{"id":10,"pid":100,"app_id":"shell","focused":true,"fullscreen_mode":2},{"id":20,"pid":200,"app_id":"retroarch","focused":false}]}"#).unwrap();
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
            "output DSI-2 power on; [con_id=20] move container to workspace current; [con_id=20] fullscreen disable; [con_id=20] floating disable; [con_id=20] move container to output DSI-2; [con_id=20] fullscreen enable; [con_id=20] opacity 1; [con_id=20] focus; output DSI-1 power off\n");
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
        place_surface(
            &control,
            &ds,
            Some(&p),
            &pids,
            &[100].into_iter().collect(),
            None,
        )
        .unwrap();
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
            None,
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
            None,
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
    fn surface_placement_refuses_zero_or_multiple_windows_owned_by_the_shell_unit() {
        let dir = tempfile::tempdir().unwrap();
        let control = sway_program(dir.path(), r#"[{"success":true}]"#);
        let shell_pids = [2603].into_iter().collect();
        for tree in [
            r#"{"nodes":[{"id":10,"pid":999,"app_id":"korri-shell"}]}"#,
            r#"{"nodes":[{"id":10,"pid":2603,"app_id":"korri-shell"},{"id":11,"pid":2603,"app_id":"second-window"}]}"#,
        ] {
            std::fs::write(dir.path().join("tree"), tree).unwrap();
            assert_eq!(
                place_surface(
                    &control,
                    &displays(),
                    None,
                    &std::collections::BTreeSet::new(),
                    &shell_pids,
                    None
                )
                .unwrap_err(),
                "no unique shell window"
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
                "[con_id=20] move container to workspace current",
                "[con_id=20] fullscreen disable",
                "[con_id=20] floating disable",
                "[con_id=20] move container to output DSI-2",
                "[con_id=20] fullscreen enable global",
                "[con_id=20] opacity 1",
                "[con_id=20] focus",
                "output DSI-2 power on",
                "[con_id=20] move container to workspace current",
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

    /// The AYN Thor's facts as the host module publishes them: native mode
    /// sizes, both panels turned 90 degrees, the bottom centred under the top.
    fn thor_facts(top_mode: &str) -> String {
        format!(
            r#"[{{"name":"DSI-2","main":true,"unwatched":false,"mode":"{top_mode}","x":0,"y":0,"width":1080,"height":1920,"touch":"/soc@0/geniqup@ac0000/i2c@a90000/touchscreen@38","transform":"90"}},{{"name":"DSI-1","main":false,"unwatched":false,"mode":"1080x1240@60Hz","x":340,"y":1080,"width":1080,"height":1240,"touch":"/soc@0/geniqup@9c0000/i2c@98c000/touchscreen@38","transform":"90"}}]"#
        )
    }

    #[test]
    fn quarter_turned_displays_place_in_their_logical_size() {
        let ds = parse_displays(&thor_facts("1080x1920@120Hz")).unwrap();
        assert_eq!(
            (ds[0].x, ds[0].y, ds[0].width, ds[0].height),
            (0, 0, 1920, 1080)
        );
        assert_eq!(
            (ds[1].x, ds[1].y, ds[1].width, ds[1].height),
            (340, 1080, 1240, 1080)
        );
        // The mode stays the panel's native one.
        assert_eq!(configured_refresh(&ds[0]), Ok("120Hz"));
        assert_eq!(
            configured_mode_command(&ds[1]),
            "output DSI-1 mode 1080x1240@60Hz"
        );
        // ADR 0086: the bottom sits directly under the top.
        assert_eq!(
            lower_output(&ds, &outputs(&["DSI-2", "DSI-1"])).as_deref(),
            Some("DSI-1")
        );
        let game = choose(None, &runner(), &ds).unwrap();
        assert_eq!((game.width, game.height), (1920, 1080));
        assert_eq!(
            surface_rectangles(&ds, Some(&game)),
            vec![SurfaceDisplay {
                id: "DSI-1".into(),
                main: false,
                x: 0,
                y: 0,
                width: 1240,
                height: 1080,
                touch: true,
            }]
        );
    }

    #[test]
    fn a_mode_that_is_not_the_native_size_is_refused() {
        assert!(parse_displays(&thor_facts("1920x1080@120Hz")).is_err());
    }

    #[test]
    fn unturned_facts_keep_their_size() {
        let ds = parse_displays(r#"[{"name":"DSI-2","main":true,"unwatched":false,"mode":"640x480@60Hz","x":0,"y":0,"width":640,"height":480,"touch":"/i2c@fe5c0000/touchscreen@14","transform":"normal"},{"name":"DSI-1","main":false,"unwatched":false,"mode":"640x480@60Hz","x":0,"y":480,"width":640,"height":480,"touch":"/i2c@fe5e0000/touchscreen@14","transform":"normal"}]"#).unwrap();
        assert_eq!(
            ds.iter().map(|d| (d.width, d.height)).collect::<Vec<_>>(),
            vec![(640, 480), (640, 480)]
        );
        assert!(parse_displays(r#"[{"name":"DSI-2","main":true,"unwatched":false,"mode":"640x480@60Hz","x":0,"y":0,"width":640,"height":480,"touch":null}]"#).is_err());
    }

    #[test]
    fn the_overlay_on_a_quarter_turned_free_display_fits_it() {
        let dir = tempfile::tempdir().unwrap();
        let control = sway_program(dir.path(), r#"[{"success":true}]"#);
        let ds = parse_displays(&thor_facts("1080x1920@120Hz")).unwrap();
        let game = choose(None, &runner(), &ds).unwrap();
        let mut timings = SurfacePlacementTimings::default();
        let rectangles = place_surface_timed(
            &control,
            &ds,
            Some(&game),
            &[200].into_iter().collect(),
            &[100].into_iter().collect(),
            None,
            &mut timings,
        )
        .unwrap();
        assert_eq!(rectangles[0].id, "DSI-1");
        let batches = std::fs::read_to_string(dir.path().join("batches")).unwrap();
        assert_eq!(
            batches.lines().next().unwrap(),
            "output DSI-2 power on; output DSI-1 power on"
        );
        assert!(batches.lines().last().unwrap().ends_with(
            "[con_id=10] resize set width 1240 px height 1080 px; [con_id=10] move absolute position 340 px 1080 px; [con_id=10] focus"
        ));
    }

    #[test]
    fn the_overlay_stays_on_main_while_the_free_display_is_absent() {
        let dir = tempfile::tempdir().unwrap();
        let control = sway_program(dir.path(), r#"[{"success":true}]"#);
        // Unplugged at boot: Sway does not list the output at all.
        std::fs::write(
            dir.path().join("outputs"),
            r#"[{"name":"DSI-2","active":true}]"#,
        )
        .unwrap();
        let ds = parse_displays(&thor_facts("1080x1920@120Hz")).unwrap();
        let game = choose(None, &runner(), &ds).unwrap();
        let mut timings = SurfacePlacementTimings::default();
        let rectangles = place_surface_timed(
            &control,
            &ds,
            Some(&game),
            &[200].into_iter().collect(),
            &[100].into_iter().collect(),
            None,
            &mut timings,
        )
        .unwrap();
        assert_eq!(rectangles[0].id, "DSI-2");
        let batches = std::fs::read_to_string(dir.path().join("batches")).unwrap();
        assert_eq!(
            batches.lines().next().unwrap(),
            "output DSI-2 power on; output DSI-1 power off"
        );
        assert!(batches.lines().last().unwrap().ends_with(
            "[con_id=10] resize set width 1920 px height 1080 px; [con_id=10] move absolute position 0 px 0 px; [con_id=10] focus"
        ));
    }

    #[test]
    fn present_displays_keeps_main_and_reported_active_outputs() {
        let ds = displays();
        let names = |present: Vec<Display>| present.into_iter().map(|d| d.name).collect::<Vec<_>>();
        assert_eq!(
            names(present_displays(&ds, Some(&outputs(&["DSI-2", "DSI-1"])))),
            vec!["DSI-2", "DSI-1"]
        );
        assert_eq!(
            names(present_displays(&ds, Some(&outputs(&["DSI-2"])))),
            vec!["DSI-2"]
        );
        assert_eq!(
            names(present_displays(
                &ds,
                Some(r#"[{"name":"DSI-2","active":true}]"#)
            )),
            vec!["DSI-2"]
        );
        assert_eq!(names(present_displays(&ds, None)), vec!["DSI-2"]);
        assert_eq!(
            names(present_displays(&ds, Some("not json"))),
            vec!["DSI-2"]
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
