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
