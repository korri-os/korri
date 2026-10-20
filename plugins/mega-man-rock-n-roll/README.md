# Mega Man Rock N Roll

`@korri:mega-man-rock-n-roll` declares Dennis Engelhard's Mega Man Rock N
Roll, public Linux 1.3 build, and plays it under FEX. The build is Godot
2.1.4 with an x86_64 executable. It is the first game whose files korrid
downloads ([ADR 0013](../../docs/adr/0013-declared-games-can-come-from-artifacts.md)).
The plugin ships only the URL and the SHA-256 values. The ZIP is not in the
plugin closure, and no build or check downloads it.

## Declaration

| Export | Key | What it declares |
|---|---|---|
| `artifacts` | `game` | `@korri:mega-man-rock-n-roll/game`, version `1.3`: the ZIP's URL, `sha256:af916d57…abd9` and 97511960 bytes; unpack program `game-unpack` |
| `games` | `mega-man-rock-n-roll` | One release, `1.3`: `hashes` is the SHA-256 of `data.pck`; `artifacts` and `file` name `data.pck` in the `game` artifact |
| `runners` | `fex` | `@korri:mega-man-rock-n-roll/fex`: program `fex-launch`, `releases` holds the `data.pck` hash, `artifacts` is `@korri:fex/rootfs` |
| `handlers` | `launch.prepare` | The launch plan below |

There are no `systems`: the runner accepts the release by its hash
(SCRIPTING.md, "Native runners for specific content"). The release's
`artifacts` and its object `file` need the ADR 0013 korrid; an earlier korrid
refuses them.

`plugin.nix` requires the exact `@korri:fex` output, so the launch input has
`files["@korri:fex/config"]`.

## Files

| Key | File |
|---|---|
| `game-unpack` | `mega-man-rock-n-roll-unpack`: unpacks the verified ZIP; korrid runs it, the runner does not |
| `fex-launch` | `mega-man-rock-n-roll-fex`: the runner program |

## `game-unpack <work-dir> <zip>`

korrid invokes it with the verified ZIP (ADR 0012).

1. It refuses the whole ZIP before it writes anything if an entry is a
   symlink or another special file, is absolute, contains `..`, `.`, an empty
   segment or a backslash, or appears twice.
2. It extracts every file and directory into `<work-dir>`.
3. It requires each file that `game/contents` lists and gives it the listed
   mode: `data.pck` 0644, `MegaMan_RocknRoll` 0755. The ZIP has MS-DOS
   attributes and no Unix modes, so the executable bit comes from that list.
   Other files, such as `readme.txt`, get 0644.

It checks no hashes. korrid verified the whole ZIP against the SHA-256 that
`plugin.ts` pins before it runs the program, and that one hash fixes every
file in the ZIP. `plugin.ts` is the only source of the `data.pck` hash.

It writes only under `<work-dir>`. Exit status: 0 ready, 1 failed (including
usage). Diagnostics go to stderr; stdout carries nothing korrid reads.
Python comes from Nix. Nothing is compiled on the device.

## Launch

`launch.prepare` refuses a core, a raw config override and typed settings.
It takes the game directory from the directory of `contentPath`, so a copy
that discovery finds in a source plays the same way as the download. It
returns:

| Field | Value |
|---|---|
| `command`, `args` | `fex-launch <game directory>` |
| `cwd` | the game directory |
| `env.HOME`, `directories` | `<accountRoot>/mega-man-rock-n-roll`; Godot keeps settings and saves in `~/.godot/app_userdata/Megaman_RnR` |
| `env.KORRI_FEX_CONFIG` | `files["@korri:fex/config"]` |
| `env.KORRI_FEX_ROOTFS` | `artifacts["@korri:fex/rootfs"]` |

`mega-man-rock-n-roll-fex` refuses with status 2 if an input is missing or
is not a directory, or if the game directory lacks `data.pck` or
`MegaMan_RocknRoll`. It sets `FEX_ROOTFS` and `FEX_APP_CONFIG_LOCATION` as the
Steam session does, sets `FEX_APP_DATA_LOCATION` to `$HOME/.fex-emu/`, changes
to the game directory (Godot loads `res://data.pck`, and `res://` is the
working directory) and runs `FEX MegaMan_RocknRoll` with the `pkgs.fex` it
bundles. FEX 2608 names its interpreter `FEX`; there is no `FEXInterpreter`.
The game is X11 only. It uses the game unit's Xwayland `DISPLAY`; nothing
sets `WAYLAND_DISPLAY`.

## Checks

Build these on a build machine, never on a device.

- `packages/check.nix` runs `game/test_unpack.py` against small generated
  ZIPs (success, modes, missing files, traversal, symlinks, duplicates,
  usage) and
  `runner/test_launch.py` with a recording `FEX` program, then `bash -n` and
  shellcheck.
- `packages/plugin-check.nix` checks the built package for the target system.
  On aarch64-linux it runs the launcher with the bundled FEX and the
  `@korri:fex` config on a hand-assembled static x86_64 ELF.
- `plugin.test.ts` (`bun test`) checks the declaration and `launch.prepare`.
