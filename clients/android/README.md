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

## Native shell (M2, #356)

`KorriShellActivity` is a `NativeActivity`. It starts the service-owned embedded
korrid before `clients/shell`'s `android_main`. The `android` feature uses the
pinned Slint interpreter, QuickJS logic worker and shared `Shell` runtime with
Slint's Android activity backend and Skia. Artemis's `Game` is unchanged.

The APK bundles `clients/shell/packages/pico` and `zone` without Android-specific edits.
The build runs Linux's surface verifier first. Java unpacks the bundled files
to app cache on launch; no user or identity record is migrated. The shell loads
both packages with the same `app.surface.declare`, preferences, model, request
and watch wire as Linux. Embedded korrid's authenticated root WebSocket shares
its surface hub with HTTP RPC. The streaming-only empty local plugin registry
and signed peer catalog producer are unchanged.

JNI carries port, capability, the existing allowed origin and the extracted
package path in-process. No capability enters an intent, URL, log or asset.
Origin/CORS removal remains M5. Android key and joystick events feed the same
Linux mapper, rather than subscribing to the Linux native input socket.

Minimal touch browsing maps a single-finger drag to momentary directional
presses, opposite the finger's movement. Each eighth of the dragged window
axis moves the cursor once. The first direction locks the axis until release;
multitouch cancels. Pico owns the focus graph and scrolls its cursor into view.
This is cursor-driven scrolling, not pixel/inertial scrolling or hit-testing.
Taps do not hit-test or activate a control; use the controller or remote to confirm.

The window is immersive and edge to edge in both orientations, with cutout
mode ALWAYS on API 30+, SHORT_EDGES on API 28–29. It reapplies system-bar handling
on resume and focus. No surface treaty inset or Pico workaround is added.
Content under a camera hole must be reported in device screenshots.

Android owns resize, pause/resume and native-thread teardown. Each invocation
owns its Shell, watch and logic workers, and drops them when the event loop
ends. The vendored backend still suspends its renderer on native-window
termination and Activity destruction. The foreground service retains korrid.
M2 neither changes ownership nor invokes Amber.

### Controller and remote input

| Android input | Semantic action (same as Linux) |
| --- | --- |
| D-pad keys, HAT_X/Y, left stick X/Y | Up / Down / Left / Right |
| A, DPAD_CENTER, ENTER | Confirm |
| B, SELECT, Android BACK | Back |
| X | Options |
| START, MENU | Menu |
| L1 + R1 on one controller | Switch Pico ↔ Zone |
| Y, MODE | No surface action (Linux reserves MODE for the host) |

Android's BUTTON_A means Confirm and BUTTON_B means Back; the mapping does not
infer a Nintendo layout from a device name. The mapper gives D-pad/hat priority,
uses the dominant stick axis above 0.6 (normalized range), and repeats directions
after 400 ms, then every 100 ms. Android's own key repeats do not double-repeat.
Focus loss clears held keys, stick state and touch drags. Android device-removal
callbacks are drained every 100 ms to release a disconnected controller's held state.

System Back opts out of predictive Activity navigation so its legacy key reaches
the native input filter. It sends the surface Back, including closing its own
menus and dialogs. At root, Back keeps the app open, exactly as the unchanged
surface decides on Linux. The shell does not inspect focus or guess whether a
surface is at root. Use Home or Recents to background the app; Back never
finishes the Activity or stops embedded korrid. The cost is that Back at root
does not perform Android's usual task-background action.

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
and the existing onboarding-finish RPC. This build has no touch activation for these controls. This stream-only build does not offer the
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

`KorriShellDeviceTest` waits for an actual rendered Pico view with a non-empty
real library, after launch, background/return, orientation changes and three
consecutive recreations. It injects real touchscreen drags and checks that
Pico's cursor moves. It injects D-pad/gamepad/remote keys, confirms cards,
closes layers with Back, and switches Pico ↔ Zone. It checks Owned identity, no WebView, health authentication,
cutout mode and hidden system bars. This device acceptance requires an already
owned phone with a reachable peer library; it is not a fresh-install fixture.
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
