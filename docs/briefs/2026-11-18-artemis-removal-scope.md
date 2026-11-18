---
date: 2026-11-18
topic: artemis-removal
artifact: scope
status: done; owner decision 2026-11-18 (ADR 0087, issues #434 and #391)
---

# Dropping the Artemis app: what went, what stayed

The owner removed the Artemis app on 2026-11-18, before the Korri app
(`clients/android-app`) reached a phone (ADR 0087). This brief records the
cut, as `2026-09-16-android-removal-scope.md` did for Android and
`2026-11-15-webos-removal-scope.md` did for webOS.

The tag `artemis-final` marks the last commit on `main` that carried the
Artemis app: `d8f862123`. Restore any part from that tag; do not bring it
back from memory.

## Whole-tree deletions

| Path | Files | What it was |
| --- | --- | --- |
| `clients/android/` | 732 (39 MB) | The Artemis app: the Gradle build and wrapper, 156 Java files of the app (151 of them in `com.limelight`), 179 C and C++ files of its JNI (`jni/moonlight-core`, with Korri's `SunshineRuntimeSettings.c`), 80 test files (JVM, Robolectric, device, one native), the signer test app, the build script, the overlay, bridge and federation acceptance scripts, `tasks.nix`, `devshell.nix`, the live-resize proof (`proof/live_resize.py`), its evidence for #376, #377 and #476, and the Odin 2 Portal launcher tools (`firmware/odin2portal/`: repack, rollback, AVB dry runs, the release certificate's SHA-256) |
| `nix/android-sdk-env.sh` | 1 | The writable SDK farm that Gradle needed (`.android-sdk/`) |
| `services/korrid/src/android/viewer.rs` | 1 | korrid's adapter over the Java viewer (`KorriStreamViewer`), with `KorridServer.viewerDisplayChanged` |
| `services/korrid/src/android/network.rs` | 1 | korrid's reader of `KorridNetwork.java`'s JSON |
| `services/korrid/src/android/app_launcher.rs` | 1 | korrid's adapter over `KorriAppLauncher.java` (local Android games) |
| `clients/shell/src/backend/android/activity.rs` | 1 | The shell's `android_main` for `KorriShellActivity` (Java) |

## Moved, because the Korri app uses them

| From | To | Change |
| --- | --- | --- |
| `clients/android/sdk.nix` | `nix/android-sdk.nix` | Only what the Korri app and the viewer's Android parts use: build tools 35.0.0, platform 36, NDK 27.0.12077973. Gone: Gradle's options, build tools 34.0.0, platform 34, the SDK's CMake and the bridge emulator profile. The Android derivations build again once, with a new SDK path |
| `clients/android/product.nix` | `clients/android-app/product.nix` | None: the product's relays for the release APK |
| `clients/android/test/nostr-relay-fixture.ts` and its test | `clients/android-app/proof/` | The holder proof's relay and `hostEnvironment` stay. `startAcceptance`, its entry point and its test went: only the Artemis federation acceptance ran them |
| `clients/android/test/federation-systemd-fixture.sh` and its test | `clients/android-app/proof/` | Paths only. `hostEnvironment` names this helper for the holder |

## Edits in files that stay

- `flake.nix`: `devShells.android` (the Artemis Gradle shell) is gone.
- `nix/tasks.nix`: `android-apk`, `android-jvm-check`, `android-brain-check`,
  `android-shell-check`, `android-device-test-apk` (from the deleted
  `clients/android/tasks.nix`) and `android-live-resize-proof` are gone.
- `.gitignore`: `.android-sdk/` is gone.
- korrid (`services/korrid`):
  - `android.rs` loses all 17 `KorridServer` JNI exports (`version`,
    `start`, `capability`, the owner-signer calls, `moonlightHostCandidates`,
    `issueFolderSelectionReceipt`, `provisionMoonlightCertificate`,
    `leaveViewer`, `stop`, `reportInstalledApps`, `appLeft`,
    `viewerDisplayChanged`). It keeps `provision_locally`, `leave_locally`
    and the proof's `resize_locally`, which the Korri app's korrid uses.
  - `lib.rs` loses the entry points that only those exports called:
    `korrid_version`, `moonlight_host_candidates`,
    `issue_folder_selection_receipt`, and `start_local_server` with its
    supplied origin. The embedded tests start korrid through
    `start_local_shell_server`, as the Korri app does, and send its
    `http://127.0.0.1:<port>` origin. The lifecycle test reads the peer's
    game through `app.catalog.snapshot`.
  - `LinuxStreamViewer::resizes_encoder` and its branch in the live shape
    are gone. Only the Artemis app changed the encode size itself; korrid
    asks the holder for every viewer now (#476 Route C). The test that ran
    the Artemis branch runs Route C: the holder applies the new size.
  - `android_network::Snapshot` no longer reads the Java producer's JSON.
  - `Cargo.toml`: the crate type `cdylib` (Artemis's `libkorrid.so`) is gone.
- The shell (`clients/shell`): the feature `android`, the crate type
  `cdylib` (Artemis's `libkorri_shell.so`) and `activity.rs` are gone. The
  Korri app uses `android-host`, which stays.
- `AGENTS.md`, `clients/android-app/README.md`, `clients/viewer/README.md`
  and the route brief's head no longer describe the Artemis app as current.
  Comments that named a file in `clients/android` now name the tag.

## What stays on purpose

| Part | Why |
| --- | --- |
| korrid's `android_app` (`android/app.rs`), `framework_network.rs`, `android_network.rs`, `android_storage.rs`, `leave_locally`, `provision_locally` | The Korri app runs on them |
| The viewer treaty (`contracts/viewer`), with its `Resize` | The treaty is shared. korrid no longer calls a viewer's `resize`, so `LinuxStreamViewer::resize` and `Resize` have no caller |
| ADR 0054's korrid side: `android_local`, `use_android_local_sessions`, `discovery::provider::report_installed_apps` | Local Android games return in D13 through korrid's launcher over JNI. Nothing calls them until then |
| korrid's owner-signer calls (`update_local_owner_signer`, `take_local_owner_binding_request`, `detach_local_owner_signer`, `apply_local_owner_binding`) | korrid's half of a NIP-55 binding, for a signer host in the Korri app. Nothing calls them today |
| `clients/android-save-bridge` | Its provider goes into game APKs (ADR 0054, case 2). Its `KorriSaveBridgeClient` was built into the Artemis app and has no host until D13 |
| The viewer's own APK (`korri-viewer-android-apk`) and its probe APKs | The route brief's D7′ listed it, but it is the Rust viewer's, not Artemis's, and the viewer's Android proofs use it |
| Historical ADRs, briefs, research, work items and proof records that name Artemis or `clients/android` | History |
| "Ported from Korri's copy of Artemis" in the viewer's sources, and Artemis in the notices | Provenance and licence. The sources now name the tag |
| `Limelight.h`, `LiStartConnection` and similar names | moonlight-common-c's own API, not Artemis |

## Owner decisions on the missing controls

ADR 0087 records them: Full keyboard and Keyboard as controller (the
on-screen controller) are dropped, to be rebuilt later, product first;
Picture-in-picture is skipped; SGSR, as a list of upscalers, and Touch
sensitivity go to the backlog.

## Not done here

- Nothing was deployed to a device. The Korri app has still run only on
  aka's emulator.
- The Korri app's package (`com.simonwjackson.korri.app`), its label
  "Korri (native)", its answer to the HOME intent (none) and its lack of a
  foreground service are unchanged. Each was decided while the Artemis app
  was beside it (owner decisions O1 and O2).
- Issues #434 and #391, and the issues of the backlog items above, are not
  updated by this change.
