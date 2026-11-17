# Toggle keyboard and Mouse mode (#434, toward D7′): the play proof on aka

Run on 2026-11-17 at 11:44 to 11:48 (aka's clock), on aka as root, from a
copy of this branch with `proof/play.py` as it is committed here. The
fixture APK was built on aka from the same Rust sources:
`/nix/store/45i4p937ywc7xahxcksqgh6mgq1jjfnq-korri-android-app-fixture-apk-0.1.0`.

```sh
# in that copy, with HOME, XDG_STATE_HOME and XDG_CACHE_HOME under /var/tmp/d7-controls-run:
nix run .#android-app-play-proof -- --build-store auto \
  --sunshine /nix/store/bvz43ydfridi8smy1gyqvzxk5v5xd3rg-sunshine-korri-2026.914.233613-korri \
  --port 5674 --adb-port 5174 --out /var/tmp/d7-controls-run/proof \
  --apk /nix/store/45i4p937ywc7xahxcksqgh6mgq1jjfnq-korri-android-app-fixture-apk-0.1.0/korri-android-app-unsigned.apk
```

No person took part. At the start aka read no turns lock, k10temp 60 °C, a
load of 2.21, no emulator and no session or flow of korri-sunshine;
`korri-sunshine-watch.log` read it every 5 s, 50 times, with none of
korri-sunshine (k10temp 96 °C at its highest). The emulator was Android 12
(API 31), google_apis, x86_64, headless, 2 cores; it and its adb server
stopped with no process left.

**Result: all 115 checks passed** (`result.json`): D10's, D11's, D5b's,
#495's and #494's, and these.

| Part | What happened |
| --- | --- |
| The listing | korrid's snapshot lists, in Artemis's order, Toggle keyboard (a command that closes the overlay), Crop to fill, Mouse mode (a choice, "0", with Artemis's six labels), Flip A/B and X/Y. The overlay reads "Toggle keyboard", "Crop tofill Off", "Mouse mode Multitouch" (`overlay-controls.png`) |
| The focus | From the stream's start NativeActivity's view holds the focus. After D10's letter keys the input method serves it and shows nothing (`input-method-after-keys.txt`); before the fix it showed for Slint's text view |
| Toggle keyboard | Two applies through the treaty with no value, each `ok`. `mInputShown` false, true, false, served to NativeActivity's view (`input-method-*.txt`). With it shown, the IME's insets leash shows (`flags=0x00000000`) and its layer is visible (`surfaceflinger-keyboard.txt`); the screenshot shows Gboard over the stream (`keyboard-0.png`) |
| A letter | One tap at (1268, 744), on its "y": the sender's Sunshine received the key 0x59 down and up (`sunshine-input.json`) |
| Mouse mode | Through the treaty with a choice: "3" (gaming track pad): a swipe reached the sender as 9 relative mouse moves, no touch and no absolute move; korrid's snapshot then said "3". "4" (disabled): a tap sent nothing. "0" (multi-touch): the tap was Artemis's absolute mouse again, since this Sunshine takes no touch (`result.json`, `steps.mouse_mode`) |
| The end | After Home: `keyboard: the soft keyboard hides; the stream's view gives the focus back`, and when the stream started again the view took the focus again (`logcat-app.txt`) |

The run before the fix (`proof/android-app-ime/` has the diagnosis) passed 104
of 105 checks: the input method showed, but the keyboard was never drawn and
no letter reached the sender. Its record is in this folder's history.

Not proven: a phone (D12); text that no key event carries; Artemis's
multi-finger taps of mouse modes 1 to 5; modes 1, 2 and 5 on the emulator
(host tests only: `clients/viewer/src/android/input.rs`).
