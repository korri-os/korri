# FEX

`@korri:fex` ships the FEX x86 emulation runtime for runners that require it.
It declares no runner and no handler.

## Files

A runner that lists this plugin's exact output in its `plugin.nix` `requires`
reads these files from its launch input under `@korri:fex/<key>`:

| Key | File |
|---|---|
| `config` | FEX configuration directory: `Config.json` and `AppConfig/steamwebhelper.json`, moved unchanged from the Steam plugin |
| `fex-unpack` | Native program that unpacks the verified rootfs files; korrid runs it, runners do not |

## Artifact `@korri:fex/rootfs`

`plugin.ts` declares the pinned x86 rootfs as the artifact `rootfs`, version
`archlinux-2026-01-08-mesa-26.1.2`: the ArchLinux 2026-01-08 squashfs
(1.16 GB) followed by ten Arch Mesa `1:26.1.2-1` overlay packages (0.03 GB),
each with its URL, `sha256:<hex>` and `bytes` from the server's
Content-Length. `plugin.ts` is the single source of these pins. The rootfs is
not in the plugin closure.

A runner that lists `@korri:fex/rootfs` in its `artifacts` receives the ready
directory, `<storage-root>/cache/fex/rootfs/archlinux-2026-01-08-mesa-26.1.2/`,
in its launch input as `artifacts["@korri:fex/rootfs"]`. korrid downloads,
resumes and verifies the files, runs `fex-unpack` in a work directory, makes
the tree read-only, flushes it and renames it to the ready directory. See ADR
0012.

## `fex-unpack <work-dir> <squashfs> <overlay>...`

korrid invokes it with the verified files in declared order.

1. It unsquashes the base into `<work-dir>` and makes the tree writable for
   the overlays.
2. It extracts each overlay package over it, in order, without the package
   metadata files.
3. It checks the Mesa files and the x86_64 and i686 Freedreno ICD machine
   types, as the former Nix build did.

It writes only under `<work-dir>`. Exit status: 0 ready, 1 failed (including
usage). Diagnostics go to stderr; stdout carries nothing korrid reads. tar,
squashfs-tools and zstd come from Nix. Nothing is compiled on the device.

## Checks

Build these on a build machine, never on a device. `packages/check.nix` runs
`rootfs/test_unpack.py` against small synthetic archives, `bash -n` and
shellcheck. `packages/plugin-check.nix` checks the built package for the target
system.
