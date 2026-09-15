# Legacy coverage inventory

All 50 folders at legacy commit `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62`.
Descriptors and substantive implementation paths were reviewed. No builds or runtime tests were run.

| Folder | Review group | Entry/reference path |
|---|---|---|
| 3dsen | runtime | `product/plugins/3dsen/src/plugin.ts` |
| acquisition-fixtures | acquisition | `product/plugins/acquisition-fixtures/index.ts` |
| am2rlauncher | games | `product/plugins/am2rlauncher/index.ts` |
| box64-runtime | substrate | `product/plugins/box64-runtime/src/plugin.ts` |
| cdp-input-bridge | runtime | `product/plugins/cdp-input-bridge/index.ts` |
| community-catalog | acquisition | `product/plugins/community-catalog/src/plugin.ts` |
| dome-romantik | games | `product/plugins/dome-romantik/index.ts` |
| fex-runtime | substrate | `product/plugins/fex-runtime/src/plugin.ts` |
| gamescope | runtime | `product/plugins/gamescope/src/plugin.ts` |
| globeba | games | `product/plugins/globeba/index.ts` |
| gmloader | runtime | `product/plugins/gmloader/src/plugin.ts` |
| itchio | acquisition | `product/plugins/itchio/index.ts` |
| levelsharesquare | acquisition | `product/plugins/levelsharesquare/src/plugin.ts` |
| mega-man-arena | games | `product/plugins/mega-man-arena/src/plugin.ts` |
| mega-man-maker | games | `product/plugins/mega-man-maker/src/plugin.ts` |
| mega-man-rock-n-roll | games | `product/plugins/mega-man-rock-n-roll/index.ts` |
| melonds | runtime | `product/plugins/melonds/src/plugin.ts` |
| midas-machine | games | `product/plugins/midas-machine/src/plugin.ts` |
| moonlight | acquisition | `product/plugins/moonlight/src/plugin.ts` |
| neverball | games | `product/plugins/neverball/index.ts` |
| pico8 | runtime | `product/plugins/pico8/src/plugin.ts` |
| portmaster | acquisition | `product/plugins/portmaster/src/plugin.ts` |
| proton-ge-runtime | substrate | `product/plugins/proton-ge-runtime/src/plugin.ts` |
| proton-runtime | substrate | `product/plugins/proton-runtime/src/plugin.ts` |
| psycho-waluigi | games | `product/plugins/psycho-waluigi/src/plugin.ts` |
| remap | runtime | `product/plugins/remap/index.ts` |
| retroarch | runtime | `product/plugins/retroarch/src/plugin.ts` |
| rpcs3 | runtime | `product/plugins/rpcs3/src/plugin.ts` |
| ryubing | runtime | `product/plugins/ryubing/src/plugin.ts` |
| shipwright | games | `product/plugins/shipwright/index.ts` |
| smb-wonderland-1987 | games | `product/plugins/smb-wonderland-1987/src/plugin.ts` |
| smbxgame | games | `product/plugins/smbxgame/src/plugin.ts` |
| smwcentral | acquisition | `product/plugins/smwcentral/src/plugin.ts` |
| sonic-3-air | games | `product/plugins/sonic-3-air/index.ts` |
| sonic-time-twisted | games | `product/plugins/sonic-time-twisted/index.ts` |
| spelunky-classic-hd | games | `product/plugins/spelunky-classic-hd/index.ts` |
| srb2 | games | `product/plugins/srb2/src/plugin.ts` |
| srb2kart | games | `product/plugins/srb2kart/index.ts` |
| stargrove-scramble | games | `product/plugins/stargrove-scramble/index.ts` |
| steam | acquisition | `product/plugins/steam/src/plugin.ts` |
| super-mario-127 | games | `product/plugins/super-mario-127/index.ts` |
| super-mario-bros-remastered | games | `product/plugins/super-mario-bros-remastered/index.ts` |
| tiny-crate | games | `product/plugins/tiny-crate/index.ts` |
| tmnt-rescue-palooza | games | `product/plugins/tmnt-rescue-palooza/index.ts` |
| turnip | substrate | `product/plugins/turnip/src/plugin.ts` |
| web-canvas | runtime | `product/plugins/web-canvas/index.ts` |
| webpage | runtime | `product/plugins/webpage/index.ts` |
| xjlt | games | `product/plugins/xjlt/index.ts` |
| yoshis-fabrication-station | games | `product/plugins/yoshis-fabrication-station/index.ts` |
| zquest-classic | games | `product/plugins/zquest-classic/src/plugin.ts` |

## What drove the operation design

- Data-only declarations: Neverball and several native-game entries.
- Shared factories hide acquisition logic: many short community-source entries.
- Mutable installers/readiness: Steam, PortMaster, gmloader, FEX/Proton consumers.
- Launch modifiers and session lifetimes: Gamescope, remap, CDP input bridge, Box64, Turnip.
- Native config and preflight: RetroArch, melonDS, RPCS3, Ryubing, 3dsen.
- Online providers: itch.io, SMWCentral, LevelShareSquare, Pico-8, Mega Man Maker.
- Streaming/live protocol work: Moonlight.
- Browser startup/input/state: webpage, web-canvas, Yoshi's Fabrication Station.

The four detailed source-review reports remain separate from the design text.
The design does not treat all exported legacy helpers as proven production registration.
