# Korri Android app with no Korri Java (#434)

Slice D8 of the route brief (`docs/research/2026-11-14-android-app-route.md`).
Android's own `android.app.NativeActivity` loads one Rust library,
`libkorri_android_app.so`. Its `android_main` starts korrid in the app's
process, then runs the native shell on the activity's window. The APK has no
dex (`hasCode="false"`). The Artemis app in `clients/android` does not change;
it stays the shipping Android app until slice D7′.

```sh
nix build .#korri-android-app-apk          # both ABIs, unsigned
nix build .#korri-android-app-fixture-apk  # the same library, the holder proof's relay
nix run .#android-app-proof                # D8: one headless API 31 emulator, no person
nix run .#android-app-holder-proof         # D9, on aka as root: binding and a holder's game
```

## Owner decisions (2026-11-14)

| # | Decision | What it means here |
| --- | --- | --- |
| O1 | The new app installs side by side with the Artemis app, under its own package name. | Package `com.simonwjackson.korri.app`, label "Korri (native)". It is a separate federation device with its own korrid state. It does not answer the HOME intent; the Artemis app keeps that. |
| O2 | korrid runs only while the app is open, with no foreground service yet. | korrid starts once per process and lives as long as the process. No service keeps the process alive in the background: Android can freeze or end it there, and nothing restarts korrid until a person opens the app (the Artemis app's `START_REDELIVER_INTENT` has no equivalent). |
| O3 | Local Android games stay, with androidx `FileProvider` bundled unchanged. | Not in D8. Slice D13 (below) brings them; `FileProvider` then needs a third-party dex and `hasCode="true"`. |
| O4 | The stream controls come in D11. | Not in D8. |

## What runs

| Part | Where | Java |
| --- | --- | --- |
| korrid | `korrid::android_app::start` (`services/korrid/src/android/app.rs`), on its own thread, once per process. It serves `http://127.0.0.1:<port>/rpc` with a capability made at start. | none |
| korrid's network and power facts | `services/korrid/src/android/framework_network.rs`: JNI into `ConnectivityManager`, `LinkProperties` and the sticky battery intent. The rules are `KorridNetwork.java`'s (`android_network.rs`, tested on the host). | none |
| The shell | `korri_shell::backend::android::run`: the same loop as the Artemis app's, with a Rust host (`src/host.rs`). Rust gives it the port and the capability; neither crosses JNI, a URL, a file or the log. | Slint's `SlintAndroidJavaHelper` (below) |
| Surface packages | Velvet, Pico and Zone in the APK's assets, as the Artemis app carries them, verified on the build machine (`clients/shell/nix/installed.nix`). `src/assets.rs` unpacks them under the cache directory at each start. | none |
| Logs | `src/logcat.rs` sends the process's standard output and error to logcat (tag `korri`): korrid's and the shell's lines, as the journal holds them on Linux. | none |

`KorriShellActivity`'s jobs, each:

| Job | Here |
| --- | --- |
| Start korrid before the shell | `android_main` calls `korrid::android_app::start` first |
| `shellPort`, `shellCapability`, `shellOrigin`, `shellPackagePath` | `Bootstrap` in Rust. The origin is `http://127.0.0.1:<port>` (`start_local_shell_server`), as on webOS |
| Unpack the packages | `assets::unpack`: `AssetManager.list` through JNI (the NDK lists no subdirectories), bytes through the NDK |
| `InputDeviceListener` | `InputDevice.getDeviceIds()` every 100 ms; `DevicePresence` (`clients/shell/src/input/android.rs`) gives the removals |
| Fullscreen, cutout, keep the screen on | `host.rs`, on Android's main thread, at each resume and focus |
| Swallow Back | The shell's input filter consumes Back. At API 31 a Back that reaches the framework moves a root launcher activity to the back instead of finishing it |
| `shellRendered` | A logcat line: `korri-app: shell rendered surface=… page=… count=… selected=…`, for the first frame after each resume and each change |
| `KorriSurfaceHost` (the overlay Surface) | The shell's `Overlay` trait; the app supplies none until slice D5 |

korrid's three Java callbacks:

| Callback | D8 |
| --- | --- |
| Network snapshot (`KorridNetwork`) | Rust through JNI |
| Local Android games (`KorriAppLauncher`) | Not installed: korrid has no Android launcher, so a local Android game does not start. Slice D13 |
| Viewer (`KorriStreamViewer`) | korrid's unconfigured viewer: a streamed launch fails with "Linux stream viewer adapter is not configured". Slice D10 |

## Third-party Java

The APK carries no `classes.dex` and keeps `hasCode="false"`. Third-party
Java that runs, unchanged:

| Code | Version | How it loads |
| --- | --- | --- |
| Slint's `SlintAndroidJavaHelper.java` | Slint 1.18.1 (`clients/shell/vendor/i-slint-backend-android-activity/java`, identical to the crates.io copy) | Its build script compiles and dexes it; the backend loads it with `InMemoryDexClassLoader` |
| rustls-platform-verifier's Kotlin component (`org.rustls:rustls-platform-verifier`) and the Kotlin standard library | 0.2.0, the version that `Cargo.lock` pins for `rustls-platform-verifier-android` (the build fails when they differ); `kotlin-stdlib` 2.0.21 | `default.nix` takes `classes.jar` from the AAR and dexes it with the standard library (`d8`, no shrinking) into one file in the library. korrid loads it with `InMemoryDexClassLoader` below the application's loader and gives the verifier that loader (`services/korrid/src/android/app.rs`); logcat says `korrid: Android TLS verifier classes loaded` |

The AAR's other part is not carried: its `res/xml/network_security_config.xml`
and the manifest attribute that names it. That file permits cleartext HTTP
only to certificate authorities' revocation-list hosts, so that Android's
revocation check can fetch a list. Without it Android's default for this
target refuses cleartext, and the verifier's revocation check, which is
`SOFT_FAIL`, passes when it cannot fetch. The Artemis app does not carry it
either: its own `network_security_config` has the same resource name and
replaces the AAR's.

What uses the verifier: nothing in this build that this slice found (a
reading of the lock and of iroh's source, not a proof). iroh 1.4.1 is
built without its `platform-verifier` feature (no crate in `Cargo.lock` but
reqwest 0.13, iroh-metrics, noq-proto and korrid depends on
rustls-platform-verifier), so its relay connections check certificates
against the compiled-in Mozilla roots (`iroh_relay::tls::CaTlsConfig`
defaults to `EmbeddedWebPki`), and iroh hands its own TLS configuration to
every reqwest client it builds. korrid's federation relays are `http://`.
The verifier's code is linked (reqwest 0.13 keeps it for a client with
default TLS), so a later dependency that uses it now finds its classes. Not
proven: a certificate check through the verifier on the emulator; the proof
shows only that the classes load.

## Build

`default.nix`, as `clients/viewer/android.nix` builds the viewer's APK: cargo
for `aarch64-linux-android` and `x86_64-linux-android` at API 31 with the NDK
of `clients/android/sdk.nix`, one derivation for each ABI's library; then
`aapt2 link` against android-36 and `zipalign` in a derivation that only
packs, so a change to the packages or the manifest compiles nothing. No
Gradle. The output is unsigned; the proof signs it with the user's debug key.

The APK is debuggable: `aapt2 link --debug-mode` sets
`android:debuggable="true"`, which lets `run-as` and a debugger into the
app's process and its files, korrid's private state included. It must be
built without `--debug-mode`, and signed with a release key, before any
distribution.

- One `Cargo.lock` for korrid, the shell and Slint. It was made from korrid's
  and the shell's locks: every crate is at a version that one of them locks.
  Where they differed, the newer release won. Against korrid's own lock that
  is 57 patch and 16 minor bumps, all semver-compatible, and 4 versions that
  only korrid's dev-dependencies use are absent (`aes` 0.9.3,
  `block-padding` 0.4.2, `cbc` 0.2.1, `cpubits` 0.1.1). Also absent:
  `itertools` 0.12.1 (bindgen takes 0.13.0, inside its own range) and two
  optional crates whose features nothing here enables (`tinyvec_macros`,
  `tracing-serde`). Both pin `rquickjs` 0.14.0, so QuickJS links once.
- `Cargo.toml` repeats the shell's `[patch.crates-io]`: Cargo reads it only
  from the root crate.
- Skia's prebuilt archives for both targets (skia-bindings 0.153.3) are fixed
  Nix inputs, read through `SKIA_BINARIES_URL`.
- The surface packages are verified with the shell's own release build, with
  its test suite off; `nix run .#shell-check` runs those tests.
- The relay list is an APK asset, `relays.json`, not a build input of the
  library: `apk` carries `clients/android/product.nix`, the Artemis app's
  list; `fixtureApk` carries one loopback relay, `ws://127.0.0.1:47800`,
  which the holder proof maps to its own relay with `adb reverse`. So both
  APKs share one library, and the proof's APK runs the product's code. korrid
  takes the list as the product's relays, as before
  (`korrid::android_app::start`); a device setting does not replace it, as
  `KORRID_RELAYS` is not replaced on Linux.
- The TLS verifier's dex (above) is a fixed input of the library
  (`KORRI_ANDROID_VERIFIER_DEX`).

Cost: the library is large, about 103 MB for each ABI (54 MB of code, 26 MB
of symbols, kept for crash reports): korrid, Slint, Skia and QuickJS in one
file. The D1–D4 viewer's library is about 33 MB. Stripping the symbols is a
choice for a release build. Skia's archives are downloaded binaries pinned by
hash, not built here.

## Proof

`nix run .#android-app-proof -- --out DIR` (`proof/run.py`), with no person:
one headless Android 12 emulator (google_apis x86_64, 2 cores, `nice` 10,
console port 5612, its data wiped), the APK signed with the debug key. It
runs where it is started, so run it on aka, not zao (the owner's rule), with
an adb server of its own (`ANDROID_ADB_SERVER_PORT`) and a free console port
(`--port`); the record shows the command.

| Check | How |
| --- | --- |
| No Korri code | `unzip -l`: no dex. The manifest: `hasCode="false"`. `aapt2 dump badging`: `android.app.NativeActivity`, package `com.simonwjackson.korri.app`, both ABIs |
| korrid serves and answers | The app logs its loopback port and the answer of the shell's authenticated `system.health`. Through `adb forward`, the same port refuses a call without the capability and one with a wrong capability |
| The shell's home | The readiness line names Velvet, the surface the app opens (#424), on its library page (`empty` on a new device with no games); a screenshot shows it |
| korrid's network facts in Rust | korrid logs how many addresses it observed, and on which interfaces |
| Window flags | The app logs that it applied them on Android's main thread |
| Back | Two Backs: the activity stays resumed, in the same process |
| Home, then a relaunch | The launcher resumes; a start shows the shell again in the same process; korrid still serves |
| A new activity, the same process | A density change, which the manifest does not handle, makes Android destroy the activity and make a new one; its `android_main` finds korrid running on the same port |
| A new process | After `am force-stop`, a start runs korrid again on its saved state |
| Nothing crashed | No fatal signal, panic or ANR in logcat |

Records: `proof/android-app-d8/`.

Not proven: a real phone (D12), the arm64 library on a device, speed.

Known gaps:

- The status bar stays drawn on the emulator. The window manager reports
  it hidden (`InsetsSource type=ITYPE_STATUS_BAR … visible=false`), with its
  insets animation's leash still in place, and Velvet draws under it. Not
  known whether the Artemis app, which makes the same calls, shows the same.
- On a new device Velvet shows "Connect your identity": this app has no
  signer route yet (route brief, the NIP-55 row). The emulator proof does not
  need an owner.
- Found on the way: the Android shell gives a surface its first `Resized`
  before Android gives it the window, so 0 x 0. With that question up,
  Velvet's first view never ended, and the shell opened Pico instead.
  `784e44831` (#482) fixed the same loop in Velvet's `split`, found on ibuki;
  `test/motion.test.ts` holds this case. The shell still sends 0 x 0 first; a
  host that waited for the window would not.

## The holder proof (D9)

`nix run .#android-app-holder-proof -- --out DIR` (`proof/holder.py`). It
runs on aka, as root: aka has `/dev/kvm`, and since 2026-11-15 no build or
emulator runs on zao. Copy a checkout there (`/var/tmp/<name>/`) and run it
from that copy. One headless Android 12 emulator (2 cores, console port
5640, its own adb server on 5141), the fixture APK signed with the debug key.

| Piece | What it is | Where it comes from |
| --- | --- | --- |
| The holder | korrid's federation fixture: a real host whose owner is the fixture's person signer (secret 3, public key `f9308a01…`). Its one game, "Acceptance Game", streams from the address in its endpoint record | `services/korrid/examples/federation_fixture.rs`, built as `korrid.federationFixture`; its fourth argument is the Moonlight address (#434 D9) |
| Its relay | The NIP-01 relay of the Artemis app's federation acceptance, unchanged | `clients/android/test/nostr-relay-fixture.ts`; `proof/holder.ts` runs it with the holder, in the acceptance's isolated environment |
| The private sender | stream-cadence's sender fixture on aka, offering `Korri Stream` (the application korrid's viewer asks for) and showing the frame-ID source | `nix/stream-cadence/sender.py` with `application` (#434 D9); the proof checks its `serverinfo` at the address the holder publishes |
| The test device's owner | A private state made by the fixture's own producer, `federation_fixture member DIR`: `DeviceIdentity::load_or_create` and an owner statement signed by secret 3, as the holder binds itself. The proof writes `identity/device.key` and `identity/owner.event.json` into `no_backup/korrid-state` through `run-as` (the APK is debuggable: `aapt2 link --debug-mode`) before the first start, and deletes its own copy | No signer route: the app still has none (route brief, NIP-55 row) |

The app's korrid then does what a bound device does: it publishes its owner
statement to the relay, reads the owner's roster, finds the holder, receives
the holder's endpoint record (encrypted to its device key) and reads the
holder's catalog at `http://127.0.0.1:<port>` through `adb reverse`. The
holder learns of the app at its next discovery poll (60 s), so the game can
take two minutes to appear.

| Check | How |
| --- | --- |
| The APK | No dex, `hasCode="false"`, one loopback relay in `assets/relays.json`, the verifier's classes in the library |
| The sender | It offers `Korri Stream`; its `serverinfo` answers at the holder's Moonlight address |
| The binding | The seeded files are the app's own (`-rw-------`); the relay holds an owner statement for the app's device key, signed by the test owner, which only the app's korrid publishes |
| The holder | The relay holds the holder's endpoint record for the app's device; the app's korrid remembers the holder (`federation/peers.json`) |
| The library | The readiness line says Velvet's `home` page with a count of 1 or more. A newly bound device first shows korrid's "Identity connected" over the library (`identity-connected.png`); Confirm ("Open library") takes it away, and the screen must change (`library.png`) |
| The verifier | `korrid: Android TLS verifier classes loaded` |
| Nothing crashed | No fatal signal, panic or ANR |
| Cleanup | The emulator and its adb server stop; the holder joins its discovery; the sender fixture's restore check passes |

`--no-sender` starts no sender fixture and lets the holder publish
`127.0.0.1:9`: for a hot aka, where the sender fixture refuses to start at
85 °C or more. The binding and the library are proven without it; the
sender's checks are then missing from the record.

Records: `proof/android-app-d9/`.

Known gaps:

- The first key that `adb shell input keyevent` injects after the app starts
  is lost: in three runs the first Confirm, Enter or D-pad centre left the
  screen unchanged and the second worked (`confirm_presses: 2`). Not known
  whether a real controller's first press is lost too, or whether the cause
  is the shell's input path or `input`'s virtual device. The proof presses
  up to three times and records the count.
- The holder runs as root when the proof does, and korrid refuses its launch
  helpers for a runtime UID of 0 (`holder.log`: "runtime UID and GID must
  both be unprivileged"). Listing needs no launch; D10's launch does, so D10
  must start the holder as an unprivileged user.
- iroh's signalling offers from the app go to the holder's relay, which takes
  only the two discovery protocols and refuses them (logcat: `offer_published
  … failed`). The catalog does not need them: it is read over the holder's
  loopback endpoint.

What D10 still needs from this fixture: the viewer needs a client
certificate that the sender accepts. The sender pairs by PIN, not through
korrid's certificate broker (the holder's broker socket is a missing path),
so D10 must bridge the two, or pair the app's key by PIN before the launch.

## The event loop: one poller (plan for D10)

Slint's android-activity platform owns `AndroidApp::poll_events` and the input
queue (`clients/shell/vendor/i-slint-backend-android-activity/lib.rs`, its
`run_event_loop`); only one poller can hold `AndroidApp`. The viewer app of
D1–D4 (`clients/viewer/android/src/lib.rs`, `drive`) polls it itself. That APK
keeps its loop unchanged until D7′. In this app the viewer moves onto the
shell's loop in D10:

| D1–D4 `drive` | In this app (D10) |
| --- | --- |
| `poll_events` with a timeout from the input's next deadline | Slint's loop. The shell's event listener hands `TerminateWindow`, `Destroy` and window resizes to the stream; it never blocks |
| `input_events_iter`: keys and motions to the stream's `Input` | The shell's input filter: while a stream is in front it gives each key and motion to `Input` (D4's `input::key`, `input::motion`) and reports it handled; Guide opens the overlay (D5); otherwise the shell's mapper, as now |
| The input's timers (click, emulated mouse, 250 ms heartbeat) and the 500 ms device check | A Slint timer: `Input::tick` and `next_deadline` from the shell's 16 ms timer |
| `VideoLayer::make` waits with `poll_events` for Android's main thread | A state on the loop thread: post with `run_on_java_main_thread`, then read its channel at each tick. The loop thread never waits for the main thread, which waits for it in `onDestroy` |
| The runtime and its requests on their own threads | Unchanged: `Lifecycle::in_process`, Start and Stop on worker threads |
| The intent's extras start a launch | korrid's Rust `LinuxStreamViewer` adapter over `Lifecycle::in_process`. Its calls arrive on korrid's workers; it posts "show this launch" to the loop thread through the shell's event queue, because the layer and the input belong to that thread |

## What the gameplay overlay needs here (D5)

From the D5 worker (branch `b0299bd21`): its throwaway check drew a
see-through `.slint` panel into the overlay layer's Surface through
`SurfaceWindow::from_surface`.

| Need | Status |
| --- | --- |
| korrid in the process | Done (D8) |
| The shell under the one loop, taking the overlay Surface without `KorriSurfaceHost` | D8 gives the seam: `backend::android::Overlay`, the Artemis host's methods in a trait. D5 implements it in Rust over its `SurfaceControl` layer |
| The surface packages | Done (D8) |
| The overlay's input queues (`shellTakeOverlayKeys`, `Axes`, `Pointers`) | They exist because Artemis's overlay lived in `Game`'s window. Here the layer is in the shell's window, so input comes through the shell's input filter; `take_keys`, `take_axes` and `take_pointers` return nothing, and D5 holds the stream's input while the overlay shows |
| Leave and Resume with korrid (the viewer treaty has no Leave) | D5 and D10: Leave is korrid's `session.leave`; Background and Resume stop the runtime and start a new one, as `KorriStreamViewer.background` and `resume` do |

## Next slices

| Slice | What |
| --- | --- |
| D9 | Done: the holder proof above, and the TLS verifier's classes |
| D10 | The Rust viewer adapter in korrid, on the shell's loop (above) |
| D5 (moved) | The gameplay overlay through `Overlay` |
| D11 | Stream controls (O4) |
| D13 (new) | Local Android games (O3): korrid's launcher through JNI, usage polling on a Rust thread, androidx `FileProvider` bundled unchanged (`hasCode="true"`), and the Guide service, which Android allows only as a Java `AccessibilityService` subclass: one Korri Java file on #434's list, or no Guide over other apps. Gates D7′ |
| D12 | On the Galaxy Z Fold 8 |
