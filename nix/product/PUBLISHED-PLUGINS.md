# Catalog-pinned Linux images

ADR 0043 and approved schema-sheet row 13 define the pin in
`published-plugins.nix`: `{ catalogUrl, catalogSha256 }`. No per-path pin list,
raw-cache fallback, receipt migration, or outside repository is in this lane.

## Published input

- Plugins source: `72627749471e4cd8089c292dc2f0f0245cd50481`.
- Immutable tag: `build-72627749471e-plugin-direction`.
- Publication dry run: `41000811619`; live publication: `41011369204`.
- Catalog: <https://github.com/korri-os/plugins/releases/download/build-72627749471e-plugin-direction/catalog.json>.
- Actual catalog SHA-256: `e9a642a2271c78dc9aa4c32ec7d5ad6c9c24dcb6fa5cdb9be1bac079bba9075b`.
- SRI: `sha256-6aZCoicceNyapMMux9WtbJwk3Lb6XNub4brAebupB1s=`.
- 33 plugin IDs, 63 records. Homebrew Hub, PICO-8 and RetroBrews use `any`.
- Bound key remains
  `korri-plugins-1:qlK5Mgb3dYhF76WC4jGhrvL+CHsU93De7GpBFtrXb98=`.

These identify the actual live bytes. They do not establish dry/live payload
reproducibility. The Pages catalog is a moving discovery address, not an image
pin.

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
   dependencies stay independently selected. Image `officialCatalogUrl`
   names the pinned catalog. Gamescope's initial modifier remains unchanged.
6. Before `restore-all`, the offline proof unit compares each archive proof
   with Nix's registered bytes and requires the full bound key. Nix copies
   those signatures and rehashes the complete closure without a substituter.
   Omitted upstream paths keep system-image/root-registration authority;
   the publisher's key does not become their authority. No network or
   compilation is needed for seed recovery, enable or reboot.

Preparation is mandatory even with a warm store. Existing unsigned or
foreign-signed publisher-private paths fail closed. It does not overwrite
conflicting input-addressed bytes already present in the build store.

## Verification

```sh
nix run .#korri-published-plugins-check
```

This checks both architectures' actual catalog archives, signatures, closure
registration and exact Repository receipts; unchanged plugin selection;
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
