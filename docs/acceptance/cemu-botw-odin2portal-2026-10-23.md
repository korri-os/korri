# Cemu (Wii U) and Breath of the Wild on the Odin 2 Portal — acceptance, 2026-10-23

The `@korri:cemu` plugin (korri-os/plugins `plugins/cemu/`) runs Breath of the
Wild on the AYN Odin 2 Portal. It meets the Odin target over a 15-minute soak
with a Cemu-only Mesa 25.3.2 Turnip driver and Cemu's asynchronous shader
compilation off. The full device reports are in korri-os/plugins
`plugins/cemu/bench/devices/odin2portal-*.md`.

## Device and build

| Item | Value |
|---|---|
| Device | AYN Odin 2 Portal (SM8550, Adreno 740), internal UFS install |
| Korri | `main` 4e34b8a9e, system generation 8 `xqbqdwrn3ws25z9dpnhb3nwl47366m8q`, bundle `983r04y6kbmmygj6gwqi3f9hbniqzyaz` |
| System graphics | Mesa 26.2.3; gamescope 3.16.18 |
| Cemu | upstream `main` 29f2f1f; clang 21.1.2, SDL3 3.4.18, fmt 12.1.0, wxWidgets 3.3.1; graphic packs Github989 bundled |
| Cemu Vulkan driver | Turnip from Mesa 25.3.2, loaded only by Cemu through `VK_DRIVER_FILES` in the plugin wrapper |
| Install | Pre-publication test build: the same `plugins/cemu` source with the `@simonwjackson` owner namespace, signed, checked and installed through `korri-plugin inspect`, `install` and `enable` |

## Why the plugin pins its own Turnip

System Mesa 26.2.3 crashes while compiling a BotW vertex shader (pipeline
BLAKE3 `026211ce39d10e5e02021680ebff7da347ac4ee27a5064daeacd045d15efb127`):
ir3 receives a 32-bit `nir_op_fmadz`, reports `Unhandled ALU op`, then crashes
in `ralloc_free`. Mesa 26.2.0 introduced the new multiply-add scheme that
creates `fmadz`; 26.0.x and 26.1.x do not have it. Mesa 26.2.5 and `main`
still carry the bug. The Cemu-only pin leaves gamescope and the portal on
system Mesa.

With Mesa 25.3.2, Cemu 29f2f1f asserts in `VulkanRenderer::draw_execute`
(null `activeVertexDS`) while asynchronous compilation is on. The plugin's
seeded Cemu settings therefore set `AsyncCompile=false`. The owner approved
both choices.

## Results (100 s warm-up, 180 s sample, 720p, May 2026 Cemu settings, game-layer `cpu.min=100`/`gpu.min=100`)

| Run | Median | p10 | p1 | min | Peak |
|---|---:|---:|---:|---:|---:|
| Baseline | 39 | 38 | 36 | 33 | 96.7 °C |
| 15-minute soak | 40 | 38 | 34 | 4 | 97.5 °C |

The soak had isolated stalls down to 4 FPS (shader compilation without async).
It had no crash, no device loss and no thermal shutdown.

| Check | Result |
|---|---|
| Launch from the Korri library through korrid → gamescope → gamescopereaper → Cemu, Vulkan backend | pass |
| Median ≥ 40, p10 ≥ 35 | pass in the soak; the baseline median was 39 |
| Audio: Cemu Cubeb stream in PipeWire, audible | pass; the owner heard it |
| Staged save loads from `saveRoot` | pass |
| A new in-game save survives a relaunch | **not proven**: the scripted menu route never reached System > Save |
| 15-minute soak | pass |
| Launch settings restored to device defaults | pass |

Bubblewrap was not observed: core at 4e34b8a9e does not implement ADR 0015.

## Experimental: patched Mesa 26.2.5 Turnip (not shipped)

A 7-line ir3 fix (run the final `nir_opt_algebraic_late` to a fixed point in
`ir3_context.c`) removes the `fmadz` crash. It lives on korri-os/plugins branch
`exp/cemu-turnip-26-2-fmadz` (not pushed). In 7 launches there was no crash;
only one launch reached gameplay: 42 / 41 / 41 at 95.5 °C. The async-on and
soak questions are open, so the plugin stays on 25.3.2.

## Limits

- The final plugin build adds a Cemu file-cache shutdown fix (a lost wakeup
  that hung `cemu --help` in the package check). It changes only shutdown
  ordering and was not re-run on the device.
- Device acceptance used the `@simonwjackson` test namespace; the published
  `@korri:cemu` bytes were not installed on a device (owner decision).

## Findings filed separately

Gamescope modifier missing from `device.yaml` and library items despite
`KORRID_INITIAL_MODIFIERS`; `korrid-control.socket` restarting korrid during
an offline `korrid catalog pair`; a normal exact stop classified as failed
(wrapper exit 143, transient `HostRecoveryBlocked`); ADR 0015 not implemented.
