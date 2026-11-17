# Why the input method's keyboard was never drawn (#434)

Run on 2026-11-17 at 11:02 to 11:03 (aka's clock), on aka as root, with
`proof/ime.py` and the fixture APK built on aka from this branch with the
proof trigger's `ime` commands and without the fix:
`/nix/store/phhrla502zcphpzgaqks63flks8dwzc8-korri-android-app-fixture-apk-0.1.0`.
No turns lock; k10temp 59 °C, load 0.89, no other emulator and no session or
flow of korri-sunshine at the start (`korri-sunshine-watch.log`: 17 reads,
none of korri-sunshine). One headless Android 12 emulator (API 31,
google_apis, x86_64, 2 cores); it stopped with no process left. No stream:
the shell's Velvet in front.

| Step | Command | `mInputShown` | Served view | IME insets leash | IME layer visible | Screenshot |
| --- | --- | --- | --- | --- | --- | --- |
| 0 | none | false | DecorView | hidden (0x1) | — | |
| 1 | `ime toggle` | **true** | NativeActivity's view | **hidden** | no | `1-toggle.png`: no keyboard |
| 2 | `ime layout` (`requestLayout` on the decor) | true | NativeActivity's view | hidden | no | no keyboard |
| 3 | `ime toggle` | false | NativeActivity's view | hidden | no | |
| 4 | `ime layout` | false | NativeActivity's view | hidden | — | |
| 5 | `ime callback-off`: `setWindowInsetsAnimationCallback(null)` on `SlintInputView` | false | NativeActivity's view | hidden | — | |
| 6 | `ime toggle` | **true** | NativeActivity's view | **shown (0x0)** | **yes, 1 rectangle** | `6-toggle.png`: Gboard's keyboard |
| 7 | `ime toggle` | false | NativeActivity's view | hidden | no | |
| 8 | `ime toggle` | true | NativeActivity's view | shown | yes | keyboard |

(`steps.json`; `ime-1-toggle.txt` and `ime-6-toggle.txt` are the input
method's dumps; `logcat-ime.txt` the trigger's and Android's lines.)

**The cause, proven:** with Slint's `WindowInsetsAnimation.Callback` on its
text view (`SlintAndroidJavaHelper`, Slint 1.18.1, unchanged), the input
method shows (`mInputShown=true`, `InsetsController: show(ime(),
fromIme=true)`) but the window never shows the IME's insets leash, so
nothing of the keyboard is drawn. Without the callback, each toggle shows
and hides it.

Why, from Android 12's source (`android-12.0.0_r34`), read and not traced:
`InsetsController.applyAnimation` runs the animation on the UI thread when
the window has an animation callback (`ViewRootInsetsControllerHost.
hasAnimationCallbacks`), and on its own thread otherwise
(`InsetsAnimationThreadControlRunner`), which applies the leash at once. The
UI-thread path starts in an `onPreDraw` (`InsetsController.startAnimation`),
and NativeActivity's window never draws its views (`ViewRootImpl.draw`: "The
app owns the surface, we won't draw"): its views stay flagged invalidated
(`.F....ID` in the dumps), so `invalidate()` schedules nothing. A
`requestLayout` (step 2) did not start it either; which step of the UI-thread
path waits is not known. Slint's own reports match: issue #11344 (Android 12,
fixed by moving the callback from the root view to the text view, which
1.18.1 has) and #11791 (Android 12, the keyboard "is not visible, but it
actually exists", closed as a duplicate).

The other paths that were asked about:

| Path | Here |
| --- | --- |
| `ANativeActivity_showSoftInput` instead of `toggleSoftInput` | Not run. It calls `showSoftInput(NativeActivity's view)`, the same input method request; the leash is the window's, after the request, so it would stay hidden the same way |
| android-activity 0.6.1's `show_soft_input` | `showSoftInput` on the decor view, through JNI (no NDK call). The same request, and it needs the decor view to be the served view |
| The soft input mode, `FLAG_ALT_FOCUSABLE_IM` | The window has no `ALT_FOCUSABLE_IM` and may use the input method (it is the IME's control target, `mControlTarget`). Its mode and flags are the same in steps 1 and 6; only the callback differs |
| GameActivity | Not needed: the fix below keeps NativeActivity |

**The fix** (`korri_viewer::soft_keyboard::without_insets_animations`): take
every insets animation callback off the window's decor view, its content view
and the content's children. The app's host does it on Android's main thread at
each resume and focus, and the Toggle keyboard control again before each
toggle. Slint should still hear the keyboard's insets through its
`setOnApplyWindowInsetsListener` on the decor view, and lose only the
animated progress of the keyboard's height (read in its Java, not tested:
no text field of the shell was tried). The play proof with the fix:
`proof/android-app-keyboard/`.
