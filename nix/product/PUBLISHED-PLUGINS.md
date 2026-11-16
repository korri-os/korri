# Catalog-pinned Linux images

ADR 0043 and approved schema-sheet row 13 define the pin in
`published-plugins.nix`: `{ catalogUrl, catalogSha256 }`. No per-path pin list,
raw-cache fallback, receipt migration, or outside repository is in this lane.

## Published input

- Plugins source: `b036fa5e31604321b286f40d34adc15edee08d16`.
- Immutable tag: `build-b036fa5e3160-sunshine-0046`.
- Publication dry run: `43934354164`; live publication: `43953806767`.
- Catalog: <https://github.com/korri-os/plugins/releases/download/build-b036fa5e3160-sunshine-0046/catalog.json>.
- Actual catalog SHA-256: `392054e2a3c3f38d08928c3dd3d17bc37d5e1b17772f02b760dd485dfadfbd2d`.
- SRI: `sha256-OSBU4qPD840Ikow909F7w31eGxd3LwK3YN1IXfrfvS0=`.
- 33 plugin IDs, 63 records. Homebrew Hub, PICO-8 and RetroBrews use `any`.
- Bound key remains
  `korri-plugins-1:qlK5Mgb3dYhF76WC4jGhrvL+CHsU93De7GpBFtrXb98=`.

The selection is the same as the previous pin,
`build-7e9c19c99b6f-sunshine-resize`. Against it, 2 plugins changed: Sunshine
on both architectures (patch 0046, korrid can resize the encoder of one
stream, #476; patch 0045, a silent peer no longer holds every HTTPS client,
#443) and Steam on x86_64 (the same bytes as
`build-1c5818a1ff22-steam-native-libs`). The other 31 plugins, and Steam on
aarch64, keep their store paths and archive bytes.

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

This is a fresh-image cutover, not an operational migration. Do not rewrite or
infer provenance from existing device receipts. Existing user data requires an
explicit, separately agreed operational cutover before deployment. No device
write or deployment is part of this change.

The plugins repository's existing raw signed-cache publication remains
available to old devices. Neither preparation nor image recovery reads it.
Rollback is an existing image/generation with its original receipts, not a
fallback from Repository to RawCache.
