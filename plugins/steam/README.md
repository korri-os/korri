# Steam

The `@korri:steam/steam` runner admits system `steam` and passes the selected
provider `ref` to the packaged `steam-session <appid>` program. It declares
`STEAM_HOME=${accountRoot}/steam` and that directory. `plugin.ts` performs no
effects.

`plugin.nix` requires the exact `@korri:fex` and `@korri:proton` outputs.
`launch.prepare` refuses the launch unless korrid supplies `cacheRoot` and these
required-plugin files. It passes them to the session program:

| Variable | Launch input |
|---|---|
| `KORRI_CACHE_ROOT` | `cacheRoot` |
| `KORRI_FEX_PREPARE` | `files["@korri:fex/fex-prepare"]` |
| `KORRI_FEX_CONFIG` | `files["@korri:fex/config"]` |
| `KORRI_PROTON_CACHYOS_PREPARE` | `files["@korri:proton/cachyos-prepare"]` |

`steam-session` refuses with status 2 if one is missing. Before it seeds the
Steam client, it runs both prepare programs with `KORRI_CACHE_ROOT`. The first
run downloads the FEX rootfs and Proton-CachyOS into that cache; see
`plugins/fex/README.md` and `plugins/proton/README.md`. A prepare failure exits
126 with `runtime download failed`. The session sets `FEX_ROOTFS` to the printed
rootfs directory and `FEX_APP_CONFIG_LOCATION` to the FEX plugin's `config`
directory. It links `compatibilitytools.d/proton-cachyos-11.0-20260601-slr-arm64`
to the printed Proton directory. The manual sign-in window starts after the
prepare, seed and settings steps. The session program still owns the Steam
client, its FHS environment and the Steam settings.

Provisional slice decisions: Flinthook (401710), Valve's arm64 Steam client,
Proton-CachyOS `proton-cachyos-11.0-20260601-slr-arm64` and FEX. Steam runs under
Sway/Xwayland without gamescope. Steam Input is off; the game uses Korri Seat P1.
Steam stops with the game session. The existing game-unit sandbox is unchanged.

The current catalog is `catalog/library.yaml`: a release uses `system: steam`
and `identity: {kind: provider, value: '@korri:steam/401710'}`. Its `device.yaml`
location is `{provider: '@korri:steam', ref: '401710'}` under that identity key.
These reuse the existing provider release-key and location shapes.

No discovery, installation flow, sign-in automation, typed settings, raw config
overrides, other games or session controls are implemented by this slice.
Sign-in, when needed, remains a manual step in Steam.
