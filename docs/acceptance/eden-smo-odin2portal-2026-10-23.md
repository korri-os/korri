# Eden / Super Mario Odyssey on the AYN Odin 2 Portal — acceptance, 2026-10-23

Super Mario Odyssey (SMO) runs from the Korri library through the Eden plugin
at the 60 FPS cap on the Odin 2 Portal, with NCE and with Dynarmic, and holds
it through a 15-minute soak. The user's own save loads and survives a
relaunch. The FPS figures come from a static reference view, not open play.

## Scope

| Item | Value |
|---|---|
| Device | AYN Odin 2 Portal, SM8550, Adreno 740 |
| System | kernel 7.2.0, Mesa 26.2.3 (Turnip), Gamescope 3.16.18 |
| Plugin source | korri-plugins `feat/eden-mini` lineage, tested source `945e9d4`; landed as one squashed commit |
| Package under test | personally signed `@simonwjackson:eden` build of the same source; the only delta is the runner-id publisher literal |
| Eden | nixpkgs `eden` 0.2.1 from `nixpkgs-eden` (`02c087f3`), unchanged cache.nixos.org binary `/nix/store/010847afgzfll903n5dbzqdajmm1m0n7-eden-0.2.1` |
| Launcher | `/nix/store/77ndp24virwb0khxal8id2r46pi9w915-eden-korri-0.2.1` |
| Launch path | korrid reserve/start → plugin executor → Gamescope/Xwayland → Eden |
| View | Bonneton resume platform, camera turned to face the Odyssey. Not the open town area; FPS may be optimistic versus open-area play |
| Settings during the runs | SMO game layer: Gamescope on, `cpu.min=100`, `gpu.min=100`; all restored afterwards |

## Results

| Run | Seconds measured | Median FPS | p10 | p1 |
|---|---:|---:|---:|---:|
| NCE (Eden's ARM default) | 180 | 60 | 60 | 60 |
| Dynarmic | 180 | 60 | 60 | 59 |
| NCE soak | 900 | 60 | 60 | 59 |

- FPS source: X11 Present completions on Eden's own window, in one-second bins, after a 100 s warm-up. Not compositor Hz.
- Renderer: Turnip Adreno (TM) 740, Mesa 26.2.3, Vulkan 1.4.354.
- CPU backend: inferred from the effective-backend log line, the absence of refusal warnings and the runtime mappings. Not direct proof of instruction execution.
- Soak: peak CPU sensor 83.2 °C, RSS growth 768 kB, no swap.
- NCE ships as the default. Both backends reach the cap at this view, so this view cannot separate them.

## Save, audio, isolation

- **Save:** the user's December 2025 save was moved under Eden's profile and recorded through normal session-end capture. It loads at 1,242 coins and 3 health and survives a relaunch.
- **Audio:** checked at night without sound. Eden's PipeWire stream had two active links to the speaker sink, and the sink stayed at volume 0.50, muted. Audible output and drift are not verified.
- **Bubblewrap:** absent. korrid does not wrap launches yet (korri-os/korri#184).

## Fixes the device run forced into the plugin

| Problem | Fix, in Eden's own `qt-config.ini` |
|---|---|
| Eden 0.2.1 sorts `llvmpipe` ahead of integrated Turnip and picks it | The wrapper writes Turnip's index in Eden's order while `Renderer\vulkan_device` is at Eden's default; fails closed if there is no Adreno |
| The wrapper wrote `[Data Storage]`; Eden reads `[Data%20Storage]` | Write the encoded section name |
| Eden's player 1 binds only the keyboard | Bind P1 to InputPlumber's Korri Seat P1 while P1 is at Eden's default |

## Core gaps found

Each needed an operational workaround on the device:

- korrid launches run without bubblewrap (#184).
- No RPC writes a per-game launch modifier, so Gamescope was enabled by a hash-checked edit of the game-layer `modifiers` record (ADR 0009 shape), then restored.
- Online source pairing needs a folder-selection receipt, and nothing on the host issues one. Pairing used the offline `korrid catalog pair` with korrid stopped.
- The kiosk token cannot run a discovery rescan.
- Content staged for the runtime user needs read access for the `korrid` service user before discovery sees it.

## Evidence

korri-plugins `plugins/eden/bench/samples/odin2portal-acceptance/` (redacted
reports, hashes, reference image). The unredacted raw evidence is in a private
archive on the build host, listed in `ARCHIVE.sha256` by hash.
