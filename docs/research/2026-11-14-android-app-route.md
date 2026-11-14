# Decision brief: the Korri Android app drives the Rust viewer, with no Korri Java (#434, #391 step D)

Source: a research agent's brief for #434 (design-android-app). Not audited independently.

Date: 2026-11-14. Base: `0376f5268`. Research only: no code was changed or built.
Labels: **verified** = I read the code or ran the command; **inferred** = follows from what I read, not proven; **opinion** = my judgement.

## Answer

Build a **new APK with no Java**, next to the Artemis app. Do not grow the Artemis Gradle app. In the new APK:

- One Rust `cdylib` links korrid, the shell and the viewer. Android's own `android.app.NativeActivity` loads it.
- One `android_main` starts korrid in the process and then runs the shell. Rust gives the shell the port and the capability, so they never cross JNI.
- A Rust `LinuxStreamViewer` adapter over `Lifecycle::in_process` replaces korrid's Java viewer adapter (`services/korrid/src/android/viewer.rs` → `KorriStreamViewer.java` → `Game.java`).
- The viewer becomes a mode of the shell's process and event loop. It uses D2's video layer, D3's sound and D4's input.

Nothing on the path "pick a tile → the Rust viewer plays" needs a Java class. Java is still forced in three places: the korrid foreground service, the Guide accessibility service and the `FileProvider` (third-party). They decide only two things: whether korrid keeps running in the background, and whether local Android games (ADR 0054) work. So the Fold can play a streamed game from the Korri app with no Korri Java before any of those decisions (inferred).

Cost: about four new slices before the Fold plays (D8–D11 below). One of them is a new holder fixture, because no existing fixture serves a streamed launch to an emulator. D5 (overlay) must move after the merge, because it needs the shell and korrid.

## 1. What runs where today

**Process layout (verified).** The app is one Linux process, `com.simonwjackson.korri` (`.debug` suffix in debug builds). `AndroidManifest.xml` has no `android:process` attribute. korrid, the shell, Artemis `Game` and its decoder all share that process, so a decoder crash already takes korrid down today.

| Part | How it runs | Java involved | Evidence |
|---|---|---|---|
| korrid | `libkorrid.so`, loaded by `KorridServer` (`System.loadLibrary("korrid")`). `KorriShellActivity.onCreate` calls `KorriBrainService.ensureRunning` before `super.onCreate`, so korrid starts before the shell's `android_main`. The foreground service (`specialUse`, `START_REDELIVER_INTENT`) keeps the process alive. | `KorriBrainService` (199 lines), `KorridServer` (101). korrid also calls back into Java for: the network snapshot (`KorridNetwork`), local Android games (`KorriAppLauncher`) and the viewer (`KorridServer.viewer*` → `KorriStreamViewer`). `start` fails if the viewer or launcher adapter is missing (`android.rs:110-150`). | `KorriShellActivity.java:40-58`, `KorriBrainService.java`, `services/korrid/src/android.rs`, `android/{viewer,network,app_launcher}.rs` |
| TLS verifier | iroh uses `rustls-platform-verifier`, whose Android part is a Maven AAR (`org.rustls:rustls-platform-verifier`). | Third-party, unchanged | `app/build.gradle:5-8,115`, `android.rs:50-62` |
| Shell | `libkorri_shell.so`, loaded by `KorriShellActivity extends NativeActivity` (`lib_name korri_shell`). Its `android_main` reads the port, the capability, the origin and the unpacked package path through JNI calls on the activity (`shellPort`, `shellCapability`, …). It talks to korrid only through `http://127.0.0.1:PORT/rpc` and the WebSocket watch. | `KorriShellActivity` (159 lines): starts korrid, unpacks the surface assets, listens for removed input devices, sets fullscreen/cutout/keep-screen-on, swallows `onBackPressed`, and bridges the overlay Surface. | `clients/shell/src/backend/android.rs:259-290`, `src/korrid.rs:1-15` |
| How the shell draws | Slint 1.18.1 with the vendored android-activity backend (`aa-06`, android-activity 0.6.1) and Skia, into the NativeActivity's **own window**. Java supplies a Surface only for the in-game overlay. `KorriSurfaceHost` gives `Game`'s Surface, and the shell polls `shellTakeOverlaySurface` every 16 ms and renders with `SurfaceWindow::from_surface`. | Slint's `SlintAndroidJavaHelper.java` (731 lines) is compiled into an in-memory dex. It is identical to the crates.io 1.18.1 copy (I ran `diff`). | `backend/android.rs:300-345`, `vendor/README.md` |
| Viewer today | Artemis `Game` activity with `libmoonlight-core.so`, the Artemis fork that carries Korri's `SunshineRuntimeSettings.c` (live resize). | `Game.java` 4,832 lines | `KorriStreamViewer.java`, `jni/moonlight-core` |
| Rust viewer | A separate APK, `com.simonwjackson.korri.viewer`, with `hasCode="false"`, minSdk 31 and `DUMP`-gated. Only the proof tool starts it, through intent extras. | none | `clients/viewer/android/AndroidManifest.xml`, `android/src/lib.rs:15-23` |
| Other components | `KorriGuideService` (AccessibilityService), `DiscoveryService`, `ComputerManagerService`, `UsbDriverService` and androidx `FileProvider`. `PcView`, `AppView`, `StreamSettings` and `AddComputerManually` are started only by each other (grep). Nothing in the shell, korrid or the Korri Java starts them. `AddComputerManually` is `exported="false"`, so its `art:` filter admits no other app. | Java | manifest; grep |

Totals (verified, `wc -l`): 42,511 Java lines in `app/src/main/java`, of which 6,957 are in `Korri*` files.

**Launch path today, tile to `Game` (verified).**

1. A Velvet tile returns `SurfaceRequest::LaunchGame { gameId }` (`packages/velvet/src/browse.ts:651`).
2. The shell carries the request with `app.surface.request` (`clients/shell/src/korrid.rs:12`).
3. korrid handles it in `surface/root.rs:1696` → `confirm_launch` (`:1344`) → `upstreams.rs` `prepare_stream_for_launch` on the holder peer → `self.viewer.start(launch_id, moonlight_address, display)` (`upstreams.rs:1129`). `host/viewer_input.rs:148` wraps that call.
4. The Android adapter (`android/viewer.rs`, `START` signature) makes a JNI static call to `KorridServer.viewerStart` (`KorridServer.java:74`) → `KorriStreamViewer.start` (`:147`), on a worker thread.
5. `KorriStreamViewer.prepare` (`:395`) does Artemis `NvHTTP` serverinfo, then `provisionMoonlightCertificate`, which goes through JNI back into korrid's own localhost RPC. Then it checks the pair state, reads the app list and selects "Korri Stream". It ends with `new Intent(context, Game.class)` (`:424`).
6. `Game` connects through Artemis `NvConnection`/`MoonBridge`. Then `KorriStreamViewer.attach`/`connected` (`Game.java:349,3718`) set RUNNING, and `start` returns `"Running"` to korrid.

The settings come from Artemis `PreferenceConfiguration` plus korrid's width and height (`putStreamSize`). The settings screen (`StreamSettings`) is reachable only from `PcView`, which nothing starts. So the values in effect are Artemis's defaults (inferred).

## 2. What Java remains essential

### Moves to Rust (no Java class, framework calls through JNI or NDK)

| Job (today's Java) | Rust route | Proven where |
|---|---|---|
| Start korrid (`KorriBrainService.ensureRunning`) | `android_main` calls `start_local_server` once per process. `EmbeddedPlatform.viewer`/`reach` are `pub(crate)` (`lib.rs:6092-6093`). Make them public for the composing crate, or put the adapter inside korrid behind `cfg(android)`. | inferred |
| Port and capability to the shell (`shellPort`/`shellCapability`) | Direct Rust calls `local_server_port()` / `local_server_capability()` (`lib.rs:6468,6476`). The capability never enters an argument, URL or log. | inferred |
| Unpack surface assets (`copyPackage`) | JNI `AssetManager.list`/`open`. This mirrors the Java code. I did not re-check the NDK `AAssetDir` listing of subdirectories. | inferred |
| Input device removals (`InputDeviceListener`) | Poll `InputDevice.getDeviceIds()`, as D4 does twice a second | verified in D4 (emulator) |
| Fullscreen, cutout, keep screen on | `ANativeActivity_setWindowFlags` for keep-screen-on. JNI `WindowInsetsController` for immersive mode, as in the D5 spike. The cutout mode goes in a theme attribute or a JNI `setAttributes` call. | the spike verified the immersive JNI calls |
| Swallow Back (`onBackPressed`) | The native input filter already consumes Back. The gap: a Back that comes before the input queue attaches reaches the framework, which finishes the activity. | inferred, small window |
| Overlay Surface (`KorriSurfaceHost`) | D5 spike design C: a JNI `SurfaceControl` layer → `new Surface(sc)` → the existing `SurfaceWindow::from_surface` | spike verified the layer; Slint into it is untested |
| Network snapshot (`KorridNetwork`) | JNI `ConnectivityManager`, plus `registerReceiver(null, BATTERY_CHANGED)` for the sticky battery intent. No listener. | inferred |
| Viewer adapter (`KorriStreamViewer`, `Game`) | Rust adapter: trait method → `ViewerRequest` → `Lifecycle::request`. `ViewerEvent` → `change_signal`. | treaty and in-process lifecycle verified in D1–D4 |
| Local games: `isInstalled`, `start`, `resume`, `showKorri`, usage polling, save-bridge `ContentResolver.call` | Framework calls through JNI. The `Handler` poll becomes a Rust thread. | inferred |
| Slint helper, Rust TLS verifier | Bundled unchanged. Slint already loads its dex in memory. `rustls_platform_verifier::android::init_with_refs` takes any class loader (I read 0.7.1 `src/android.rs:152`), so the AAR's classes can load the same way and the APK keeps `hasCode="false"`. Fallback: `hasCode="true"` with the AAR dexed by `d8`. | API verified; the in-memory route is inferred |

### Still needs a Java class (Android instantiates manifest components from the app's dex)

| Piece | Why | Options (bundled unchanged Java is allowed, #434) | On the play path? |
|---|---|---|---|
| **korrid foreground service** (`KorriBrainService`) | Android keeps a background process alive only for a declared `Service` subclass. `startForeground` is an instance method of that subclass. | (a) One Korri Java file with no logic, on #434's "Android forces" list. (b) No service: korrid lives while the process lives. The play path is unaffected, because the activity is in front during a stream. When the app is in the background, the cached-app freezer can stop korrid, so peers cannot reach the phone and local Android games break (inferred). (c) A third-party Service: I found none that works without a subclass. My search was not complete. (d) Generate the dex from another language: that is still Korri-written code. Rejected. | No |
| **Notifications** | Needed only for the service. `POST_NOTIFICATIONS` is declared but never requested: no `requestPermissions` call exists, and `KorriNotificationPermissionPrompt` has no caller (verified). On Android 13+ the service runs without the grant, and its notification is not shown in the drawer (Android's documented behaviour; not checked on a device). | Rust can build the notification through JNI, but only the Service instance can post it with `startForeground`. To ask for the grant: JNI `Activity.requestPermissions`, then poll `checkSelfPermission` on resume. This needs no result callback. | No |
| **Permission results** (research finding 12) | NativeActivity has no `onRequestPermissionsResult`. | The play path needs none. Keep `targetSdkVersion` ≤36, so that `ACCESS_LOCAL_NETWORK` is not required (today 34 in both APKs). Where a grant is needed: request through JNI and poll. Usage access and all-files access are Settings app-ops: poll `AppOpsManager`. | No |
| **Guide over other apps** (`KorriGuideService`, ADR 0054) | AccessibilityService subclass | Keep one Korri Java file, or drop Guide over local Android games. A stream does not need it: Guide reaches the native filter while the activity has focus. | No |
| **Content-file launches** (`FileProvider`, ADR 0054) | ContentProvider in the manifest | androidx `FileProvider`, unchanged. This needs `hasCode="true"`. Or drop content launches. | No |
| NIP-55 owner signer | Needs `onActivityResult` | Today it is dead code: `KorriOwnerBindingHost` has no caller (verified). Options: NIP-46 bunker (Rust, `SurfaceRequest::SwitchIdentityToNip46` exists, but the Android README says identity switch is unavailable there); `jni-min-helper`'s third-party result activity; drop. | Only for a new install (see O1) |
| Controller motion sensors | `SensorEventListener` | `jni-min-helper` `DynamicProxy` (third-party dex), or keep them dropped as D4 does | No |

## 3. Slice plan

Done: D0–D4, in the standalone viewer APK. Planned: D4b, D5, D6, D7, step H. New slices are D8–D12. D7′ replaces D7. Every slice except D12 is proven on the headless API 31 emulator with no person (`android/emulator.nix`, stream-cadence's sender fixture, logcat and screenshots).

| # | Slice | Proof with no person | Gate |
|---|---|---|---|
| **D8** | **One library, one framework activity.** A new composing `cdylib` links korrid, the shell and later the viewer. One `android_main` starts korrid, then the shell. The manifest declares `android.app.NativeActivity`; `KorriShellActivity`'s six jobs go to Rust (table above). It is built by the aapt2 pipeline of `clients/viewer/android.nix`, without Gradle. It has no foreground service and no local Android games (korrid starts without the launcher adapter). Lock unification: korrid and the shell both pin `rquickjs` 0.14.0 (verified), so their C symbols do not clash. | Emulator: Velvet renders (screenshot). `adb shell input keyevent` moves the focus (a logcat readiness line replaces `shellRendered`). Back does not finish the app. Rotation and Home/return work. korrid health is in logcat. `unzip -l` shows no Korri dex; `aapt2 dump badging` shows `android.app.NativeActivity`. | — |
| **D9** | **Holder fixture.** Extend `services/korrid/examples/federation_fixture.rs` (a real host with a fixed person signer, secret 3; its game is now `/bin/true`) so that its game streams from the sender fixture's Sunshine as "Korri Stream". Make the emulator app's korrid a member: seed its private state through `run-as`, with the producer that the fixture already uses (`DeviceIdentity::load_or_create` + a signed owner statement). | Emulator: Velvet lists the fixture's streamed game. | — |
| **D10** | **A tile starts the Rust viewer in the same process.** The Rust `LinuxStreamViewer` adapter over `Lifecycle::in_process`. korrid makes the key directory (`Identity::load_or_generate`, Moonlight's `client.pem`/`key.pem`/`uniqueid.dat` names, grounded in the README) and provisions the pin through the existing broker RPC. The viewer's input and timers move from D1's own `poll_events` loop into the shell's android-activity event listener, input filter and a Slint timer. Only one poller can own `AndroidApp`, and Slint's is that owner (`vendor/.../lib.rs:146`). The video layer is made outside the event callback. Settings: Artemis's defaults plus korrid's display, as today. Their home on Android is still open (ADR 0061: not a plugin). The intent extras and the `DUMP` activity stay only as a test tool. | Emulator: keys select and confirm the tile. logcat shows korrid's `Start` request and the in-process `Running` event. `bar.py` reads frame IDs that rise. The fixture Sunshine logs the session. Back → `Stopped`, and the shell returns. | — |
| **D5** (moved) | **Gameplay overlay over the video**, inside the merged app. The overlay is the shell's own `Overlay` package with korrid's model. A JNI layer feeds the existing `SurfaceWindow::from_surface`. Guide opens the overlay, and input is held while it shows. Leave, Resume, End and Disconnect work. Open Korri = Background: the runtime stops and the layer goes, but the launch stays. Resume = a new runtime and Start. That is what `KorriStreamViewer.background`/`resume` do today. | The flow of `KorriGameplayOverlayDeviceTest`, driven by adb against D9: the overlay shows over moving video, Resume gives input back (`log_input`), Background → Resume → Running, Disconnect keeps the holder's launch live. | — |
| **D4b** | Rumble, trigger rumble, light and battery through JNI (as planned) | Its emulator proof is not designed yet (`README` gives none). The feel needs a device. | gates D7′ |
| **D11** | **Stream controls and status strip** (new finding). The Rust viewer answers `Snapshot` with no controls, battery or health, and answers `Apply`/`Notice` "unavailable" (`clients/viewer/src/lifecycle.rs:386-418`). Artemis declares up to 14 controls, battery and stream health (`KorriViewerControls.java`). The owner chooses which to carry (O4). | Emulator: the snapshot lists the controls. Fit/fill changes `setGeometry` (SurfaceFlinger dump). | gates D7′ |
| **D12** | **On the Fold (step H for the app).** Install on `usu`, play a game from aka from the Korri app, and measure ADR 0062's numbers against the Artemis app on the same phone. | adb-driven, after a one-time ADB route (see "Help needed") | Needs O1. **Plays before #476**: a fold or rotation letterboxes the picture (D2 places the layer again; the stream size stays). |
| D6 | Balanced pacing, frame-rate hint, display mode (as planned) | emulator for function, numbers on the Fold | before D7′ if step H needs it |
| **D7′** | **The old app goes**: delete `Game.java`, the Artemis Gradle app and the standalone viewer APK, and update `AGENTS.md`. | APK has no Korri Java, or only the files on #434's list | **#476** (live resize), **D4b**, D11, O2, O3 |

**Order rationale (opinion).** D8 first, because every later proof needs korrid and the shell in the no-Java process. D9 comes before D10, because the tile proof needs a game in the library. D5 moves after D10, because the overlay is the shell's package and korrid's model. Building it in the standalone viewer would mean a stand-in overlay.

## 4. Risks and open product choices

**Owner choices:**

| # | Choice | Options and cost |
|---|---|---|
| **O1 (needs the owner before D12)** | Does the old app stay installable during the transition? | **Side by side** (new application ID). The Artemis app stays the baseline for ADR 0062's comparison, and a rollback uninstalls nothing. Cost: the phone becomes two federation devices, and two apps answer the HOME intent. The new device needs an owner binding, and the no-Java app has no working signer route yet (see the NIP-55 row). **Replace in place** (same application ID and the same signing key; the debug build is `com.simonwjackson.korri.debug`, inferred). It keeps korrid's state, so the phone stays owned. Cost: no side-by-side baseline, and a rollback means reinstalling the old APK. I recommend side by side (opinion). |
| O2 | korrid foreground service | (a) one Korri Java file on #434's list; (b) no background korrid; (c) wait for a third-party option. See section 2. |
| O3 | Local Android games (ADR 0054) in the no-Java app | They need the Guide accessibility service (Java), `FileProvider` (bundled) and usage access. Options: keep them with the forced Java files, or defer them, or drop them. |
| O4 | Which stream controls the Rust viewer must carry before D7′ | SGSR, PiP and the local cursor depend on Artemis's GL renderer. The MediaCodec → SurfaceControl path has no GPU pass for SGSR (inferred). |

**Risks:**

| Risk | Why | Label |
|---|---|---|
| One event loop | D1–D4's `drive` loop polls `AndroidApp` itself. In the merged app, Slint owns polling, so the viewer must be re-hosted on the shell's listener and timer. Making a layer waits for the Java main thread, and `TerminateWindow` blocks that thread (the spike's deadlock note). | inferred |
| No auto-restart without the service | Today `START_REDELIVER_INTENT` restarts korrid after process death. Without it, korrid returns only when a person opens the app. | verified (code); effect inferred |
| One process | A native crash in the C core or MediaCodec stops korrid and the shell. Today it is the same (one process). | verified |
| Emulator proves function only | The Fold's HWC composition of video plus overlay, the toggle hitch and the decoder are unknown (spike). | unknown |
| Owner binding on a fresh install | There is no Java signer path in the new app, and the existing one is dead code. The emulator works around it with a seeded state (D9). A real phone needs O1, or a Rust signer route. | verified (dead code); route inferred |
| `hasCode="false"` and the verifier AAR | The in-memory loading of the verifier classes is unproven. | inferred |
| Notices | The OpenSSL, curl, ENet, libopus and zlib notices are not yet in the viewer APK (README). They must be there before any distribution. | verified |
| Device facts disagree | ADR 0063/0064 and the viewer README name a Galaxy Z Fold3. The task says Fold 8, SM-F971U1. `clients/shell/vendor/README.md` says "Fold8". #434's acceptance still says Pixel 3. The phone's Android version is not checked. | verified (texts) |

**Help needed (physical, not a permission):** `adb connect usu:5555` was refused on 2026-11-14. `usu` is online on Tailscale (`100.110.243.2`). D12 needs the owner to turn on wireless debugging and pair this host once, or to connect a cable.

**Found on the way (low):** `clients/android/federation-acceptance-check.sh` runs `KorriFederationAcceptanceTest`, but no such test class exists (verified with `find`). The script is stale.

## Checks run (all read-only)

- Read: `AGENTS.md`; ADRs 0060–0064; #434 with its 2 comments, #476 and #391 (`gh`); both research briefs; `clients/viewer/README.md`; `clients/viewer/android/src/lib.rs`, `src/lifecycle.rs`, `src/launch.rs`; the Android manifest, `build.gradle`, `KorriShellActivity`, `KorriBrainService`, `KorridServer`, `KorriAppLauncher`, `KorriStreamViewer`, `KorriViewerControls`; korrid's `android.rs`, `android/viewer.rs`, `linux_viewer{,/socket,/production}.rs`, `upstreams.rs`; `contracts/viewer/src/lib.rs`; the shell's `backend/android.rs` and `korrid.rs`; the vendored Slint README.
- `diff` of the vendored `SlintAndroidJavaHelper.java` against crates.io 1.18.1: identical.
- `grep` for the callers of `PcView`, `KorriOwnerBindingHost`, `KorriNotificationPermissionPrompt` and `requestPermissions`.
- Read `rustls-platform-verifier` 0.7.1 `src/android.rs`.
- `wc -l` on the Java tree. `Cargo.lock` versions of `rquickjs`, `jni`, `android-activity`, `ndk`, `openssl-sys` and `curl-sys` in the three crates.
- `tailscale status` (usu online); `adb connect usu:5555` (refused).
- No build, no test and no emulator run. The emulator `emulator-5584` that is attached is not mine, and I did not touch it.
