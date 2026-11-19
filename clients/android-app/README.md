# Korri Android app with no Korri Java (#434)

Slices D8 to D11 and D5b of the route brief (`docs/research/2026-11-14-android-app-route.md`).
Android's own `android.app.NativeActivity` loads one Rust library,
`libkorri_android_app.so`. Its `android_main` starts korrid in the app's
process, then runs the native shell on the activity's window. Since D10 the
Rust viewer runs in the same process, on the shell's loop, and korrid drives
it as its stream viewer. The APK has no dex (`hasCode="false"`). It is
Korri's only Android app: slice D7′ removed the Artemis app on 2026-11-18
(ADR 0087; the tag `artemis-final` keeps it, and
`docs/briefs/2026-11-18-artemis-removal-scope.md` records what went).

```sh
nix build .#korri-android-app-apk          # both ABIs, unsigned, debuggable
nix build .#korri-android-app-fixture-apk  # x86_64, the holder proof's relay and the D11 proof trigger
nix build .#korri-android-app-release-apk  # D12 prep: arm64, not debuggable, notices, unsigned
nix run .#android-app-release-key          # D12 prep, once, on zao: the release key
nix run .#android-app-release-sign -- UNSIGNED.apk SIGNED.apk  # D12 prep, on zao
nix run .#android-app-d12-binding -- make WORK  # D12, on zao: the owner binding (also sign-debug, seed)
nix run .#android-app-d12-binding-check         # its test, with a throwaway bunker; no phone
# The proofs run on aka and build in its own store (--build-store auto).
nix run .#android-app-proof -- --build-store auto         # D8: one headless API 31 emulator, no person
nix run .#android-app-holder-proof -- --build-store auto  # D9, on aka as root: binding and a holder's game
nix run .#android-app-play-proof -- --build-store auto --sunshine PACKAGE  # D10, D11, D5b, on aka as root: a tap plays the game
nix run .#android-app-resize-proof -- --build-store auto --sunshine PACKAGE  # #476, on aka as root: korrid carries a resize to the holder's Sunshine
```

## Owner decisions (2026-11-14)

| # | Decision | What it means here |
| --- | --- | --- |
| O1 | The new app installs side by side with the Artemis app, under its own package name. | Package `com.simonwjackson.korri.app`, label "Korri (native)". It is a separate federation device with its own korrid state. It does not answer the HOME intent; the Artemis app kept that until it went (D7′). |
| O2 | korrid runs only while the app is open, with no foreground service yet. | korrid starts once per process and lives as long as the process. No service keeps the process alive in the background: Android can freeze or end it there, and nothing restarts korrid until a person opens the app (the Artemis app's `START_REDELIVER_INTENT` has no equivalent here). |
| O3 | Local Android games stay, with androidx `FileProvider` bundled unchanged. | Not in D8. Slice D13 (below) brings them; `FileProvider` then needs a third-party dex and `hasCode="true"`. |
| O4 | The stream controls come in D11: carry every control and status item that the Artemis app reports to korrid. | D11 (below) carries 3 of Artemis's 13 controls (Crop to fill, Flip A/B and X/Y, Rumble), the battery and the health; Toggle keyboard, Mouse mode, Pan & zoom and Toggle local cursor came after it. The other 6 are missing, because the Rust viewer lacks what each needs (`clients/viewer/README.md`, slice D11): Full keyboard, Touch sensitivity, Keyboard as controller, SGSR sharpness, SGSR edge threshold, Picture-in-picture. They gated D7′ until the owner's decision of 2026-11-18 (ADR 0087): Full keyboard and Keyboard as controller (the on-screen controller) are dropped, to be rebuilt later product first; Picture-in-picture is skipped; SGSR, as a list of upscalers, and Touch sensitivity go to the backlog. |

## What runs

Java names in this README (`KorriShellActivity`, `KorriBrainService`,
`KorriStreamViewer`, `Game` and the rest) are the Artemis app's, at the tag
`artemis-final`: what this app replaced.

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
| `<no_backup>/moonlight` | The viewer's Moonlight client key directory: `client.pem` and `key.pem`, mode 0600, in a directory of mode 0700, made when the app starts, on a thread of its own (#510), by the viewer's own `Identity::load_or_generate`; a launch that asks for it meanwhile waits for it | Linux korrid's key directory `/home/korri/.config/moonlight` (`linux_viewer/production.rs`), Moonlight's own leaf name; the names are Moonlight's key-directory producer's (`clients/viewer/README.md`). In storage that no backup takes, so the key never leaves the phone that way (D10) |
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
| `KorriSurfaceHost` (the overlay Surface and its input queues) | The shell's `Overlay` trait, over the overlay layer of the stream in front (`src/viewer.rs`, slice D5b, below) |

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
`SOFT_FAIL`, passes when it cannot fetch. The Artemis app did not carry it
either: its own `network_security_config` had the same resource name and
replaced the AAR's.

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
of `nix/android-sdk.nix`, one derivation for each ABI's library; then
`aapt2 link` against android-36 and `zipalign` in a derivation that only
packs, so a change to the packages or the manifest compiles nothing. No
Gradle. The output is unsigned; the proof signs it with the user's debug key.

The APK is debuggable: `aapt2 link --debug-mode` sets
`android:debuggable="true"`, which lets `run-as` and a debugger into the
app's process and its files, korrid's private state included. The release
APK (below, D12 prep) is the build without `--debug-mode`, signed with a
release key outside the store.

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
  library: `apk` carries `product.nix`, the product's list (the Artemis
  app's list too); `fixtureApk` carries one loopback relay, `ws://127.0.0.1:47800`,
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
- Since #391 step E the viewer carries nightjar-core (`clients/viewer/vendor`,
  GPL-3.0-only), its default network core, as in the viewer's own APK; the
  C core stays in the library. The library is built with the viewer's
  `nightjar` feature, and on arm64 with the two cfgs that give nightjar-core's
  AES and POLYVAL their ARMv8 code (`clients/viewer/android.nix`); its build
  checks for `LiStartConnection`, `nightjar_core::session::start` and, on
  arm64, that ARMv8 code. The lock adds nightjar-core, the Rubylight crates,
  Nightjar's `rusty_enet` and their crates (`rsa` 0.9.10 with its `pkcs1`,
  `pkcs8`, `spki`, `der`, `const-oid`, `num-bigint-dig`, `num-iter`,
  `signature` and `spin` 0.9.9, and `socket2` 0.5.10), at the versions of the
  viewer's own lock; no crate already locked changed. korrid's launch names
  no core. Only the fixture APK can choose one: its proof trigger reads
  `<cache>/proof-network-core`, and `c` there makes the next streams use the
  C core (`src/proof.rs`, `proof/play.py --network-core c`).

Cost: the library is large, about 103 MB for each ABI (54 MB of code, 26 MB
of symbols, kept for crash reports): korrid, Slint, Skia and QuickJS in one
file. The D1–D4 viewer's library is about 33 MB. Stripping the symbols is a
choice for a release build. Skia's archives are downloaded binaries pinned by
hash, not built here.

## Proof

`nix run .#android-app-proof -- --build-store auto --out DIR` (`proof/run.py`),
with no person: one headless Android 12 emulator (google_apis x86_64, 2
cores, `nice` 10, console port 5612, its data wiped), the APK signed with the
debug key. It runs where it is started, so run it on aka, not zao (the
owner's rule), with an adb server of its own (`ANDROID_ADB_SERVER_PORT`) and
a free console port (`--port`); the record shows the command.

Each proof (this one, the holder proof, the play proof and the resize
proof) builds in the Nix store that `--build-store` names, with
stream-cadence's build (`nix/stream-cadence/cli.py`): `auto` on aka itself,
which copies nothing back. Without it the proof builds nothing: it refuses,
naming the derivations, when Nix's plan builds anything.

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
  known whether the Artemis app, which made the same calls, showed the same.
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

`nix run .#android-app-holder-proof -- --build-store auto --out DIR` (`proof/holder.py`). It
runs on aka, as root: aka has `/dev/kvm`, and since 2026-11-15 no build or
emulator runs on zao. Copy a checkout there (`/var/tmp/<name>/`) and run it
from that copy. One headless Android 12 emulator (2 cores, console port
5640, its own adb server on 5141), the fixture APK signed with the debug key.

| Piece | What it is | Where it comes from |
| --- | --- | --- |
| The holder | korrid's federation fixture: a real host whose owner is the fixture's person signer (secret 3, public key `f9308a01…`). Its one game, "Acceptance Game", streams from the address in its endpoint record | `services/korrid/examples/federation_fixture.rs`, built as `korrid.federationFixture`; its fourth argument is the Moonlight address (#434 D9) |
| Its relay | The NIP-01 relay of the Artemis app's federation acceptance (tag `artemis-final`), moved here unchanged | `proof/nostr-relay-fixture.ts`; `proof/holder.ts` runs it with the holder, in the acceptance's isolated environment (`hostEnvironment`, with `proof/federation-systemd-fixture.sh`) |
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
| `Adapter`, korrid's `LinuxStreamViewer` | korrid's workers | The client key (`<no_backup>/moonlight`, made when the app starts; loaded here), the server certificate from korrid's broker (`korrid::android_app::provision_moonlight_certificate`: the host UUID from Sunshine's `serverinfo`, then the holder's certificate control, as Linux korrid provisions Moonlight), the runtime's `Lifecycle::in_process`, and every request as the treaty's: `Start`, `Status`, `Snapshot`, `Stop`. Events and replies go to logcat as the treaty's JSON, and each status that korrid reads as `korri-app: viewer status for korrid: LAUNCH STATUS` |
| `Stream` | the shell's loop | The video and overlay layers, the runtime's thread, the stream's input, the window's size, and the activity's orientation while the stream plays (Artemis's Korri mode: the stream's shape) |
| The controllers' feedback (`korri_viewer::feedback_jni`, the viewer's slice D4b) | its own thread, one per launch | The stream's rumble, light and battery reports. The input tells it which controller is which; the runtime stops it after the input's gate and before `LiStopConnection`. The manifest asks for `VIBRATE` for it |

The pinned server certificate is a file of its launch in the app's cache
directory (the storage table above), never in the key directory.

Resume closes the gameplay overlay (slice D5b, below). Background, korrid's
"Open Korri", answers that the viewer cannot leave the front yet. Resize and
Notice stay the treaty's typed "unavailable"; Apply came with D11 (below).

What the person does, by slice D5's rules (`korri_viewer::play`) and D5b's:

| Input | What happens |
| --- | --- |
| Keys, touches, controllers | To the stream (D4's mapping). A touch never reaches the shell under the video, and nor does a key that the stream does not map: on the emulator such a key raised the soft keyboard over the stream. Android's own keys (volume, power) stay Android's |
| Back or Guide | Opens Korri's gameplay overlay (D5b, below) and holds the stream's input until it closes. In the overlay, Back and Guide (as Menu) go to the shell's surface package, which dismisses the overlay through korrid |
| Volume and power under the overlay | Android's |
| Home, another app | The runtime stops (`Stopped`). korrid's recovery starts the stream again while its launch is selected and the app is in front: it fails at once while the app has no window, and plays again when the app returns |
| Ctrl+Alt+Shift+Q | The runtime stops, as on Home; korrid's recovery restarts it while the app is in front |

The play proof (`proof/play.py`, `nix run .#android-app-play-proof --
--build-store auto --sunshine PACKAGE`, on aka as root) runs these steps on one emulator with no person: the tap, Running in
the app and in korrid, the frame-ID bar, the overlay, the keys and a tap at
the sender, the tone, Home, and the return. Its docstring lists them. In D10,
on the emulator, a letter key started an input method session and the next
Back did not open the overlay (#491), so the proof opens the overlay before
it sends letters. Since the stream's view holds the focus (Toggle keyboard,
below) a letter key shows no input method, and the proof also presses Back
after the letters: the overlay must open, and Back must close it.

Records: `proof/android-app-d10/`; Back after the letters,
`proof/android-app-back-after-keys/`, 153 of 153 checks.

### From the tap to the first frame (#391)

korrid's `viewer.start` hands the launch to the loop thread before korrid's
broker has provisioned the server certificate. The loop thread asks
Android's main thread for the layers and turns the window to the stream's
shape at once; the pinned certificate follows over a channel of its own,
and the runtime starts once the loop thread has both (`rules::making`,
host-tested). A stop, a window that goes, or a part that fails ends the
launch without waiting for the other part: the layers go, made ones at once
and ones still being made once the main thread has made them, and the
window turns back, as it does when the runtime does not start. A launch
that arrives while the stream in front ends (its stop asked for, its
runtime returned, or its Stopped reported) waits for it and then begins,
instead of being refused: korrid starts again as soon as it reads Stopped,
which can come before the loop thread's next tick lets the old stream go
(`rules::arriving`). It waits at most the lifecycle's stop deadline (30 s,
`lifecycle::STOP_DEADLINE`): if the old stream has not ended by then, the
waiting launch is refused ("the stream in front did not end within 30 s"),
so that a stream that never ends cannot hold korrid's start, and the
adapter's one start or stop at a time, for ever (`rules::Waiting`). The client key is made when the app starts, on a
thread of its own (#510), not in the first launch's start. Until #391 the layers were asked only after the
certificate, and the window turned only after Start.

korrid's broker asks each owner device whether its Sunshine has the host
UUID, each for at most 5 s (`NATIVE_RPC_TIMEOUT`). Until #510 it waited for
all of them, so one offline owner device held every start for those 5 s: on
the owner's phone, 5.04 s of the 5.4 s from the viewer's start to the first
frame (2026-11-19). Now the holder of the selected stream decides as soon
as it attests the UUID, and the others are not waited for; without its
attest every device decides, as before. korrid logs the step's times
(`korrid: certificate peer: …` or `korrid: certificate peers: …`, then
`korrid: certificate provision at …`), and the holder logs its own part
(`korrid: certificate provision for peer …`). The play proof's
`--offline-peer` adds an owner device that never answers and checks the
step. Record: `proof/android-app-510/` (aka's emulator: 5 457 ms before,
420 ms after).

The viewer's steps and the app's write `start:` lines with their
monotonic times (`clients/viewer`, "The start's timeline"). The play proof
reads them from the tap to the first frame into `result.json`
(`steps.start_timeline`), with the emulator's TCP counters over the same
time (`steps.emulator_tcp_from_tap_to_first_frame`). Records:
`proof/android-app-391/`.

### The bars beside the picture (#495)

The shell draws its library into the activity's window, and goes on drawing
it while a stream plays (its readiness line logs Velvet's home page under
the overlay). Until #495 the video layer, the picture's size, lay straight
above that window, so the bars beside a 16:9 picture showed the library,
and so did the overlay's panel, which is 94 % opaque. The viewer's layers
(`korri_viewer::layer`) now hold a black backdrop between the window and
the video: opaque, covering the window, stretched with it when it turns, and
shown from the first decoded picture on, as Artemis's Korri mode turns its
window black at the first frame. The play proof reads the bars in each
screenshot of the stream (`outside` in `proof/play.py`): black with the
picture fitted, the panel over black with the overlay open. With Crop to
fill it reads the picture's real bounds in SurfaceFlinger's dump
(`fills_the_window`): the video's buffers show over the whole window, and
the backdrop's buffers and the shell's window show nowhere (#497). Records:
`proof/android-app-495/`.

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

## Korri's gameplay overlay (D5b)

The shell draws its surface package's gameplay overlay into the overlay
layer over the stream, and korrid's overlay controller drives it, as in the
Artemis app. The D5 spike proved that the shell's renderer draws into such a
layer (`SurfaceWindow::from_surface`); D5's stand-in is gone from this app
(the viewer's own app keeps it). The steps and their Artemis sources:

| Step | Here | Artemis |
| --- | --- | --- |
| Guide or Back opens it | `korri_viewer::play` holds the stream's input through D4's gate (`Input::hold`) and answers `Routed::Opened` | `Game.showSurfaceOverlay`: `NvConnection.setInputHeld(true)` |
| The shell takes the Surface | The `Overlay` host is the stream (`viewer.rs`): `take_surface` gives the shell the overlay layer's `android.view.Surface`, once an opening, with the window's density | A new `SurfaceView`; `KorriSurfaceHost.attach`, `shellTakeOverlaySurface` |
| korrid hears that the person left | After the shell took the Surface, a thread of its own calls korrid's `session.leave` (`korrid::android_app::leave_viewer`, the body of Artemis's JNI `leaveViewer`, now shared). korrid publishes Home, and its overlay controller (`surface/overlay.rs`) presents the overlay; it polls the viewer's Snapshot every second | `KorriStreamViewer.leave` |
| It shows | The layer stays hidden until the shell has drawn into it (`Overlay::drawn`, a hook the shell gained here): until then the layer keeps the last frame of an earlier opening | The new view starts empty |
| Its input | Keys, sticks and touches go to the shell through the `Overlay` queues (`take_keys`, `take_axes`, `take_pointers`), by `KorriSurfaceHost.forward`'s rules (`korri_viewer::overlay::Forwarded`). Guide reaches the package as Menu. A Guide or Back press that began with the overlay open goes whole, down and up, at its release. Volume and power stay Android's | `KorriSurfaceHost.forward`, `forwardTouch`; `Game.handleKorriHomeKey` |
| Its controls | The package's `InvokeGameplayControl` of a stream control reaches korrid's overlay controller, then the adapter's `apply` (D11): Crop to fill moves the video layer under the overlay | `KorriViewerControls.apply` |
| It closes | The package's Dismiss (Back, Menu) makes korrid resume the stream: korrid's `resume_and_focus` reaches the loop thread, which hides the layer and asks the shell for the Surface (`closing`); the shell lets it go at its next tick (`released`); then the stream's input goes on, and korrid's Resume is answered, within 15 s. It hears Ok only when its own resume closed the overlay: a stop, the stream's end or the shell's teardown in the meantime answers with that failure (`hosted::Closing`) | `KorriStreamViewer.resume` → `Game.hideSurfaceOverlay`: `KorriSurfaceHost.detach` waits for the release, then `setInputHeld(false)` |

The rules of the hand-off are pure and host-tested (`src/viewer/hosted.rs`,
through `tests/host.rs`); the input's are the viewer's
(`korri_viewer::overlay`, its tests).

- No thread waits for Android's main thread: the layer's visibility is a
  posted transaction, and korrid's Resume waits on a channel that the loop
  thread answers. korrid's `session.leave` is an RPC on a thread of its own.
- Teardown order: the shell lets the Surface go before the layers go. A
  stream that ends or stops while the overlay is open closes it first; the
  layers go at the tick after the shell's release. When the activity ends,
  the shell's own teardown releases its renderer before the stream drops.
- A refused `session.leave`, a leave whose thread does not start, or a
  shell that cannot draw into the Surface, closes the overlay and logs why.
  Artemis also shows a toast; this app shows none.
- The plan of D8 above said that the overlay's input would come through the
  shell's input filter and its queues would stay empty. A touch must reach
  the overlay's own Slint window, which the filter does not know, so the
  queues carry the input, as in Artemis.
- Not carried: "Open Korri" (the treaty's Background). Artemis ends its
  activity and keeps the launch; this app cannot take the stream out of the
  front yet, so korrid's overlay shows "Could not leave the stream". If
  korrid never presents its overlay after the leave, the stream's input
  stays held until Home, as in Artemis.
- The camera cutout (#494): the overlay layer stands out of it, as Artemis
  keeps its overlay view inside the display cutout's safe area
  (`KorriStreamCutout.keepOutOfCutout`: the view's margins are the cutout's
  insets). The layer covers the window less the cutout's insets
  (`korri_viewer::placement::keep_out_of_cutout`), which Android's main
  thread reads from the window each time it places the layer, and the
  shell draws at that size. It follows the cutout when the window turns: a
  new window size places it again, and a turn by 180°, which keeps the
  size and moves the cutout to the other side, reaches the loop thread as
  a main event (`layer::may_move_the_cutout`); its next tick asks the main
  thread to place the layer again, which moves it only if its place
  changed. A touch reaches the shell's overlay from the layer's corner, as
  `Game.dispatchTouchEvent` offsets it. The video and the backdrop still
  reach the window's edges, into the cutout. Proven on the emulator with
  its emulated Pixel 5, in both landscape turns (`proof/android-app-494/`);
  not yet on a phone (D12).

The logcat lines: the viewer library's `overlay: open on Guide; the
stream's input is held`, `overlay: layer placed at (L, T)-(R, B) in the
window WxH, out of the display cutout (left …, top …, right …, bottom …)`,
`overlay: layer shown`, `overlay: layer hidden` and
`overlay: closed on korrid resumed the stream after … s; it took … keys and
… motion events`; the app's `korri-app: overlay: the shell took the layer's
Surface; korrid hears that the person left`, `korri-app: overlay: korrid's
session.leave for LAUNCH: ok`, `korri-app: overlay: the shell drew it; the
layer shows`, `korri-app: overlay: the shell let the layer's Surface go`
and `korri-app: viewer resume for korrid: LAUNCH: the stream is in front,
its input goes on`.

The play proof (`proof/play.py`) runs D5b's steps where D10's stand-in step
was, before any letter key: Guide opens the overlay over the moving stream;
the shell takes the layer, korrid hears the leave and the shell draws; the
picture beside the overlay's panel moves (the panel covers the left of the
frame-ID bar, which then does not read); tesseract reads the overlay's words
(Disconnect, Open Korri, then Crop to fill and Flip A/B and X/Y once the
focus has taken the list down to them); Crop to fill, chosen in the overlay,
moves the video layer (the app's log, korrid's Apply through the treaty,
SurfaceFlinger's crop, the window's right edge, beside the panel), and
chosen again fits it again; the sender's Sunshine receives no input while
the overlay is open; Back closes it through korrid's Resume, after the shell
let the Surface go, and the picture goes on. D10's keys and tap then reach
the sender. Records: `proof/android-app-d5b/`.

The emulator's display has a camera cutout since #494 (`--cutout`, by
default the system image's emulated Pixel 5: a hole in the top left corner,
and rounded corners; a cutout overlay alone, such as `corner`, crashes this
image's SystemUI, whose screen decorations need rounded corners or a second
cutout). With the overlay open and drawn, and
before its controls, the proof reads the overlay layer in SurfaceFlinger:
it stands in the window less the cutout's insets that the window manager
reports, shows exactly there with buffers of that size, meets none of the
cutout's rectangles, and the app logged that place; the video and the
backdrop still reach the window's edges. The emulator's accelerometer then
turns the display by 180°, to the other landscape turn, and back; the layer
follows the cutout each time. Records: `proof/android-app-494/`.

## Stream controls and status (D11)

O4: korrid's snapshot of the stream carries the controls, the battery and
the health, and korrid's `apply` sets a control, as with the Artemis app.
The Rust viewer produces them (`clients/viewer/README.md`, slice D11: which
controls, their source in Artemis, and the ones it cannot carry). Here:

| Part | What |
| --- | --- |
| Attach | When a stream starts, the loop thread attaches its controls to the runtime's lifecycle (`Lifecycle::controls`), with the viewer's readings (`korri_viewer::status::readings`: the battery through JNI, the decoder's health, the core's round trip). The activity's number is the controls' lease |
| `snapshot` | The treaty's Snapshot, on a thread of korrid's blocking pool (the readings go into Android), as Apply's request; its health sample is dated in korrid's clock without losing its age (`ViewerSnapshot::date_sample`, the rule of korrid's Java adapter, now shared). The log names each snapshot whose controls changed: `korri-app: viewer snapshot for korrid: LAUNCH {…}` |
| `apply` | The treaty's Apply (`korri-app: viewer request {…Apply…}` and its reply). The loop thread performs the change at its next tick: Crop to fill moves the video layer and counts, and answers, only once Android's main thread has applied the transaction (one that it did not apply is undone), then moves the input's map; Flip A/B and X/Y changes the controllers' mapping; Rumble goes to the controllers' feedback thread (D4b's), and is offered only while a controller that can rumble is in the stream. Each logs `korri-app: viewer control EFFECT is VALUE`. Toggle keyboard (a command, offered on a touch screen) answers once Android's main thread has given NativeActivity's view the focus and called `toggleSoftInput` (`korri_viewer::soft_keyboard`), and logs `korri-app: viewer control toggle-keyboard ran`; NativeActivity's view holds the focus while a stream plays, and when the stream ends the keyboard hides and the view gives the focus back. Mouse mode (a choice, offered on a touch screen) changes the touch screen's mapping and logs `korri-app: viewer control set-mouse-mode is N`. Pan & zoom (a toggle, offered on a touch screen) gives the touch screen's gestures to the picture's zoom: each moves the video layer, and the placement line, once the gestures stop, ends with `, zoomed N times` |

korrid's own callers:

| Caller | In this app |
| --- | --- |
| The live-shape poll (`upstreams/live_shape.rs`, `synchronize_shape`, #476) reads the viewer's snapshot while a stream runs | Runs: the D11 proof's log shows korrid's own reads with the controls, and with Crop to fill on after the apply |
| The gameplay overlay's snapshot poll, every second, and its `InvokeGameplayControl`, which reaches `apply` (`surface/overlay.rs`; korrid's unit tests `surface/viewer.rs`, `surface/overlay.rs`) | Runs since D5b (above): the overlay lists the controls, and Crop to fill chosen in it moves the video layer (`proof/android-app-d5b/`) |

Before D5b nothing in the product called `apply` here, so the D11 proof
called korrid's adapter through a trigger, which it still does.

**The proof trigger** (`src/proof.rs`, cargo feature `proof`, which also
sets korrid's feature `proof`): only the fixture APK's library is built with
it, for x86_64. The proof writes one
command into `cache/proof-command` with `run-as` (`snapshot LAUNCH`, or
`apply LAUNCH CONTROL toggle true`), and the trigger calls korrid's own viewer
adapter, the object that korrid holds (`viewer::adapter` returns the same
`Arc` each time), and logs the answer: `korri-app: proof: snapshot LAUNCH {…}`
(korrid's `ViewerSnapshot` as JSON), `korri-app: proof: apply … ok`. The
product library's build fails if cargo built it or korrid with the feature
(their fingerprints) or if its bytes hold the trigger's log prefix
(`default.nix`).

The play proof (`proof/play.py`) runs D11's steps after D10's input: korrid's
own reads and the trigger's snapshot list the controls, the battery and the
health; Crop to fill on moves the video layer (the app's log, SurfaceFlinger's
crop, a screenshot) and korrid reads it On; a stale control is refused; Off
fits the picture again. Records: `proof/android-app-d11/`.

Toggle keyboard and Mouse mode (toward D7′) run next in the same proof,
through the same trigger. Toggle keyboard (`apply LAUNCH CONTROL none`):
each apply must turn the input method's keyboard over, served to
NativeActivity's view (`dumpsys input_method`); shown, it must be drawn (the
IME's insets leash shown in SurfaceFlinger), and a tap in the middle of its
letters must reach the sender's Sunshine as a letter key; the last apply
hides it; after Home the app logs that the view gave the focus back. A
hardware letter key in the stream must summon no input method. Mouse mode
(`apply LAUNCH CONTROL choice N`): with the gaming track pad (3) a swipe is
relative mouse moves at the sender, disabled (4) a tap sends nothing, and
multi-touch (0) sends it again. On the emulator's touch screen both are
listed, Toggle keyboard first, so the overlay step's D-pad path takes one
more step down to Crop to fill. Records: `proof/android-app-keyboard/`, 115 of
115 checks.

Why the keyboard was never drawn before (`proof/android-app-ime/`): Slint's
helper sets a `WindowInsetsAnimation.Callback` on its text view, and with one
Android 12 runs the keyboard's insets animation on the UI thread, where in
this NativeActivity window the IME's leash stays hidden (Slint's issue
#11791). The host takes every such callback off the window's views at each
resume and focus, and the Toggle keyboard control before each toggle
(`korri_viewer::soft_keyboard::without_insets_animations`). No surface
package has a text field that uses Android's keyboard: Velvet and Pico draw
their own keyboards, and Slint asks for Android's only for its `TextInput`,
which none of them uses. So no proof shows the keyboard over a shell field.

Pan & zoom runs last in the same proof (`apply LAUNCH CONTROL toggle
true|false`): on, a pinch of two fingers, sent with `sendevent` to the
emulator's touch screen, must zoom the video layer (the app's placement line,
SurfaceFlinger's crop), and a drag must move it, with no packet at the
sender; off, the zoom stays and a tap must reach the sender at the point of
the picture under the finger; a new mouse mode must turn it off. Records:
`proof/android-app-pan-zoom/`, 125 of 125 checks.

Toggle local cursor follows (`apply LAUNCH CONTROL none`), with the
emulator's PS/2 mouse: the stream must have asked for the pointer capture at
its start, and Android's dispatcher must give the app's window the capture
(`dumpsys input`); the mouse's own moves and click, sent with `sendevent`,
must reach the sender as relative moves and buttons; korrid's apply must
let the capture go, and the same moves then reach the sender as absolute
moves; applied again, the mouse must be captured again; Home must let it
go. Records: `proof/android-app-local-cursor/`, 138 of 138 checks. A
captured touch pad follows: Android's `uinput` command makes one for a few
seconds, and while the view has the capture, its finger's move must reach
the sender as relative moves, a tap and its button with one finger as left
clicks, and its button with two fingers as a right click. Records:
`proof/android-app-touchpad/`, 141 of 141 checks. Last, Guide opens the
overlay with the mouse captured: the overlay must let the capture go, the
pointer must show on the row Crop to fill, two clicks there must turn it on
and off through korrid with nothing at the sender, and Back must close the
overlay and capture the mouse again (`proof/android-app-overlay-mouse/`,
151 of 151 checks, with the touch pad again).

## korrid's resize request (#476 Route C, slices 2 and 3)

korrid, not the viewer, asks for a stream's new encode size. The viewer's
korrid sends `app.session.viewer.resize` (the size) to the holder over the
peer wire. The holder checks the exact launch and that the caller takes part
in it, reads the `CertificateGrant` that it recorded when it provisioned the
caller's certificate, and sends `resize` with that certificate to its
Sunshine's certificate-control socket (patch 0020). Sunshine (patch 0046)
finds the running session with that certificate and resizes its encoder
through patch 0044's channel. The answer is `Applied`, `Refused` or
`Unavailable`, as the viewer treaty's `Resize` answers.

Sunshine serves one control connection at a time, and a resize holds it for
up to 3 s. A request that waited in Sunshine's queue could pass its own
deadline in korrid and still be applied later. So korrid sends one
certificate-control operation at a time (attest, provision, revoke, resize
and its readiness probe), in the order they asked for their turn, and each
deadline starts when the operation has the socket: 1.5 s, and 4 s for a
resize. The order matters: an older resize must not run after a newer one
that restores the size. At most four of the holder's operations run or wait
at once; a fifth is refused as busy before it reaches Sunshine.

When korrid reads no answer it can trust (a timeout, a failed exchange, an
answer it cannot read, also after an "ok"), Sunshine may still apply the
frame. That can still happen when Sunshine itself is slower than the
deadline. The caller can also go away first: the viewer's korrid gives up
after 5 s, and the holder's request handler is then dropped. A resize's
blocking exchange still sends its frame. A provision or a revocation runs in
a task of its own and ends anyway. So only a failure that proves Sunshine
did not apply a frame (korrid never sent it, or Sunshine refused it before
any change) counts as a failure.

The holder runs one device's provisions and revocations, and their
reconciliation, one at a time: it takes the device's lock before the socket
turn and holds it from the first grant change to the last. A provision or a
revocation does not wait for that lock. While another operation of the same
device holds it, the holder answers `StreamCertificateControlBusy` before
anything changes, so no queue of tasks outlives its callers.

| Operation | What korrid does |
| --- | --- |
| resize | Any other failure is an error, not `Refused`: the viewer's korrid confirms the previous size, as after a lost answer |
| provision | If the device's grant names another certificate, the holder first revokes that one in Sunshine (#502). Only Sunshine's confirmation lets the provision go on; after any other answer Sunshine may still trust it, so the holder keeps that grant and refuses the provision. Then the holder writes the new grant before it sends the provision, and puts back the grant that it replaced (now only one for the same certificate) or removes it only after a proven failure. A grant write that fails may still have left the grant, when only the directory sync after the rename failed: the holder then sends nothing and undoes nothing. So every certificate that a provision makes Sunshine trust has its grant before Sunshine applies it, even if the caller went away |
| revoke | The holder removes the device's grant only after Sunshine confirmed the revocation, and only when the grant names the certificate that was revoked. A revocation whose answer korrid did not read leaves the grant of a certificate that Sunshine may no longer trust |

A grant for a certificate that Sunshine does not trust is harmless: revoking
it changes nothing, and a resize with it reaches at most the device's own
stream. It stays until the device revokes or provisions again, or until
reconciliation retires it when the device's authority ends.

A grant names one certificate for each device. Since #502 a provision of a
new certificate also retires the old one in Sunshine, so a device's
provisions leave at most one trusted certificate, and its grant names it. A
certificate whose grant an older korrid replaced before #502 is still
trusted without a grant; reconciliation does not see it.

`Refused` is only Sunshine's own refusal (a pending request, a failed swap, a
size out of bounds) or a busy korrid; `Unavailable` is a missing or
misconfigured channel, a grant that Sunshine does not know, a Sunshine
without patch 0046, an ended stream or an encoder without live resolution.

Known limit: the holder records the certificate that the peer sent in its
provision. Nothing proves that the peer holds that certificate's private key,
so a grant does not prove that the caller is the stream's TLS client. A
certificate is public: a peer that has another viewer's certificate can
provision it under its own grant and, while it takes part in the same
launch, resize that viewer's stream. It cannot stream with it: Sunshine's
TLS still requires the private key.

korrid's Portrait control uses it for a viewer that does not send its own
request (every viewer since the Artemis app went in D7′) and that reports
`ready`. This app's viewer reports `ready: true`: it
follows a new stream size since #476 slice 4, and its `encoded` size follows
each change (`clients/viewer/README.md`, "A new stream size in the middle of
the stream"). It reports a shape because its Start names a screen: the
window's size, longer side first (`rules::screen`), as the Artemis app
reported its screen (`KorriStreamViewer.screenStreamSize`); Portrait is a
stream control, not the way the phone is held. So korrid's live-shape runs
for this app as it did for the Artemis app: on a holder with display authority the
holder's display and the stream take the phone's shape. korrid's viewer
display at prepare is still none here, so a stream starts at Moonlight's
1280x720 and live-shape changes it after the start.

The proof (`proof/resize.py`, `nix run .#android-app-resize-proof --
--build-store auto --sunshine PACKAGE`, on aka as root) uses D10's holder,
sender and fixture APK, built as D10 builds them. The proof trigger's
`resize LAUNCH WIDTH HEIGHT` sends the same request to the holder, because
the federation fixture has no compositor: **korrid's Portrait toggle and the
holder's Sway mode change were not run live; unit tests cover them only.**
The fixture's holder refuses the viewer's shape reports, so korrid lists no
Portrait control there; the proof reads what enables it: korrid's own
snapshot of the viewer says `ready: true`, and its `encoded` size follows
each change in the same connection. Record: `proof/android-app-476/` (35 of
35 checks).

## The release APK (D12 prep)

What the app needs before it goes on the owner's Galaxy Z Fold 8 (ADR
0064). Build it on aka (the owner's rule), copy only the APK to zao, and sign
it there, where the key is. zao does not trust aka's store signatures
(`app-updater-1` is not in its `trusted-public-keys`), so the copy needs
`--no-check-sigs`, which a trusted user of zao's store may pass:

```sh
nix build --eval-store auto --store ssh-ng://root@aka --print-out-paths \
  .#korri-android-app-release-apk          # from a clean, committed checkout
nix copy --no-check-sigs --from ssh-ng://root@aka /nix/store/...-korri-android-app-release-apk-0.1.0
nix run .#android-app-release-sign -- \
  /nix/store/...-korri-android-app-release-apk-0.1.0/korri-android-app-release-unsigned.apk \
  ~/korri-android-app-release.apk
```

| | `korri-android-app-apk` | `korri-android-app-release-apk` |
| --- | --- | --- |
| ABIs | arm64-v8a and x86_64 | arm64-v8a only: phones. The emulator proofs keep the other two APKs |
| Debuggable | yes (`--debug-mode`) | no: the manifest has no `android:debuggable` at all |
| The proof trigger (`proof`) | refused (the library's fingerprint and bytes) | the same library, so refused |
| korrid's `fixture` feature | refused in every library of the app, as in the shipped korrid | refused |
| Notices | none | `assets/notices/NOTICES.txt` (below) |
| Signed | no; the proofs sign with the debug key | no; the signing step signs it outside the store (below) |

The build fails when: the checkout is not clean (the notices name the source
commit, `self.rev`); `aapt2 dump xmltree` shows `debuggable` in the manifest
or `aapt2 dump badging` shows `application-debuggable`; the APK's native
code is other than `arm64-v8a`; the notices are missing; a dex is present.
The library is the product library that `korri-android-app-apk` packs, with
its symbols, as there (crash reports on the bench phone).

The release APK and the debug APK have the same package and version code,
and different signers: Android refuses one over the other. Uninstall first,
which deletes the app's korrid state on that phone.

Not proven: the release APK on a device or an emulator (D12). Nothing in the
app reads whether it is debuggable (a reading of the source), so its
behaviour is inferred to be the debug APK's; only `run-as` stops working,
which the proofs use.

### Signing

The Artemis release key is not grounded enough to follow. The repository
pinned its certificate's SHA-256 (`clients/android/firmware/odin2portal/contract/korri-release-cert-SHA256.txt`,
at `android-final` and `artemis-final`) and called it "the selected Korri Android release
key", but records neither where the key lives nor a command that signs with
it. This app is also another package (owner decision O1). So the app has a
key of its own, which the build never sees: `sign-release.sh` signs the
built APK on the machine that holds the key.

The key's folder is the owner's choice (2026-11-16, #434 D12): the key on
zao, under `~/.config/korri/android-signing/`, with mode 0600. The file
names inside it are this slice's:

| Path on zao | What | Mode |
| --- | --- | --- |
| `~/.config/korri/android-signing/` | the key directory | 0700 |
| `korri-android-app-release.p12` | PKCS#12 keystore with one key, alias `korri-android-app`: RSA 4096, self-signed, 30 years, `CN=Korri Android App Release, O=Korri` | 0600 |
| `korri-android-app-release.password` | its password: 32 random bytes in Base64, one line | 0600 |
| `korri-android-app-release-cert.pem` | its certificate, public | 0600 |

The key was made on 2026-11-16. Its certificate's SHA-256 is
`e473449db4d9abd868a114e07b844b51c8398120ec27949eb394f77b95e48f91`. No
backup of it exists. Without it, an update needs an uninstall first.

The signing tools need Android's build tools, which are unfree. zao never
builds them: build both apps on aka and copy their closure to zao, in a
checkout on zao. `nix run` then finds the apps in zao's store and builds
nothing. The copy needs `--no-check-sigs` for the reason above:

```sh
for task in android-app-release-key android-app-release-sign; do
  drv="$(nix eval --raw ".#apps.x86_64-linux.$task.program" \
    --apply 'program: builtins.head (builtins.attrNames (builtins.getContext program))')"
  out="$(nix build --eval-store auto --store ssh-ng://root@aka \
    --print-out-paths --no-link "$drv^out")"
  nix copy --no-check-sigs --from ssh-ng://root@aka "$out"
  nix build --dry-run --no-link "$drv^out"   # prints nothing: nothing to build
done
```

| Step | What it does |
| --- | --- |
| `nix run .#android-app-release-key [-- --key-dir DIR]` | Refuses a key directory inside a Git work tree. Makes the three files with keytool, which reads the password from its file. It refuses to replace a file that exists |
| `nix run .#android-app-release-sign -- [--key-dir DIR] UNSIGNED SIGNED` | Refuses a key directory inside a Git work tree, and a key file or directory that is a link, belongs to another user or has other modes; refuses a debuggable APK and one without notices; refuses an output in the Nix store. Signs with apksigner, reading the password from its file: APK Signature Scheme v3 alone, which apksigner picks for `minSdkVersion` 31, and no v4 file. Checks one signer, whose certificate is the key's. Prints the certificate's and the APK's SHA-256 |

Checked on zao on 2026-11-16 with a release APK of this branch: `apksigner
verify --print-certs` says v3, one signer, `CN=Korri Android App Release,
O=Korri`, the certificate SHA-256 above; `aapt2 dump badging` shows no
`application-debuggable` and `native-code: 'arm64-v8a'`. The step refused a
debuggable APK, an output in the store, a password file of mode 0644, a key
directory of mode 0755, a password file that is a link, and a certificate
file of another key (a throwaway key in a temporary directory).

Both steps also refused a key directory inside this checkout (it did not
exist yet; the key step made nothing), the same with `GIT_DIR` set
elsewhere, and a key directory in which `git init` had been run.

The key, its password and the signed APK never enter the repository, the
store or a log. If the keystore or its password file is ever copied into a
checkout, the pre-commit hook refuses it: `.gitleaks.toml` has the path rule
`korri-android-release-key` for both names. gitleaks skips a staged file
that git calls binary, so `.gitattributes` gives `*.p12` the `diff`
attribute; then gitleaks's own `pkcs12-file` rule fires too. Checked with
the hook's gitleaks (8.30.1) in scratch repositories: a real keystore and
password file under those names, at the root and in subfolders, are
refused; without the attribute the keystore passed; `x…` and `….bak`
names and the certificate pass.

### Notices

`assets/notices/NOTICES.txt` is one UTF-8 text, about 1.0 MB. No screen
shows it yet: the shell has no view for it, which is design work.

| Part | From |
| --- | --- |
| The app's licence, GPL-3.0-only (#481, ADR 0059), and its source | the packing (`default.nix`, `sourceNotice`) |
| moonlight-common-c, ENet, its Reed-Solomon code; Valve's and SDL's controller lists in the viewer; OpenSSL; libopus | their sources: the viewer's parts (`clients/viewer/android.nix`) and the viewer's source |
| nightjar-core, rubylight-protocol, rubylight-traverse and rubylight-native-api, GPL-3.0-only (#391 step E), and the GPL-3.0 projects that nightjar-core ports code from | a part of their own, which names their projects, the ported projects and Korri's copies in `clients/viewer/vendor`; the build checks that both projects' licence files are still the GPL version 3. cargo-about also lists the three that are not `publish = false`, under GPL-3.0-only, and Nightjar's `rusty_enet` under MIT with its own licence |
| nanors (MIT, Joseph Calderon), whose Reed-Solomon code nightjar-core's `fec.rs` ports | nanors' `LICENSE` at commit 0b09d1ee, fetched with its hash (`clients/viewer/vendor/nanors-licence.nix`). `clients/viewer/vendor/check-licence-mentions.sh` reads every line with one of its words, in any case, of every vendored nightjar or Rubylight text file but their two GPL licence files, `nightjar/LICENSE.txt` and `rubylight-native-api/LICENSE`. The build fails on such a line that `licence-mentions.txt` beside it does not record, on a recorded line that its file no longer has, and on a recorded part or crate that is not here. Its words, in any case: "licenc", "licens", "copyright", "public domain" (also with "-", "_" or nothing between), "©" and "(c)" before a year, anywhere in the line; "GPL", "LGPL" and "AGPL" with no letter, digit or underscore just before; and, as whole words, also with a version glued on, MIT, BSD, 0BSD, Apache, ISC, Zlib, MPL, CC0, CC-BY, BSL, Boost, EUPL, WTFPL, OFL, EPL, CDDL, Artistic, Unicode-DFS, "Unicode-" and a digit, SSLeay and "(c)". The script's header says what it does not catch |
| The Android Open Source Project: the viewer's gesture detectors (`gestures.rs`, Apache-2.0) | `frameworks/base`'s NOTICE at `android-12.0.0_r34`, fetched: its notice for the Android code and the Apache License, Version 2.0 |
| curl, QuickJS, libsecp256k1, ring's BoringSSL code | the C that `curl-sys`, `rquickjs-sys`, `secp256k1-sys` and `ring` build, from the vendored crates |
| Skia, and the four libraries of its archive that the library keeps: libpng, zlib (Chromium's copy), FreeType, Expat | the archive's `LICENSE_SKIA`; nixpkgs' sources and Chromium's for the four |
| The Kotlin standard library | its project's licence, NOTICE and the Boost licence of `MathJVM.kt`, fetched at v2.0.21 |
| LLVM libc++ and compiler-rt | the NDK toolchain's NOTICE |
| The Rust standard library (core, alloc, std and the crates they are built from) | the toolchain's own `COPYRIGHT-library.html`, as text (`html-notice.py`) |
| The surface packages' fonts; Pico's `qrcode` and `dijkstrajs` | `pico-fonts.ts` (read with bun), the font licence files, the vendored licences |
| 643 Rust crates of the arm64 library (with nightjar-core, 2026-11-18) | `cargo about generate --fail` (`about.toml`, `about.hbs`) |
| The GNU GPL version 3, the licence of the app, moonlight-common-c, nightjar-core and the Rubylight crates | moonlight-common-c's `LICENSE.txt` |

Skia's archive also holds libjpeg-turbo, Wuffs and the Vulkan Memory
Allocator. The arm64 library keeps none of their code: `llvm-nm` finds no
`jpeg_` (only the Rust crate zune-jpeg), `wuffs_` or VMA symbol, and finds
libpng's, zlib's, FreeType's and Expat's. The same check found ring's,
libsecp256k1's, QuickJS's, libc++'s and libunwind's code. Copyright lines of
the four Skia libraries come from nixpkgs' versions, not from the revisions
that Skia pins. No lawyer has read the notices (ADR 0059).

The source is a link, not a written offer (GPL-3.0 section 6(d)): the notices
name the commit on `https://github.com/korri-os/korri` and the command that
builds the APK from it. That commit must be on the public repository before
anyone receives the APK. The link works only while `korri-os/korri` is
public: `gh repo view korri-os/korri --json visibility` said `PUBLIC` on
2026-11-16. If the repository becomes private, the link fails, and the APK
needs a written offer or the source beside it. The Linux viewer ships its corresponding source as
a tarball (`clients/viewer/default.nix`, `correspondingSource`); the Android
app's would hold korrid, the shell and Slint, so the APK names it instead.

### The owner binding on a real phone

The fixture binds the emulator's app to the test owner (secret 3,
`f9308a01…`), whose key korrid's offline tool retires. The owner's real
federation is owner `a80db13f…3dca`: on 2026-11-16 relay.primal.net held ten
owned statements signed by that key, zao's device key `7bb368b2…` among them
(signed 2026-09-06). The release APK's relays are the product's five
(`product.nix`); three of them held the owner's statements
that day (relay.primal.net 10, offchain.pub 9, nostr.oxtr.dev 9), so the
phone can read the owner's roster once it is bound.

What exists:

| Part | Where |
| --- | --- |
| korrid's half of a binding through a native person signer (NIP-55): the coordinator, which the app's korrid starts with; the RPCs `OwnerBindingSnapshot` and `OwnerBindingStart`; `update_local_owner_signer`, `take_local_owner_binding_request`, `apply_local_owner_binding`; the "Connect your identity" prompt | `services/korrid/src/owner_binding.rs`, `lib.rs`, `surface/owner_binding.rs`, `android/app.rs` |
| The check of a returned signature against the device's template and the expected owner, and its storage | `DeviceIdentity::apply_signed_owner_binding` |
| A NIP-46 person signer in Rust, and a first binding through any person signer | `remote_signer.rs` (`Nip46PersonSigner`), `identity::bind_automatic_owner` |
| Discovery once bound: the owner's roster on the product relays | `RelayCoordinator::read_owner_roster` |
| The relay messages `OwnerBindingRequested` and `OwnerBindingResponse` | `relay.rs`; nothing outside it and its tests produces or consumes them |

What is missing:

- A signer host. Nothing in this app attaches a signer to the coordinator
  or runs NIP-55, which needs an activity result; the Artemis app's
  `KorriOwnerBindingHost` was Java with no caller (tag `artemis-final`). "Set up owner" answers
  "Open Korri to choose a signer".
- An identity switch. It needs Linux's local signer
  (`KORRID_LOCAL_SIGNER_SOCKET`); the app's korrid has none, so it answers
  `can_switch_identity: false` and offers neither a backup nor a NIP-46
  bunker.
- D9's seeding. `run-as` works only in a debuggable APK, so the release APK
  cannot be given an owner statement that way.
- A peer that signs for the phone: no consumer of `OwnerBindingRequested`.
- Which signer holds the owner's private key: no record in the repository
  says.

So no route binds the release APK to the real owner today. The routes were:
a NIP-55 signer such as Amber on the phone, with a host for the existing
coordinator (a third-party result activity, or one Korri Java file on
#434's list); a NIP-46 bunker, with korrid's `Nip46PersonSigner` wired to
the first binding and a way to enter its address; a debuggable build seeded
as in D9 with a statement that the owner signs; or a peer that answers
`OwnerBindingRequested`. The owner chose the third for D12 (below).

### D12's debuggable route (owner decision, 2026-11-16)

D12 runs a debuggable build of the app, seeded as in D9, but bound to the
owner's real key `a80db13f…3dca` with a statement that the owner signs
(`d12-binding.sh`, `nix run .#android-app-d12-binding`).

The statement is korrid's own owner statement for the phone's new device
key: a Nostr event of kind 30078 with empty content and exactly three tags,
`["d", "org.korri.device-owner:DEVICE"]`, `["device", "DEVICE"]` and
`["status", "owned"]` (`identity.rs`: `owner_tags`, `parse_owner_statement`),
signed by `a80db13f…3dca`. `korrid identity owner-binding-request` makes the
device key and prints that template; `korrid identity import` checks a
signed statement and stores it as `identity/owner.event.json` beside
`identity/device.key`. The app reads both from
`<no_backup>/korrid-state/identity/`, where D9 seeds them.

Who can sign it today: only the owner's own signer. Checked on zao on
2026-11-16 (public files, read with sudo):

| Fact | Where |
| --- | --- |
| zao's local signer holds the person key `f77dae0a…`, not the owner's | `/run/korri-local-signer/public/person.pub` |
| zao's korrid has `nip46-client.key` (2026-10-26) but no `nip46.connection.json`: no NIP-46 signer is connected there | `/var/lib/korrid/identity/` |
| zao's owner statement: by `a80db13f…3dca`, created 2026-09-06 04:24:20 UTC, stored 2026-09-07 | `/var/lib/korrid/identity/owner.event.json` |

zao's statement came from the guarded offline test-owner replacement
(`korrid identity replace-test-owner-offline`, approved 2026-09-06,
`docs/research/korrid-identity-protocol.md`), whose template "the user's
signer" signed; the repository does not say which signer. The record of
2026-10-22 (ADR 0021 on the unlanded branch `docs/device-join-design`,
`b9d1c77a2`) says that the Odin 2 Portal and the RP Mini V2 joined the same
way: the person's phone signer, Amber, signed each template over NIP-46,
and the person key stayed in the phone. The RG35XX SP's repair note records
its NIP-46 owner `a80d…` and that its local signer does not hold that key
(`services/korrid/deploy/save-sync-migration/SP-ACCOUNT-REPAIR.md`). So the
owner's private key is in Amber on the owner's phone: inferred from those
records, which do not say which phone. The script asks that signer over
NIP-46 and never holds the key.

The steps, on zao in a checkout. Build `.#korri-android-app-apk` and the
app `android-app-d12-binding` on aka and copy them as the signing tools are
copied above (the loop, with `android-app-d12-binding` and
`android-app-d12-binding-check` as the tasks; their closure carries korrid
and `owner-sign`). Why korrid's own client signs, and not nak's: below.

```sh
nix run .#android-app-d12-binding -- sign-debug \
  /nix/store/...-korri-android-app-apk-0.1.0/korri-android-app-unsigned.apk ~/korri-android-app-debug.apk
# The owner makes a new NIP-46 bunker address (bunker://...) in Amber, on
# relays that keep NIP-46 events (relay.nip46.com, nrs.primal.net); its
# secret works once, so each make needs a new one. Put it in a 0600 file,
# or paste it when make asks.
nix run .#android-app-d12-binding -- make ~/d12-binding --bunker-file ~/d12-bunker.uri
# Approve in Amber the connection "Korri <first 8 characters of the phone's
# device key>" (sign_event:30078) within 300 s, then the signing request
# (kind 30078) within 120 s, unless the connection already allowed it. make
# prints the name and, after the connection, the key that answered.
# The owner then removes this connection in Amber: make's client key was
# made for this run only, and nothing uses the connection again.
adb devices -l                                            # the phone's adb serial
adb -s SERIAL uninstall com.simonwjackson.korri.app       # only if installed; deletes its state
adb -s SERIAL install --no-incremental ~/korri-android-app-debug.apk  # do not start the app yet
nix run .#android-app-d12-binding -- seed ~/d12-binding --serial SERIAL
# Start the app. Its korrid publishes its statement:
nak req -k 30078 -t d=org.korri.device-owner:DEVICE wss://relay.primal.net
```

| Step | What it does |
| --- | --- |
| `sign-debug` | Refuses an APK that is not debuggable (run-as needs one). Signs with Android's debug key, `~/.android/debug.keystore`, as the proofs do |
| `make WORK` | Resolves WORK first (`realpath -m`), so `nope/../keep`, `x/..` and `.` name the folders they are. Refuses a WORK that is a link, that exists, whose parent is missing, or that is inside a Git work tree (or that git cannot place, or when git is missing: `outside-git.sh`), and a bunker file that is a link or not 0600. Reads the bunker address before it makes anything, from the file or from the terminal (not shown), and gives it to `owner-sign` only on its standard input, from bash's builtin `printf`, never in a command line. Makes the device key with the repository's korrid, has the owner's signer sign the template over NIP-46 with korrid's own client (`owner-sign`), refuses a statement that is not the template or is by another key than `--owner` (default `a80db13f…3dca`), imports it with korrid and checks `Owned` by that owner. `owner-sign` checks the template and the owner before it connects, makes a client key new for the run in a temporary folder that `TMPDIR` puts inside WORK, and removes it with the connection record, which holds the secret. Its messages name the step and korrid's reason (refused, or no answer within 300 s), never the secret or the signer's text; they name public keys only, such as the one that answered `connect`. The owner has 300 s for the connection and 120 s for the signature. Only make's own `mkdir WORK`, which fails on an existing folder, makes WORK; if make fails after that, it removes that one resolved folder, device key and any client key included |
| `seed WORK --serial SERIAL` | Needs the phone's adb serial, and prints the phone's model, its own serial and its Android version before it writes. Refuses an app that is not installed, not debuggable or has a device key already (it has started once). Writes both files with run-as and `adb exec-in`, reads their sizes back, checks they are the app's own 0600 files, then deletes `WORK/state`: the phone's device key stays on the phone only |

`nix run .#android-app-d12-binding-check` (`d12-binding.test.sh`) runs
`make`'s cases with no phone: nak's own bunker, holding a throwaway owner
key, stands in for Amber, on a strfry relay (it keeps NIP-46 events, as
relay.nip46.com does) on a free loopback port, in a temporary folder. It
passed on zao on 2026-11-16 (17 cases, 6 s): `nope/../keep`, `x/..` and
`.` (run in a folder), an existing folder, a link to one (also with a
trailing slash), a link that points nowhere, a missing parent and an
absolute `nope/../keep` are refused and change nothing; two new folders
bind; korrid's client asked (`connect` with `sign_event:30078`, named
`Korri` and the first 8 characters of the phone's device key); make named
the key that answered `connect`; no client key, connection record or
secret is left in WORK; a
signer of another key and a refused connection remove only their own new
folder, the latter with "The signer refused connect" and no secret in the
output; three runs signed with three client keys. Against the nak version
of `make` (`origin/main` before this change), the same test fails 2 cases:
nak's `connect` carries only the key and the secret, and its refusal names
no step. Against `4812bb070`'s `make`, the earlier test failed 11 cases:
`nope/../keep` and `x/..` bound into the existing folders, which a failure
would then have removed.

nak's bunker refuses a wrong secret with `{"id", "error":
"unauthorized"}` and no `result`. korrid's client required `result`, so it
dropped that reply and waited 120 s; it now reads it as a refusal
(`remote_signer.rs`, `Nip46Response`). Amber's refusals already reached
korrid as refusals: its refused `get_public_key` stopped the SP's identity
switch on 2026-10-22 (`45cfafc38`), so they carry `result` (inferred).

Checked by hand on zao on 2026-11-16, with korrid's `owner-sign`, nak
0.20.6's bunker holding a throwaway owner key, and a local relay:

| Case | Result |
| --- | --- |
| `make`, on strfry | The template went over NIP-46, the signed statement matched it, korrid imported it and said `Owned`; the files are 0600 in 0700 folders |
| The bunker refuses the connection (a wrong secret) | "owner-sign: The signer refused connect. Use a fresh bunker URL."; WORK removed; the secret is nowhere in the output |
| nak's own client, for comparison | Its `connect` carries `[key, secret]` only; its requests are NIP-44, as korrid's |
| No relay on the address's port | "Remote signer connection failed: remote signer request failed" at once; WORK removed |
| A relay, but no signer behind the address | "The signer did not answer connect within 300 s" after 300 s; WORK removed |
| `nak serve`, which keeps no ephemeral event | The bunker answered `connect`, but korrid never saw the reply and gave up at its deadline (checked when that was 120 s). So korrid's client needs at least one relay in the address that keeps NIP-46 events |
| A signer of another key than `--owner` | Refused, nothing imported |
| Each failure after the device key was made (the three above, and a script error) | WORK removed, key included |
| An existing folder, a folder in this checkout, a bunker file of mode 0644, an address that is not `bunker://` | Refused; nothing made |
| `seed` without `--serial`; with a serial that a private adb server does not know | Refused before any write |
| `outside-git.sh`, through the release steps and `make` | Refused: git missing, a repository that git calls of dubious ownership (the earlier check read only `true` from git and would pass there, by its code), this checkout, the same with `GIT_DIR` elsewhere; the real key folder passes |
| `sign-debug` | Refused the release APK; signed the debug APK (`CN=Android Debug`, apksigner verifies it) |
| The closure copy | Built nothing on zao |

Not checked: `seed` (no phone; aka already ran another agent's emulator),
Amber answering korrid's client on the Fold 8, and anything on the phone.

Costs: a debuggable app lets `run-as` and a debugger read its state, the
phone's device key included, through USB or wireless debugging. The owner
statement on the public relays names the phone's device key, as for every
device. The debug key signs the app, so a later release install needs an
uninstall first, which deletes the phone's korrid state.

### Which NIP-46 client: korrid's own, not nak's (2026-11-16)

`make` first had nak 0.20.6 ask Amber. On the Fold 8 that never finished
(the owner's check, 2026-11-16): Amber 6.6.9 showed the connect dialog and,
after approval, logged "Connect … ack"; nak's requests (`-p <bunker>`)
reached relay.nip46.com, nrs.primal.net and bucket.coracle.social, but no
reply from Amber reached any of them (none `-a <bunker>`, none
`-p <client>`, watched live with `nak req -k 24133 --stream`); nak waited
five minutes. Amber's NIP-55 `nostrsigner:` intent, sent with
`adb shell am start`, showed no approval screen either. The test of that
version passed only against nak's own bunker.

The route that Amber did answer is korrid's own client,
`Nip46PersonSigner` (`services/korrid/src/remote_signer.rs`):

| What | korrid's client | Where |
| --- | --- | --- |
| Who makes the URI | The signer: a `bunker://` address made in Amber, pasted into korrid. korrid has no `nostrconnect://` | `remote_signer.rs`, `parse_bunker_uri`; `owner_sign.rs`, `sign` |
| Relays | Only the ones the `bunker://` address names | `remote_signer.rs`, `bunker_relay_list` (since `5dffbe0a3`, 2026-10-22) |
| Encryption | NIP-44 v2, both ways | `identity.rs`, `Nip46ConnectionIdentity::sign_encrypted_event`, `decrypt_event_from` |
| `connect` | The address's key, its secret, `sign_event:30078`, `{"name":"Korri <first 8 characters of the device key>"}`; no `get_public_key`, which Amber refuses unless granted | `remote_signer.rs`, `establish`, `connection_name` (`45cfafc38`, 2026-10-22) |
| Replies it takes | Kind 24133, `p`-tagged to its client key once, NIP-44 encrypted with the conversation key of the client and the reply's author, read back from the relays (`since` now − 300 s, every 50 ms). The reply to `connect` may come from any key that carries the request's random id, which only the `bunker://` key could read; that key then gets every later request, and its replies only count. The owner is the signed statement's author | `relay.rs`, `read_signer_packets`; `identity.rs`, `decrypt_event_from_any_author`; `remote_signer.rs`, `send_request`, `verify_exact_signed_event` |
| Client key | `nip46-client.key`, beside `nip46.connection.json` (which holds the secret) in its private state | `identity.rs`, `Nip46ConnectionIdentity` |
| Time | 300 s for `connect`, then 120 s for `sign_event` | `remote_signer.rs`, `CONNECT_TIMEOUT`, `REQUEST_TIMEOUT` |

Its record with Amber: the RG35XX SP's identity switch on 2026-10-22
(`45cfafc38` names Amber; `5dffbe0a3` and `4e34b8a9e` are fixes of the
same afternoon), and the LG TV on 2026-11-06
(`docs/briefs/2026-11-06-webos-milestones.md`, "Joining"), through the
workstation tool `webos-owner-sign`, whose README names the signer as Amber
6.6.9 (`clients/shell/webos/README.md` at `webos-final`, "Hidden
development-only workstation signer"). The webOS cut removed that tool
(`bed2ee4db`). ADR 0021 (`b9d1c77a2`) says the Odin 2 Portal and the RP
Mini V2 joined "by hand" through Amber over NIP-46 the same day as the SP,
but names no client. The tool is back as `owner-sign`
(`services/korrid/src/owner_sign.rs`, `src/bin/owner-sign.rs`), restored
from `webos-final`; korrid's package builds it apart
(`package.nix`, `ownerSign`), so it never ships to a device.

What Amber 6.6.9 does with a `bunker://` connection, read in its source
(tag `v6.6.9`, `app/src/main/java/com/greenart7c3/nostrsigner/`):

| Fact | Where |
| --- | --- |
| The address names a key of this connection only, not the account's key | `ui/NewNsecBunkerScreen.kt` 264, 282, 324 |
| It signs its replies with the key of the connection that answers: the `bunker://` key, or a new key after "Create new connection" (seen on the Fold 8, below) | `service/BunkerRequestUtils.kt` 184–213 |
| It sends them to the connection's relays, the ones in the address | `service/BunkerRequestUtils.kt` 554–562; `service/EventNotificationConsumer.kt` 384 |
| It replies in the request's encryption (NIP-04 or NIP-44) | `service/EventNotificationConsumer.kt` 209–213; `service/BunkerRequestUtils.kt` 195–201 |
| "Connect … ack" is its history entry, written before it sends the reply: the entry does not show that a reply left the phone. If the send fails, it shows an error and undoes the connection | `service/BunkerRequestUtils.kt` 299, 509, 554, 247–253 |

So Amber's replies go to the address's relays, from the key of the
connection that answers. The two clients' requests differ, as
seen at a local `nak bunker`: nak's `connect` carries `[key, secret]`
only, korrid's adds `sign_event:30078` and its name; both are
NIP-44 and `p`-tag the `bunker://` key. Why no reply to nak left the phone
is not known: Amber's source shows nothing that answers korrid and not
nak. The change moves `make` to the route with a record, korrid's client;
it does not prove that the cause was nak.

Then korrid's client failed on the Fold 8 too, and the owner found why
(2026-11-16): its `connect` was named `Korri`, Amber 6.6.9 already had
connections of that name, and it asked "Replace connection" or "Create new
connection". With "Create new connection" it made a new connection with a
new key and answered from that key, not from the `bunker://` key. korrid
took replies from the `bunker://` key only, so it waited 120 s and failed.
A run with the new connection's address (key `4f926d…`) got an answer at
once ("The signer refused connect": that secret was spent), so replies did
reach korrid when the key matched. korrid now names each connection after
the device it binds, takes the reply to `connect` from whichever key
answers with the request's id, and asks that key from then on; it waits
300 s for `connect`. NIP-46 does not allow another key in the bunker flow,
so this is for Amber: korrid finds the signer key as the `nostrconnect://`
flow does, from the reply author, with the random request id in place of
the secret. The tests in `remote_signer.rs` cover a signer that answers
from another key, and replies from another key that do not decrypt or
answer another request, which korrid ignores.

korrid's client reads replies back instead of staying subscribed, so it
needs a relay that keeps the ephemeral kind 24133 for a while. Checked on
2026-11-16 from zao with a throwaway key and an unencrypted kind 24133:
relay.nip46.com and nrs.primal.net returned it to a new request 2 s later;
bucket.coracle.social did not return it 85 s later; relay.nsec.app did not
connect.

## Next slices

| Slice | What |
| --- | --- |
| D9 | Done: the holder proof above, and the TLS verifier's classes |
| D10 | Done: the Rust viewer, driven by korrid, on the shell's loop (above) |
| D5b | Done: Korri's gameplay overlay through `Overlay` (above), proven on the emulator (`proof/android-app-d5b/`). "Open Korri" (Background) is not carried yet; the camera cutout came with #494 |
| D11 | Done: 7 of Artemis's 13 stream controls (Toggle keyboard, Crop to fill, Pan & zoom, Mouse mode, Toggle local cursor, Flip A/B and X/Y, Rumble), the battery and the health, through the treaty (above); shown and applied in the gameplay overlay since D5b. The other 6 are decided in ADR 0087: Full keyboard and Keyboard as controller dropped, Picture-in-picture skipped, Touch sensitivity and SGSR in the backlog |
| D7′ | Done 2026-11-18: the Artemis app went, by the owner's decision, before D12 and D13 (ADR 0087, `docs/briefs/2026-11-18-artemis-removal-scope.md`). Until this app reaches a phone (D12), no Android app streams for a person |
| D12 prep | Done in part: the release APK, its key and signing step, its notices (below). Not done: an owner binding that a release build can make (below) |
| D13 (new) | Local Android games (O3): korrid's launcher through JNI, usage polling on a Rust thread, androidx `FileProvider` bundled unchanged (`hasCode="true"`), and the Guide service, which Android allows only as a Java `AccessibilityService` subclass: one Korri Java file on #434's list, or no Guide over other apps. It gated D7′ until ADR 0087 |
| D12 | On the Galaxy Z Fold 8 |
