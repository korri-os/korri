//! Choose which compositor window belongs to one exact launch.
//!
//! korrid owns session lifecycle truth, so it must also decide what "resume"
//! means before anything moves a window. This module is only that decision: it
//! reads a Sway tree snapshot and the processes of the exact launch, and names
//! the window to focus. It never talks to the compositor, spawns helpers, or
//! guesses between several equally plausible windows.

use serde::Deserialize;
use std::{
    collections::BTreeSet,
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

/// A Sway tree is large on a busy workspace but never megabytes. The cap stops
/// a broken compositor from feeding korrid unbounded output.
const MAX_TREE_BYTES: u64 = 4 * 1024 * 1024;

/// The compositor operations korrid needs to return to a game. Keeping this
/// behind a trait means session tests never depend on a live compositor.
pub(crate) trait CompositorControl: Send + Sync + std::fmt::Debug {
    fn tree(&self) -> Result<String, String>;
    fn focus(&self, node_id: i64) -> Result<(), String>;
}

/// Talks to Sway through the same `swaymsg -s <socket>` shape the Linux host
/// already uses for its compositor actions.
#[derive(Debug, Clone)]
pub(crate) struct SwaymsgCompositorControl {
    swaymsg: PathBuf,
    socket: PathBuf,
    timeout: Duration,
}

impl SwaymsgCompositorControl {
    pub(crate) fn new(swaymsg: PathBuf, socket: PathBuf, timeout: Duration) -> Option<Self> {
        if !swaymsg.is_absolute() || !socket.is_absolute() || timeout.is_zero() {
            return None;
        }
        Some(Self {
            swaymsg,
            socket,
            timeout,
        })
    }

    fn run(&self, arguments: &[&str]) -> Result<String, String> {
        let mut child = Command::new(&self.swaymsg)
            .arg("-s")
            .arg(&self.socket)
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            // korrid never reads this stream, and an unread pipe that fills
            // would block the child until the timeout killed it.
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| error.to_string())?;
        // Drain the answer while the child still runs. Waiting first would
        // deadlock as soon as a tree outgrew the pipe buffer.
        let mut stream = child.stdout.take().ok_or("swaymsg produced no output")?;
        let reader = std::thread::spawn(move || {
            let mut answer = Vec::new();
            stream
                .by_ref()
                .take(MAX_TREE_BYTES)
                .read_to_end(&mut answer)
                .map(|_| answer)
        });
        let deadline = std::time::Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait().map_err(|error| error.to_string())? {
                Some(status) => break status,
                None if std::time::Instant::now() >= deadline => {
                    // Killing the child also ends the read, so the drain
                    // thread cannot outlive this call.
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return Err("swaymsg did not answer before its timeout".into());
                }
                None => std::thread::sleep(Duration::from_millis(10)),
            }
        };
        let answer = reader
            .join()
            .map_err(|_| "swaymsg reader failed".to_string())?
            .map_err(|error| error.to_string())?;
        if !status.success() {
            return Err(format!("swaymsg failed: {status}"));
        }
        String::from_utf8(answer).map_err(|error| error.to_string())
    }
}

impl CompositorControl for SwaymsgCompositorControl {
    fn tree(&self) -> Result<String, String> {
        // `-r` keeps the reply compact, as every other Sway call in this repo
        // does; pretty-printed trees waste the read budget.
        self.run(&["-r", "-t", "get_tree"])
    }

    fn focus(&self, node_id: i64) -> Result<(), String> {
        // `--` and a literal criteria string keep the node id out of any
        // shell parsing; node ids come from the compositor, never a game.
        self.run(&["--", &format!("[con_id={node_id}] focus")])
            .map(|_| ())
    }
}

/// A node of Sway's `get_tree` reply. Unknown fields are ignored so a
/// compositor upgrade cannot break session focus.
#[derive(Debug, Deserialize)]
struct TreeNode {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    pid: Option<i32>,
    #[serde(default)]
    app_id: Option<String>,
    #[serde(default)]
    window_properties: Option<WindowProperties>,
    #[serde(default)]
    focused: bool,
    #[serde(default)]
    nodes: Vec<TreeNode>,
    #[serde(default)]
    floating_nodes: Vec<TreeNode>,
}

#[derive(Debug, Deserialize)]
struct WindowProperties {
    #[serde(default)]
    class: Option<String>,
}

/// One window that the exact launch owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FocusCandidate {
    pub(crate) node_id: i64,
    pub(crate) pid: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusOwnership {
    Launch,
    Excluded,
    Other,
}

/// What korrid may do with the compositor for one exact launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FocusTarget {
    /// Exactly one window belongs to the launch; focusing it is unambiguous.
    Window(FocusCandidate),
    /// The launch owns no window yet, or its window is already gone.
    NoWindow,
    /// Several windows belong to the launch. korrid refuses to guess.
    Ambiguous(Vec<FocusCandidate>),
    /// The compositor reply could not be read.
    Unreadable(String),
}

/// Select the window of one launch from a Sway tree snapshot.
///
/// `launch_pids` are the processes of that exact launch unit. `excluded_ids`
/// are surfaces korrid must never focus as a game, such as the kiosk browser.
pub(crate) fn select_focus_target(
    tree: &str,
    launch_pids: &BTreeSet<i32>,
    excluded_ids: &[&str],
) -> FocusTarget {
    let root: TreeNode = match serde_json::from_str(tree) {
        Ok(root) => root,
        Err(error) => return FocusTarget::Unreadable(error.to_string()),
    };
    select_focus_target_from_root(&root, launch_pids, excluded_ids)
}

fn select_focus_target_from_root(
    root: &TreeNode,
    launch_pids: &BTreeSet<i32>,
    excluded_ids: &[&str],
) -> FocusTarget {
    if launch_pids.is_empty() {
        return FocusTarget::NoWindow;
    }
    let mut candidates = Vec::new();
    collect(root, launch_pids, excluded_ids, &mut candidates);
    candidates.sort_by_key(|candidate| candidate.node_id);
    match candidates.len() {
        0 => FocusTarget::NoWindow,
        1 => FocusTarget::Window(candidates.remove(0)),
        _ => FocusTarget::Ambiguous(candidates),
    }
}

fn collect(
    node: &TreeNode,
    launch_pids: &BTreeSet<i32>,
    excluded_ids: &[&str],
    found: &mut Vec<FocusCandidate>,
) {
    if let Some(pid) = node.pid {
        // A window is only a game window when the launch itself owns the
        // process. Titles and classes are attacker-controlled decoration.
        if launch_pids.contains(&pid) && !is_excluded(node, excluded_ids) {
            found.push(FocusCandidate {
                node_id: node.id,
                pid,
            });
        }
    }
    for child in node.nodes.iter().chain(node.floating_nodes.iter()) {
        collect(child, launch_pids, excluded_ids, found);
    }
}

/// Classify restart ownership only after the same unique exact-launch window
/// selection used by Return succeeds. Focus facts never loosen that rule.
pub(crate) fn focused_ownership(
    tree: &str,
    launch_pids: &BTreeSet<i32>,
    excluded_ids: &[&str],
) -> Result<FocusOwnership, String> {
    let root: TreeNode = serde_json::from_str(tree).map_err(|error| error.to_string())?;
    let candidate = match select_focus_target_from_root(&root, launch_pids, excluded_ids) {
        FocusTarget::Window(candidate) => candidate,
        FocusTarget::NoWindow => return Err("exact launch has no compositor window".into()),
        FocusTarget::Ambiguous(_) => {
            return Err("exact launch has multiple compositor windows".into());
        }
        FocusTarget::Unreadable(error) => return Err(error),
    };
    let mut focused = Vec::new();
    collect_focused(&root, &mut focused);
    if focused.len() != 1 {
        return Err("compositor tree does not identify one focused node".into());
    }
    let focused = focused[0];
    if focused.id == candidate.node_id && focused.pid == Some(candidate.pid) {
        Ok(FocusOwnership::Launch)
    } else if is_excluded(focused, excluded_ids) {
        Ok(FocusOwnership::Excluded)
    } else {
        Ok(FocusOwnership::Other)
    }
}

fn collect_focused<'a>(node: &'a TreeNode, found: &mut Vec<&'a TreeNode>) {
    if node.focused {
        found.push(node);
    }
    for child in node.nodes.iter().chain(node.floating_nodes.iter()) {
        collect_focused(child, found);
    }
}

fn is_excluded(node: &TreeNode, excluded_ids: &[&str]) -> bool {
    let class = node
        .window_properties
        .as_ref()
        .and_then(|properties| properties.class.as_deref());
    excluded_ids
        .iter()
        .any(|excluded| node.app_id.as_deref() == Some(excluded) || class == Some(*excluded))
}

/// What korrid did about bringing one launch to the front.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FocusOutcome {
    Focused(i64),
    /// The launch owns no window, or several windows made the exact target
    /// ambiguous. A configured compositor cannot prove game focus.
    NothingToFocus,
    /// The compositor was reachable but the focus attempt itself failed.
    Failed(String),
}

/// Bring the unique window of one exact launch to the front, or report that
/// no unique target could be proven.
pub(crate) fn focus_launch_window(
    control: &dyn CompositorControl,
    launch_pids: &BTreeSet<i32>,
    excluded_ids: &[&str],
) -> FocusOutcome {
    // A launch with no processes owns no window, so the compositor is not
    // consulted at all and a compositor problem cannot fail that resume.
    if launch_pids.is_empty() {
        return FocusOutcome::NothingToFocus;
    }
    let tree = match control.tree() {
        Ok(tree) => tree,
        Err(error) => return FocusOutcome::Failed(error),
    };
    match select_focus_target(&tree, launch_pids, excluded_ids) {
        FocusTarget::Window(candidate) => match control.focus(candidate.node_id) {
            Ok(()) => FocusOutcome::Focused(candidate.node_id),
            Err(error) => FocusOutcome::Failed(error),
        },
        FocusTarget::NoWindow | FocusTarget::Ambiguous(_) => FocusOutcome::NothingToFocus,
        FocusTarget::Unreadable(error) => FocusOutcome::Failed(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    struct ScriptedCompositor {
        tree: String,
        tree_error: Option<String>,
        focus_error: Option<String>,
        focused: Mutex<Vec<i64>>,
    }

    impl CompositorControl for ScriptedCompositor {
        fn tree(&self) -> Result<String, String> {
            match &self.tree_error {
                Some(error) => Err(error.clone()),
                None => Ok(self.tree.clone()),
            }
        }

        fn focus(&self, node_id: i64) -> Result<(), String> {
            self.focused.lock().unwrap().push(node_id);
            match &self.focus_error {
                Some(error) => Err(error.clone()),
                None => Ok(()),
            }
        }
    }

    #[test]
    fn reads_exact_launch_and_excluded_portal_focus_from_the_tree() {
        let launch = pids(&[9100]);
        let game_focused = r#"{"id":1,"nodes":[{"id":2,"pid":4100,"app_id":"korri-portal","focused":false},{"id":3,"pid":9100,"focused":true}]}"#;
        let portal_focused = r#"{"id":1,"nodes":[{"id":2,"pid":4100,"app_id":"korri-portal","focused":true},{"id":3,"pid":9100,"focused":false}]}"#;
        assert_eq!(
            focused_ownership(game_focused, &launch, &["korri-portal"]).unwrap(),
            FocusOwnership::Launch
        );
        assert_eq!(
            focused_ownership(portal_focused, &launch, &["korri-portal"]).unwrap(),
            FocusOwnership::Excluded
        );
    }

    #[test]
    fn restart_ownership_rejects_ambiguous_launch_windows_and_focus_facts() {
        let launch = pids(&[9100, 9101]);
        let ambiguous_launch = r#"{"id":1,"nodes":[
            {"id":2,"pid":4100,"app_id":"korri-portal","focused":true},
            {"id":3,"pid":9100,"focused":false},
            {"id":4,"pid":9101,"focused":false}
        ]}"#;
        assert!(focused_ownership(ambiguous_launch, &launch, &["korri-portal"]).is_err());

        let multiple_focused_launch_nodes = r#"{"id":1,"nodes":[
            {"id":3,"pid":9100,"focused":true},
            {"id":4,"pid":9101,"focused":true}
        ]}"#;
        assert!(
            focused_ownership(multiple_focused_launch_nodes, &launch, &["korri-portal"]).is_err()
        );

        let several_focused = r#"{"id":1,"nodes":[
            {"id":2,"pid":4100,"app_id":"korri-portal","focused":true},
            {"id":3,"pid":9100,"focused":true}
        ]}"#;
        assert!(focused_ownership(several_focused, &pids(&[9100]), &["korri-portal"]).is_err());

        let mismatched_pid = r#"{"id":1,"nodes":[
            {"id":2,"pid":4100,"app_id":"korri-portal","focused":true},
            {"id":3,"pid":9200,"focused":false}
        ]}"#;
        assert!(focused_ownership(mismatched_pid, &pids(&[9100]), &["korri-portal"]).is_err());
    }

    #[test]
    fn focuses_the_window_of_the_exact_launch() {
        let compositor = ScriptedCompositor {
            tree: tree(9100),
            ..ScriptedCompositor::default()
        };
        assert_eq!(
            focus_launch_window(&compositor, &pids(&[9100]), &[PORTAL]),
            FocusOutcome::Focused(4)
        );
        assert_eq!(*compositor.focused.lock().unwrap(), vec![4]);
    }

    #[test]
    fn a_launch_without_a_window_is_not_a_failure_and_focuses_nothing() {
        let compositor = ScriptedCompositor {
            tree: tree(9100),
            ..ScriptedCompositor::default()
        };
        assert_eq!(
            focus_launch_window(&compositor, &pids(&[7777]), &[PORTAL]),
            FocusOutcome::NothingToFocus
        );
        assert!(compositor.focused.lock().unwrap().is_empty());
    }

    #[test]
    fn a_launch_with_no_processes_never_asks_the_compositor_anything() {
        let compositor = ScriptedCompositor {
            tree_error: Some("socket missing".into()),
            ..ScriptedCompositor::default()
        };
        assert_eq!(
            focus_launch_window(&compositor, &pids(&[]), &[PORTAL]),
            FocusOutcome::NothingToFocus
        );
    }

    #[test]
    fn an_unreachable_compositor_is_reported_instead_of_focusing() {
        let compositor = ScriptedCompositor {
            tree_error: Some("socket missing".into()),
            ..ScriptedCompositor::default()
        };
        assert_eq!(
            focus_launch_window(&compositor, &pids(&[9100]), &[PORTAL]),
            FocusOutcome::Failed("socket missing".into())
        );
        assert!(compositor.focused.lock().unwrap().is_empty());
    }

    #[test]
    fn a_failed_focus_command_is_reported() {
        let compositor = ScriptedCompositor {
            tree: tree(9100),
            focus_error: Some("swaymsg failed".into()),
            ..ScriptedCompositor::default()
        };
        assert_eq!(
            focus_launch_window(&compositor, &pids(&[9100]), &[PORTAL]),
            FocusOutcome::Failed("swaymsg failed".into())
        );
    }

    #[test]
    fn swaymsg_control_requires_absolute_paths_and_a_real_timeout() {
        let absolute = PathBuf::from("/run/korri-compositor/sway-ipc.sock");
        let program = PathBuf::from("/run/current-system/sw/bin/swaymsg");
        assert!(SwaymsgCompositorControl::new(
            program.clone(),
            absolute.clone(),
            Duration::from_secs(2)
        )
        .is_some());
        assert!(
            SwaymsgCompositorControl::new("swaymsg".into(), absolute, Duration::from_secs(2))
                .is_none()
        );
        assert!(SwaymsgCompositorControl::new(
            program,
            "sway-ipc.sock".into(),
            Duration::from_secs(2)
        )
        .is_none());
    }

    /// Two windows of one Sway workspace: the portal browser and a game.
    fn tree(game_pid: i32) -> String {
        format!(
            r#"{{
              "id": 1, "nodes": [
                {{"id": 2, "nodes": [
                  {{"id": 3, "pid": 4100, "app_id": "chromium-browser",
                    "nodes": [], "floating_nodes": []}},
                  {{"id": 4, "pid": {game_pid}, "app_id": null,
                    "window_properties": {{"class": "Neverball"}},
                    "nodes": [], "floating_nodes": []}}
                ], "floating_nodes": []}}
              ], "floating_nodes": []
            }}"#
        )
    }

    const PORTAL: &str = "chromium-browser";

    fn pids(values: &[i32]) -> BTreeSet<i32> {
        values.iter().copied().collect()
    }

    #[test]
    fn selects_the_window_owned_by_the_exact_launch() {
        assert_eq!(
            select_focus_target(&tree(9100), &pids(&[9100, 9101]), &[PORTAL]),
            FocusTarget::Window(FocusCandidate {
                node_id: 4,
                pid: 9100
            })
        );
    }

    #[test]
    fn refuses_a_launch_that_owns_no_window() {
        assert_eq!(
            select_focus_target(&tree(9100), &pids(&[7777]), &[PORTAL]),
            FocusTarget::NoWindow
        );
    }

    #[test]
    fn never_focuses_the_portal_browser_even_when_it_shares_the_launch() {
        // A shared-UID host means the browser can appear among launch
        // processes. Focusing it would hide the game behind the hub.
        assert_eq!(
            select_focus_target(&tree(9100), &pids(&[4100]), &[PORTAL]),
            FocusTarget::NoWindow
        );
    }

    #[test]
    fn ignores_a_window_that_only_claims_the_game_by_name() {
        let imposter = r#"{"id": 1, "nodes": [
            {"id": 5, "pid": 6000, "app_id": null,
             "window_properties": {"class": "Neverball"},
             "nodes": [], "floating_nodes": []}
          ], "floating_nodes": []}"#;
        assert_eq!(
            select_focus_target(imposter, &pids(&[9100]), &[PORTAL]),
            FocusTarget::NoWindow
        );
    }

    #[test]
    fn refuses_to_guess_between_several_windows_of_one_launch() {
        let two = r#"{"id": 1, "nodes": [
            {"id": 7, "pid": 9100, "app_id": null, "nodes": [], "floating_nodes": []},
            {"id": 6, "pid": 9101, "app_id": null, "nodes": [], "floating_nodes": []}
          ], "floating_nodes": []}"#;
        assert_eq!(
            select_focus_target(two, &pids(&[9100, 9101]), &[PORTAL]),
            FocusTarget::Ambiguous(vec![
                FocusCandidate {
                    node_id: 6,
                    pid: 9101
                },
                FocusCandidate {
                    node_id: 7,
                    pid: 9100
                },
            ])
        );
    }

    #[test]
    fn finds_a_floating_game_window() {
        let floating = r#"{"id": 1, "nodes": [], "floating_nodes": [
            {"id": 8, "pid": 9100, "app_id": null, "nodes": [], "floating_nodes": []}
          ]}"#;
        assert_eq!(
            select_focus_target(floating, &pids(&[9100]), &[PORTAL]),
            FocusTarget::Window(FocusCandidate {
                node_id: 8,
                pid: 9100
            })
        );
    }

    #[test]
    fn tolerates_unknown_compositor_fields() {
        let extended = r#"{"id": 1, "type": "root", "unknown": {"a": 1}, "nodes": [
            {"id": 9, "pid": 9100, "app_id": null, "shell": "xwayland",
             "nodes": [], "floating_nodes": []}
          ], "floating_nodes": []}"#;
        assert_eq!(
            select_focus_target(extended, &pids(&[9100]), &[PORTAL]),
            FocusTarget::Window(FocusCandidate {
                node_id: 9,
                pid: 9100
            })
        );
    }

    #[test]
    fn reports_an_unreadable_compositor_reply_instead_of_focusing() {
        assert!(matches!(
            select_focus_target("not json", &pids(&[9100]), &[PORTAL]),
            FocusTarget::Unreadable(_)
        ));
    }

    #[test]
    fn refuses_focus_for_a_launch_with_no_processes() {
        assert_eq!(
            select_focus_target(&tree(9100), &pids(&[]), &[PORTAL]),
            FocusTarget::NoWindow
        );
    }
}
