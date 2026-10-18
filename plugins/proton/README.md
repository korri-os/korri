# Proton

`@korri:proton` ships Proton compatibility tools for runners that require it.
It declares no runner and no handler. `cachyos` names the one build it carries.
That name is a file-key prefix and a cache-path segment only. A runner that
lists this plugin's exact output in its `plugin.nix` `requires` reads this file
from its launch input under `@korri:proton/<key>`:

| Key | Artifact |
|---|---|
| `cachyos-prepare` | Native program that prepares the pinned Proton-CachyOS arm64 build |

`cachyos/compatibilitytool.vdf` is not a file key. `cachyos-prepare` installs
it from its own closure.

`cachyos/pin.nix` holds the pin, moved from the former
`plugins/steam/packages/proton-cachyos-arm64/default.nix`, and defines the tool
name `proton-cachyos-11.0-20260601-slr-arm64` once. Nix substitutes it into
`compatibilitytool.vdf` and into the Steam plugin's seed, session and VDF
settings.

`compatibilitytool.vdf` is the legacy file with one change: `install_path` is
`.`. The file now sits inside the tool directory, and Valve's own template in
the release says a subdirectory of `compatibilitytools.d` uses `.`. The legacy
value named the tool directory because the file sat directly in
`compatibilitytools.d`.

## `cachyos-prepare <cache-root>`

The tool is not in the plugin closure. `cachyos-prepare` downloads it on the
device into korrid's reconstructible cache.

1. If `<cache-root>/proton/cachyos/11.0-20260601-slr-arm64/` exists and
   nothing is left over, it prints that path and exits 0 without the lock. It
   makes no other check of the ready directory.
2. Otherwise it takes `<cache-root>/proton/cachyos/.lock` and removes `.tmp-*`
   work directories left by an interrupted run. If the ready directory exists,
   it also removes `.downloads` left by a run killed after its rename, prints
   the path and exits 0.
3. It downloads the release into `.downloads/<sha256>.part` and checks its
   SHA-256, with the same resume, restart and mismatch rules as `fex-prepare`.
4. It unpacks the release's single top-level directory into `.tmp-<random>`,
   removes `require_tool_appid` from `toolmanifest.vdf`, and installs
   `compatibilitytool.vdf` in place of the release's own file.
5. It makes the tree read-only, flushes the filesystem, renames the work
   directory to the ready directory and deletes `.downloads`. It prints the
   ready directory as its last stdout line.

Exit status: 0 ready, 1 download or verify failure, 2 usage. Diagnostics go to
stderr. curl, tar and xz come from Nix. Nothing is compiled on the device.

## Checks

Build these on a build machine, never on a device. `check.nix` runs the prepare
tests against a local fixture server, `bash -n` and shellcheck.
`plugin-check.nix` checks the built package for the target system.
