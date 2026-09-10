# `@korri:retroarch`

This plugin owns the RetroArch launcher across platforms:

- `plugin.ts` declares the Linux kind and its default launcher instance. Its
  `launch` callback owns configuration and argv; `plugin.nix` owns the patched
  prebuilt executable and InputPlumber autoconfig payload.
- `android/plugin.ts` retains the Android record and session controls. The
  adjacent package obtains, patches, builds, verifies, and installs the signed
  `com.korri.retroarch` APK.
- Linux consumes only administrator-approved installed selections. Korrid no
  longer bundles a Linux emulator or receives plugin executable environment keys.

Libretro cores are independent plugins. A library route selects this launcher
and a compatible runtime such as `@korri:mgba/mgba`; korrid composes them using
the runtime's explicit launcher and system compatibility declarations.

The APK temporarily carries the independently built Android mGBA `.so` so it
can install into RetroArch's private executable core directory. On Linux, Nix
supplies mGBA through its own `plugin.nix`, with an exact required RetroArch
plugin output. Neither packaging bridge makes
mGBA part of the RetroArch plugin.

Android session control uses a launch-derived high loopback UDP port and
nonce-bound HMAC-SHA256 frames. RetroArch rejects duplicate request nonces with
a fixed 32-entry ring reset for each launch authority. Korrid computes the
ROM's full-byte CRC32 while preparing the signed launch and requires both that
checksum and the normalized content basename in authenticated `GET_STATUS`
before resume or control materialization. While that active Android record
remains, status failures, different content, and different routes return an
`ActiveSessionConflict`; only positive process-end evidence clears authority
and permits a fresh start. SHA-based library identity remains a separate catalog
fact. MAC-covered status also reports native menu liveness and selection for
acceptance without making screenshots a pass criterion.

The launch token never crosses UDP, JavaScript, or logs. Android's
cross-process Intent handoff does unavoidably materialize it as a transient
Java String until RetroArch copies and wipes its native bootstrap storage.
Repository checks prove the patch/config/build contract; installed-device
behavior remains a separate acceptance gate.

## Linux typed settings

The nested schema in `policy.ts` is the unchanged legacy RetroArch policy
(`legacy:product/plugins/retroarch/src/policy.ts`, Effect `4.0.0-beta.78`).
`render-settings.ts` extracts only the pure cfg-pair renderer from legacy
`launch-spec.ts`; it retains nested names, enums, defaults and null omission.
It does not port argv, environment or persistence. The callback consumes
**rendered scalar pairs**, not a new flat user-settings schema. Supplying and
persisting nested policy through the host remains an unresolved integration
boundary; do not encode it in the scalar `overrides.settings` map.

`settings-types.json` is the kind-owned table of all 166 fixed renderer output
keys and their scalar types. The schema-driven test checks it against the
actual renderer. `settings-check.nix` accepts an instance's own `program`;
it inherits that program's unpack/patch phases and checks its
`configuration.c`. It derives the executable path as `${program}/bin/retroarch`,
like the existing manifest file producer. It does not compile plugin code. The installed declaration
remains TS source evaluated by korrid at runtime.

`plugin.nix` registers the generated evidence under the existing
`packages.retroarch-settings` and `files.retroarch-settings`. The generic
consumer derives the file key from the launcher's existing `program` file key.
Artifact fields have existing consumers: `program` is the exact callback
executable path; `version` and `keys` are `SourceCheckedSettings` evidence.
The consumer checks that executable before binding the enclosing installed
plugin package as `build`. It never reads evidence from the kind package for
another instance. No second catalog, manifest extension, `since` table, user
schema or runtime migration exists.

For the pinned 1.22.2 source, 155 keys match. Eleven are withheld:
`config_save_on_exit` and `menu_driver` are reserved; `libretro_log_level` is a
legacy String but a source Number; `content_directory`,
`core_updater_buildbot_url`, `input_overlay_scale`, `menu_show_start_screen`,
`preemptive_frames`, `rewind_auto_stride`, `video_hdr_contrast`, and
`video_shader` have no verified key. Dynamic input-port outputs also have no
evidence yet. These remain legacy policy fields; they are not silently renamed,
coerced or admitted. Requests produce warnings with setting, launcher, version
and exact build. Source recognition does not prove conditional feature support.

The callback rejects reserved keys in both typed and raw input. Typed pairs
follow Korri's baseline. Raw prepend follows typed pairs; raw append comes
last. Quoted strings use the legacy JSON escaping; booleans are quoted and
numbers are bare.

Focused build-machine verification (no VM):

```sh
# Also called by the existing korrid-check CI task.
KORRI_ROOT="$PWD" nix develop .#korrid --command bash plugins/retroarch/check.sh
package="$(nix build --no-link --print-out-paths .#korri-plugin-retroarch)"
KORRI_ROOT="$PWD" KORRI_TEST_RETROARCH_PACKAGE="$package" \
  nix develop .#korrid --command cargo test \
  --manifest-path services/korrid/Cargo.toml --test typed_settings -- --include-ignored
```

The Nix check is `checks.<system>.korri-retroarch-settings`. Building it runs
the parser tests and checks the pinned patched source. Building the plugin
also requires this evidence. The packaged callback test reads the real
manifest, evidence and shipped TS, then checks its emitted configuration bytes.
It does not boot a VM or run a game. An uncached native program dependency can
make this build-machine gate expensive; it must never run on a target device.

## Distribution builds

`.github/workflows/retroarch-distribution.yml` builds and stages the custom
arm64 APK with `nix run .#ra-dist`. Relevant pull requests build an unsigned
candidate without secrets. Relevant `main` changes and manual `main` runs then
sign in a protected, checkout-free job and update a rolling prerelease named
from the APK's upstream version, such as `retroarch-v1.22.2-korri`. The rolling
tag moves forward as Korri patches change. Immutable release tags use the same
upstream-aware prefix plus a revision, such as
`retroarch-v1.22.2-korri.1`.

Configure these repository secrets before running the workflow:

- `RETROARCH_RELEASE_KEYSTORE_BASE64`
- `RETROARCH_RELEASE_STORE_PASSWORD`
- `RETROARCH_RELEASE_KEY_ALIAS`
- `RETROARCH_RELEASE_KEY_PASSWORD`
- `RETROARCH_RELEASE_CERT_SHA256`

The certificate fingerprint and single-signer count are checked after the
isolated signing task, preventing a generated debug key, unexpected release
key, or additional signer from producing a distribution. Signing remains
outside the Nix store; Nix owns compilation, validation, and candidate staging.
Release secrets are materialized only in the checkout-free signing job after
compilation and an independent package, Activity, ABI, and bundled-core check.

Restrict the `retroarch-release` GitHub Environment to `main` and
`retroarch-v*` tags, protect `main`, and restrict creation of release tags.
Automatic rolling publication intentionally has no required-review pause. The
workflow removes the keystore before invoking artifact-upload code.
