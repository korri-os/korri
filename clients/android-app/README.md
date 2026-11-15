# Korri Android app with no Korri Java (#434)

Slices D8 to D10 of the route brief (`docs/research/2026-11-14-android-app-route.md`).
Android's own `android.app.NativeActivity` loads one Rust library,
`libkorri_android_app.so`. Its `android_main` starts korrid in the app's
process, then runs the native shell on the activity's window. Since D10 the
Rust viewer runs in the same process, on the shell's loop, and korrid drives
it as its stream viewer. The APK has no dex (`hasCode="false"`). The Artemis
app in `clients/android` does not change; it stays the shipping Android app
until slice D7′.

```sh
nix build .#korri-android-app-apk          # both ABIs, unsigned
nix build .#korri-android-app-fixture-apk  # x86_64, the holder proof's relay and the D11 proof trigger
nix run .#android-app-proof                # D8: one headless API 31 emulator, no person
nix run .#android-app-holder-proof         # D9, on aka as root: binding and a holder's game
nix run .#android-app-play-proof -- --sunshine PACKAGE  # D10, on aka as root: a tap plays the game
```

## Owner decisions (2026-11-14)

| # | Decision | What it means here |
| --- | --- | --- |
| O1 | The new app installs side by side with the Artemis app, under its own package name. | Package `com.simonwjackson.korri.app`, label "Korri (native)". It is a separate federation device with its own korrid state. It does not answer the HOME intent; the Artemis app keeps that. |
| O2 | korrid runs only while the app is open, with no foreground service yet. | korrid starts once per process and lives as long as the process. No service keeps the process alive in the background: Android can freeze or end it there, and nothing restarts korrid until a person opens the app (the Artemis app's `START_REDELIVER_INTENT` has no equivalent). |
| O3 | Local Android games stay, with androidx `FileProvider` bundled unchanged. | Not in D8. Slice D13 (below) brings them; `FileProvider` then needs a third-party dex and `hasCode="true"`. |
| O4 | The stream controls come in D11: carry every control and status item that the Artemis app reports to korrid. | D11 (below) carries 3 of Artemis's 13 controls (Crop to fill, Flip A/B and X/Y, Rumble), the battery and the health. The other 10 are missing, because the Rust viewer lacks what each needs (`clients/viewer/README.md`, slice D11): Toggle keyboard, Full keyboard, Pan & zoom, Mouse mode, Touch sensitivity, Keyboard as controller, Toggle local cursor, SGSR sharpness, SGSR edge threshold, Picture-in-picture. They gate D7′. |

## What runs

| Part | Where | Java |
| --- | --- | --- |
| korrid | `korrid::android_app::start` (`services/korrid/src/android/app.rs`), on its own thread, once per process. It serves `http://127.0.0.1:<port>/rpc` with a capability made at start. | none |
| korrid's network and power facts | `services/korrid/src/android/framework_network.rs`: JNI into `ConnectivityManager`, `LinkProperties` and the sticky battery intent. The rules are `KorridNetwork.java`'s (`android_network.rs`, tested on the host). | none |
| The shell | `korri_shell::backend::android::run`: the same loop as the Artemis app's, with a Rust host (`src/host.rs`). Rust gives it the port and the capability; neither crosses JNI, a URL, a file or the log. | Slint's `SlintAndroidJavaHelper` (below) |
| Surface packages | Velvet, Pico and Zone in the APK's assets, as the Artemis app carries them, verified on the build machine (`clients/shell/nix/installed.nix`). `src/assets.rs` unpacks them under the cache directory at each start. | none |
| Logs | `src/logcat.rs` sends the process's standard output and error to logcat (tag `korri`): korrid's and the shell's lines, as the journal holds them on Linux. | none |
| The stream viewer | `src/viewer.rs` (D10): korrid's `LinuxStreamViewer` over the Rust viewer's `Lifecycle::in_process`, and the stream on the shell's loop. The viewer library (`clients/viewer`) logs with the tag `korri-viewer`. | none |

What the app keeps in its private storage:

| Path | What | Grounding |
| --- | --- | --- |
| `<no_backup>/korrid-state` | korrid's private state | `KorriBrainService.privateStateRoot` (`services/korrid/src/android_storage.rs`) |
| `<no_backup>/moonlight` | The viewer's Moonlight client key directory: `client.pem` and `key.pem`, mode 0600, in a directory of mode 0700, made at the first launch by the viewer's own `Identity::load_or_generate` | Linux korrid's key directory `/home/korri/.config/moonlight` (`linux_viewer/production.rs`), Moonlight's own leaf name; the names are Moonlight's key-directory producer's (`clients/viewer/README.md`). In storage that no backup takes, so the key never leaves the phone that way (D10) |
| `<cache>/surfaces` | The unpacked surface packages, made again at each start | `KorriShellActivity.copyPackage` |
| `<cache>/viewer-pin-LAUNCH-N.pem` | The server certificate that korrid's broker pinned for one start of a launch, which the runtime reads (`KORRI_MOONLIGHT_SERVER_CERTIFICATE`). Mode 0600, made new for each start, removed when the start ends; the adapter removes any that an earlier process left | Linux korrid hands the same certificate to its viewer as the launch's credential `moonlight-server.pem` (`services/korrid/src/host/viewer_units.rs`). The file name is this app's own: no producer names one in the cache directory (D10) |

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
| Viewer (`KorriStreamViewer`) | Since D10, the Rust viewer in this process (`src/viewer.rs`, below) |

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
- The library builds for Android only. The viewer's rules that need no
  Android (`src/viewer/rules.rs`) have their tests in `tests/host.rs`, which
  each ABI's library derivation runs on the build machine after it compiles
  (`cargo test --test host`).
- Skia's prebuilt archives for both targets (skia-bindings 0.153.3) are fixed
  Nix inputs, read through `SKIA_BINARIES_URL`.
- The surface packages are verified with the shell's own release build, with
  its test suite off; `nix run .#shell-check` runs those tests.
- The relay list is an APK asset, `relays.json`, not a build input of the
  library: `apk` carries `clients/android/product.nix`, the Artemis app's
  list; `fixtureApk` carries one loopback relay, `ws://127.0.0.1:47800`,
  which the holder proof maps to its own relay with `adb reverse`. Until D10
  both APKs shared one library. Since D11 the fixture's library is the same
  source built with the cargo feature `proof` (the controls' proof trigger,
  below), for x86_64 only: the proof's APK runs the product's code plus the
  trigger. korrid takes the list as the product's relays, as before
  (`korrid::android_app::start`); a device setting does not replace it, as
  `KORRID_RELAYS` is not replaced on Linux.
- The TLS verifier's dex (above) is a fixed input of the library
  (`KORRI_ANDROID_VERIFIER_DEX`).
- Since D10 the library links the Rust viewer (`clients/viewer`) with its C
  parts: the unmodified network core and ENet, OpenSSL and libopus, as static
  archives that `clients/viewer/android.nix` builds for each ABI
  (`viewerParts`), and curl from the `curl-sys` crate, exactly as the viewer's
  own APK links them. The lock adds only the viewer's crates, at the versions
  of the viewer's own lock (`curl` 0.4.51, `curl-sys` 0.4.91, `openssl`
  0.10.82, `openssl-sys` 0.9.118, `libz-sys` 1.1.33 and their helpers); no
  crate already locked changed. The viewer's Android parts port Artemis code
  under GPL-3.0-only (`clients/viewer/README.md`), so the library is
  GPL-3.0-only. The library also needs `libz.so`, `libmediandk.so` and
  `libaaudio.so`, which Android ships.

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

What D10 still needed from this fixture, and how D10 met it (below): a
client certificate that the sender accepts (korrid's broker, through the
patched Sunshine's certificate control: no PIN, no bridge), an unprivileged
holder, and a holder that can launch at all.

## The viewer in the app (D10)

A tap on a game that a holder streams makes korrid start the Rust viewer in
this process. korrid's prepare at the holder returns the launch; korrid's
`viewer.start` reaches `src/viewer.rs`:

| Part | Thread | What it does |
| --- | --- | --- |
| `Adapter`, korrid's `LinuxStreamViewer` | korrid's workers | The client key (`<no_backup>/moonlight`, made at the first launch), the server certificate from korrid's broker (`korrid::android_app::provision_moonlight_certificate`: the host UUID from Sunshine's `serverinfo`, then the holder's certificate control, as Linux korrid provisions Moonlight), the runtime's `Lifecycle::in_process`, and every request as the treaty's: `Start`, `Status`, `Snapshot`, `Stop`. Events and replies go to logcat as the treaty's JSON, and each status that korrid reads as `korri-app: viewer status for korrid: LAUNCH STATUS` |
| `Stream` | the shell's loop | The video and overlay layers, the runtime's thread, the stream's input, the window's size, and the activity's orientation while the stream plays (Artemis's Korri mode: the stream's shape) |
| The controllers' feedback (`korri_viewer::feedback_jni`, the viewer's slice D4b) | its own thread, one per launch | The stream's rumble, light and battery reports. The input tells it which controller is which; the runtime stops it after the input's gate and before `LiStopConnection`. The manifest asks for `VIBRATE` for it |

The pinned server certificate is a file of its launch in the app's cache
directory (the storage table above), never in the key directory.

Background and Resume answer that the viewer cannot leave the front yet:
the shell's gameplay overlay (slice D5) is what will take the stream out of
the front. Resize and Notice stay the treaty's typed "unavailable"; Apply
came with D11 (below).

What the person does, by slice D5's rules (`korri_viewer::play`):

| Input | What happens |
| --- | --- |
| Keys, touches, controllers | To the stream (D4's mapping). A touch never reaches the shell under the video, and nor does a key that the stream does not map: on the emulator such a key raised the soft keyboard over the stream. Android's own keys (volume, power) stay Android's |
| Back or Guide | Opens the overlay's stand-in and holds the stream's input; again closes it |
| Volume and power under the overlay | Android's |
| Home, another app | The runtime stops (`Stopped`). korrid's recovery starts the stream again while its launch is selected and the app is in front: it fails at once while the app has no window, and plays again when the app returns |
| Ctrl+Alt+Shift+Q | The runtime stops, as on Home; korrid's recovery restarts it while the app is in front |

The play proof (`proof/play.py`, `nix run .#android-app-play-proof`, on aka
as root) runs these steps on one emulator with no person: the tap, Running in
the app and in korrid, the frame-ID bar, the overlay, the keys and a tap at
the sender, the tone, Home, and the return. Its docstring lists them. On the
emulator a letter key starts an input method session that takes the next
Back, so the proof opens the overlay before it sends letters.

Records: `proof/android-app-d10/`.

## The event loop: one poller (D10)

Slint's android-activity platform owns `AndroidApp::poll_events` and the input
queue (`clients/shell/vendor/i-slint-backend-android-activity/lib.rs`, its
`run_event_loop`); only one poller can hold `AndroidApp`. The viewer app of
D1–D5 (`clients/viewer/android/src/lib.rs`, `drive`) polls it itself and
keeps its loop until D7′. In this app the viewer runs on the shell's loop,
through three host hooks of `korri_shell::backend::android::Host`:

| D1–D5 `drive` | In this app |
| --- | --- |
| `poll_events` with a timeout from the input's next deadline | Slint's loop. `Host::main_event` hands `InitWindow`, `TerminateWindow` and `Destroy` to the stream; it never blocks |
| `input_events_iter`: keys and motions to the stream's `Input` | `Host::input`, first in the shell's input filter: while a stream is in front it gives each event to `korri_viewer::play::Play` (D4's mapping, D5's overlay) and reports it taken, or Android's; otherwise the shell's mapper, as before |
| The input's timers and the 500 ms device check | `Host::tick` on the shell's 16 ms timer: `Play::tick` |
| The decoder's new picture size (#476) wakes the loop (`create_waker`), which places the video again (`Layers::restream`) | The decoder's output thread stores the size; the next `Host::tick` takes it and places the video again. Nothing wakes Slint's loop: its timer runs every 16 ms |
| `Layers::make` waits with `poll_events` for Android's main thread | `Layers::request` posts to the main thread; `Pending::poll` reads the answer at each tick. The loop thread never waits for the main thread, which waits for it in `onDestroy` |
| The runtime and its requests on their own threads | Unchanged: `Lifecycle::in_process`, Start and Stop on threads of their own |
| The intent's extras start a launch | korrid's `viewer.start` (the adapter) sends the launch to the loop thread over a channel; the layers and the input belong to that thread |

korrid starts once per process and keeps the adapter it was given; each
activity's `android_main` attaches a new `Stream` to it.

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
| Leave and Resume with korrid (the viewer treaty has no Leave) | D5: Leave is korrid's `session.leave`; Background and Resume stop the runtime and start a new one, as `KorriStreamViewer.background` and `resume` do. D10's adapter answers both as unavailable until then |

## Stream controls and status (D11)

O4: korrid's snapshot of the stream carries the controls, the battery and
the health, and korrid's `apply` sets a control, as with the Artemis app.
The Rust viewer produces them (`clients/viewer/README.md`, slice D11: which
controls, their source in Artemis, and the ones it cannot carry). Here:

| Part | What |
| --- | --- |
| Attach | When a stream starts, the loop thread attaches its controls to the runtime's lifecycle (`Lifecycle::controls`), with the viewer's readings (`korri_viewer::status::readings`: the battery through JNI, the decoder's health, the core's round trip). The activity's number is the controls' lease |
| `snapshot` | The treaty's Snapshot, on a thread of korrid's blocking pool (the readings go into Android), as Apply's request; its health sample is dated in korrid's clock without losing its age (`ViewerSnapshot::date_sample`, the rule of korrid's Java adapter, now shared). The log names each snapshot whose controls changed: `korri-app: viewer snapshot for korrid: LAUNCH {…}` |
| `apply` | The treaty's Apply (`korri-app: viewer request {…Apply…}` and its reply). The loop thread performs the change at its next tick: Crop to fill moves the video layer and counts, and answers, only once Android's main thread has applied the transaction (one that it did not apply is undone), then moves the input's map; Flip A/B and X/Y changes the controllers' mapping; Rumble goes to the controllers' feedback thread (D4b's), and is offered only while a controller that can rumble is in the stream. Each logs `korri-app: viewer control EFFECT is VALUE` |

korrid's own callers:

| Caller | In this app |
| --- | --- |
| The live-shape poll (`upstreams/live_shape.rs`, `synchronize_shape`, #476) reads the viewer's snapshot while a stream runs | Runs: the D11 proof's log shows korrid's own reads with the controls, and with Crop to fill on after the apply |
| The gameplay overlay's snapshot poll, every second, and its `InvokeGameplayControl`, which reaches `apply` (`surface/overlay.rs`; korrid's unit tests `surface/viewer.rs`, `surface/overlay.rs`) | Not yet: korrid's overlay starts only after its `session.leave`, and its input reaches it only through the shell's overlay in this app, slice D5b |

So nothing in the product calls `apply` here until D5b, and the overlay does
not show the controls yet. Until then the emulator proof calls korrid's
adapter through a trigger.

**The proof trigger** (`src/proof.rs`, cargo feature `proof`): only the
fixture APK's library is built with it, for x86_64. The proof writes one
command into `cache/proof-command` with `run-as` (`snapshot LAUNCH`, or
`apply LAUNCH CONTROL toggle true`), and the trigger calls korrid's own viewer
adapter, the object that korrid holds (`viewer::adapter` returns the same
`Arc` each time), and logs the answer: `korri-app: proof: snapshot LAUNCH {…}`
(korrid's `ViewerSnapshot` as JSON), `korri-app: proof: apply … ok`. The
product library's build fails if cargo built it with the feature (its
fingerprint) or if its bytes hold the trigger's log prefix (`default.nix`).

The play proof (`proof/play.py`) runs D11's steps after D10's input: korrid's
own reads and the trigger's snapshot list the controls, the battery and the
health; Crop to fill on moves the video layer (the app's log, SurfaceFlinger's
crop, a screenshot) and korrid reads it On; a stale control is refused; Off
fits the picture again. Records: `proof/android-app-d11/`.

## Next slices

| Slice | What |
| --- | --- |
| D9 | Done: the holder proof above, and the TLS verifier's classes |
| D10 | Done: the Rust viewer, driven by korrid, on the shell's loop (above) |
| D5 (moved) | The gameplay overlay through `Overlay` |
| D11 | Done in part: 3 of Artemis's 13 stream controls (Crop to fill, Flip A/B and X/Y, Rumble), the battery and the health, through the treaty (above). The product path to Apply and the overlay that shows them is D5b's. Missing, and gating D7′: Toggle keyboard, Full keyboard, Pan & zoom, Mouse mode, Touch sensitivity, Keyboard as controller, Toggle local cursor, SGSR sharpness, SGSR edge threshold, Picture-in-picture |
| D13 (new) | Local Android games (O3): korrid's launcher through JNI, usage polling on a Rust thread, androidx `FileProvider` bundled unchanged (`hasCode="true"`), and the Guide service, which Android allows only as a Java `AccessibilityService` subclass: one Korri Java file on #434's list, or no Guide over other apps. Gates D7′ |
| D12 | On the Galaxy Z Fold 8 |
