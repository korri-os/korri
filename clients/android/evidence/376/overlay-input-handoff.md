# Controller handoff after viewer exit (2026-11-08)

## Reproduced cause

Releasing a hosted Surface calls `Shell::detach_overlay_window`, which resets the semantic mapper and removes its device registrations. When the main NativeActivity has already regained focus, `AndroidInput::set_active(true)` did not clear its own device-registration cache. Later key/axis samples were therefore deltas for devices the mapper no longer knew. They were ignored.

The supplied log is consistent with that order: main-window focus at 23:05:16.335 precedes destruction of Game's overlay Surface at 23:05:16.747. A focused input regression reproduced the result: the next D-pad press yielded `[]`, not `Pressed Down`.

The Surface-release callback now resets the Android source's cached device registrations even if the main window already has focus. Its next sample declares and initializes that controller again. Slint's overlay component is also hidden before being dropped, removing its shown-component reference before renderer release.

Disconnect and End stop the viewer; Open Korri backgrounds it. Both `stopAndAwait` and `background` finish their Game activities through `finishActivities`, so all three release the same hosted Surface and use the repaired handoff. No session presentation, far-game state or input-hold policy changed. No PiP behavior changed.

## Checks

- Regression before fix: next D-pad press ignored (`[]` versus `Button(Down)`).
- `cargo test --lib input::android`: 7 passed.
- `cargo test --features fixture --test shell hands_controller_pressed_back_to_the_catalog`: 3 passed. The real shell, package engine, Slint renderer and fixture model-watch path cover the shared Surface-release/catalog transition for Disconnect, End and Open Korri. They cover both focus orders and model-before/model-after teardown. They assert one visible catalog window and delivery of key and stick `Pressed` messages using the same controller ID as the overlay. These are shell-side handoff tests, not remote session-effect tests.
- `cargo test --test hosted_overlay`: 1 passed.
- `cargo fmt --check` and `cargo clippy --features fixture --lib --test shell --test hosted_overlay -- -D warnings`: passed (including typecheck).
- `nix run .#android-apk`: passed, including both surface verifiers and arm64 native compilation.
- `git diff --check`: passed.
- No Java changed. Android lint was not repeated; the preceding run had 20 errors/281 warnings, as already reported.

The parent authorized an install with `adb install -r` on `100.110.243.2:39705`, without clearing data. The user owns all navigation and device verification after installation.

APK SHA256: `8eb2ca76808f7c941d75ed1db31cd80f9b56d3d1e822cf82fc9147b85af3bb3d`.
