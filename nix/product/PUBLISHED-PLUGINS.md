# Catalog-pinned Linux images

ADR 0043 and approved schema-sheet row 13 define the pin in
`published-plugins.nix`: `{ catalogUrl, catalogSha256 }`. No per-path pin list,
raw-cache fallback, receipt migration, or outside repository is in this lane.

## Published input

- Plugins source: `1b865cbd546161464fb019ed3dc260c87102434a`.
- Immutable tag: `build-1b865cbd5461-no-moonlight`.
- Publication dry run: `45481298144`, all four jobs passed.
- Live publication: `45505023459` passed; Pages deployment `45508255945`
  passed on attempt 2. The first two publication attempts hit GitHub HTTP 504
  responses. Normal retries verified and retained their existing draft assets.
- Catalog: <https://github.com/korri-os/plugins/releases/download/build-1b865cbd5461-no-moonlight/catalog.json>.
- Catalog SHA-256: `f18201cc234adb2a0c59b0120ff64bad999b104bce2b5f0d0dd8add7f6c2f7a0`.
- SRI: `sha256-8YIBzCNK2yoMWbASD/ZLrZmbEEvOK18NDdit1/bC96A=`.
- 34 plugin IDs, 64 records; 34 x86_64 and 33 aarch64 selections.
  Homebrew Hub, PICO-8 and RetroBrews use `any`.
- Bound key remains
  `korri-plugins-1:qlK5Mgb3dYhF76WC4jGhrvL+CHsU93De7GpBFtrXb98=`.

ADR 0099 removes `@korri:moonlight` from this catalog. Every other plugin ID
from `build-e3343461a76e-engine-default-2` remains. `@korri:viewer` is built
against Core `a27386cb0d2efcd7bf2e20dc909f30b19ae0fee0`; its viewer treaty is
`834af1f4a4f1f2beecf6a75bbe2d1ef69933a5c34d62ccca7b1cb1014c9c9cc9`.
The Rust viewer is x86_64 only in this release. ARM image defaults no longer
select a viewer; the accepted streaming loss is recorded in the removal brief.

The publisher output is now `korri-plugin-tailscale`, matching Core's catalog
selection. Its plugin ID, native identity and store paths do not change. The
old output name has no alias. This repairs the mismatch exposed by the newer
Core pin, without a selection workaround.

These identify the actual live bytes. Publication verifies the dry-run artifact
digests and requires its catalog byte for byte before uploading those artifacts.
The Pages catalog is a moving discovery address, not an image pin.

## Build and recovery path

Run on a Linux **build host**, not a device:

```sh
bash .github/workflows/fetch-image-plugins.sh rpminiv2
nix build .#rpminiv2-sd-image
```

The other product targets are `rg353m`, `rgds`, `r36tmax`, `odin2portal`,
`rg35xxsp` and `rg35xxpro`. `--inputd` prepares the native Sunshine record;
`--all` prepares both architectures for combined acceptance.

1. `catalog-plugins.nix` fetches the exact catalog by hash. The existing
   `plugin-selection.nix` still chooses each device's plugins. A matching
   native record or its `any` record comes from the catalog's latest mark.
2. Preparation fetches selected archives by `archive_sha256`. The build-host
   `image prepare` command reuses the repository archive extractor, signature
   checker, slim importer and native Nix. It never installs receipts, runs
   plugin programs, activates services, or builds plugin software.
3. Signed native archives omit cache.nixos.org paths. The existing slim
   importer checks per-path authority and asks Nix to download these upstream
   paths with upstream keys only. It imports archive paths with the bound
   publisher key only. Source-only `any` archives remain full CA closures.
4. Existing Nix path contexts and `closureInfo` carry the **complete** runtime
   closure into the image. Original selected archives also stay in the system
   generation. Small original narinfo caches stay unpacked; duplicate NARs do
   not.
5. `seed-catalog` uses the existing graph approval and receipt producers, with
   `Provenance::Repository` taken from each selected catalog record. The
   Starter pack stays Disabled; its FAKE-08, Solarus and ZQuest Classic
   dependencies stay independently selected. The product's `officialCatalogUrl`
   names the pinned catalog on every device, not only images. Gamescope's initial modifier remains unchanged.
6. Before `restore-all`, the offline proof unit compares each archive proof
   with Nix's registered bytes and requires the full bound key. Nix copies
   those signatures and rehashes the complete closure without a substituter
   on first registration. Later boots reuse Nix's verified bound signatures,
   as the previous image unit did; there is no second proof state file.
   Omitted upstream paths keep system-image/root-registration authority;
   the publisher's key does not become their authority. No network or
   compilation is needed for seed recovery, enable or reboot.

Preparation requires a clean builder store, or an already verified selection
of these exact archives. Unsigned/foreign-signed private paths and NAR
mismatches fail closed; no path is overwritten, deleted or re-signed to make a
warm build pass. Runtime import rules are unchanged. When a builder contains
conflicting local outputs, use a fresh native Nix store under an isolated
`/nix` mount. Copy only prebuilt tools and sources with native Nix, not plugin
payloads. Record that initial inventory before acquiring archives. Isolation
costs another store and upstream closure transfer; it is not a fallback lane.
Steam's known FHS nondeterminism is tracked in #351. The live bytes remain the
image pin; a store-path match alone cannot establish its NAR fingerprint.

## Verification

```sh
nix run .#korri-published-plugins-check
```

This checks both architectures' actual catalog archives, signatures, closure
registration and exact Repository receipts; unchanged plugin selection;
ARM seed checks execute the actual ARM host binary under QEMU user emulation,
not a cross-platform validation bypass or an ARM hardware/lifecycle claim;
missing-proof refusal; published mGBA route/launch dispatch and its interface
negative control; product/delivery checks; and a cold, network-blocked Linux
VM using the real SSH, mGBA and Starter pack outputs. The VM tests approval
tampering, SSH access, dependency approvals, 27 Starter pack games and reboot
recovery.

The real-repository test installs the published `any` and native outputs and
updates them through the same release. This is **not** a changed-output
upgrade. No direction-compatible predecessor has been published. That upgrade
acceptance check remains unmet; tests must not invent a predecessor or count
same-release update as an upgrade.

## Existing devices and rollback

The catalog pin controls fresh images, not an operational migration. Do not rewrite or
infer provenance from existing device receipts. Existing user data requires an
explicit, separately agreed operational cutover before deployment. ibuki's matching viewer, bundle and CLI cutover is recorded separately in the
Moonlight removal brief. No other device write is part of this pin.

The plugins repository's existing raw signed-cache publication remains
available to old devices. Neither preparation nor image recovery reads it.
Rollback is an existing image/generation with its original receipts, not a
fallback from Repository to RawCache.
