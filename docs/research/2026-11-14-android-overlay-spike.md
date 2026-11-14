# D5 spike: the gameplay overlay over live video, with no Java (korri-os/korri #434)

Date: 2026-11-14. Agent: android-d5-spike. Throwaway code in `/tmp/korri-android-spike/`. Nothing was committed to the Korri repository.

Labels: **verified** = I ran it or read the primary source; **inferred** = follows from what I saw, not proven; **opinion** = my judgement.

## Answer

**Yes, it works, but not by the NDK-only route that the research brief assumed.** A Rust NativeActivity with no Korri Java and no `classes.dex` showed MediaCodec video full screen. It showed a separate EGL overlay layer above the video and toggled the overlay with no measurable hitch. I ran it on Android 12 (API 31) and Android 14 (API 34) emulators. The one route that fails is the brief's main design: `ASurfaceControl_createFromWindow(<NativeActivity window>)` returns NULL on API 31 and on API 34. The layers therefore need a parent that Rust makes with about six JNI calls into Android's own public classes (API 29 and 31).

| Claim | Label |
|---|---|
| `ASurfaceControl_createFromWindow` on the NativeActivity window returns NULL. API 31 logs `SurfaceComposerClient::createWithSurfaceParent error Invalid argument`. API 34 returns NULL and logs nothing. | verified (API 31 and 34) |
| `View.getRootSurfaceControl()` → `SurfaceControl.Builder` → `AttachedSurfaceControl.buildReparentTransaction()` → `new Surface(sc)` → `ANativeWindow_fromSurface` gives layers that MediaCodec and EGL can render into. NDK `createFromWindow` also accepts that Surface as a parent. | verified (API 31 and 34) |
| No video frame passes through the CPU in the app. MediaCodec writes to a BufferQueue, and SurfaceFlinger reads the same buffer. | verified (code); the emulator decoder is host-backed |
| A visibility toggle causes no extra video gaps. Pooled over 15 runs, static toggles gave 175 gaps against 181.6 expected (1,163 intervals). Toggles that start an animated overlay gave 236 against 228.4 (1,133 intervals). | verified on the emulator. Real-device HWC behaviour is unknown. |
| Background → foreground (window destroyed and remade) rebuilds the layers and the decoder. The app does not crash. | verified (API 31, two designs) |
| The APK holds only `lib/<abi>/libkorrispike.so`, the manifest, one resource table and the clip. `hasCode="false"`. Korri Java: none. Third-party Java: none. | verified (`unzip -l`, `aapt2 dump badging`) |

## Recommendation for D5 (opinion)

Build **design C plus a Java-made overlay layer**. All picture layers are framework `SurfaceControl`s made through JNI under `View.getRootSurfaceControl()`:

1. **Video layer (z 0).** `new Surface(SurfaceControl)` goes straight to `AMediaCodec_configure`. The app runs no code per frame: the framework's BLASTBufferQueue inside the process hands each buffer to SurfaceFlinger. JNI places the layer with `Transaction.setGeometry` (API 29, deprecated at 33; at 33+ use `setPosition`/`setScale`/`setCrop`). Aspect and position do not depend on the window.
2. **Overlay layer (z 1).** Same construction. The shell already renders Slint into an arbitrary `android.view.Surface`: `clients/shell/vendor/i-slint-backend-android-activity/surface_window.rs`, `SurfaceWindow::from_surface` (Skia/OpenGL). Today `KorriShellActivity` (Java) supplies that Surface through `shellTakeOverlaySurface`. D5 replaces that Java call with the Rust JNI layer. The overlay renderer does not change.
3. **Show/hide.** JNI `new SurfaceControl.Transaction().setVisibility(sc, v).apply()`. Apply time was p50 0.39–0.40 ms. A hidden layer is not composed at all.
4. **Lifecycle.** Make the layers on `InitWindow`, outside the android-activity callback, because the call waits for the Java main thread. Release them on `TerminateWindow` with a fire-and-forget post to the main thread. A blocking post there would deadlock (inferred).
5. **Background.** In letterbox bars the NativeActivity window shows through. While streaming, the shell draws black there once and stops drawing.

**Cost.** About 200 lines of Rust JNI: the spike's `jroot.rs` is 196 lines. D5 needs no NDK `ASurfaceControl` FFI and no `AImageReader` code. In the spike those are `sc.rs` (150 lines) and `present.rs` (279 lines), and D5 can drop both. **What it does not solve:**
- At API 31–32 the app gets no per-frame present time for the video. `AMediaCodec_setOnFrameRenderedCallback` starts at API 33. Design A below gets the time from `OnComplete`, at higher risk.
- Whether the device HWC puts video and overlay on hardware planes is unknown. GPU composition of each video frame would add latency and power. Only the Fold3 can answer this.
- The spike drew the overlay with plain EGL, not with Slint. I expect `SurfaceWindow::from_surface` to work on a `Surface(SurfaceControl)` because that is an ordinary `android.view.Surface` (inferred, not tested).

**Alternatives that also ran** (details below): A (AImageReader → `ASurfaceTransaction`), B (MediaCodec into the NativeActivity window) and D (video under a translucent NativeActivity window). Do not use B. Keep A only as the D6 timing path at API 31–32. Keep D as the fallback if the Fold3 shows that the extra overlay layer costs too much.

## The question and the evidence

Question: can a Rust NativeActivity (android-activity 0.6.1, native-activity backend, minSdk 31) show decoded video full screen and a separate overlay layer above it, toggle the overlay without a video hitch, keep video off the CPU, and use no Korri-written Java?

Evidence I looked for: screenshots of video + overlay and video alone; SurfaceFlinger's layer tree, transforms and HWC composition types; per-frame present times of the video layer from `dumpsys SurfaceFlinger --latency` (polled every second, deduplicated, split by the app's phase log on the same `CLOCK_MONOTONIC`); and the APK contents.

## What I built and how to run it

| File | Lines | Job |
|---|---|---|
| `flake.nix` | — | Same nixpkgs and rust-overlay revisions as Korri's `flake.lock`. Same NDK (27.0.12077973) and build-tools (35.0.0) as `clients/android/sdk.nix`. Adds google_apis x86_64 images for 31 and 34. |
| `app/src/lib.rs` | 626 | `android_main`, config, the four picture designs, phase schedule, immersive mode through JNI |
| `app/src/jroot.rs` | 196 | Framework layers through JNI: root SC, Builder, reparent, `Surface(sc)`, visibility |
| `app/src/sc.rs` | 150 | Hand-written FFI for `<android/surface_control.h>`. ndk-sys 0.6 has no bindings for it (verified). |
| `app/src/present.rs` | 279 | AImageReader → `ASurfaceTransaction_setBuffer` presenter, with fences, crop, dataspace and the stall fix |
| `app/src/egl.rs` | 195 | EGL 1.4/GLES2 test pattern (scissored clears), animated or static |
| `app/src/video.rs` | 197 | Annex B H.264 → `AMediaCodec`. 60 fps input pacing; output released at "now" (Moonlight min-latency). |
| `build.sh` | — | cargo (x86_64 + arm64) → aapt2 link (`hasCode=false`) → zipalign → apksigner. No Gradle. |
| `emu.sh`, `run.sh`, `lifecycle.sh`, `quick.sh`, `analyze.py`, `pool.py` | — | Emulator, phase run with captures, lifecycle test, analysis |

```sh
cd /tmp/korri-android-spike
nix develop .#api31 --command ./build.sh                # both ABIs -> out/korri-d5-spike.apk
nix develop .#api31 --command ./emu.sh create           # once
nix develop .#api31 --command ./emu.sh start &          # headless, port 5584, 2 cores
nix develop .#api31 --command ./run.sh jnisurface       # MODE: child|direct|jnisurface|under
OVERLAY=jni nix develop .#api31 --command ./run.sh jnisurface   # recommended design
PARENT=window nix develop .#api31 --command ./run.sh direct     # shows the NULL parent
nix develop .#api31 --command ./lifecycle.sh child      # HOME, relaunch, tap
```

Each run writes `results/<api>-<mode>-<parent>[-ovl…]/` with `analysis.txt`, `logcat.txt`, three screenshots, `sf-dump-P4.txt` (layer tree + HWC) and raw `latency/` polls. The phase schedule is 3 s warm-up, then five 8 s phases:
- P0: overlay hidden.
- P1: toggle every 250 ms, static overlay.
- P2: overlay visible and animated at 60 fps.
- P3: toggle every 250 ms while animated.
- P4: overlay visible, static.

The display is portrait-native 864×1920, and the app runs landscape at 1920×864. The clip is a 1280×720 60 fps H.264 Main stream with no B-frames. It plays pillarboxed at 1.2× in designs A, C and D.

## Results

### 1. The four picture designs (all verified on the API 31 emulator; A, B and C also on API 34)

| | A `child` | B `direct` | C `jnisurface` (+ `overlay=jni`) | D `under` |
|---|---|---|---|---|
| Video goes to | AImageReader (PRIVATE, 6 images) → app `ASurfaceTransaction_setBuffer` on a native child | NativeActivity window | `Surface(SurfaceControl)` of a JNI-made layer (z 0) | `Surface(SurfaceControl)` of a JNI-made layer at **z −1**, below the window's own buffer |
| Overlay goes to | EGL → AImageReader → native child (z 1) | EGL → AImageReader → native child | native child, or (`overlay=jni`) EGL straight into a second JNI-made layer | EGL into the NativeActivity window, made translucent with JNI `Window.setFormat(PixelFormat.TRANSLUCENT)` |
| App code per video frame | listener + transaction + release callback | none | none | none |
| Toggle | native `setVisibility`, p50 0.09–0.29 ms | native | native, or JNI p50 0.39–0.40 ms | redraw transparent / pattern (one swap) |
| Aspect / position | app sets crop/position/scale | **stretched to the window** | JNI `setGeometry` | JNI `setGeometry` |
| Per-frame present time at API 31 | yes (`OnComplete` latch + present fence) | no | no | no |
| Problem found | silent stall (section 4, item 3) | overlay inherits the video buffer's scale (section 4, item 2) | — | a full-screen translucent window is composed on every frame, even with the overlay hidden |
| Screenshot | `screenshots/01`, `02`, `06`, `07` | `03`, `04`, `10` | `05`, `08`, `09` | `11`, `12` |

The SurfaceFlinger dumps confirm the stacking. In every design the emulator HWC (ranchu) marked all layers `DEVICE`, with `ROT_90` for the landscape app on a portrait panel. A real HWC can choose differently.

### 2. Frame pacing per design (emulator, noisy host)

Video layer, from `dumpsys SurfaceFlinger --latency`. Columns: presented frames per second / gaps > 1.5 vsync / release→present p50. The "release" is the timestamp passed to `AMediaCodec_releaseOutputBufferAtTime` (time "now"). I chose the quietest run of each design; `load` is the host 1-min load average.

| Design (run, load) | P0 hidden | P1 toggle | P2 animated | P4 visible |
|---|---|---|---|---|
| A child (api31, 15.6) | 59.1/s · 6 · 9.4 ms | 55.1/s · 44 · 8.6 ms | 29.5/s · 131 · 17.5 ms | 49.9/s · 74 · 7.9 ms |
| B direct (api31r3, 9.5) | 59.9/s · 0 · 13.1 ms | 59.9/s · 7 · 12.1 ms | 53.4/s · 60 · 12.4 ms | 54.9/s · 48 · 11.9 ms |
| C jnisurface (api31r2, 11.5) | 58.9/s · 9 · 9.1 ms | 46.1/s · 110 · 8.6 ms | 36.7/s · 108 · 39.1 ms | 50.1/s · 80 · 16.9 ms |
| D under (api31, 5.9) | 59.2/s · 8 · 15.7 ms | 59.6/s · 4 · 15.4 ms | 58.5/s · 17 · 15.6 ms | 59.1/s · 12 · 15.5 ms |

These numbers do not rank the designs (inferred). zao ran other agents' Nix builds and VM tests at load 5–90 during the spike. The host-backed decoder's p50 moved from 3.4 ms in quiet phases to 61.6 ms in starved ones, which shows the emulator was CPU-starved. P2's drop matches the swiftshader GPU emulation, which is on the host CPU, rendering a full-screen overlay at 60 fps. ADR 0063 already says emulator latency means nothing. In P0 (overlay hidden) every design presented 58.9–59.9 frames per second.

### 3. Toggling does not hitch the video (verified on the emulator)

To control for host noise, I compared the video intervals that end ≤ 50 ms after each toggle with the gap rate of the whole phase (`pool.py`, first-layer video only, 15 runs, 31 toggles per phase per run):

| Phase | Intervals after a toggle | Gaps > 1.5 vsync, observed | Expected from phase rate |
|---|---|---|---|
| P1 static toggles | 1,163 | 175 | 181.6 |
| P3 toggles that also start the 60 fps overlay | 1,133 | 236 | 228.4 |

The cleanest single runs: B direct 2 gaps in 93 windows (1.0 expected), D under 1 in 94 (0.8 expected). Toggle → SurfaceFlinger latch (native transaction `OnComplete`) had a p50 of 3.6–25.3 ms over 14 phase measurements. 12 of the 14 were ≤ 15.3 ms, that is, the next vsync or the one after. The two above that came from starved runs.

### 4. Problems found

1. **The NDK-only parent fails** (verified, API 31 and 34). `ASurfaceControl_createFromWindow` needs a Surface that carries its layer handle. I inferred the cause from the API 31 log: the NativeActivity (ViewRootImpl) Surface does not carry the handle, and a `Surface` built from a `SurfaceControl` does. The brief's finding 7 and its "Picture" bullet need a correction.
2. **In design B, children of the window inherit the scale of the video buffer** (verified). With 1280×720 into a 1920×864 window, `sf-dump-P4.txt` shows `tr=[1.50, 0.00][0.00, 1.20]` on the window layer and on every child: container, `bbq-wrapper` and overlay. The overlay is stretched, and its right and bottom are cut off (`screenshots/03`, `10`). When the stream matches the window (1920×864 clip), the overlay is correct (`04`). B also takes the window away from Slint.
3. **AImageReader can stall without any error** (verified in design A). The emulator presented slowly, the app held all 6 images, and `acquireLatestImageAsync` failed with MAX_IMAGES (`NdkImageReader: Unable to acquire a lockedBuffer`). From then on the video layer never updated again (set=150 for the rest of the run), while the decoder kept releasing frames. Inferred cause: the decoder's BufferQueue *replaces* the pending frame, and AImageReader does not report `onFrameReplaced`, so the listener is never called again. Fix used: after each release, try to acquire again (`kick`), plus a 2 ms poll. With it the video ran to the end. The kick fired 82 times in that 43 s run.
4. **Design A needs more app work.** The app must apply the decoder crop (buffer 1280×736 → crop 1280×720) and the BT.709 dataspace (`ASurfaceTransaction_setBufferDataSpace`), and it must release each `AImage` with the previous-release fence. The framework does all three in B, C and D.
5. **Making layers through JNI waits for the Java main thread.** android-activity's `TerminateWindow` callback blocks that thread. A synchronous `run_on_java_main_thread` there would deadlock (inferred from the android-activity glue; I avoided it and did not trigger it). Creation took 0.9–110 ms, measured once per window.

### 5. Lifecycle (verified, API 31)

`lifecycle.sh` ran first launch → HOME → relaunch → tap, in design A and in design C with `overlay=jni`. On HOME the decoder stopped, the native children were reparented to null, and the Java layers were released on the main thread. On relaunch everything was rebuilt (JNI container in 1.6–7.5 ms), and the tap hid the overlay. There was one process and no crash (`results/api31-lifecycle-*`).

## API calls and minimum API levels

All levels verified: NDK r27 headers (`__INTRODUCED_IN`) and `platforms/android-31|34/data/api-versions.xml`.

| Call | Min API | Used by |
|---|---|---|
| `ANativeActivity` / `android.app.NativeActivity` with `hasCode="false"` | 9 | all |
| `AMediaCodec_configure` with a window; `AMediaCodec_releaseOutputBufferAtTime` | 21 | all |
| `AMEDIAFORMAT_KEY_LOW_LATENCY` (`"low-latency"`) | 30 (from the brief; the emulator accepted it) | all |
| `ANativeWindow_fromSurface` (JNI) | baseline | C, D, JNI parent |
| `View.getRootSurfaceControl()`, `AttachedSurfaceControl.buildReparentTransaction()` | **31** | JNI parent, C, D |
| `SurfaceControl.Builder` (`setName`, `setBufferSize`, `build`), `SurfaceControl.Transaction` (`setLayer`, `setVisibility`, `setGeometry`, `apply`), `new Surface(SurfaceControl)` | 29 (`setGeometry` deprecated at 33) | JNI parent, C, D |
| `Transaction.setPosition` / `setScale` / `setCrop` / `setBufferTransform` | 33 | geometry at 33+ |
| `Transaction.setFrameRate(sc, fps, compat, strategy)` | 31 | frame-rate hint on a Java layer |
| `Window.getInsetsController()`, `setDecorFitsSystemWindows()`, `WindowInsetsController.hide()` | 30 | immersive mode (JNI) |
| `Window.setFormat(PixelFormat.TRANSLUCENT)` | 1 | D |
| `ASurfaceControl_createFromWindow`, `ASurfaceTransaction_*` (`setBuffer`, `setVisibility`, `setZOrder`, `setOnComplete`, `setBufferDataSpace`, `setBufferTransparency`) | 29 | A, native overlay |
| `ASurfaceTransaction_setCrop` / `setPosition` / `setScale` / `setFrameRateWithChangeStrategy` / `setOnCommit` / `setEnableBackPressure` | 31 | A |
| `AImageReader_newWithUsage`, `acquire{Next,Latest}ImageAsync`, `AImage_getHardwareBuffer`, `AImage_deleteAsync`, `sync_file_info` | 26 | A |
| `ASurfaceControl_fromJava`, `ASurfaceTransaction_fromJava` | 34 | would replace the `Surface(sc)` trick on 34+ |
| `AMediaCodec_setOnFrameRenderedCallback`, `ASurfaceTransaction_setFrameTimeline` | 33 | not used; per-frame timing for B/C/D at 33+ |

## Java

- **Korri-written Java or Kotlin: none.** The APK has no `classes.dex`; the activity is the framework's `android.app.NativeActivity`.
- **Android framework classes called through JNI**, the same category the brief allows: `Activity.getWindow`, `Window.getDecorView`, `View.getRootSurfaceControl`, `SurfaceControl.Builder`, `SurfaceControl.Transaction`, `AttachedSurfaceControl`, `Surface`, `Rect`, `Window.getInsetsController`/`addFlags`/`setFormat`, `WindowInsetsController`, `WindowInsets$Type`. No listener object or subclass is needed for the picture path.
- **Third-party Java: none.**
- The current Java this replaces: `StreamContainer` (SurfaceView/GLSurfaceView), `KorriSurfaceHost` and `KorriShellActivity`'s overlay Surface bridge (`shellTakeOverlaySurface`, `shellOverlayDensity`, `shellOverlayClosing`). Input forwarding in `KorriSurfaceHost` is outside this spike.

## Risks a production build still faces

| Risk | Why | Label |
|---|---|---|
| HWC composition on the Fold3 | Two full-screen layers (video + translucent overlay) may push the HWC to GPU composition, which adds latency on every frame. The emulator HWC says `DEVICE` for everything and proves nothing here. | unknown — Fold3 |
| Toggle hitch on real HWC | When a layer appears, HWC validation can change the composition strategy for one frame. The emulator showed no hitch. | unknown — Fold3 |
| No per-frame present time at API 31–32 for C | The render callback starts at API 33. The ADR 0062 "time to display" needs A's `OnComplete` path or another source at 31–32. | verified (API levels) |
| JNI layer creation depends on ViewRootImpl state | `buildReparentTransaction` returns null when the window surface is not valid. I saw no failure in about 25 launches and 2 lifecycles. | inferred |
| Behaviour differences across OEM Android 12–16 builds | Tested only on the google_apis emulator images 31 and 34. Both behaved the same (NULL parent, scale inheritance in B). OEM builds and API 35–36 are untested. | unknown |
| `Surface(SurfaceControl)` makes an extra `bbq-wrapper` child layer in the app process | It is the framework's own BLAST path, the same one ViewRootImpl uses. The release→present p50 in P0 was 9.1 ms for C (run r2) and 9.7 ms for B (run 1). | verified (emulator) |
| Design D: a translucent window is always composed | When the overlay is hidden, a fully transparent full-screen layer still goes to the compositor on every frame. | inferred |
| android-activity 0.6.1 | ALVR pins 0.6.0 for freezes. I saw none with JNI on the main thread. | verified (no freeze in this spike only) |

## What surprised me

- The core NDK call the brief relied on fails on both API 31 and API 34. I expected it to fail at most on 31.
- Design B's overlay inherits the video buffer's scale. Without the SF dump it would look like a rendering bug.
- AImageReader can stall for good, with no error after the first warning. A production design A must have the kick.
- The shell already has a Slint "render into any Surface" path (`SurfaceWindow::from_surface`). That makes C + a Java-made overlay layer the smallest change.

## Checks I ran

- `cargo build --release` for `x86_64-linux-android` and `aarch64-linux-android` (NDK clang, API 31): passes. One dead-code warning (`Presenter.name`).
- `apksigner verify`: passes. `aapt2 dump badging`: `launchable-activity: android.app.NativeActivity`. `unzip -l`: no `.dex`.
- Emulator runs: 15 kept phase runs (API 31: 11; API 34: 4), 2 lifecycle runs, and several short launches. Two earlier API 31 runs were overwritten: one failed on an adb serial error, and one exposed the AImageReader stall. I used one emulator at a time, 2 cores, nice 5, and stopped it at the end (`qemu-system` count 0).
- I did not run the Korri repository's tests. The spike changed no repository file (`git status` clean).

## Suggested issues (not filed)

1. `design` (#434, D5): picture path = framework `SurfaceControl` layers made through JNI under `getRootSurfaceControl()`; video into `Surface(sc)`; overlay through `SurfaceWindow::from_surface`. Cite this report.
2. `documentation`: correct `docs/research/2026-11-13-viewer-rust-android.md` finding 7 and the "Picture" bullet of finding 27. `createFromWindow(<NativeActivity window>)` returns NULL on API 31 and 34, and children of the window inherit the buffer scale.
3. `enhancement` (D6): per-frame display timing at API 31–32 (AImageReader `OnComplete` path with the stall kick, or accept no timing below 33).
4. `needs-hardware`, `performance` (step H): Fold3 check of the HWC composition type for video + overlay, toggle hitch and release→present latency on design C.
