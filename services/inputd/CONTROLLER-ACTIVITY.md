# Controller activity and graphical idle

The Mini V2 enables `controller-activity` through inputd's existing immutable
argv action configuration. Other boards leave it unset. The shared compositor
ships the activity command, but that alone does not reset idle.

`seat <name|*> idle_notify` calls wlroots' native idle notifier. It does not
move or unhide a cursor, send input events, change focus, or alter the timeout.
It is runtime-only. The small patch lives in `nix/sway-idle-notify.patch`.
`nix/sway-activity.nix` asserts Sway 1.11 and its pinned source hash, keeps the
upstream patches and dependencies, and is the package used by the shared host.
The flake also exports it as `packages.<system>.sway-controller-activity`.

Inputd observes its already validated normalized Xbox target and authenticated
InputPlumber DBus actions. Observation does not consume, modify or duplicate
routed controller events. In particular, direct volume and Home actions still
run their existing policies. Both game and portal ownership count as activity.

| Input or condition | Idle observation |
|---|---|
| Buttons, hats, direct semantic actions | Press, release, and held state count |
| Stick axes | Engage above 4096/32768 (12.5%); release at 3072/32768 |
| Triggers | Engage above 8/255; release at 4/255 |
| Neutral noise, duplicates, sync records, invalid analog values | No new activity |
| Target loss, provider change, ambiguous sources | Clear holds and pending activity |
| Event bursts or repeated hotplug | At most one activity action per second |

Axis ranges come from InputPlumber 0.75.2 `src/input/target/xb360.rs`. The idle
noise margins are not a new calibration schema. They deliberately count partial
travel below the portal navigation threshold. The margins still need physical
acceptance on the Mini V2. Drift outside these margins can keep the screen on;
input inside the deadzone does not wake it.

The existing 50 ms runtime tick drains one coalesced activity flag. A held
control refreshes it. The command uses the existing cgroup-contained action
runner as the action user, with a separate single permit, a 500 ms command
limit, and a 1024-byte output limit. It cannot occupy volume/Home action slots.
There is no queue, raw-device access change, or shell polling service. The
production OLED timeout remains 300 seconds. Coalescing can extend the last
activity time by up to about one second.

## Checks

- `nix build .#checks.x86_64-linux.sway-controller-activity` starts real headless
  Sway, two Wayland event observers, and swayidle. Only the test uses a two-second
  timeout. It checks native idle/resume, sustained activity, re-idle, unchanged
  window focus and no input events from the activity command. A test-only
  persistent virtual pointer validates that the observers work before and after.
- A separate phase checks actual headless output power-off and power-on through
  swayidle. Sway's existing output-power path can re-enter pointer focus; this is
  distinct from the activity command, which emits no pointer events.
- `nix develop .#inputd --command cargo test --manifest-path services/inputd/Cargo.toml --all-targets`
  includes burst, deadzone, direct-action authentication, ownership routing and
  topology-loss regressions.

Build on Zao, Fuji or CI. Do not build on a device or relax cache signatures.
Hardware acceptance still needs all buttons, both sticks and triggers, volume,
and Home waking `DSI-1`; continuous controller use beyond five minutes; and a
five-minute neutral-controller blank without losing USB recovery. These checks
do not prove physical OLED behavior or hardware noise margins.
