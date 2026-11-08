# Spike #382: Guide opens an overlay over any app on the Galaxy Z Fold7

Throwaway spike. It is not part of the Korri app, and nothing in the product
build uses it.

## Answer

Yes. On the stock Fold7, an AccessibilityService that filters key events sees the
physical Guide press in every app. It opens a `TYPE_ACCESSIBILITY_OVERLAY`
window over that app in 11 to 30 ms and holds the controller's input while the
window is open. Samsung and Google do nothing with the Guide press.

Device: SM-F971U1, fingerprint
`samsung/h8quew/h8q:17/CP2A.260605.016/F971U1UEU3AZJ8_OYM3AZJ8`. That is
**Android 17 (SDK 37), One UI 9**, not Android 16 as issue #382 says. The
controller was a Nacon MG-X over Bluetooth LE (`/dev/uhid`, sources
`KEYBOARD | GAMEPAD | JOYSTICK`). The phone was folded, so the test used the
cover screen.

| Test | Result | Evidence |
|---|---|---|
| Injected Guide (`adb shell input keyevent` / `input gamepad keyevent`) | Does not work. Injected keys never reach `onKeyEvent`. | `evidence/injected-logcat.txt` |
| Physical Guide | Works. 53 Guide presses, 27 opens, 26 closes, every one seen. | `evidence/physical-guide-logcat.txt` |
| Overlay over another app | Works over Chrome, the launcher, the dialer and the probe app. | `evidence/physical-guide-overlay-over-chrome.png`, `evidence/held-overlay-over-probe.png` |
| Buttons held from the app behind | Works. `onKeyEvent` returns true, so A never reached the probe. | `evidence/held-logcat.txt` |
| Stick and D-pad held from the app behind | Works, but only because the overlay window takes input focus. See below. | `evidence/held-logcat.txt` |
| `TYPE_APPLICATION_OVERLAY` with SYSTEM_ALERT_WINDOW | Not tested. Phone time ran out. | — |

## Numbers (physical Guide, Nacon MG-X)

- Key event to the filter (`uptimeMillis - KeyEvent.getEventTime()`): 0 to 4 ms in
  the Chrome run; 103 of 106 events took 1 or 2 ms.
- Guide key event to the overlay's first `onDraw`: n=27, min 11 ms, median 17 ms,
  p95 22 ms, max 30 ms. This is first draw in the app process. It is not photon
  time.

## What the held-input run showed (`evidence/held-logcat.txt`)

Order of the user's presses, with the probe app in front:

1. A twice with no overlay. The filter logs each press and returns false. The
   probe receives A DOWN/UP twice.
2. Guide. The overlay opens. The probe logs `window focus=false`. It gets
   **no** `onPause`.
3. A. The filter logs `HELD from app behind (1)` and `(2)`. The probe receives
   nothing.
4. D-pad down. **This controller sends the D-pad as `AXIS_HAT_Y` motion, not as
   a key.** The accessibility filter never sees it (it sees only `KeyEvent`).
   The focused overlay window receives it (`overlay window got motion ... hatY=1.0`).
5. Left stick. 81 motion events go to the overlay window and none to the probe.
6. Guide. The overlay closes and the probe gets focus back.
7. A. The probe receives it again.

## Surprises

1. **Injected keys skip the accessibility key filter.** On this phone,
   `adb shell input keyevent` never reaches `onKeyEvent`, from either the
   keyboard or the gamepad source. An adb-only test of this design is
   therefore useless. A test needs a real controller, or a uinput/uhid virtual
   device.
2. **The key filter alone does not hold a controller.** Sticks, triggers and
   (on many pads) the D-pad are `MotionEvent`s, and an AccessibilityService
   cannot filter them. The spike held them only because the overlay window is
   focusable and so takes input focus. A `FLAG_NOT_FOCUSABLE` overlay would let
   the sticks and D-pad through to the game.
3. **Taking focus has a cost.** The app behind gets `onWindowFocusChanged(false)`
   (no `onPause`). A game that pauses or mutes on focus loss will do so. This
   matters for ADR 0053 ("the game keeps running"). The fault is in the game, but
   Korri cannot avoid it while the overlay holds focus.
4. **Another key-filtering accessibility service is already on the phone:**
   `com.simonwjackson.irori/.IroriKeyboardOwnershipService` ("Kamoi keyboard
   control"). It is listed before the spike's service. The spike's filter still saw
   every physical key, but two key filters could conflict in a product.
5. `PhoneWindowManager interceptKeyTq interceptKeyResult=1` on Guide: One UI
   passes `KEYCODE_BUTTON_MODE` to the user. Game Launcher, Game Booster
   (`com.samsung.android.game.gametools`, `...gos`) and Google logged nothing.
6. `screencap` without `-d <display>` prints a warning into the PNG stream on
   the Fold. Display 0 (the inner screen) is black when the phone is folded.

## What the person would see

The spike enabled the service with `settings put secure ...`, so no prompt
appeared. A production install would show these (inferred from the platform,
**not observed in this spike**):

- Settings > Accessibility > Installed apps > Korri toggle, then the system
  "Allow Korri to have full control of your device?" dialog. That dialog lists
  "View and control screen" and "View and perform actions"; filtering keys falls
  under the second.
- A sideloaded APK on Android 13+ first shows "Restricted setting". The person
  must allow it in App info > ⋮ > "Allow restricted settings".
- `TYPE_ACCESSIBILITY_OVERLAY` needs no "Display over other apps" permission.

## How to run it again

```sh
nix develop .#android -c bash clients/android/spike-382/build.sh   # writes out/guide.apk and out/probe.apk
adb install -r clients/android/spike-382/out/guide.apk
adb install -r clients/android/spike-382/out/probe.apk
adb shell settings get secure enabled_accessibility_services           # record the value
adb shell settings put secure enabled_accessibility_services "<old>:com.simonwjackson.korri.guidespike/com.simonwjackson.korri.guidespike.GuideService"
adb shell settings put secure accessibility_enabled 1
adb logcat -s KorriGuideSpike KorriGuideProbe                          # press Guide on a real controller
# overlay variants without a controller:
adb shell am broadcast -p com.simonwjackson.korri.guidespike -a com.simonwjackson.korri.guidespike.CONTROL --es type app --ez focus false --es cmd show
```

`capture-physical.sh` and `capture-held.sh` take screenshots and logcat during
a timed window (set `SERIAL`, `DISPLAY_ID`, and optionally `DURATION`).

Restore: put the recorded value back, then
`adb uninstall com.simonwjackson.korri.guidespike` and `adb uninstall com.simonwjackson.korri.guidespike.probe`.
`evidence/settings-before.txt` and `evidence/settings-after.txt` show that this run restored both values exactly.
