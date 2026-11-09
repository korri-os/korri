# #377: far-session plugin gameplay controls

Base: `2d59dabda` (includes #376). Runtime commit: `d366651ae`.
No Game.java, Surface host, Slint backend,
surface package, treaty, generated file or pause effect changed. #399 is untouched.

## Trace and fixes

Local controls already follow this path:

1. `HostRuntime::session_controls` checks the exact live launch, reads the
   runner identity from its live unit, then materializes that runner's plugin
   control declarations.
2. `OverlayController::refresh` asks for those `SessionControls` and maps the
   groups to `SurfaceGameplayControl` groups in `GameplayOverlay`.
3. Pico and Zone draw the groups and emit `InvokeGameplayControl`. Both already
   support command, toggle, choice and range controls. No view change is needed.
4. The surface controller sends `SessionControlInvoke`; the holder validates
   the declaration and submitted value, then korrid performs the plugin effect.

Two gaps prevented the same path for a streamed game:

- Both the phone's brain dispatch and a Linux holder's selected-peer dispatch
  returned an unconditional Unavailable instead of fetching or invoking the
  selected peer's controls. They now use the existing native peer RPC types.
  Selection identity and stream-player restrictions apply before forwarding;
  peer authentication and the holder's starter check remain unchanged.
- Composed launches (including peer `SessionPrepare`) lost
  `HostLaunchCommand.runner_id` before the existing `KORRI_LIVE_RUNNER_ID`
  unit environment was written. Explicit local runner launches kept it.
  Composition now supplies that same existing runner ID. No data schema changed.

The native response checker now preserves `SessionControlFailure` rather than
trying to parse it as `RpcFailure`. Successful replies must name the requested
launch. All six refusal reasons and all existing control value forms are tested.

## Reproductions and real-path exercise

Before forwarding, the new empty-control regression failed with:
`Unavailable: The selected peer does not expose live gameplay controls.`

After forwarding but before preserving the composed runner ID, the new real
plugin regression reached the far holder but the overlay had zero plugin groups.

After both fixes, the real-plugin regression passes. It launches the existing
mGBA plugin fixture over a real loopback native RPC server and uses the real
local dispatch, registry, OverlayController and RetroArch UDP executor. Both
brain and holder local modes receive the declared menu/quit group. Invoking
Open RetroArch menu sends and verifies `GET_STATUS`, `MENU_TOGGLE`, `GET_STATUS`
at the far executor. Unknown controls and invalid values retain their typed
refusals. The desktop test uses its existing frozen overlay policy; it does not
claim physical Android verification.

## Checks

| Check | Result |
| --- | --- |
| `nix run .#korrid-test -- --lib upstreams::` | 79 passed |
| `nix run .#korrid-test -- --lib surface::tests::overlay` | 6 passed |
| `nix run .#korrid-test -- --lib surface::overlay::` | 1 passed |
| `nix run .#korrid-test -- --lib stream_play::` | 17 passed, 1 existing ignored |
| Runner unit metadata regression (`systemd_runner_identity_query_reads_live_unit_environment`) | 1 passed |
| `nix run .#korrid-fmt-check` | Passed |
| `cargo clippy --lib --tests` with the korrid-test app's pinned toolchain/environment | Passed; 35 library warnings and 55 test warnings (31 duplicates), none in the new forwarding or integration-test code |
| Focused Android JVM tests (HomeKey, Input, SurfaceHost, StreamViewerBoundary, StreamViewerState) | 29 passed |
| Pico overlay update and Zone overlay Home tests | 13 passed |
| Pico and Zone TypeScript typechecks | Passed |
| `nix run .#android-shell-check` | Passed |
| Native shell `cargo test --locked --release --test hosted_overlay` | 1 passed |
| Native shell `cargo test --locked --release --lib input::android` | 7 passed |
| Pico and Zone real shell package verifiers (APK prerequisite) | Both passed |
| `nix run .#android-apk` | Passed, including arm64 korrid and shell release compilation, Java compilation and debug packaging |
| `git diff --check` | Passed |

The full repository test suite and Android lint were not run in this child
worktree. No Java source changed. Secret pre-commit hooks ran without bypass.

APK: `clients/android/app/build/outputs/apk/debug/app-arm64-v8a-debug.apk`
(85,828,489 bytes).
SHA256: `4d6ff3de6e1284ff1781c32dba663e2a3911cde2090c46ba3d272049cbc47174`.
The APK contains the runtime source in `d366651ae`; the following commit adds
only this evidence. It was built, not installed.

## Device evidence and handoff

Read-only `adb devices` found the Fold7 at `100.110.243.2:39705`.
Read-only `ssh root@aka` confirmed korrid active. After the parent installed
`@korri:retroarch` and `@korri:snes9x`, `korri --json launch-settings show --game
actraiser` reported both `@korri:snes9x/snes9x` and
`@simonwjackson:actraiser/actraiser` choosable, neither selected, and
`routeLayers: NeedsRunnerChoice`. Snes9x prepares the existing
`/var/lib/korri/roms/snes/ActRaiser (USA).sfc`.

The parent will review and land this branch, then deploy main's korrid to aka
and install main's APK after asking the phone owner. This child did not install
an APK, send phone input, deploy to aka, restart services, or start/stop games.
Physical overlay presentation and effects on aka remain unverified.

### User controller test after deployment

1. Open Korri on the Fold7. Launch ActRaiser from aka. In the runner choice
   prompt, select **Snes9x** (`@korri:snes9x/snes9x`), not ActRaiser Recomp.
   This must be a new launch after aka receives the runner-ID fix.
2. Once the stream is playing, press Guide/Home to open the gameplay overlay.
   The game must keep running until a plugin action changes it. Navigate with
   the controller, not taps (#394).
3. Confirm the plugin group lists **Open RetroArch menu** and **Quit game**.
   No standalone Pause is expected: this plugin declares no pause effect.
4. Invoke **Open RetroArch menu**. The Korri overlay should dismiss on success,
   and the stream should show RetroArch's menu on aka. Close that menu in
   RetroArch to continue the game. Repeat with Pico and Zone if available.
5. If safe to lose this test session's progress, reopen the overlay and invoke
   **Quit game**, confirm its question, and verify that the exact game on aka
   ends. Do this last.
6. For a game whose plugin declares no gameplay controls, verify that the
   overlay has only Korri's existing core controls and no invented Pause.

Do not use Recents as part of this test; #399 is separate. Capture screenshots
of the control group and the streamed RetroArch menu, plus aka's exact-session
status before/after the optional Quit test. No screenshot or physical controller
result has been claimed here.
