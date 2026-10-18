# FEX

`@korri:fex` ships the FEX x86 emulation runtime for runners that require it.
It declares no runner and no handler. A runner that lists this plugin's exact
output in its `plugin.nix` `requires` reads these files from its launch input
under `@korri:fex/<key>`:

| Key | Artifact |
|---|---|
| `fex-prepare` | Native program that prepares the pinned x86 rootfs |
| `config` | FEX configuration directory: `Config.json` and `AppConfig/steamwebhelper.json`, moved unchanged from the Steam plugin |
| `fex` | nixpkgs FEX `bin/FEX` |

## `fex-prepare <cache-root>`

The rootfs is not in the plugin closure. `fex-prepare` downloads it on the
device into korrid's reconstructible cache. `rootfs/pins.nix` holds the pins,
moved from the former `plugins/steam/packages/fex-rootfs.nix`: the
ArchLinux 2026-01-08 squashfs and ten Arch Mesa `1:26.1.2-1` overlay packages.

1. If `<cache-root>/fex/rootfs/archlinux-2026-01-08-mesa-26.1.2/` exists, it
   prints that path and exits 0. It makes no other check.
2. It takes `<cache-root>/fex/rootfs/.lock`, so concurrent runs download once.
   It removes `.tmp-*` work directories left by an interrupted run.
3. It downloads each pin into `<cache-root>/fex/rootfs/.downloads/<sha256>.part`
   and checks its SHA-256. A dropped transfer keeps its partial file, and the
   next run resumes it. If the server cannot resume, or the resumed bytes are
   wrong, the run starts that file again. A hash mismatch deletes the file and
   exits 1.
4. It unsquashes the base into `.tmp-<random>`, extracts the overlay packages
   without their package metadata, and checks the Mesa files and the x86_64
   and i686 Freedreno ICD machine types, as the former Nix build did.
5. It makes the tree read-only, flushes the filesystem, renames the work
   directory to the ready directory and deletes `.downloads`. It prints the
   ready directory as its last stdout line.

Exit status: 0 ready, 1 download or verify failure, 2 usage. Diagnostics go to
stderr. curl, squashfs-tools, zstd and tar come from Nix. Nothing is compiled
on the device.

## Checks

Build these on a build machine, never on a device. `check.nix` runs the prepare
tests against a local fixture server, `bash -n` and shellcheck.
`plugin-check.nix` checks the built package for the target system.
