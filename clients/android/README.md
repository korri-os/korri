# Android client

This is the trimmed Artemis client from `android-final`, not a new app or a new
upstream import. The original upstream was `simonwjackson/artemis` at `cf12c432`;
its native Moonlight library was `ClassicOldSong/moonlight-common-c` at `c9994368`.
See `LICENSE.txt` and the revival record in
`docs/briefs/2026-09-16-android-removal-scope.md`.

Issue #292 restores stream play in two slices. The first reconnects this shell
to the current embedded korrid and portal. The second connects Artemis's Game
activity to korrid's exact-launch stream-viewer contract. The old Java-driven
launch authorization and publication path does not return.

## Build

Run these commands from the repository root:

- `nix run .#android-brain-check` checks the Rust library for ARM64 Android.
- `nix run .#android-jvm-check` runs the kept JVM/Robolectric tests.
- `nix run .#android-shell-check` checks the native Slint shell for ARM64.
- `nix run .#android-apk` builds korrid, the native Slint shell and builds
  `clients/android/app/build/outputs/apk/debug/app-arm64-v8a-debug.apk`.

The Nix apps supply the SDK, NDK, host compiler, Java and Rust toolchains. Cargo
uses the repository's pinned ProseQL source. The build targets ARM64 and Android
API 22 or later. The minimum follows iroh's Android TLS verifier. Its JVM
component version comes from `services/korrid/Cargo.lock`; do not update it
separately. The Rust library uses 16 KiB load-segment alignment.

The SDK farm and native libraries are generated files. Do not
commit them. Builds run on the development machine, never on the phone.

## Native shell (M0, #356)

`KorriShellActivity` is an Android `NativeActivity`. It starts the existing
embedded korrid service before Android starts `clients/shell`'s `android_main`.
`libkorri_shell.so` uses the new `android` feature, Slint 1.18.1's official
`backend-android-activity-06`, and its Skia renderer. Desktop and webOS keep
their existing backends. Artemis's `Game` activity is unchanged.

The launch screen interprets `clients/shell/src/backend/android-m0.slint` with
the vendored `slint-interpreter` and evaluates `21 * 2` with rquickjs 0.14.
Android enables rquickjs's `bindgen` feature, as korrid already does, because
rquickjs does not ship an `aarch64-linux-android` binding. Startup readiness
requires the interpreted view and the QuickJS evaluation to succeed. This is an execution probe, not
yet Pico/Zone loading or the full surface engine. M1 can replace this view
inside the same library, dependency pins and Android event loop.

The screen shows BuildConfig's app version and authenticated `system.health`
status from the embedded korrid. JNI reads the port and capability directly
from the activity in-process. Only a boolean health result reaches the UI;
RPC runs on a worker with the shell's existing timeouts. No capability enters
an intent, URL, argument, log, asset, or new file. The existing origin/CORS
policy and korrid JNI start signature are unchanged until M5.

Android owns window resize, pause/resume and native-thread teardown. The
foreground service owns korrid, so activity recreation and backgrounding do
not stop it. M0 has no owner-signing or game-launch controls. It does not
change existing private state or Amber ownership. Native signer integration
and the real surface packages remain later milestones.

The diagnostic uses a quiet navy (`#182b40`) field, white (`#f4f8fc`) status,
and pale blue (`#c3d4e6`) version text. Android's sans-serif carries a large
Korri title and two left-aligned readouts. Padding and gaps yield to the
container; status wraps rather than disappearing. Loading, running and failed
health use the same layout. There are no lists, counts or user-entered values.
The version comes from the existing fixed BuildConfig version. Folded,
unfolded, rotated and background/resumed states need device acceptance; this
screen does not claim the full surface layout proof.

## Identity and discovery

A compatible NIP-55 signer such as Amber holds the person key. The retained native
signer adapter consumes owner-signing requests from authenticated local RPC and returns public
signer status. Rust creates the unsigned request, verifies the exact signed
statement, and updates the existing federation credentials. No new JavaScript
bridge or Java-owned launch authority is involved.

The public snapshot fields and signer states come from the retained
`KorriOwnerBindingController.snapshotJson()` and `PersonSigner.State` producers.
The old web signer URI is not part of this RPC: only the native adapter launches
the signer. Activity generations prevent an old activity from consuming requests
or detaching its replacement. The existing NIP-55 preferences retain an in-flight
request across recreation.

`product.nix` derives public relays from
`nix/product/requirements.nix`, the same policy as the Linux product. The build
passes that generated input through `BuildConfig` into the embedded brain. It
neither copies a second relay list into Java nor rewrites existing user data.

The retained owner setup path uses the surface's existing confirmation/actions
and the existing onboarding-finish RPC. M0 does not expose these controls. This stream-only build does not offer the
console's automatic person key or local-plugin starter pack. Identity backup and
identity switching are not available on Android; the public status reports that
limit instead of exposing controls which cannot work. Device ownership and peer
membership are re-read after binding without rotating the browser capability.

The embedded stream client selects an explicitly empty local plugin registry.
It does not read Linux's installed-package record or weaken Linux plugin checks.
Its runtime starts the existing signed peer RPC and reach transport, retains
them with the localhost router, and stops them with the brain.

`KorridNetwork` reads Android's `ConnectivityManager` and battery broadcast for
the existing Rust `InterfaceAddress` and `RelayFacts` consumers. Rust captures
the application class and context during JNI startup; worker threads attach to
the VM when they need a fresh observation. Linux keeps its route and power
readers. No observation is persisted. QUIC uses physical links; unknown power
or metering denies relaying. No public traffic relay or STUN default is added.

As ADR 0035 requires, a signed peer catalog read returns this device's local
games, not another federation fan-out. The local portal still reads its peers'
games. The peer projection reuses the local game producer and existing source,
runner and identity fields; non-owner callers do not receive owner play stats.

## Device check

`KorriShellDeviceTest` exercises the shipped NativeActivity, JNI library,
interpreted Slint view, QuickJS probe and HTTP server. It checks no WebView
exists in the launch tree, unauthorized health is rejected, authenticated
health succeeds, and startup succeeds after backgrounding and three consecutive recreations.
The vendored Android backend releases its EGL surface on native-window
termination and Activity destruction; see `clients/shell/vendor/README.md`.
Build with `nix run .#android-device-test-apk`, then run on an ARM64 phone:

```sh
adb -s SERIAL install -r clients/android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk
adb -s SERIAL shell am instrument -w -e class com.limelight.KorriShellDeviceTest \
  com.simonwjackson.korri.debug.test/androidx.test.runner.AndroidJUnitRunner
```

Before updating the app, take a protected backup of its private `files`,
`shared_prefs`, `databases` and `no_backup` directories plus its installed APK.
Use `install -r`; never uninstall, clear app data, or replace Amber ownership.
JVM/source tests alone do not prove native rendering or lifecycle behavior.
