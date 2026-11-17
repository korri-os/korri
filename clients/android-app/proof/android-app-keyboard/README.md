# Toggle keyboard (#434, toward D7′) — the play proof on aka

Run on 2026-11-17 at 10:30 to 10:34 (aka's clock), on aka as root, from a
copy of this branch as `22a4a62df` has it plus the keyboard step's
SurfaceFlinger and window dumps (`proof/play.py`). The fixture APK was built
on aka from `2df5e3988` (the same Rust sources):
`/nix/store/qmavlhc6zcrc060pjxip3p4gzf552dyw-korri-android-app-fixture-apk-0.1.0`.

```sh
# in that copy, with HOME, XDG_STATE_HOME and XDG_CACHE_HOME under /var/tmp/d7-controls-run:
nix run .#android-app-play-proof -- --build-store auto \
  --sunshine /nix/store/bvz43ydfridi8smy1gyqvzxk5v5xd3rg-sunshine-korri-2026.914.233613-korri \
  --port 5674 --adb-port 5174 --out /var/tmp/d7-controls-run/proof \
  --apk /nix/store/qmavlhc6zcrc060pjxip3p4gzf552dyw-korri-android-app-fixture-apk-0.1.0/korri-android-app-unsigned.apk
```

No person took part. The run started when aka read no turns lock, k10temp
59 °C, a load of 2.46, no emulator and no TCP session or UDP flow of
korri-sunshine; `korri-sunshine-watch.log` read the same every 5 s, 52 times,
with no session or flow of korri-sunshine (k10temp 96 °C at its highest). The
emulator was Android 12 (API 31), google_apis, x86_64, headless, 2 cores; it
and its adb server stopped with no process left.

**Result: 104 of 105 checks passed; the run fails.** The one that failed is
`a_letter_of_the_soft_keyboard_reaches_the_sender`.

| Part | What happened |
| --- | --- |
| The listing | korrid's snapshot lists `…:toggle-keyboard` first, label "Toggle keyboard", `{"kind":"command"}`, `dismissOnSuccess: true`; then Crop to fill and Flip A/B and X/Y. In the overlay, tesseract reads "Toggle keyboard", "Crop tofill Off", "Flip A/B and X/Y Off" (`overlay-controls.png`), and Crop to fill, chosen one row below Toggle keyboard, took effect |
| The apply | Three applies of the command, each through the treaty with no value (`{"kind":"Apply","value":{"control":"…:toggle-keyboard"}}`), each `ok`, each with `keyboard: the soft keyboard toggled; the stream's view has the focus` and `korri-app: viewer control toggle-keyboard ran` (`logcat-app.txt`) |
| The focus | Before the first apply, the input method served `SlintInputView`, and showed for it: D10's letter keys had left it there (`input-method-before.txt`). After each apply it served `android.app.NativeActivity$NativeContentView` (`input-method-first.txt` to `-last.txt`) |
| The toggles | `mInputShown`: true, then false, true, false: each apply turned the keyboard over, and Gboard ran `onStartInputView` for the stream's view (`logcat-input-method.txt`) |
| The keyboard | Not drawn. While `mInputShown=true` the window manager says `mImeShowing=true`, the IME's frame `[136,520][2400,1080]` and `mControlTarget` the app's NativeActivity window (`window-insets-keyboard.txt`). In SurfaceFlinger the IME's insets leash (`animation-leash of insets_animation`) has `flags=0x00000001`, hidden, and every IME layer has an empty visible region (`surfaceflinger-keyboard.txt`). The screenshot shows the stream only (`keyboard-2.png`). The app logged `InsetsController: show(ime(), fromIme=true)` |
| A letter | Three taps at (1268, 744), in the IME's touchable region, each after 5 s: no keyboard packet at the sender (`sunshine-input.json`) |
| The end | After Home: `keyboard: the soft keyboard hides; the stream's view gives the focus back` |

What this proves: korrid lists, applies and answers the control through the
treaty; the app moves the input method from Slint's text view to
NativeActivity's view and toggles it there. What it does not prove: that a
person sees the keyboard or types through it. On this emulator nobody can:
the app's window controls the IME's insets, and the IME's leash stays hidden.

Not known: why the leash stays hidden. The input method also shows nothing
for `SlintInputView` over the stream: D11's screenshot `controls-fit-again.png`,
taken while `mInputShown=true` for it, before this control ran, shows the
status bar's keyboard icon and no keyboard. So the hidden keyboard is not
this change's view. One guess, not checked: NativeActivity takes the
window's surface, so its ViewRootImpl draws no frame of its own, and the
insets animation's transaction waits for one. Not known either: whether a
phone (D12) shows it.
