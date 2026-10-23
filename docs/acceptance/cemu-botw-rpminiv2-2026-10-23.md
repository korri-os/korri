# Cemu (Wii U) and Breath of the Wild on the Retroid Pocket Mini V2 — acceptance, 2026-10-23

The `@korri:cemu` plugin runs Breath of the Wild on the Mini V2 at the panel's
native 31:27 aspect. Short runs reach 30 FPS at 620×540. **No configuration has
passed a 15-minute soak**: both soak attempts ended in an unclean device reset.
The full ladder is in korri-os/plugins
`plugins/cemu/bench/devices/rpminiv2-ladder-2026-10-23.md`.

## Device and build

| Item | Value |
|---|---|
| Device | Retroid Pocket Mini V2 (SM8250, Adreno 650), internal UFS install |
| Korri | system generation 30 `861m162v29s7cj9dcsfmdr7ldgh5qgfz` and bundles `c6n0dpw4dn4rm65dp6jbvwdw20kay4qd`, later `vxmm8d95ndax40x650pz47bgxyif05bk`. Another session installed both outside this run; the results were measured on them as found |
| Display | 1080×1240 panel, 1240×1080 logical (31:27) |
| Cemu build | as on the Odin (`cemu-botw-odin2portal-2026-10-23.md`): Cemu-only Mesa 25.3.2 Turnip, `AsyncCompile=false` |
| Native aspect | optional CC0 presets added to the bundled BotW Graphics pack: "31:27 (1240x1080 handheld)" with matching resolutions. 16:9 stays the default for every device; the 31:27 choice lives in the account's Cemu settings |

## Ladder (100 s warm-up, 180 s sample, FPS++ 30, May 2026 Cemu settings)

| Rung | Aspect / resolution | Median / p10 / p1 | Peak | Kept |
|---|---|---:|---:|---|
| 16:9 reference | 1280×720, black bars | 30 / 30 / 30 | 70.5 °C | reference only |
| Native baseline | 31:27, 1028×896 | 25 / 24 / 22 | 87.2 °C | – |
| Game `cpu.min`/`gpu.min=100` | 1028×896 | 25 / 24 / 23 | 88.0 °C | reverted |
| Device governor `performance` | 1028×896 | 25 / 24 / 23 | 88.4 °C | reverted |
| Resolution | 827×720 | 29 / 28 / 27 | 89.2 °C | superseded |
| Resolution | **620×540** | **30 / 30 / 29** | 85.7 °C | best short run |
| Patched Mesa 26.2.5 (experimental) | 1028×896 | 25 / 24 / 24 | 88.0 °C | not shipped |
| Patched Mesa 26.2.5 (experimental) | 827×720 | 29 / 29 / 28 | 86.8 °C | not shipped |

The 827×720 and 620×540 rows on 25.3.2 ran with the device governor left at
`performance` by a helper error; they did not use default launch settings.

## Acceptance

| Check | Result |
|---|---|
| Median ≥ 30, p10 ≥ 27 | short runs only (620×540, governor caveat) |
| 15-minute soak | **fail**: unclean resets at 06:57 and 14:46 UTC before measurement; cause unknown (thermal, GPU, power or USB not distinguished) |
| Everything shipped in Cemu's own files or existing launch settings | pass; no new Korri schema |
| Launch settings restored to device defaults | pass |

Final device state: Mesa 25.3.2 build, 31:27 at 620×540 in the owner's Cemu
account, default limits, speaker muted.

## Save data loss (filed separately)

The owner's own BotW session (06:17:58–06:26:26 UTC) produced snapshot
`5f3c80b8…`. Its record exists on both the Mini V2 and the Odin, but 7 of its 58
payload blobs (including `user/80000001/3/game_data.sav`) are missing from
korrid's object store on both devices, so `app.saves.restore` returns
`SavePayloadUnavailable`. The account was left on the complete staged May 2026
snapshot `80c5f056…`, as a continuation. The cause (retention or object GC
against an unsynced write lost in a reset) is unknown.
