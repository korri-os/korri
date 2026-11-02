# Pico

Korri's fantasy-console surface as a native surface package
(`docs/briefs/2026-10-31-native-surfaces-contract.md`): PICO-8's sixteen
colours on black, pixel faces with real upper and lower case, and a shelf of
cartridges that take the shape of their art. Every page the web Pico had is
here: home (shelf, grid, hero, MENU, PLAYING NOW, Korri's launch and failure
screens), Find, a game's own screen, Settings (Wi-Fi, identity, the editor),
launch settings, the runner picker, the store, first-boot setup, invites,
attract, the two-display showcase and the pause menu.

The web Pico (`surfaces/pico/`) was removed in the same cut. Its state, view
helpers, fixtures and behaviour tests moved here unchanged except for their
imports.

## Layout

| Path | What |
|---|---|
| `main.slint` | The binding: the one file that knows the view. `Surface` (catalog and setup) and `Overlay` (pause menu). |
| `ui/palette.slint`, `ui/tokens.slint` | The only globals: PICO-8's sixteen and Pico's roles; the scale (`Tokens`, from the window's width and height), the focus cycle, where the cursor stands (`Focus`). The ten faces are imported here. |
| `ui/atoms`, `ui/molecules`, `ui/organisms`, `ui/templates`, `pages/` | One component per file, one per React component of the web Pico at the same layer (`test/inventory.ts`), each with `<Name>.<layer>.preview.slint` beside it. |
| `logic.ts` → `logic.js` | `globalThis.surface`. Rebuild with `./build.sh` (bun). |
| `src/state`, `src/pico-*.ts` | Pico's behaviour, from the web Pico. |
| `src/model.ts` | The generated treaty as Pico's code reads it: the cursor window handed over as `catalog.games`. |
| `src/native/` | The adapter: Pico's own cursor (`focus.ts`), the screens as views and focus targets, requests to Korri, the QR code as a path. |
| `src/vendor/` | `qrcode`'s encoder and `dijkstrajs`, MIT, unchanged (a package ships no `node_modules`). |
| `fonts/` | The ten faces as TTF (converted from the web Pico's woff2) and the licences that travel with them. |
| `test/` | The ported behaviour tests and the gates (below). |
| `preview/` | Scenarios, the render script, the Caliper references and the side-by-side and five-size images. |

## How the contract is met

- **Input.** The shell sends buttons. Back, Options, Menu and System are Pico's
  host buttons, as before. Up, down, left and right move Pico's own cursor:
  each screen lays its focus targets out on a plan of its own, and the move
  is the portal's `spatial-focus.ts` rule (nearest in the pressed direction,
  off-axis distance counted three times), with a neighbour named outright
  where the geometry alone would jump (MENU back to the chosen cart). Confirm
  presses the focused target. A question traps the cursor, as
  `data-block-exit` did.
- **Catalog.** Find's query, collection and order go to Korri as
  `MoveCatalog` (`korri | title | mostPlayed | recent`); the window follows
  the cursor with `Around(gameId)` near either edge; a game's screen keeps its
  game in the window. `FocusGame` names the game under the cursor or on its
  own screen; its actions arrive as `model.focusedGame`.
- **Preferences.** `pico.font`, mark `device`, initial `tiny5`, stored with
  `RememberPreference` when Settings › PICO › Font changes.
- **Displays.** `displays: "all"`. With two or more displays the plan
  (`src/pico-display-plan.ts`) puts the showcase on the main one and every
  control on the other.
- **Cover art.** `{"$image": url, "pico8": "vivid"}`.
- **Time.** The shell sends `Tick { idleMs }` once a second (contract A1,
  7.1); attract starts when `idleMs` reaches 45 s (`pico-attract.ts`), and
  nothing in Slint decides idleness. An invite notice hides after 8 s in
  `main.slint` and ends at the next press in the logic. A range in the
  pause menu sends each step at once.
- **Back.** Before any of Pico's own Back (contract A1, 7.2): a status that
  carries `declineActionId` runs it; Busy with none swallows Back; anything
  else is Pico's own Back. Pico reads no meaning from action ids.

## Gates

Ported from the web Pico's gates and run with `bun test`:

| Test | Holds |
|---|---|
| `decomposition-gate` | Every React component is one Slint component at its layer, in its own file, with a preview that imports and draws it; imports point down; only the tokens and palette are globals; only `main.slint` and previews make windows. |
| `authoring-gate` | No colour outside the palette, which is PICO-8's sixteen; no blend; no pixel literal outside the tokens; the virtual pixel derives from width and height. |
| `contrast-gate` | A note on a card's plastic reaches 4.5:1; a card's large ink 3:1; components draw notes in the note role. |
| `previews` | Every preview state renders at 320x240, 640x480, 480x800, 1280x720 and 1920x1080. |
| `focus-visible` | Every focus target the logic reaches (breadth-first over every button, on 16 models) is drawn inside the window at those five sizes. |

The last two run the shell's headless probe
(`clients/shell/examples/surface-probe.rs`); build it with
`cargo build --release --features fixture --example surface-probe` in
`clients/shell`, or point `PICO_PROBE` at it.

## Preview

```sh
bun preview/scenarios.ts /tmp/gen                     # one Korri model per scenario
preview/render.sh SHELL FIXTURE_KORRID /tmp/gen OUT    # the real shell under Xvfb, at the six Caliper devices
surface-probe previews pico --sizes 640x480 --out OUT  # every preview state, headless
```

Each `*.preview.slint` also renders with `slint-viewer --component STATE --screenshot out.png FILE`.
