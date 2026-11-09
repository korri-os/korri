# Premature overlay render (2026-11-08)

## Cause and fix

At 22:23:55.309 the Android Surface adapter tried to render before asynchronous korrid/model/logic delivery had created and shown its component. Skia rejected that render: `Renderer must be associated with component before use`.

The host-neutral `ComponentWindow` now owns the existing WindowAdapter/redraw lifecycle. `SurfaceWindow` uses it with Skia. It draws only while Slint's own `Window::is_visible()` is true, without consuming a pending redraw while waiting for `show()`. Surface release also hides that exact Slint window before suspending EGL, so its shown-component reference does not survive release. No game window or decoder is involved in that release.

The regression test runs this same WindowAdapter with Slint's software renderer. It polls before any component exists, creates but does not show a component, shows/draws it, hides it, then shows/draws it again. Before the visibility guard, it failed on the first poll because the adapter rendered with no component. After the guard it passes.

## Stream-drop attribution

The supplied original log shows two separate events:

- 22:23:55.310: only overlay `61fa3da` is removed. The video Surface `ff4627b` remains; SurfaceFlinger still presents its buffers at 22:23:55.329.
- 22:24:02.031: Android changes focus from Game to Samsung `com.android.quickstep.RecentsActivity`.
- 22:24:02.526: Game receives `stopped(true)`; its video Surface is destroyed at 22:24:02.528 and stream teardown starts at 22:24:02.532. The decoder watchdog and interrupted control stream occur during that teardown.

The `IllegalStateException` text at 22:24:02.583 is a `W LegacyMessageQueue` diagnostic about posting to an already-dead handler, not a shown `AndroidRuntime` fatal exception. The overlay error callback does not stop Game. A new JVM test invokes the real `Game.hideSurfaceOverlay` through the renderer-failure bridge and verifies that only the overlay is removed, the stream Surface and Game remain, and the input hold is released. Decoder/background policy is unchanged.

## Checks and device handoff

- `cargo test --test hosted_overlay`: 1 passed (after reproducing 1 failure without the guard).
- `cargo fmt --check`: passed.
- `cargo clippy --test hosted_overlay -- -D warnings`: passed, including typecheck.
- Focused Android JVM checks: 23 passed (`KorriOverlayInputTest`, `KorriSurfaceHostTest`, `KorriStreamViewerBoundaryTest`, `KorriStreamViewerStateTest`).
- `nix run .#android-apk`: passed, including arm64 release builds of korrid and the shell.
- `git diff --check`: passed.
- Existing Android lint result remains 20 errors/281 warnings (first: unchanged `StreamSettings.java:134`, `MissingSuperCall`); not rerun for this Rust-only runtime fix.

With the parent's confirmation, installed using `adb -s 100.110.243.2:39705 install -r` (Success). No data was cleared. No phone navigation or test input followed installation: the user owns the manual BotW/Guide test.

Installed APK SHA256: `1e63a3b649d7ce924d282669ad40724197905864f06ff33487e73f30cab99bb1`.
