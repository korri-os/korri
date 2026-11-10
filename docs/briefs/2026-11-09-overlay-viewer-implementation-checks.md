# Overlay viewer facts and stream controls: Phase 2 checks

Implemented against `origin/main` at
`04feba517740de5527094d3bdf4d13a9ff837826`, after rebasing the original work.
The treaty-first commit is `cd91ebfc749d2fad6b7f1ca0362b691613a6869b`.
The approved design remains in
`2026-11-09-overlay-viewer-facts-and-stream-controls.md`.

## Scope and handoff

- `LinuxStreamViewer` supplies an exact-launch snapshot and apply operation.
  Android implements them through the existing `KorridServer` JNI class and
  `KorriStreamViewer.Session`. No new surface RPC or interaction kind exists.
- The surface hub merges the root's clock. The overlay projects phone battery
  capacity, RTT, measured encoded-video kbps and actual display discards, plus
  the viewer's declared gameplay controls in a “Stream controls” group.
- Measurements are independent of Moonlight's HUD. Actual discard releases,
  not the misleading `frameDropped` pacing flag, increment the drop count.
  JNI carries legacy `StreamHealthSample.sampledAtMs` internally so a new
  snapshot receipt cannot make an old measurement fresh. It is not a surface
  field. Both the producer and korrid enforce the 3,000 ms cutoff.
- Overrides belong to the current Session. Reads use the live Game config;
  writes do not use SharedPreferences. Activity recreation restores the same
  stream's changes. A new Session uses the original stream-start baseline.
  Zoom/pan teardown previously wrote `number_zoom_scale` and pan offsets
  (`36f645eac:clients/android/app/src/main/java/com/limelight/Game.java:1850–1856`).
  That write no longer runs for a Korri viewer. Session captures and restores
  the existing `PanZoomHandler.setInitialZoomAndPan(scale, offsetX, offsetY)`
  values instead. No persisted schema changes or preference rollback occur.
- Touch-only and conditional controls are omitted when unsupported. The HUD
  and floating menu button are not declared or displayed in Korri mode.
- Pico's overlay adapter carries `clockLabel`, `batteryPercent` and
  `streamHealth`. Zone exposes `overlayViewerFacts(model)` in its overlay
  state module. Existing group/control renderers consume the stream group.
  No Slint view, layout, color or focus model was changed. The designer owns
  the status strip and section visuals.

## Rebase integration

The rebase had no textual conflicts. A semantic conflict needed a fix:
ADR 0054's accessibility service consumed Guide even over Game, which would
raise the full shell rather than use #376's in-place overlay. The service
now forwards Guide to the exact focused stream's existing Game key gate.
Native/external apps retain their show-Korri behavior. The integration test
checks both routes, including Menu delivery to the hosted surface.

Both JNI keep-rule families remain: viewer snapshot/apply and ADR 0054's app
launcher/save bridge. A JVM test checks the actual Java descriptors without
initializing the ARM library. The built APK was also inspected with `dexdump`.

## Checks after the rebase

| Check | Result |
|---|---|
| `nix run .#korrid-check -- --types-only`, then `check-treaty.sh --check` | Pass; regeneration changed no additional treaty files. |
| korrid `cargo fmt --check` | Pass. |
| `cargo test --lib surface::overlay::live_overlay_tests` | 2 pass: local dispatch/readback, no far invoke, core controls. |
| `cargo test --lib surface::viewer::tests` | 1 pass: projection, invalid values, receipt/sample stale clearing. |
| `cargo test --lib surface::tests::overlay` | 6 pass, including root clock propagation. |
| korrid `cargo clippy --lib --tests` | Pass, with existing warnings. |
| `nix run .#android-brain-check` | Pass for aarch64 Android. |
| `nix run .#android-jvm-check -- --tests …` | 34 pass across ViewerControls, ViewerGuide, StreamViewerState, SurfaceHost, OverlayHomeKey, ViewerVideoStats and MoonlightActionExecutor. |
| Pico/Zone typechecks and four focused overlay/fact test files | Pass; 15 tests. |
| `nix run .#android-apk` | Pass, including both package verifiers and R8. |
| APK DEX inspection | Viewer snapshot/apply, Guide forwarding, native-app reporting and app-launcher entry points remain present. All three arm64 native libraries are packaged. |
| `apksigner verify --verbose` | Verifies with v1 and v2; v1 reports META-INF-entry warnings. |
| `:app:lintDebug` | Still fails on exactly 20 baseline errors. Every error site exists unchanged on `origin/main`; none is on changed/new lines. No suppression was added. |
| `git diff --check`, commit secret scan | Pass. |

APK: `clients/android/app/build/outputs/apk/debug/app-arm64-v8a-debug.apk`

- Size: 86,649,355 bytes.
- SHA-256: `f744c74ea355e985390739cbe8348e20a47238da06bad795c910df4e3bb9da67`.

## Not claimed

No installation, deployment or landing occurred. The phone/aka acceptance path
has not been run with this build. Strip and section visuals remain with the
designer. This is not a claim that either issue's on-phone done condition has
passed. The full combined test suite was not run in this child worktree.
