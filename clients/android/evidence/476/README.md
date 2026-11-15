# #476 slice 1 — today's Artemis live resize, on the emulator

**Finding: today's live resize breaks after the encoder swap. The picture
freezes.** Sunshine swaps its encoder, and the app's decoder follows to
the new size. The picture then shows the first frame of the new encoder
and does not move again. This happens after the Portrait resize and again
after the resize back. ADR 0058 says "still unproven on hardware". This
run shows where the feature breaks.

## What ran

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

### Where it breaks (inferred, high confidence)

The new encoder encodes one picture again and again. The fault is on
Sunshine's side, after `session.swap(candidate)`:

- The decoder hands out about 58 frames each second, and each frame
  differs from the one before by coding noise only. A frozen display would
  give identical screenshots. The source did not stop: the frame ID moved
  from 3023 to 3657 between the two swaps. Each frozen frame is the frame of
  its own swap.
- Patch 0044 makes the candidate session, feeds it captured frames until
  it has an IDR, and swaps it in. The old session ends where the
  `candidate` variable goes out of scope, on the encoder thread.
- Sunshine's `egl::ctx_t` deleter (`src/platform/linux/graphics.h`,
  lines 464 to 470 of v2026.914.233613) calls
  `eglMakeCurrent(…, EGL_NO_CONTEXT)` before `eglDestroyContext`. So the
  old VAAPI converter's teardown releases the new converter's GL context
  on that thread. After that, `va_vram_t::convert` (`sws.load_vram`,
  `convert_nv12`) has no current context. The NV12 surface keeps the last
  picture that the candidate converted, and the encoder encodes that
  picture.

This is from reading the source. No changed Sunshine has been run. The
product on aka uses `capture=kms` with the same VAAPI VRAM converter, so
it probably freezes in the same way (inferred). The plugins'
`channel-check` cannot see this, because it has no GPU.

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

```sh
nix run .#android-apk
nix run .#android-device-test-apk
nix run .#android-live-resize-proof -- \
  --sunshine /nix/store/bvz43ydfridi8smy1gyqvzxk5v5xd3rg-sunshine-korri-2026.914.233613-korri
```

The proof waits until aka's heat guard admits the fixture. It uses its own
adb server (port 5137) and console port 5638. Other proofs share zao, and
an adb client of another version restarts a shared adb server, which takes
every emulator offline.
