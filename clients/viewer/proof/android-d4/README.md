# Slice D4 — the input, on the Android emulator

Run on 2026-11-14 at 14:38 to 14:41 (zao local time) from commit
`7c108cc56a9a0aa9b42bd8c5a0dbc40f1c3a17de`, with a clean tree:

```sh
nix run .#viewer-android-proof -- --out /tmp/korri-d4/proof-2
```

No person took part. The tool built everything on zao. The emulator was
Android 12 (API 31), google_apis, x86_64, headless, 2 cores, `nice` 10, on
zao, with its sound to a WAV file on zao. The sender was stream-cadence's
private Sunshine fixture on aka, at aka's Tailscale address
`100.117.97.45`, port 50293, streaming 1280x720 at 60 fps with its
1000 Hz tone. Its Sunshine logged each input packet that it received
(`log_input`) and passed none on: its `mouse`, `keyboard` and `controller`
stay `disabled`. A game was live on aka (`live_games` in `result.json`);
the fixture runs beside the product and does not touch it, and no product
unit changed. The emulator was stopped at the end; no `qemu-system`
process was left.

| Store path | |
| --- | --- |
| APK (unsigned) | `/nix/store/qk428xsfkxky171dwksyphp0gjxdv9gc-korri-viewer-android-apk-0.1.0` |
| Refusal probe (unsigned, test only) | `/nix/store/00fgx992pnszmr7i0vkmvw9ldjssngkk-korri-viewer-refusal-probe-apk-0.1.0` |
| Linux viewer, for pairing | `/nix/store/73mxhaxnq0g8k7yyij45xvdmfq3di73z-korri-viewer-0.1.0` |
| Android tests (x86_64) | `/nix/store/l32q8psc366p3519wm11jm5s6bh81nrh-korri-viewer-android-check` |

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

All 50 passed (`result.json`), and the fixture's restore check passed.
The checks of D1 to D3 are the same as in `../android-d3/`. The new ones:

| Check | Evidence |
| --- | --- |
| The sender logs its input | `sender-facts.json`: `log_input` true |
| The Android tests pass, the input's included | `android-test-lib.txt`: 85 tests, 34 of them `input::`, `gamepad::`, `keyboard::`, `touch::` and `controller_list::` |
| Unbuffered dispatch asked for | `logcat.txt`: `input: unbuffered dispatch requested` |
| The pad's identity from Android | `logcat.txt`: `input: controller Korri test gamepad (16) vendor 045e product 7e57 external true`, then its axes: sticks X, Y and Z, RZ, triggers BRAKE and GAS, a hat, deadzones 5 %, 5 % and 13 % |
| The pad is controller 0 | `input: Korri test gamepad (16) is controller 0`; no joystick was attached at the start (`mask 0x0`) |
| The pad went | `input: controller Korri test gamepad (16) removed` 3.9 s after it first sent, then `controller number 0 is free` |
| The keys arrived | `sunshine-input.json`: ten keyboard packets, down and up of 0x80 and 'K', 'O', 'R', 'R', 'I' in order, flag 1 (non-normalized), no modifier |
| The touches arrived | 88 touch packets: two downs, two ups and 84 moves; the tap's down and up at (0.5, 0.5); the swipe's down at (0.25, 0.5) and its up at (0.75, 0.5) |
| The mouse arrived | Two absolute mouse packets at (960, 540) of 1919x1079: the core sends the picture's 1920x1080 less one (`InputStream.c`) |
| The pad arrived as Xbox number 0 | One arrival: controller 0, type 01 (Xbox, by Microsoft's vendor ID), capabilities 0001 (analog triggers), buttons 0000F7FF |
| Its button, stick, trigger and hat arrived | Controller packets with A (00001000), left stick X 32766, right trigger FF, the hat's left (00000004), B (00002000), X (00004000) |
| A held button kept arriving | 17 packets with X held: the press, the heartbeat, and Android's repeats of the held key, about every 50 ms from 0.44 s on (as Artemis, the app sends a repeat again) |
| It let go when it went | The last five controller packets have no button and mask 0000 |
| The heartbeat repeats every 250 ms | Those five are 246, 251, 250 and 268 ms apart: the release and its four repeats |
| No input failed | `input: ended: 10 keys, 46 controller states, 1 arrivals, 2 mouse, 124 touches sent; 0 unsupported, 0 failed` |
| The picture still moves after the input | `after-input-1.png` to `after-input-3.png`: frame IDs 4337, 4435, 4532, after 2970 to 3425 in `streaming-1.png` to `streaming-4.png` |

The counts at the two ends differ for known reasons. Sunshine logged 47
controller packets for the app's 46: the core sends a neutral state after
each arrival, for hosts without arrivals (`InputStream.c`). It logged 88
touch packets for the app's 124: Sunshine merges moves of one pointer that
wait in its queue (`src/input.cpp`, `batch`). The two relative mouse
moves at the start are the core's own wake-up (`Connection.c`).

The D3 checks held with the input: the tone played (43.7 s at 1000 Hz),
the sound went on after Android's audio server restarted, the frame record
was written (2991 rows, 2827 handed off, IDs rising), and Back still left
the stream: the virtual keyboard's Back is no controller's, so it reaches
the app, which sends Stop.

`sunshine.log` keeps Sunshine's info, warning and error lines only: its
debug and verbose lines can hold the launch's input key.

## The run before this one

The first run, from the same code with an earlier `run.py`, passed every
check of the app. It failed in the proof itself: `run.py` read Sunshine's
hexadecimal fields as decimal. That record is not kept.

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
