# Plugin runtimes download into the shared cache through their own prepare programs

**Owner decision, 2026-10-17.** A plugin downloads its own large third-party
payload on first use, with its own native prepare program. korrid gets no
download feature. This is an explicit exception to the rule in AGENTS.md
(lines 64-67) that plugins return declarations and korrid performs the effect.
There are two reasons. The precedent already exists: `steam-session` downloads
Valve's client through `steam-arm64-seed`
(`plugins/steam/session/steam-session.sh`,
`plugins/steam/packages/steam-korri/scripts/steam-arm64-seed`). A
korrid-owned download needs the capability model, and AGENTS.md says not to
build that model before real cases need it. The planned upgrade is
korrid-owned downloads that show their progress in the portal. The pinned
URLs and SHA-256 values stay the same when that happens.

**Owner decision, 2026-10-17: location.** Payloads go in
`<storage-root>/cache/<plugin>/<contribution>/<version>/`, which is
`/var/lib/korri/cache/...` by default. Today that is
`fex/rootfs/archlinux-2026-01-08-mesa-26.1.2/` and
`proton/cachyos/11.0-20260601-slr-arm64/`. This is the "reconstructible
caches" class in `docs/briefs/2026-09-15-plugin-model/DESIGN.md` §9.

**Owner decision, 2026-10-17: IDs.** The plugins are `@korri:fex`, and
`@korri:proton` with the build named `cachyos`. `cachyos` is only a file-key
prefix and a cache-path segment. No Proton contribution kind exists yet.

**Owner decision: drop the plugin size and fixed time limits.** This applies
DESIGN.md §10, "Proportionate trust". The archive SHA-256, `--sigs-needed`,
content-address checks and the publisher's bound key establish integrity, and
they are unchanged. The bounds that remain do not grow with a package: the
archive entry count, the catalog, release-evidence and source-manifest
limits, connection setup, and stall detection on archive downloads.

**Provisional: chosen for the owner, who can reverse it.**

- `PluginLaunchInput.cacheRoot` is the storage root joined with `cache`. It
  follows the `accountRoot` precedent in
  `services/korrid/src/launcher/linux_plugin.rs`.
- `PluginLaunchInput.files` gives a runner its own files under their bare keys
  and the files of each directly required plugin under
  `<pluginId>/<fileKey>`, for example `@korri:fex/fex-prepare`. It follows the
  existing contribution-ID grammar `<pluginId>/<localId>`. A bare key cannot
  contain `/` or `@`, so the two forms cannot collide.
- A prepare program runs as `<prepare> <cache-root>`. It prints the ready
  directory as its last stdout line and exits 0 (ready), 1 (download or
  verify failure) or 2 (usage), the conventional usage status.
- Inside its cache directory, a prepare program keeps partial downloads as
  `.downloads/<sha256>.part`, named by the pinned hash so a resumed file
  belongs to exactly one pin, and serializes runs with a `.lock` file.
- The Steam plugin passes `KORRI_CACHE_ROOT`, `KORRI_FEX_PREPARE`,
  `KORRI_FEX_CONFIG` and `KORRI_PROTON_CACHYOS_PREPARE` to `steam-session`.
  The `KORRI_` prefix follows the existing `KORRI_STEAM_INPUT_GUARD`
  variables.

## Considered Options

- **korrid downloads and verifies the payloads.** It keeps the effect in
  korrid and can report progress, but it needs a download contribution and
  the capability model, which have no other real case yet.
- **Keep the payloads in the Nix closure**, as `fex-rootfs.nix` and
  `proton-cachyos-arm64` did inside the Steam plugin. It needs no network at
  launch, but the publisher cache and every installation carry about 1.5 GB
  more.

## Consequences

- The first Steam launch downloads about 1.5 GB (1.4 GiB) with no progress
  shown: the FEX rootfs (1.16 GB), Proton-CachyOS (0.34 GB) and the pinned
  Arch Mesa packages (0.03 GB), measured from the servers' `Content-Length`
  on 2026-10-17.
- A new device needs `rootfs.fex-emu.gg`, `archive.archlinux.org` and the
  CachyOS GitHub releases to be reachable for that first launch.
- The plugin-host and korrid changes and the plugins changes must ship
  together. The Steam plugin refuses to launch without `cacheRoot` and the
  required files.
