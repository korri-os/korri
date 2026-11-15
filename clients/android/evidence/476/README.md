# #476 slice 1 — today's Artemis live resize, on the emulator

**Fixed.** Today's live resize froze the picture after the encoder swap.
Sunshine swapped its encoder, and the app's decoder followed to the new
size. The picture then showed the first frame of the new encoder and did
not move again, after the Portrait resize and again after the resize back
(runs 1 and 3). A debugger run confirmed the cause in Sunshine (run 3).
korri-plugins `7e9c19c` changes patch 0044. With the fixed Sunshine the
picture continues at the new size in both directions (run 5). The Artemis
app is not changed and needs no change.

| Run | Where | Sunshine | Result |
| --- | --- | --- | --- |
| 1 (`run-1/`) | emulator on zao, fixture on aka | approved `bvz43…` (patch set `edf475f2…`) | picture freezes after each swap |
| 2 (`run-2/`) | the same, zao load about 23 | the same | the app refused a resize that it then applied (below) |
| 3 (`run-3/`) | emulator and fixture on aka, gdb on the fixture's Sunshine | the same | picture freezes; the debugger shows why |
| 4 (`run-4/`) | the same, with gdb | fixed `wr5cindz…` (patch set `e3614508…`) | picture continues; the debugger shows the fix at work |
| 5 (`run-5/`) | emulator and fixture on aka, no debugger | the same fixed package | **picture continues at 720x1280 and back at 1280x720** |

Runs 3 to 5 follow the owner's rule of 2026-11-15: the emulator runs on
aka (2 cores, nice 10, its own adb server), and the proof starts the
sender fixture there as a local process (`--sender local`). The emulator
reaches the fixture at 10.0.2.2, the host's loopback. Moonlight Embedded
on aka pairs at 127.0.0.1.

## Cause (confirmed by run 3)

`run-3/gdb.txt` has breakpoints on `eglMakeCurrent`, `eglDestroyContext`
and `eglCreateImage` in the fixture's Sunshine (`run-3/gdb-commands.txt`).
At each swap, on the encoder thread (LWP 3000211):

1. Before the swap, `eglCreateImage` (the import of each captured frame)
   runs with the old device's context current (`0x707db80c0ee0`).
2. The new VAAPI device's `egl::make_ctx` (from `va::va_t::init`) calls
   `eglMakeCurrent` with the new context (`0x707db8261390`).
3. After the swap, the old device is destroyed (`~va_vram_t`, `~va_t`,
   from `encode_run`). At its `eglMakeCurrent(…, EGL_NO_CONTEXT)` the
   current context is still the new one. So the old device's textures,
   framebuffers and programs, destroyed just before, were deleted in the
   new device's context. Then `egl::ctx_t`'s deleter releases that context
   and `eglDestroyContext` destroys the old one.
4. Every `eglCreateImage` after that runs with no current context (`0x0`).
   The new device's OpenGL conversion does nothing, the NV12 surface keeps
   the last picture that the new device converted, and the encoder encodes
   that picture again and again.

The second swap shows the same sequence. Only the VAAPI encode device
converts with EGL; Sunshine's Vulkan encoder has no EGL context.

## The fix (korri-plugins `7e9c19c`)

Patch 0044 adds `va_t::make_current()` to `src/platform/linux/vaapi.cpp`.
It makes the device's own EGL context current when it is not current. Each
VAAPI device calls it before it converts a frame and before it is
destroyed. So a replaced or failed device deletes its objects in its own
context, and the surviving device takes its own context back. In run 4
the debugger shows it: the old device makes its own context current before
its teardown, the teardown releases it, and the new device makes its
context current before the next `eglCreateImage`, which then runs with
the new context (`run-4/gdb.txt`). The approved hash of 0044 and both
patch-set digests change (`approved-patches.nix`). The x86_64 package
`/nix/store/wr5cindzb0iqwc1kviyfmwg8ppmy99pj-sunshine-korri-2026.914.233613-korri`
(profile `x86_64-linux-cuda`) was built on aka.

## Proof with the fixed Sunshine (run 5)

Run 5 ran on aka on 2026-11-15 from 09:06 to 09:11, from Korri `64041db`
with a clean tree, with the same app and test APKs as run 1. Every check
passed (`run-5/result.json`), and the fixture's restore check passed.

| Step | Result | Evidence |
| --- | --- | --- |
| The viewer streams before the resize | yes | `streaming-1..5.png`: frame IDs 2238, 2346, 2458, 2559, 2661 |
| `viewerResize(720, 1280)` returns `Applied` | yes | `instrument.txt`: OK (1 test) |
| Sunshine swaps its encoder | yes | `sunshine.log`: `Creating encoder [h264_vaapi]` at 09:10:32.026, `Runtime resolution applied: 720x1280` at 09:10:32.047 |
| The decoder follows | yes | `New output format: … crop-right=719 …`; the snapshot's `encoded` is 720x1280; 57.9 frames/s |
| **The picture goes on at 720x1280** | **yes** | `portrait-1..5.png`, 720x1280, read frame IDs 3202, 3300, 3401, 3500, 3609. Each one differs from the one before by 255 of 255 in some pixel |
| `viewerResize(1280, 720)` returns `Applied` | yes | `Runtime resolution applied: 1280x720` at 09:10:48.775 |
| **The picture goes on at 1280x720** | **yes** | `landscape-1..5.png`: frame IDs 4147, 4247, 4346, 4447, 4546; 56.6 frames/s |

At the second swap the client logged `Network dropped 7 frames` and asked
for an IDR frame. The picture went on after it.

## Run 1 in detail

Run 1 ran on 2026-11-14 from 21:33:19 to 21:34:03 (zao local time), with
`nix run .#android-live-resize-proof` (`clients/android/proof/live_resize.py`).
Its record is in `run-1/`.

| Part | Value |
| --- | --- |
| App | The Artemis app of `clients/android`, built by `nix run .#android-apk` at `688613419`. Its `clients/android` is the same as `5ff83d91f`. `app-arm64-v8a-debug.apk`, sha256 `681b3e1c08fe81324403a6c8e5e7af55d9cf953a8f73c4882660d5f2c9fd462a` |
| Test APK | `nix run .#android-device-test-apk`, sha256 `5e3ac305780d82be82ef51d208fb0a5388c609a9a2483f62427ca42a9104bb4b`. Of Korri's code it adds only the test hook (below) |
| Emulator | Android 12 (API 31), google_apis, x86_64. The arm64 app runs through the image's ARM translation. Headless, 2 cores, nice 10, panel 720x1280, on zao. It was stopped at the end. |
| Decoder | `c2.goldfish.h264.decoder`, adaptive playback (`FEATURE_AdaptivePlayback`) |
| Sender | stream-cadence's sender fixture on aka (`100.117.97.45`), with `--sender-sunshine`. Package `/nix/store/bvz43ydfridi8smy1gyqvzxk5v5xd3rg-sunshine-korri-2026.914.233613-korri`, patch set `edf475f2…`, 9 patches ending with `0044-live-resolution-control.patch` (`sender-facts.json`). Encoder `h264_vaapi` on the RX 7900 XT, capture `wlgrab` |
| Stream | 1280x720 at 60 fps: the viewer's screen (`KorridServer.viewerDisplay`) |
| aka | No product unit changed (`result.json`, `sender_restore.product_units`). The fixture's restore check passed. |

### The trigger: test hook `KorriLiveResizeFixtureTest`

korrid's Portrait control ends on the client in one JNI call:
`KorridServer.viewerResize(launch, connection, width, height)`. korrid
makes that call after it reads `KorridServer.viewerSnapshot`
(`upstreams/live_shape.rs`, `apply_shape_response`). Nothing else reaches
`Game.resizeLiveStream`. korrid makes the call only for a streamed launch
on a holder korrid. The fixture has no korrid, so it has no launch and no
certificate-control socket.

The hook is in the test APK only. The app's code is not changed. The hook
does korrid's part and nothing else:

1. It opens the exact viewer as `KorriStreamViewer.start` does after
   `prepare()`, with the same Game extras. It reaches the private session by
   reflection. The fixture's pinned certificate takes the place of korrid's
   provisioning: before the test, the proof pairs a key pair with the
   fixture's PIN and writes it into the app's files.
2. It waits until the snapshot's `shape.ready` is true. Then it asks for
   the Portrait size, which is the holder's answer: the owner's screen,
   swapped (`host/live_shape.rs`, `desired`). Then it asks for the size
   back. It uses the same entry each time, from a thread that is not the
   main thread.

## Results of run 1

| Step | Result | Evidence |
| --- | --- | --- |
| The viewer streams before the resize | yes | `streaming-1..3.png`: frame IDs 2619, 2734, 2833 |
| `shape.ready` before each resize | yes | `logcat.txt`, `KorriResizeProof: phase=…` |
| `viewerResize(720, 1280)` returns `Applied` | yes | `instrument.txt`: OK (1 test) |
| Sunshine swaps its encoder | yes, 19 ms after it made the new one | `sunshine.log`: `Creating encoder [h264_vaapi]` at 04:33:37.772, `Runtime resolution applied: 720x1280` at 04:33:37.791 |
| The decoder follows | yes | `logcat.txt` 21:33:37.150 `New output format: … crop-right=719 … crop-bottom=1279`; the snapshot's `encoded` is 720x1280 |
| The window follows | yes | Game's display size became 720x1280 (`surfaceChanged-->1280 x 720----720 x 1280` at 21:33:37.829: surface, then display size). The activity turned to portrait: `portrait-*.png` are 720x1280 and show the 16:9 capture scaled into the portrait frame |
| **The picture goes on at 720x1280** | **no** | `portrait-1..3.png` all read frame ID **3023**. Between screenshots no grey pixel changes by more than 4 of 255, which is coding noise |
| The decoder keeps decoding | yes, 58.0 frames/s in that phase | Codec2 buffer pool, `bufferpool2 … 4/801` → `4/1098` |
| `viewerResize(1280, 720)` (back) returns `Applied` | yes | `Runtime resolution applied: 1280x720` at 04:33:48.725; output format 1279x719 at 21:33:48.123 |
| **The picture goes on at 1280x720** | **no** | `landscape-1..3.png` all read frame ID **3657**. The largest change is 3 of 255. The decoder handed out 57.9 frames/s |

At each swap the client also logged `Network dropped 26 frames` (the first
swap) and `4 frames` (the second swap). It then asked for an IDR frame.

### Where it breaks

Run 1's first reading of the cause, from the source, was the released
context. Run 3 confirmed it and showed a second effect: the old device's
objects were deleted in the new device's context. See "Cause" above.

## Run 2: the client's deadline under load

In a second run, at 21:50 with zao's load average at about 23, the hook
asked for 720x1280 at about 21:51:04.2: the streaming phase's log line at
21:50:48.211 plus its 16 s hold. Sunshine applied 720x1280. The decoder's
output format changed at 21:51:04.523, about 0.3 s after the request.
`viewerResize` still threw: `Game.resizeLiveStream` returned false. The app
logs no reason. Its deadlines are 1.5 s for the decoder's new size and
1.2 s for the UI thread; under this load one of them passed (inferred).
The app had set its display size to 720x1280 all the same: its
`surfaceChanged` line at 21:51:06.725 shows `1280 x 720----720 x 1280`
(surface, then display size). So the encoder, the app and korrid's answer
(Refused) disagree about the size until korrid's rollback. The emulator's
ARM translation on a loaded host makes these times mean little (ADR 0063).
`run-2/` keeps its `instrument.txt`, `sunshine.log` and log lines.

## What this does not show

- A phone: the Fold3's decoder and a real panel.
- The holder's side: a Sway mode change and the modifier hook before the
  resize, and Sunshine's capture reinitialization that the defer path
  handles. The fixture's output stays 1280x720, and the new encoder scales
  it.
- KMS capture, and the Vulkan encoder.

## Rerun

Since 2026-11-15 the proof runs on aka. Build the APKs with
`nix run .#android-apk` and `nix run .#android-device-test-apk` (on aka,
per the owner's rule), build the proof's runtime on aka
(`nix build --eval-store auto --store ssh-ng://root@aka` of the
`android-live-resize-proof` app), copy a clone of the commit to aka, and
run there as root:

```sh
KORRI_ROOT=$PWD android-live-resize-proof --sender local \
  --stream-host 10.0.2.2 --pair-host 127.0.0.1 \
  --sunshine /nix/store/wr5cindzb0iqwc1kviyfmwg8ppmy99pj-sunshine-korri-2026.914.233613-korri \
  --apk app-arm64-v8a-debug.apk --test-apk app-debug-androidTest.apk --out DIRECTORY
```

The proof waits until aka's heat guard admits the fixture. It uses its own
adb server (port 5137) and console port 5638, and refuses a console port
that another emulator holds.
