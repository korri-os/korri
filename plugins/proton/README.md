# Proton

`@korri:proton` ships Proton compatibility tools for runners that require it.
It declares no runner and no handler. `cachyos` names the one build it
carries. That name is the artifact key, a file-key prefix and a cache-path
segment only.

## Files

A runner that lists this plugin's exact output in its `plugin.nix` `requires`
reads this file from its launch input under `@korri:proton/<key>`:

| Key | File |
|---|---|
| `cachyos-unpack` | Native program that unpacks the verified release; korrid runs it, runners do not |

`cachyos/compatibilitytool.vdf` is not a file key. `cachyos-unpack` installs
it from its own closure.

## Artifact `@korri:proton/cachyos`

`plugin.ts` declares the pinned Proton-CachyOS arm64 release as the artifact
`cachyos`, version `11.0-20260601-slr-arm64`, with its URL, `sha256:<hex>`
and `bytes` (0.34 GB, the server's Content-Length). `plugin.ts` is the single
source of the pin. The tool is not in the plugin closure.

A runner that lists `@korri:proton/cachyos` in its `artifacts` receives the
ready directory, `<storage-root>/cache/proton/cachyos/11.0-20260601-slr-arm64/`,
in its launch input as `artifacts["@korri:proton/cachyos"]`. korrid downloads,
resumes and verifies the release, runs `cachyos-unpack` in a work directory,
makes the tree read-only, flushes it and renames it to the ready directory.
See ADR 0012.

## Tool name

`cachyos/tool-name.nix` defines Steam's name for the tool,
`proton-cachyos-11.0-20260601-slr-arm64`, once. Nix substitutes it into
`compatibilitytool.vdf` and into the Steam plugin's session and VDF settings.
It embeds the artifact version pinned in `plugin.ts`; the two move together.

`compatibilitytool.vdf` is the legacy file with one change: `install_path` is
`.`. The file sits inside the tool directory, and Valve's own template in the
release says a subdirectory of `compatibilitytools.d` uses `.`.

## `cachyos-unpack <work-dir> <release.tar.xz>`

korrid invokes it with the verified release.

1. It unpacks the release's single top-level directory into `<work-dir>`.
2. It removes `require_tool_appid` from `toolmanifest.vdf`, so the tool
   serves as Steam's global default instead of chaining to an unregistered
   runtime tool.
3. It installs `compatibilitytool.vdf` in place of the release's own file.

It writes only under `<work-dir>`. Exit status: 0 ready, 1 failed (including
usage). Diagnostics go to stderr; stdout carries nothing korrid reads. tar and
xz come from Nix. Nothing is compiled on the device.

## Checks

Build these on a build machine, never on a device. `packages/check.nix` runs
`cachyos/test_unpack.py` against a small synthetic release, `bash -n` and
shellcheck. `packages/plugin-check.nix` checks the built package for the
target system.
