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
- `nix run .#android-apk` builds korrid, bundles the current portal and builds
  `clients/android/app/build/outputs/apk/debug/app-arm64-v8a-debug.apk`.

The Nix apps supply the SDK, NDK, host compiler, Java and Rust toolchains. Cargo
uses the repository's pinned ProseQL source. The build targets ARM64 and Android
API 22 or later. The minimum follows iroh's Android TLS verifier. Its JVM
component version comes from `services/korrid/Cargo.lock`; do not update it
separately. The Rust library uses 16 KiB load-segment alignment.

The SDK farm, native libraries and portal bundle are generated files. Do not
commit them. Builds run on the development machine, never on the phone.

## Shell boundary

The restored WebView origin and navigation policy stay in
`KorriTrustedPortalWebViewPolicy`. The only portal binding is the current
`contracts/bridge/korri-rpc-bridge.ts`: `korridPort()` and `korridCapability()`.
`KorriRpcBinding` installs it at document start, in the trusted top frame only.
It is frozen and non-enumerable. The host reports ready only after both methods
are consumed. It fails startup after 15 seconds without that signal.

The port and bearer capability live only in memory. Neither enters a page URL,
static assets, browser storage or logs. A current Android System WebView with
document-start scripts and origin-scoped web messages is required. The host does
not fall back to a JavaScript interface exposed to every frame.

The stream client uses Android's existing app-files directory for korrid's
readable storage and the restored `no_backup/korrid-state` private root. It does
not request all-files or accessibility permissions. The old shared-storage
library, local RetroArch launches and cross-app overlay service are outside this
slice; their source remains at `android-final` for #293.

## Identity and discovery

A compatible NIP-55 signer such as Amber holds the person key. The native shell
consumes owner-signing requests from authenticated local RPC and returns public
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

Initial owner setup uses the surface's existing confirmation/actions and the
existing onboarding-finish RPC. This stream-only build does not offer the
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

## Device check

`KorriRpcBindingDeviceTest` exercises the real WebView, JNI library and RPC
server. It checks the frozen binding, child-frame exclusion, rejection without
a capability, successful authenticated health, and absence of the capability
from browser storage and the URL. Compile the test APK with
`nix run .#android-device-test-apk`.
The check still needs a real ARM64 phone. JVM tests alone do not prove this
boundary.
