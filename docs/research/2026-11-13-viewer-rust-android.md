# Research: Korri's Android viewer with no Java, in Rust (#391 step D, #434)

Date: 2026-11-13. Researcher: research-android-2. Labels: **verified** = I read the primary source or the code; **inferred** = follows from what I read, not proven; **unknown** = I could not find evidence.

Source: a researcher agent's brief for #391, step D. Its claims were not audited independently; each carries its own label.

## Summary

Use `android.app.NativeActivity` with `android-activity` (native-activity backend). Of the routes I checked, it is the only one that needs no Java or Kotlin file from Korri. GameActivity needs a Korri-written subclass (verified). Every viewer part except one has an NDK route at API 31: hardware decode (`AMediaCodec` with `low-latency` and vendor keys), timed frame release, AAudio, key, motion and touch input, frame-rate hints and child surfaces. The gaps are calls into Android's own classes through JNI (decoder lists, controller identity, rumble, battery, Wi-Fi lock, window flags). Controller motion sensors are the exception: they need a Java listener object, so they need a dependency's Java code or they are dropped. The largest open risk is not decode. It is drawing the gameplay overlay over live video without a `SurfaceView`. That needs a new `ASurfaceControl`/`AImageReader` picture path that Korri has never built. Build it as one in-process viewer mode of the existing NativeActivity, in 8 slices (D0–D7). Prove each slice for function on a headless emulator. Leave latency to the Fold3 (step H).

## Findings

### Part 1 — Each platform part without Korri-written Java

| Part | No-Java route at API 31 | Still needs Android's own classes (JNI) | Needs a Java *listener/subclass* | Label |
|---|---|---|---|---|
| Activity lifecycle | `android.app.NativeActivity` + `android-activity` 0.6.1 `native-activity` | Window flags, insets, display mode (via `run_on_java_main_thread`) | No | verified (route); inferred (JNI list) |
| Surface | NativeActivity `ANativeWindow`; second layer via `ASurfaceControl_createFromWindow` (API 29) + `AImageReader` | — | No | verified (APIs); inferred (design) |
| Hardware decode | `ndk::media::MediaCodec` (`AMediaCodec`), `low-latency` key (API 30), vendor keys via `AMediaFormat`, async callbacks (API 28), timed release | Decoder list, `FEATURE_LowLatency`, vendor-parameter probe, `MEDIA_PERFORMANCE_CLASS` until API 36 | No | verified |
| Frame pacing | `AChoreographer_postFrameCallback64` (API 29), `ANativeWindow_setFrameRateWithChangeStrategy` (API 31) | Display mode choice (`preferredDisplayModeId`) | No | verified (APIs) |
| Audio | AAudio (`ndk::audio`), `LowLatency` mode, data callback; libopus in C | — | No | verified |
| Controllers, touch | `android-activity` `KeyEvent`/`MotionEvent` (axes, history) | `InputDevice` name/VID/PID, `VibratorManager`, `BatteryState`, `requestUnbufferedDispatch` | Sensors (`SensorEventListener`), device add/remove (`InputDeviceListener`) | verified (APIs); inferred (listener gap) |
| Network, foreground | Manifest permissions; Rust sockets | `WifiManager` locks; `requestPermissions` | Permission *result*; a foreground `Service` | verified |
| Packaging | Nix + Gradle with no Java sources, or `cargo-apk` | — | No | verified |

1. **Claim:** NativeActivity is a framework class. An APK with only native code declares it with `android:hasCode="false"` and the `android.app.lib_name` meta-data. **Sources:** [Sample: native-activity](https://developer.android.com/ndk/samples/sample_na). **Support:** direct evidence. **Confidence:** high. **Label:** verified.
2. **Claim:** `android-activity` states: "`NativeActivity` is technically the only way to build a native Android application purely in Rust without any Java or Kotlin code at all", and "It's not possible to subclass an Activity from Rust / JNI code alone." Version 0.6.1 (2026-03-30) adds `android_on_create`, `run_on_java_main_thread` and `java_main_looper`. It also changed `ndk-context` to the *Application* context. **Sources:** [android-activity README](https://github.com/rust-mobile/android-activity), [CHANGELOG](https://github.com/rust-mobile/android-activity/blob/main/android-activity/CHANGELOG.md). **Support:** direct evidence. **Confidence:** high. **Label:** verified.
3. **Claim:** GameActivity counts against "no Java Korri writes" because of its required subclass, not because of Google's own AAR. Google: "At present, GameActivity *cannot* be used as the startup activity. Apps must derive a class from `GameActivity`." It also needs Gradle and `androidx.games:games-activity:4.4.0`. rust-mobile's own GameActivity example ships a `MainActivity.java`. **Sources:** [Migrate from NativeActivity](https://developer.android.com/games/agdk/game-activity/migrate-native-activity), [android-activity README](https://github.com/rust-mobile/android-activity). **Support:** direct evidence (facts) + interpretation (policy). **Confidence:** high. **Label:** verified (facts). The policy reading is inferred: #434 accepts "no `.java` or `.kt` file that Korri wrote", and a GameActivity subclass is not forced by Android, because NativeActivity exists.
4. **Claim:** The Korri records do not decide whether *bundled third-party* Java (an AndroidX AAR, or a crate's embedded dex) is allowed. ADR 0062 bans Java "that Korri writes, patches or forks" and keeps "unpatched system libraries". A bundled AAR or dex is neither. **Sources:** `docs/adr/0062-the-viewer-moves-to-rust-in-measured-stages.md`, #434. **Support:** interpretation. **Confidence:** medium. **Label:** inferred. The recommended design avoids the question except for controller sensors.
5. **Claim:** The `ndk` crate's `MediaCodec` configures with an `Option<&NativeWindow>`. It has `release_output_buffer_at_time[_by_index]`, `set_parameters` (api-level-26) and `set_async_notify_callback` (api-level-28). The crate has `audio`, `media`, `native_window`, `hardware_buffer`, `surface_texture` and `trace` modules. It has **no** choreographer or surface-control module. **Sources:** [ndk MediaCodec](https://docs.rs/ndk/latest/ndk/media/media_codec/struct.MediaCodec.html), [ndk crate index](https://docs.rs/ndk/latest/ndk/index.html). **Support:** direct evidence. **Confidence:** high. **Label:** verified. Korri must write safe wrappers over `ndk-sys` for `AChoreographer` and `ASurfaceControl` (inferred).
6. **Claim:** These are the NDK media API levels, read from the platform's export map: `AMEDIAFORMAT_KEY_LOW_LATENCY` introduced=30; `AMediaCodec_setAsyncNotifyCallback` introduced=28; `AMediaCodec_setOnFrameRenderedCallback` introduced=Tiramisu (33); `AMediaCodecStore_*`, `AMediaCodecInfo_isFeatureSupported` and `AMediaCodecInfo_FEATURE_LowLatency` introduced=36. **Sources:** [libmediandk.map.txt](https://android.googlesource.com/platform/frameworks/av/+/refs/heads/main/media/ndk/libmediandk.map.txt), [NDK Media reference](https://developer.android.com/ndk/reference/group/media). **Support:** direct evidence (I decoded the base64 file myself). **Confidence:** high. **Label:** verified. Consequences:
   - At API 31–35, the decoder list and the low-latency feature query need `android.media.MediaCodecList` through JNI. This agrees with ADR 0064.
   - At API 31–32, an NDK decoder gives **no render-time callback**. The "time to the display" measure in ADR 0062 needs another source there. Inferred.
7. **Claim:** A second on-screen layer is possible from native code. `ASurfaceControl_createFromWindow` takes an `ANativeWindow` parent (API 29). `ASurfaceTransaction_setBuffer`, `setZOrder` and `setDesiredPresentTime` are NDK functions; `setFrameTimeline` is API 33. Z order "is relative to other surfaces which are siblings of this surface." **Sources:** [NDK Native Activity / surface_control reference](https://developer.android.com/ndk/reference/group/native-activity). **Support:** direct evidence. **Confidence:** high. **Label:** verified (API). I found no NDK function that gives an `ANativeWindow` for an `ASurfaceControl`. A producer such as MediaCodec or EGL must therefore write to an `AImageReader` window. The app then pushes each `AHardwareBuffer` with `ASurfaceTransaction_setBuffer`. Inferred. Whether a child layer draws above its parent window's own buffer is unknown: the docs only describe sibling order.
8. **Claim:** Frame-pacing primitives exist in the NDK. `AChoreographer_getInstance` (API 24, "must be called on an ALooper thread"), `postFrameCallback64` (29), `registerRefreshRateCallback` (30), `postVsyncCallback` and frame timelines (33). `ANativeWindow_setFrameRate` (30) and `setFrameRateWithChangeStrategy` (31) are also there. The frame-rate call "has no effect" if a media codec consumes the window. **Sources:** [Choreographer](https://developer.android.com/ndk/reference/group/choreographer), [Native Window](https://developer.android.com/ndk/reference/group/a-native-window). **Support:** direct evidence. **Confidence:** high. **Label:** verified.
9. **Claim:** AAudio is the C audio API. Google suggests Oboe, a C++ wrapper that "calls AAudio when it is available, and falls back to OpenSL ES". Lowest latency needs `AAUDIO_PERFORMANCE_MODE_LOW_LATENCY` with a callback. **Sources:** [AAudio getting started](https://developer.android.com/ndk/guides/audio/aaudio/aaudio). **Support:** direct evidence. **Confidence:** high. **Label:** verified. At API 31 the fallback is not needed. Oboe would add C++ for little gain. Inferred.
10. **Claim:** All four controller-feature methods on `InputDevice` were added in API 31: `getVibratorManager`, `getBatteryState`, `getSensorManager` and `getLightsManager`. **Sources:** [InputDevice](https://developer.android.com/reference/android/view/InputDevice). **Support:** direct evidence. **Confidence:** high. **Label:** verified. Rumble and battery are call-or-poll APIs, so JNI is enough. Sensors deliver data only to a `SensorEventListener`, a Java interface. Inferred.
11. **Claim:** A Rust crate can implement Java interfaces with a Rust closure: `jni-min-helper` `DynamicProxy`. It works because the crate compiles its own `InvocHdl.java`, `BroadcastRec.java` and `RunOnce.java` to a dex at build time, or uses a prebuilt `classes.dex`. Its `PermissionRequest` "*requires* the activity `rust.jniminhelper.PermActivity` to be declared in the AndroidManifest.xml". **Sources:** [jni_min_helper docs](https://docs.rs/jni-min-helper/latest/jni_min_helper/), [build.rs](https://docs.rs/crate/jni-min-helper/latest/source/build.rs), [PermissionRequest](https://docs.rs/jni-min-helper/latest/jni_min_helper/struct.PermissionRequest.html). **Support:** direct evidence. **Confidence:** high. **Label:** verified. This is the escape hatch for listeners. It is third-party Java (see finding 4).
12. **Claim:** From target SDK 37, apps need the runtime permission `ACCESS_LOCAL_NETWORK` to reach the LAN. At target SDK ≤36, "local network access is implicitly granted using the INTERNET permission". **Sources:** [Local network permission](https://developer.android.com/privacy-and-security/local-network-permission). **Support:** direct evidence. **Confidence:** high. **Label:** verified. A no-Java app can *start* a permission prompt through JNI; ALVR does this. It cannot receive the result without an Activity override, or else it must poll `checkSelfPermission` (finding 11; inferred). So keep `targetSdkVersion` ≤36 until #434 decides this.
13. **Claim:** No NDK or framework route declares a service without a `Service` subclass. korrid's `KorriBrainService` (foreground, `specialUse`) will therefore stay Java, or need a dependency's dex. **Sources:** Korri `AndroidManifest.xml` on `main`. **Support:** interpretation. **Confidence:** high. **Label:** inferred. The viewer itself needs no service while its window is visible (inferred).
14. **Claim:** Packaging tools disagree about their own status:
    - The published `ndk-build` 0.10.0 README says "This crate and `cargo-apk` are deprecated in favour of `xbuild`".
    - The `xbuild` repository description says "(unmaintained)".
    - The `cargo-apk` repository was pushed on 2026-11-12 with an unreleased change to default `target_sdk_version` 36.
    - `android-activity` recommends "a more-standard, Gradle-based build system" with `cargo ndk`.
    - ALVR builds its client with `cargo apk build`.

    **Sources:** [ndk-build README](https://docs.rs/crate/ndk-build/latest/source/README.md), [xbuild](https://github.com/rust-mobile/xbuild), [cargo-apk](https://github.com/rust-mobile/cargo-apk), [ALVR xtask build.rs](https://github.com/alvr-org/ALVR/blob/master/alvr/xtask/src/build.rs). **Support:** direct evidence. **Confidence:** high. **Label:** verified.
15. **Claim:** The Java in Korri's shell activity today (`KorriShellActivity extends NativeActivity`) does six jobs:
    - starts korrid's foreground service;
    - unpacks surface assets;
    - registers an `InputDeviceListener` for removals;
    - sets fullscreen, cutout and keep-screen-on;
    - swallows `onBackPressed`;
    - bridges the overlay `Surface` from `KorriSurfaceHost`.

    **Sources:** `clients/android/app/src/main/java/com/limelight/KorriShellActivity.java`. **Support:** direct evidence. **Confidence:** high. **Label:** verified. The Rust routes for these jobs are: service = finding 13; assets = `AAssetManager` or JNI `AssetManager.list`; removals = poll `InputManager.getInputDeviceIds`; window = JNI on the UI thread or theme XML; Back = mark the key handled in the native input handler; overlay = finding 7. All of these routes are inferred.

### Part 2 — Prior art

| Project | Activity / Java | Decode / audio | Published latency | Label |
|---|---|---|---|---|
| ALVR client (`alvr/client_openxr`) | `android-activity` `=0.6.0` native-activity; pinned because 0.6.1's ndk-context change caused freezes. No Java in the client tree. minSdk 28. `cargo apk`. | `ndk` MediaCodec → `AImageReader` (PRIVATE, 10 images) → GL; `release_output_buffer_at_time`; AAudio `LowLatency` | Quest 3, one user, AI-assisted: decoder 9.4–10 ms, pipeline 42–50 ms at 4096×2144@200 Hz | verified |
| moonlight-common-rust | Library only, no Android client | Sunshine yes; AV1, RFI, **video encryption** no | none | verified |
| Bevy | GameActivity is the default; NativeActivity "Legacy, should be avoided" | not a streaming client | none | verified |
| macroquad/miniquad | Own `MainActivity.java` since 0.3: "some of the android API's are really, really hard to use through java native interface" | not a streaming client | none | verified |

16. **Claim:** ALVR's Rust client, with no Java, calls Android's own classes from Rust for the same chores Korri needs:
    - a Wi-Fi lock, `WIFI_MODE_FULL_LOW_LATENCY` on API ≥29, else `HIGH_PERF`;
    - a multicast lock;
    - `requestPermissions`, whose result it does not handle (`// todo: handle case where permission is rejected`);
    - battery state through the sticky `BATTERY_CHANGED` broadcast.

    **Sources:** [system_info/android.rs](https://github.com/alvr-org/ALVR/blob/master/alvr/system_info/src/android.rs), [client_openxr/Cargo.toml](https://github.com/alvr-org/ALVR/blob/master/alvr/client_openxr/Cargo.toml). **Support:** direct evidence. **Confidence:** high. **Label:** verified.
17. **Claim:** ALVR's MediaCodec defaults pass vendor keys through `AMediaFormat`: `operating-rate`=i32::MAX, `priority`=0 and `vendor.qti-ext-dec-low-latency.enable`=1. It leaves `low-latency` commented out with "Android smartphones crashes enabling this feature". **Sources:** [ALVR settings.rs](https://github.com/alvr-org/ALVR/blob/master/alvr/session/src/settings.rs). **Support:** direct evidence. **Confidence:** high. **Label:** verified. The crash report itself (a PhoneVR issue) I did not read. Unknown which phones it covers.
18. **Claim:** On Qualcomm (Quest 3, XR2 Gen 2), `vendor.qti-ext-dec-low-latency.enable=1` makes "the decoder [return] each frame before starting the next, so throughput ≈ 1 ÷ latency". The user saw a ~60–70 fps cap at 4096×2144. With 0 and 4 buffered frames: 9.4–10 ms decode at 200 fps, at a cost of "about two extra frames of latency". **Sources:** [ALVR Discussion #3410](https://github.com/alvr-org/ALVR/discussions/3410). **Support:** direct evidence (one setup, AI-assisted measurement). **Confidence:** medium. **Label:** verified (report). It also shows that vendor keys set through the NDK take effect. Whether phone streams at 1080p–1440p hit this cap is unknown; their pixel rate is several times lower (inferred).
19. **Claim:** I found no Rust Moonlight client for Android and no published Android latency for any Rust streaming client except ALVR. **Sources:** [moonlight-common-rust README](https://github.com/MrCreativ3001/moonlight-common-rust); searches listed under Missing evidence. **Support:** absence of evidence. **Confidence:** medium. **Label:** unknown beyond what was searched.

### Part 3 — What Artemis does that matters for latency on phones

20. **Claim:** Decoder choice in `MediaCodecHelper.java` works like this:
    - Round 1 takes only decoders that report `FEATURE_LowLatency`. This finds separate low-latency decoders such as `c2.qti.hevc.decoder.low_latency` (Pixel 4).
    - Software-only decoders are blocked except on the emulator (`Build.HARDWARE == "ranchu"`).
    - HEVC is allowed when Media Performance Class ≥ S, when the decoder has `FEATURE_LowLatency`, or when an Adreno ≥4xx is present.

    **Sources:** `clients/android/.../binding/video/MediaCodecHelper.java`. **Support:** direct evidence. **Confidence:** high. **Label:** verified.
21. **Claim:** Low-latency `MediaFormat` options are applied in "tries", most risky first, and dropped one try at a time if `configure()` fails:
    - try 0: `low-latency`=1;
    - try <2: `vdec-lowlatency`=1;
    - try <3: `operating-rate`=Short.MAX_VALUE on Qualcomm/RFI decoders except Adreno 620, else `priority`=0;
    - Qualcomm: `vendor.qti-ext-dec-picture-order.enable` (0 OMX / 1 C2), `vendor.qti-ext-dec-low-latency.enable`=1, and output-fence keys commented "CONFIRMED WORKING: Snapdragon Elite, SD8 gen 3, SD8 gen 2".

    The fence keys and the "ALONSOJR1980" edits come from upstream Artemis (`ClassicOldSong/moonlight-android`, branch `moonlight-noir`), not from Korri. **Sources:** local `MediaCodecHelper.java`; [Artemis MediaCodecHelper (moonlight-noir)](https://github.com/ClassicOldSong/moonlight-android/blob/moonlight-noir/app/src/main/java/com/limelight/binding/video/MediaCodecHelper.java). **Support:** direct evidence. **Confidence:** high. **Label:** verified. I did not check whether moonlight-stream upstream has the fence keys.
22. **Claim:** The H.264 SPS is rewritten with jcodec: lower `level_idc`, `num_ref_frames`=1 when RFI is off, and `max_dec_frame_buffering` = `num_ref_frames` in the bitstream restrictions. This keeps the decoder from buffering frames. CSD is batched; IDR is fused when adaptive playback exists; "direct submit" is used for `c2.*` and `omx.qcom`. **Sources:** local `MediaCodecDecoderRenderer.java`. **Support:** direct evidence. **Confidence:** high. **Label:** verified. A Rust port needs its own SPS bit writer (inferred).
23. **Claim:** moonlight-stream upstream has three frame-pacing modes:
    - min-latency: drain to the newest output buffer and call `releaseOutputBuffer(idx, System.nanoTime())`, "a PTS that will cause this frame to be dropped if another comes in within the same V-sync period";
    - max-smoothness / cap-fps: release with timestamp 0, so the frame is never dropped;
    - balanced: a `Choreographer` callback on its own `HandlerThread`, an output queue capped at 2, render only when ≥80% of a frame interval has passed, release at `frameTimeNanos − appVsyncOffset`.

    **Sources:** [moonlight-android MediaCodecDecoderRenderer.java](https://github.com/moonlight-stream/moonlight-android/blob/master/app/src/main/java/com/limelight/binding/video/MediaCodecDecoderRenderer.java). **Support:** direct evidence. **Confidence:** high. **Label:** verified.
24. **Claim:** Korri's copy, inherited from Artemis `moonlight-noir`, mixes two pacing paths:
    - `Game.java` sets `FRAME_PACING_BALANCED` in *both* branches of its "latency profile selection". This comes after the intent's `FramePacing` override in source order.
    - With the default `preferLowerDelays=false`, the render thread first drains output without blocking and releases the newest frame at "now" (`LATEST_ONLY_LOW_LATENCY`).
    - A frame that arrives during the following 2 ms blocking dequeue goes to the Choreographer queue instead.
    - The "C2 sleep" watchdog sits after `while (!stopping)`.

    **Sources:** [Korri Game.java (main)](https://github.com/korri-os/korri/blob/main/clients/android/app/src/main/java/com/limelight/Game.java), local `MediaCodecDecoderRenderer.java`, [Artemis renderer (moonlight-noir)](https://github.com/ClassicOldSong/moonlight-android/blob/moonlight-noir/app/src/main/java/com/limelight/binding/video/MediaCodecDecoderRenderer.java). **Support:** direct evidence (code) + interpretation (runtime effect). **Confidence:** medium. **Label:** verified for the code. Inferred: the release path changes from frame to frame with timing, and the watchdog never runs during a stream. A Rust port should start from upstream's three modes, not this loop.
25. **Claim:** Display and input settings in Korri and upstream:
    - Choose the best display mode (`preferredDisplayModeId`). On API 31+, when only the rate differs, call `Surface.setFrameRate(fps, FRAME_RATE_COMPATIBILITY_FIXED_SOURCE, CHANGE_FRAME_RATE_ALWAYS)`.
    - Korri's decoder also calls `setFrameRate(..., COMPATIBILITY_DEFAULT)` itself.
    - Request unbuffered dispatch on API 30+ for all input classes: "Without this, input events are buffered to be delivered in lock-step with VBlank". Touch also calls `requestUnbufferedDispatch(event)`.
    - Acquire Wi-Fi `HIGH_PERF` and `LOW_LATENCY` locks.
    - Set `preferMinimalPostProcessing` on the Game activity.
    - Newest upstream moonlight-android also calls `setProducerThrottlingEnabled(false)` on `CINNAMON_BUN`.

    **Sources:** Korri Game.java and manifest (main); [moonlight-android Game.java](https://github.com/moonlight-stream/moonlight-android/blob/master/app/src/main/java/com/limelight/Game.java). **Support:** direct evidence. **Confidence:** high. **Label:** verified. NDK equivalents: `ANativeWindow_setFrameRateWithChangeStrategy` (verified, API 31). `View.requestUnbufferedDispatch` on NativeActivity's content view through JNI (inferred to apply; unproven).
26. **Claim:** Audio uses `AudioTrack` `PERFORMANCE_MODE_LOW_LATENCY` and tries a 2-frame buffer first. It skips low latency when the native rate does not match the stream. It drops input when more than 40 ms of audio is pending. Opus decode is libopus in C (`LOCAL_STATIC_LIBRARIES := libopus libssl libcrypto cpufeatures`). **Sources:** [Korri AndroidAudioRenderer.java](https://github.com/korri-os/korri/blob/main/clients/android/app/src/main/java/com/limelight/binding/audio/AndroidAudioRenderer.java), local `jni/moonlight-core/Android.mk`. **Support:** direct evidence. **Confidence:** high. **Label:** verified.

### Part 4 — Recommended architecture, slices, emulator test, risks

27. **Claim (opinion):** These are the recommended architecture choices:
    - **One Activity: `android.app.NativeActivity`.** The viewer becomes a Rust *mode* inside the same process as the shell, not a second activity. A no-Java manifest cannot declare a second activity class, and an alias shares its target's process (inferred).
    - **Shared crate.** `clients/viewer` keeps the platform-neutral session part (options, pair, `Transport`, stream config, lifecycle treaty). It gains an `android` backend that implements moonlight-common-c's decoder, audio and connection callbacks as `extern "C"` Rust functions, as `stream.rs` already does for Linux.
    - **Picture.** While no overlay shows, MediaCodec renders straight into the NativeActivity `ANativeWindow`, and Slint releases that window. The overlay is a child `ASurfaceControl`. Slint draws into an `AImageReader` window, and its buffers go to the child with `ASurfaceTransaction_setBuffer`. The fallback puts video in the child (AImageReader → transaction) and keeps Slint on the main window. Both are inferred and need the D5 spike.
    - **Pacing.** Port upstream's min-latency and balanced modes: `release_output_buffer_at_time`, an `AChoreographer` thread on its own `ALooper`, `setFrameRateWithChangeStrategy(FIXED_SOURCE, ALWAYS)`.
    - **JNI to Android's classes, no Korri Java:** `MediaCodecList` and `MediaCodecInfo` for decoder choice; `InputDevice`, `VibratorManager` and `BatteryState`; the Wi-Fi lock; window insets, cutout and keep-screen-on; `requestUnbufferedDispatch`; poll for device add/remove.
    - **Audio.** libopus (C) + AAudio `LowLatency` through `ndk::audio`, with the 40 ms cap.
    - **Packaging.** Keep the existing Nix + Gradle build (`nix run .#android-apk`) as a packager with no viewer Java.

    **Support:** interpretation. **Confidence:** medium. **Label:** opinion/inferred. **Cost:**
    - The overlay path is new.
    - A decoder crash takes the shell down with it (no process split, unlike Linux ADR 0060).
    - Controller motion sensors stay out unless third-party Java is accepted.
    - About 2,000 lines of decoder errata and SPS logic must be ported.

| Slice | What lands | Proof with no person | Removes |
|---|---|---|---|
| D0 | `clients/viewer` session part builds for `aarch64`/`x86_64-linux-android`; moonlight-common-c built by `cc` for Android | `cargo check` for both targets; host unit tests | — |
| D1 | In-process viewer mode: start/stop/lifecycle against the fixture Sunshine with a null decoder and no audio output (as Linux does today) | Emulator: lifecycle `connected`/`stopped` events in logcat; Sunshine fixture log shows the session | NvConnection/MoonBridge JNI glue for session |
| D2 | MediaCodec → main window, min-latency pacing, decoder choice via JNI, low-latency key tries, SPS rewrite | Emulator: frames-in/out/released counters; `screencap` not black; `dumpsys SurfaceFlinger` | MediaCodecDecoderRenderer, MediaCodecHelper |
| D3 | libopus + AAudio `LowLatency`, 40 ms cap | Emulator with audio on: callback count, `AAudioStream_getXRunCount` | AndroidAudioRenderer |
| D4 | Input: keys, axes, touch → `LiSend*`; device metadata/polling; rumble/battery via JNI; unbuffered dispatch | `adb shell input …` and the `hid` command (virtual gamepad); the fixture host records received input | ControllerHandler (non-USB parts) |
| D5 | Overlay as child `ASurfaceControl`; Leave/Resume/Open Korri/Disconnect | Re-express `KorriGameplayOverlayDeviceTest`'s flow as an adb-driven script | KorriSurfaceHost, Game overlay glue |
| D6 | Balanced pacing (AChoreographer), frame-rate hint, display mode, frame-timing export (ADR 0062 numbers) | Emulator for function; numbers only on Fold3 (step H) | — |
| D7 | Delete Game activity, UsbDriverService (ADR 0064) and the viewer's Java | APK has no viewer `.java`; JVM tests for removed code go | Game.java and its viewer helpers |

28. **Claim:** The emulator supports a test run with no person:
    - start it headless with `-no-window` and `-gpu swiftshader_indirect` (both are documented options);
    - `-no-audio` "disables audio support" — avoid it for D3;
    - inject a virtual USB/Bluetooth HID gamepad from a pseudo-JSON file or from stdin with the `hid` command; its docs say to wait for `onInputDeviceAdded` before you send reports;
    - read Perfetto FrameTimeline, which "requires Android 12(S) or higher" but notes "SurfaceViews are currently not supported".

    **Sources:** [Emulator command line](https://developer.android.com/studio/run/emulator-commandline), [hid README](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/cmds/hid/README.md), [Perfetto FrameTimeline](https://perfetto.dev/docs/data-sources/frametimeline). **Support:** direct evidence. **Confidence:** high. **Label:** verified. Three things are unknown: whether `hid` and `/dev/uhid` work on a `google_apis` emulator image; whether FrameTimeline covers a MediaCodec-driven NativeActivity window; and whether the emulator's NAT carries Sunshine's UDP. Artemis's emulator exception (finding 20) shows that decode works there for function. Latency numbers from the emulator mean nothing (ADR 0063).
29. **Claim:** `clients/viewer/src/stream.rs` calls `std::arch::is_x86_feature_detected!("aes")` with no `cfg`. **Sources:** local `clients/viewer/src/stream.rs`. **Support:** direct evidence (code). **Confidence:** medium (effect). **Label:** verified (code). Inferred: it will not compile for aarch64 (Android, RG353M, RG35XX). The Rust std page links the macro to `std_detect/.../x86.rs` but does not state the target rule. Check with `cargo check --target aarch64-linux-android`. The session part also links `curl` and `openssl` (`session.rs`, `Cargo.toml`). Android has no system copies of either, so D0 must cross-build them or move to Rust TLS/HTTP (inferred).

**Risks**

| Risk | Why | Mitigation | Label |
|---|---|---|---|
| Overlay without SurfaceView | New `ASurfaceControl`/`AImageReader` path; child-over-parent order unknown; no prior art in Korri or ALVR | D5 spike first on the emulator, then Fold3 | inferred / unknown |
| Listener-only APIs | Sensors, device add/remove, permission result need Java objects | Poll where possible; drop controller sensors, or accept a dependency's dex (owner) | verified (finding 11) |
| Decoder errata loss | ~1,200 lines of MediaCodecHelper + SPS patching | Port lists as data; keep "tries" retry logic | inferred |
| `low-latency` crash on some phones | ALVR disabled it for phones | Keep Artemis's retry order; record per-device result | verified (comment) |
| Qualcomm low-latency throughput cap | One report shows serialized decode | Measure on Fold3 at target resolution and rate | verified (report) |
| No render callback at API 31–32 | `setOnFrameRenderedCallback` is API 33 | Use AChoreographer timing and SurfaceFlinger dumps; callback on 33+ | verified |
| `android-activity` 0.6.1 context change | ALVR pinned `=0.6.0` for freezes | Pin the version the shell's Slint backend uses; test JNI that needs an Activity | verified (comment) |
| Local-network permission | targetSdk 37 blocks the LAN without a runtime grant | Stay at targetSdk ≤36 until #434 decides | verified |
| korrid foreground service | Needs a `Service` subclass | #434's "Android forces" list | inferred |
| Two moonlight-common-c copies | Android builds the Artemis fork (`SunshineRuntimeSettings.c`); #391 says "one pinned upstream copy" | Decide the one core in D0 | inferred |

## Contradictions

- `cargo-apk` status: the published `ndk-build` 0.10.0 README says it is deprecated in favour of `xbuild`. `xbuild` says it is unmaintained. The `cargo-apk` repo had a commit on 2026-11-12, and ALVR still uses it. ([ndk-build](https://docs.rs/crate/ndk-build/latest/source/README.md), [xbuild](https://github.com/rust-mobile/xbuild), [cargo-apk](https://github.com/rust-mobile/cargo-apk))
- NativeActivity guidance: `android-activity` calls it the only pure-Rust route. Bevy calls it "Legacy, should be avoided". Google recommends GameActivity for new games. Both hold; they optimise for different goals (no Java vs. text input and AndroidX).
- Bench phone: ADR 0064 names a Galaxy Z Fold3 because the Pixel 3 was not found. #391's stage table and #434's acceptance still say "Pixel 3".
- `AGENTS.md` says "Artemis's Game activity is korrid's Android stream viewer". The recommended design deletes the Game activity (D7). This is consistent with ADR 0062/0064 and #391 stage 3, but `AGENTS.md` needs an edit when D7 lands.

## Missing evidence

- Whether an `ASurfaceControl` child of the NativeActivity window draws above the window's own buffer, and whether Slint's EGL can render into an `AImageReader` window on the Fold3 (unknown; D5 spike).
- Whether `View.requestUnbufferedDispatch` on NativeActivity's content view removes vsync batching for the native input queue (unknown).
- Whether Paddleboat's AAR contains Java (not checked). It is not needed.
- Fold3 SoC and Android version (not checked here; ADR 0064 also says unchecked).
- Moonlight Android or Artemis published end-to-end latency on phones (not found). The only numbers found are ALVR's, on a headset.
- Whether moonlight-stream upstream has Artemis's Qualcomm output-fence keys (not checked).
- Which moonlight-common-c copy the Linux viewer pins (not checked).
- Searches run: android-activity backends; ndk MediaCodec; AMediaCodecStore; cargo-apk/xbuild/cargo-ndk; Paddleboat; emulator `hid`; miniquad Java; moonlight-common-rust Android; Rust Moonlight Android client; Bevy Android; ALVR latency; local-network permission. One combined search timed out (jni-min-helper/AAssetDir/hasCode); I fetched each source directly instead.

## Sources

Kept:
- `docs/adr/0064-the-android-viewer-needs-android-12-and-has-no-usb-controller-driver.md`, `0062-…`, `0063-…`, `0059-…` (local) — decisions and stage rules — fetched
- [Issue #434](https://github.com/korri-os/korri/issues/434), [Issue #391](https://github.com/korri-os/korri/issues/391) and its [2026-11-12 comment](https://github.com/korri-os/korri/issues/391#issuecomment-6827550416) — scope, step D — fetched
- `clients/android/README.md`, `KorriShellActivity.java`, `MediaCodecHelper.java`, `MediaCodecDecoderRenderer.java`, `jni/moonlight-core/Android.mk`, `clients/viewer/{Cargo.toml,build.rs,src/stream.rs,src/session.rs}`, `clients/shell/Cargo.toml`, `contracts/viewer/src/lib.rs` (local) — current code — fetched
- [Korri Game.java (main)](https://github.com/korri-os/korri/blob/main/clients/android/app/src/main/java/com/limelight/Game.java), [AndroidAudioRenderer.java](https://github.com/korri-os/korri/blob/main/clients/android/app/src/main/java/com/limelight/binding/audio/AndroidAudioRenderer.java), [AndroidManifest.xml](https://github.com/korri-os/korri/blob/main/clients/android/app/src/main/AndroidManifest.xml) — pacing override, audio, services — fetched
- [rust-mobile/android-activity](https://github.com/rust-mobile/android-activity) (README, CHANGELOG, Cargo.toml, agdk example `MainActivity.java`) — backends, Java needs — fetched
- [MediaCodec in ndk::media::media_codec](https://docs.rs/ndk/latest/ndk/media/media_codec/struct.MediaCodec.html), [ndk crate index](https://docs.rs/ndk/latest/ndk/index.html) — Rust API surface — fetched
- [Media | Android NDK](https://developer.android.com/ndk/reference/group/media), [libmediandk.map.txt](https://android.googlesource.com/platform/frameworks/av/+/refs/heads/main/media/ndk/libmediandk.map.txt) — API levels — fetched
- [Native Activity | Android NDK](https://developer.android.com/ndk/reference/group/native-activity), [Choreographer](https://developer.android.com/ndk/reference/group/choreographer), [Native Window](https://developer.android.com/ndk/reference/group/a-native-window) — surface control, pacing — fetched
- [AAudio: Getting started](https://developer.android.com/ndk/guides/audio/aaudio/aaudio) — audio — fetched
- [Sample: native-activity](https://developer.android.com/ndk/samples/sample_na) — `hasCode="false"` — fetched
- [InputDevice](https://developer.android.com/reference/android/view/InputDevice) — API 31 controller methods — fetched
- [Migrate from NativeActivity](https://developer.android.com/games/agdk/game-activity/migrate-native-activity) — GameActivity subclass rule — fetched
- [Use the Game Controller library](https://developer.android.com/games/sdk/game-controller/controller) — Paddleboat accepts NativeActivity — fetched
- [Local network permission](https://developer.android.com/privacy-and-security/local-network-permission) — targetSdk 37 rule — fetched
- [Android Emulator command line](https://developer.android.com/studio/run/emulator-commandline), [hid command README](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/cmds/hid/README.md), [Perfetto FrameTimeline](https://perfetto.dev/docs/data-sources/frametimeline) — emulator testing — fetched
- [alvr-org/ALVR](https://github.com/alvr-org/ALVR): [video_decoder/android.rs](https://github.com/alvr-org/ALVR/blob/master/alvr/client_core/src/video_decoder/android.rs), [client_openxr/Cargo.toml](https://github.com/alvr-org/ALVR/blob/master/alvr/client_openxr/Cargo.toml), [settings.rs](https://github.com/alvr-org/ALVR/blob/master/alvr/session/src/settings.rs), [audio.rs](https://github.com/alvr-org/ALVR/blob/master/alvr/client_core/src/audio.rs), [system_info/android.rs](https://github.com/alvr-org/ALVR/blob/master/alvr/system_info/src/android.rs), [xtask build.rs](https://github.com/alvr-org/ALVR/blob/master/alvr/xtask/src/build.rs) — prior art — fetched
- [ALVR Discussion #3410](https://github.com/alvr-org/ALVR/discussions/3410) — Qualcomm low-latency numbers — fetched
- [moonlight-common-rust README](https://github.com/MrCreativ3001/moonlight-common-rust) — Rust protocol port status — fetched
- [moonlight-android MediaCodecDecoderRenderer.java](https://github.com/moonlight-stream/moonlight-android/blob/master/app/src/main/java/com/limelight/binding/video/MediaCodecDecoderRenderer.java), [Game.java](https://github.com/moonlight-stream/moonlight-android/blob/master/app/src/main/java/com/limelight/Game.java) — upstream pacing — fetched
- [Artemis renderer (moonlight-noir)](https://github.com/ClassicOldSong/moonlight-android/blob/moonlight-noir/app/src/main/java/com/limelight/binding/video/MediaCodecDecoderRenderer.java), [Artemis MediaCodecHelper (moonlight-noir)](https://github.com/ClassicOldSong/moonlight-android/blob/moonlight-noir/app/src/main/java/com/limelight/binding/video/MediaCodecHelper.java), [simonwjackson/artemis](https://github.com/simonwjackson/artemis) — upstream Artemis origin — fetched
- [rust-mobile/cargo-apk](https://github.com/rust-mobile/cargo-apk) (README, CHANGELOG), [ndk-build 0.10.0 README](https://docs.rs/crate/ndk-build/latest/source/README.md), [rust-mobile/xbuild](https://github.com/rust-mobile/xbuild) — packaging status — fetched
- [jni_min_helper](https://docs.rs/jni-min-helper/latest/jni_min_helper/), [build.rs](https://docs.rs/crate/jni-min-helper/latest/source/build.rs), [PermissionRequest](https://docs.rs/jni-min-helper/latest/jni_min_helper/struct.PermissionRequest.html) — listener/permission escape hatch — fetched
- [Bevy Cargo Features](https://github.com/bevyengine/bevy/blob/main/docs/cargo_features.md) — Bevy default — fetched
- [macroquad: Java](https://macroquad.rs/articles/java/) — miniquad's Java — fetched

Rejected or deprioritized:
- [Rust std `is_x86_feature_detected`](https://doc.rust-lang.org/std/macro.is_x86_feature_detected.html) — fetched; does not state the target rule
- [Asset | Android NDK](https://developer.android.com/ndk/reference/group/asset) — fetched; did not confirm the subdirectory listing limit, claim dropped
- [Bevy android_basic readme](https://raw.githubusercontent.com/bevyengine/bevy/main/examples/mobile/android_basic/readme.md) — failed (404)
- [ClassicOldSong master-branch renderer](https://github.com/ClassicOldSong/moonlight-android/blob/master/app/src/main/java/com/limelight/binding/video/MediaCodecDecoderRenderer.java) — fetched; stale branch, superseded by `moonlight-noir`
- moonlight-web-stream, Reddit threads, search-provider summaries — search only; not primary

## Next steps

1. D5 spike first: a child `ASurfaceControl` with an `AImageReader`-fed buffer over a MediaCodec-filled NativeActivity window on the emulator and the Fold3. It decides the picture design.
2. Run `cargo check --target aarch64-linux-android` and `x86_64-linux-android` on `clients/viewer` to confirm finding 29 and the curl/openssl gap.
3. Check `hid` and `/dev/uhid` and Sunshine UDP on the headless emulator image the project uses.

Suggested issues for the parent to file (not filed):
- `bug`: `clients/viewer/src/stream.rs` `is_x86_feature_detected!` blocks aarch64 builds (Android and ARM handhelds). Inferred; confirm with `cargo check`.
- `enhancement` (#391 step D): slices D0–D7 above, D5 spike first.
- `documentation`: #391 and #434 still name the Pixel 3; ADR 0064 names the Fold3.
- `design` (#434): targetSdk ≤36 vs. `ACCESS_LOCAL_NETWORK`; whether a dependency's dex (`jni-min-helper`) or an AAR counts as Java under ADR 0062; the korrid foreground service as an "Android forces" file.
- `design`: one moonlight-common-c copy for both platforms (Linux pin vs. the Artemis fork with `SunshineRuntimeSettings.c`).
