# Slice D4 — the input, on the Android emulator

Run on 2026-11-14 at 15:17 to 15:22 (zao local time) from commit
`d87faad8f24013bbf2f209f0cc2de931972c71ca`, with a clean tree: D4 with
the review's fixes, among them the core's input gate.

```sh
nix run .#viewer-android-proof -- --out /tmp/korri-d4/proof-3
```

No person took part. The tool built everything on zao. The emulator was
Android 12 (API 31), google_apis, x86_64, headless, 2 cores, `nice` 10, on
zao, with its sound to a WAV file on zao. The sender was stream-cadence's
private Sunshine fixture on aka, at aka's Tailscale address
`100.117.97.45`, port 45047, streaming 1280x720 at 60 fps with its
1000 Hz tone. Its Sunshine logged each input packet that it received
(`log_input`) and passed none on: its `mouse`, `keyboard` and `controller`
stay `disabled`. A game was live on aka (`live_games` in `result.json`);
the fixture runs beside the product and does not touch it, and no product
unit changed. The emulator was stopped at the end; no `qemu-system`
process was left.

| Store path | |
| --- | --- |
| APK (unsigned) | `/nix/store/xmbsfg69n2343rypzxwjw7n1hpc4fk0m-korri-viewer-android-apk-0.1.0` |
| Refusal probe (unsigned, test only) | `/nix/store/00fgx992pnszmr7i0vkmvw9ldjssngkk-korri-viewer-refusal-probe-apk-0.1.0` |
| Linux viewer, for pairing | `/nix/store/jl0j6j1id8c6pyzng4k82yhkciy2y2a0-korri-viewer-0.1.0` |
| Android tests (x86_64) | `/nix/store/d3x64sh3xdzn0f7rr62k9xiha49474qz-korri-viewer-android-check` |

## What the proof did

While the stream ran, with Android's own tools in the emulator
(`result.json`, `input.steps`):

1. `input keyevent KEYCODE_K KEYCODE_O KEYCODE_R KEYCODE_R KEYCODE_I`: the
   virtual keyboard.
2. `input tap 1200 540`, the middle of the picture, which the app placed at
   (240, 0)-(2160, 1080) in its 2400x1080 window.
3. `input swipe 720 540 1680 540 300`: a quarter of the picture to each
   side of the middle.
4. `input mouse tap 1200 540`.
5. `hid` with `gamepad.json`: a USB game pad through `/dev/uhid`,
   "Korri test gamepad", vendor 045e (Microsoft), product 7e57, which no
   kernel driver or key layout knows. After 2 s: A, the left stick fully
   right, the right trigger (gas) fully in, the hat left, B, X held for a
   second; then `hid` ends and the pad goes.

## Checks

All 51 passed (`result.json`), and the fixture's restore check passed.
The checks of D1 to D3 are the same as in `../android-d3/`. The new ones:

| Check | Evidence |
| --- | --- |
| The sender logs its input | `sender-facts.json`: `log_input` true |
| The Android tests pass, the input's included | `android-test-lib.txt`: 90 tests, 39 of them `input::`, `gamepad::`, `keyboard::`, `touch::` and `controller_list::`, the gate's among them |
| Unbuffered dispatch asked for | `logcat.txt`: `input: unbuffered dispatch requested` |
| The pad's identity from Android | `logcat.txt`: `input: controller Korri test gamepad (16) vendor 045e product 7e57 external true`, then its axes: sticks X, Y and Z, RZ, triggers BRAKE and GAS, a hat, deadzones 5 %, 5 % and 13 % |
| The pad is controller 0 | `input: Korri test gamepad (16) is controller 0`; no joystick was attached at the start (`mask 0x0`) |
| The pad went | `input: controller Korri test gamepad (16) removed` 3.8 s after it first sent, then `controller number 0 is free` |
| The keys arrived | `sunshine-input.json`: ten keyboard packets, down and up of 0x80 and 'K', 'O', 'R', 'R', 'I' in order, flag 1 (non-normalized), no modifier |
| The touches arrived | 143 touch packets: two downs, two ups and 139 moves; the tap's down and up at (0.5, 0.5); the swipe's down at (0.25, 0.5) and its up at (0.75, 0.5) |
| The mouse arrived | Two absolute mouse packets at (960, 540) of 1919x1079: the core sends the picture's 1920x1080 less one (`InputStream.c`) |
| The pad arrived as Xbox number 0 | One arrival: controller 0, type 01 (Xbox, by Microsoft's vendor ID), capabilities 0001 (analog triggers), buttons 0000F7FF |
| Its button, stick, trigger and hat arrived | Controller packets with A (00001000), left stick X 32766, right trigger FF, the hat's left (00000004), B (00002000), X (00004000) |
| A held button kept arriving | 18 packets with X held: the press, the heartbeat, and Android's repeats of the held key, about every 50 ms from about 0.4 s on (as Artemis, the app sends a repeat again) |
| It let go when it went | The last five controller packets have no button and mask 0000 |
| The heartbeat repeats every 250 ms | Those five are 252, 249, 254 and 255 ms apart: the release and its four repeats |
| No input failed | `input: ended: 10 keys, 47 controller states, 1 arrivals, 2 mouse, 177 touches sent; 0 unsupported, 0 outside the stream, 0 failed` |
| The gate closed before the core stopped | `logcat.txt`: `input: the core takes no more input` at 91.180, before the core's `Stopping input stream...` in the same millisecond, on the viewer's thread; the app had sent Stop at 91.130 |
| The picture still moves after the input | `after-input-1.png` to `after-input-3.png`: frame IDs 4329, 4424, 4522, after 3015 to 3457 in `streaming-1.png` to `streaming-4.png` |

The counts at the two ends differ for known reasons. Sunshine logged 48
controller packets for the app's 47: the core sends a neutral state after
each arrival, for hosts without arrivals (`InputStream.c`). It logged 143
touch packets for the app's 177: Sunshine merges moves of one pointer that
wait in its queue (`src/input.cpp`, `batch`). The two relative mouse
moves at the start are the core's own wake-up (`Connection.c`).

The D3 checks held with the input: the tone played (42.8 s at 1000 Hz),
the sound went on after Android's audio server restarted, the frame record
was written (2911 rows, 2771 handed off, IDs rising), and Back still left
the stream: the virtual keyboard's Back is no controller's, so it reaches
the app, which sends Stop.

`sunshine.log` keeps Sunshine's info, warning and error lines only: its
debug and verbose lines can hold the launch's input key.

## The runs before this one

Two earlier runs, before the review's fixes, are not kept. The first, with
an earlier `run.py`, passed every check of the app and failed in the proof
itself: `run.py` read Sunshine's hexadecimal fields as decimal. The second,
from `7c108cc56`, passed all 50 of its checks; a review then found the
race that the gate closes.

The gate is not exercised here at its hard moment: this run ends with
Back, and the app drops its input before it sends Stop. A stream that the
host or the network ends while the app's thread sends is proven by the
gate's test only (`input::tests::no_send_reaches_the_core_once_the_gate_has_closed`,
which failed against a plain flag and against a bare `RwLock`).

## Not proven here

- A real phone: its controllers (Bluetooth, the cabled pads that Artemis's
  USB driver took, ADR 0064), its touch screen, a real keyboard and mouse,
  and the arm64 library.
- Any time from a press to its packet (number 5 of ADR 0062), and whether
  unbuffered dispatch changes anything on NativeActivity: step H, on the
  Galaxy Z Fold3. The emulator's times mean nothing (ADR 0063).
- A host that takes no touch: the absolute mouse is proven by the tests
  only. Sunshine takes touch.
- Two controllers at once, a controller attached before the stream, mouse
  emulation, and a key with modifiers: the tests only.
- What the host does with the input: the fixture passes none of it on.
