# Zone (native surface package)

Korri's zone select, as the package the native shell loads
(`docs/briefs/2026-10-31-native-surfaces-contract.md`). It replaces the web
surface `surfaces/zone/` (deleted in the same cut); its behaviour, plans,
fixtures' words and look are Zone's.

| Path | What |
| --- | --- |
| `main.slint` | The binding: `Surface` and `Overlay`. The only file that knows the view; it feeds the `Tokens` global the window's size, the look and the ground, and hands each page plain properties. |
| `ui/tokens.slint` | `global Tokens`: Zone's tokens (`zone-tokens.css`, `zone-fonts.css`, the plans' thresholds). Design inputs are literals; everything else derives from them and the window. |
| `ui/types.slint` | The view's structs. |
| `ui/atoms`, `ui/molecules`, `ui/organisms`, `ui/templates`, `pages/` | One component per file, at the layer of its React component in `surfaces/zone/src` (the mapping, and every merge with its reason, is `test/decomposition-gate.test.ts`). Each has its preview states beside it, `<Name>.<layer>.preview.slint`. |
| `ui/preview-frame.slint` | The window a preview state inherits; it feeds `Tokens` as `main.slint` does. |
| `logic.ts` → `logic.js` | The adapter around Zone's own program (`src/state/`, `src/plan/`, ported to the generated treaty types). `bun run zone:build` in `clients/shell`, with the flake's bun (`nix shell --inputs-from . nixpkgs#bun`): `zone-check` rebuilds it and fails on any difference. |
| `src/press.ts` | Focus in the model: each press becomes the fact Zone's page reported in the browser. |
| `src/view.ts`, `src/rows.ts` | The view. |
| `src/keys.ts`, `src/qr.ts` | The keyboard's grid; the backup's QR code (own encoder, held equal to `qrcode` by test). |
| `fonts/` | Zone Caps, Jersey 10 (converted from Zone's woff2), Anton; their licences; `fonts/source/` the Zone Caps grid and the font build scripts. |
| `art/` | The veil's checker and PAUSED's grey checker, one tile per art pixel size. |
| `test/` | The gates and plan tests (`bun test packages/zone/test` in `clients/shell`), and `bite.sh`, which plants a defect for each gate and shows it fail. |
| `preview/` | `scenarios/` (Zone's fixtures as models, with the presses to each page), `shots.sh` (the real shell under Xvfb), side-by-side and five-size images. |

## Contract use

- `preferences`: `zone.look`, mark `shared`, initial `blend`; stored with
  `RememberPreference` when OPTIONS › SYSTEM › LOOK steps it. `displays: "main"`.
- `FocusGame` whenever the band's focused game changes; GAME OPTS. lists
  `model.focusedGame.actions`.
- `MoveCatalog`: the FIND draft is the query's text, `Around(focused game)`;
  near either end of the window, a window around the focused game. The band
  travels in absolute places (`firstIndex` plus the card), so a new window
  does not move it.
- `Overlay` draws PAUSED from `overlay()`.

## Checks

```sh
cd clients/shell
bun test packages/zone/test            # gates, plans, contrast, QR (142 tests)
bun run zone:typecheck
bash packages/zone/test/bite.sh        # each gate fails on a planted defect
cargo test --release --features fixture --test zone_package   # previews at five sizes; focus in view
korri-shell verify packages/zone       # the install-time verifier, 0 / 12 / 10,000 games
bash packages/zone/preview/probe.sh PROBE   # the shell's probe (built surface-probe) over every scenario model, five sizes
nix run .#zone-check                   # the bun checks and logic.js freshness
```

## Limits

Each `view()` call runs under the shell's 50 ms budget, and the RG353M at
408 MHz runs Zone's logic about 28 times slower than an i9-11900H.
`tests/zone_package.rs` (`view_calls_stay_well_inside_the_budget_on_a_slow_clock`)
measures thread CPU time over 10,000 games at 640x480 (#368), with the
existing 28× workstation-to-device factor and /2 CPU allowance. Instruction
counts are optional diagnostics, not pass marks. The device's measured time: the attract's next game about 7 ms there (it patches the last
view), a move along the band about 12–13 ms. A call's JSON is about half
of it, and it grows with the number of objects, not bytes: the view sends
whole cards only for the band's pool and the attract's game (and none past
them), every mark but the pool's as one shared empty target, a hidden layer
as `{ shown: false }`, and no SAVE DATA or first-boot parts on other pages.

## Differences from the web Zone

See the report of the conversion (stage `native-zone`); the lasting ones:

- Stairs: every 45 degree edge is one straight line in all three looks
  (Zone's pixel looks cut them as one-art-px stairs).
- PAUSED's game art is muted by the veil, not turned grey (Slint has no grey filter).
- A section spine's caption stands letter under letter (Slint draws no rotated text).
- Timers come from the shell's Tick (contract 7.1, once a second): the idle
  attract, and the results tally's end of count-up and its 8 s close (to
  the nearest second). The tally's count-up and the launch card's sweep
  are drawn on the view's own clock, and the moments leave at once.
- The shell's keyboard has `S` for the system button (gamepads have none).
