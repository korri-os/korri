use std::process::Command;

const ANDROID_PLUGIN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/plugins/android-app.plugin.ts");
const MGBA_PLUGIN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../plugins/mgba/plugin.ts");
const RETROARCH_PLUGIN: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../plugins/retroarch/plugin.ts"
);
const MOONLIGHT_PLUGIN: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../plugins/moonlight/plugin.ts"
);

const ENABLED_REPORT: &str = concat!(
    "plugin: @korri:android-app\n",
    "registered: yes\n",
    "enabled: yes\n",
    "registered-session-control: none\n",
    "provider: @korri:android-app\n",
    "system: android\n",
    "launcher: @korri:android-app/android-app\n",
    "transport: none\n",
    "runtime: none\n",
    "discovery: none\n",
    "session-control: none\n",
);

const DISABLED_REPORT: &str = concat!(
    "plugin: @korri:android-app\n",
    "registered: yes\n",
    "enabled: no\n",
    "registered-session-control: none\n",
    "provider: none\n",
    "system: none\n",
    "launcher: none\n",
    "transport: none\n",
    "runtime: none\n",
    "discovery: none\n",
    "session-control: none\n",
);

const MGBA_ENABLED_REPORT: &str = concat!(
    "plugin: @korri:mgba\n",
    "registered: yes\n",
    "enabled: yes\n",
    "registered-session-control: @korri:retroarch/open-menu\n",
    "registered-session-control: @korri:retroarch/quit\n",
    "provider: @korri:mgba\n",
    "system: gba\n",
    "launcher: none\n",
    "transport: none\n",
    "runtime: @korri:mgba/mgba\n",
    "discovery: @korri:mgba/gba-files\n",
    "session-control: @korri:retroarch/open-menu\n",
    "session-control: @korri:retroarch/quit\n",
);

const RETROARCH_ENABLED_REPORT: &str = concat!(
    "plugin: @korri:retroarch\n",
    "registered: yes\n",
    "enabled: yes\n",
    "registered-session-control: @korri:retroarch/open-menu\n",
    "registered-session-control: @korri:retroarch/quit\n",
    "provider: @korri:retroarch\n",
    "system: none\n",
    "launcher: @korri:retroarch/retroarch\n",
    "transport: none\n",
    "runtime: none\n",
    "discovery: none\n",
    "session-control: @korri:retroarch/open-menu\n",
    "session-control: @korri:retroarch/quit\n",
);

const SESSION_CONTROL_PLUGIN: &str = r#"
export const name = "retroarch";
export const launchers = {
        retroarch: {
          id: "@korri:retroarch/retroarch",
          plugin: "@korri:retroarch",
          command: "retroarch",
        },
      };
export const sessionControls = {
      openMenu: {
        order: 0,
        id: "@korri:retroarch/open-menu",
        owner: { kind: "launcher", id: "@korri:retroarch/retroarch" },
        label: "Open RetroArch menu",
        interaction: { kind: "command" },
        effect: "@korri:retroarch/open-menu",
      },
    };
"#;

#[test]
fn review_probe_explains_enabled_and_disabled_announcements() {
    assert_eq!(run_probe(&[ANDROID_PLUGIN]), ENABLED_REPORT);
    assert_eq!(run_probe(&[ANDROID_PLUGIN, "--disabled"]), DISABLED_REPORT);
    assert_eq!(
        run_probe(&[ANDROID_PLUGIN, "--review"]),
        format!("== enabled plugin ==\n{ENABLED_REPORT}\n== disabled plugin ==\n{DISABLED_REPORT}")
    );
    assert_eq!(run_probe(&[MGBA_PLUGIN]), MGBA_ENABLED_REPORT);
    assert_eq!(run_probe(&[RETROARCH_PLUGIN]), RETROARCH_ENABLED_REPORT);

    let moonlight = run_probe(&[MOONLIGHT_PLUGIN]);
    assert!(moonlight.contains("plugin: @korri:moonlight\nregistered: yes\nenabled: yes\n"));
    assert!(moonlight.contains("transport: @korri:moonlight/moonlight\n"));
    assert!(moonlight.contains("session-control: @korri:moonlight/disconnect\n"));
    assert!(moonlight.contains("session-control: @korri:moonlight/sgsr-sharpness\n"));
}

#[test]
fn probe_distinguishes_registered_and_enabled_session_controls() {
    let root = tempfile::tempdir().expect("temporary plugin directory");
    let path = root.path().join("retroarch.plugin.ts");
    std::fs::write(&path, SESSION_CONTROL_PLUGIN).expect("write session-control plugin");
    let path = path.to_str().expect("UTF-8 fixture path");

    let enabled = run_probe(&[path]);
    assert!(enabled.contains("registered-session-control: @korri:retroarch/open-menu\n"));
    assert!(enabled.contains("session-control: @korri:retroarch/open-menu\n"));

    let disabled = run_probe(&[path, "--disabled"]);
    assert!(disabled.contains("registered-session-control: @korri:retroarch/open-menu\n"));
    assert!(disabled.contains("session-control: none\n"));
}

fn run_probe(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_plugin_registry_probe"))
        .arg("@korri")
        .args(args)
        .output()
        .expect("plugin registry probe should start");
    assert!(
        output.status.success(),
        "probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("probe output should be UTF-8")
}
