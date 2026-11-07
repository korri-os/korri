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
| `ui/palette.slint`, `ui/tokens.slint` | The only globals: PICO-8's sixteen and Pico's roles; the scale (`Tokens`, from the window's width and height) and the focus cycle. The ten faces are imported here. |
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
- **Window (contract A2).** The shell sends `Resized { width, height }`
  before the first view and on every resize. `picoLayoutPlan` in
  `src/pico-display-plan.ts` turns the window into one plan: the virtual
  pixel, the grid's columns, whether settings' spines stand beside the card
  or above it as tabs, and whether a game's screen stacks its cart over its
  facts. The focus graph moves by that plan, and `view()` hands its
  decisions to main.slint (`home.columns`, `panel.beside`,
  `detail.stacked`), which draws them and decides no structure itself.
  `displays` only place the two screens of a split. A resize keeps the
  cursor on its control.
- **Regions and exits (contract 8.4).** Every focus target belongs to a
  region (`src/native/focus.ts`): `floor`, `menu`, `invites`, `shelf`,
  `grid`, `hero`, `resume`, `card`, `stage`, `sections`, `orders`, `keys`,
  `results`, `search`, `play`, `actions`, `spines`, `scopes`, `rows`, `answers`,
  `list`, `pack`, `go`. Inside a region a press moves to the nearest target
  ahead. Leaving one follows `picoExits` in `src/pico-display-plan.ts`,
  which the plan computes per size (spines beside the rows or above them).
  `view()` emits the convention: `focusRegion` on every entry that is a
  target, `focusActive` on a region's active one (the chosen cart, the open
  tab, the chosen order), and `focusPlan`. An entry that is no target drops
  `focused`. The shell strips these keys before Slint.
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
| `layout-plan` | The plan from the window alone at the five sizes; the grid's rows and the spines' arrangement drawn as the plan says and moved through by it; a resize keeps the cursor. |
| `focus-convention` | On the logic alone, at 320x240 and 1280x720: every entry with `focused` names its region, one active target per region at most, every exit leads to a region the view has, and every press that leaves a region goes where `focusPlan` says. |
| `focus-visible` | Every focus target the logic reaches (breadth-first over every button, on 17 models and one with a 1920x1080 display, with `Resized` at each size) is drawn inside the window at those five sizes. `test/walk-plan.ts` runs the same walk on the logic alone first: each model walks, at each size, only as far as its last target no earlier model reached, and the probes run side by side (`PICO_WALK_JOBS`, `PICO_FOCUS_STATES`). |

The last two run the shell's headless probe
(`clients/shell/examples/surface-probe.rs`); build it with
`cargo build --release --features fixture --example surface-probe` in
`clients/shell`, or point `PICO_PROBE` at it.

## Limits

Each `update`, `view` and `overlay` call runs under the shell's 50 ms budget
(contract 3), and the RG353M idles at 408 MHz. `tests/pico_package.rs` in
`clients/shell` (`calls_stay_well_inside_the_budget_on_a_slow_clock`) measures
the thread CPU time of every call the shell makes over 10,000 games at
640x480 (#368). It keeps the existing 28× i9-11900H-to-RG353M factor and
/2 CPU allowance, and holds each kind of call under 10 ms on the device
(the median, as Zone's test), but for these (`limit_ms`). Instruction counts
are optional diagnostics, not pass marks:

- A window of games not seen before (the library's first arrival, the shelf
  or the hero after a change of order): about 11-14 ms, as every cart it
  shows is worked out then.
- Find and the grid: their views are bound by their size, not by the logic.
  QuickJS writes JSON by the value (about 42,000 instructions a cart), and
  Find sends its keyboard (351 values), the grid the window's 50 carts (at
  640x480 every row is within reach of the cursor, so none can be left
  out). Find's views are about 13-20 ms, the grid's 17-22 ms.

QuickJS collects its garbage inside whichever call crosses its threshold,
which may add up to about 9 ms to that call; the test allows 10 ms more for
the worst call of each kind. `long_lists_keep_the_cursor_in_sight` (same
file) walks the shelf, the hero's RESUME list, the grid and Find's results
far past their first screen and fails a press whose control is out of sight.
Find's results at 320x240 stay out of sight (on main too) and are not walked
there until that is fixed.

QuickJS walks and builds objects slowly and writes JSON by the value, so the
logic does as little of each as it can:

- A publish keeps every part of the model that did not change, and each game
  of the window, as the last model's objects (`src/native/intern.ts`): a game
  is compared field by field, any other part by its JSON text. The screen,
  the carts and the view built from them are reused.
- A game's shelf conversion, its cart, its target and the chosen game's
  stage are kept per game object, so they last across windows.
- The shelf sends only the carts the rack can show: those a screen's width
  from the chosen and focused carts (`shelfReach` in `src/native/catalog.ts`),
  with `firstIndex` and `selectedIndex` counted from the first sent. Find
  sends only the results its rack can show around the focused one and the
  one the cursor comes back to (`resultReach`), with `firstResult` and
  `resultCount` (PicoLibraryBrowser).
- The hero's RESUME list is a rack that moves only when the cursor passes
  its right edge (PicoResumeList), and it sends only the carts it can show
  around the cursor's (`resumeReach`), with `resumeFirst`, `resumeCount`
  and `resumeFocused`. The rack keeps the place the row of all those carts
  took, with titles of the design's two lines, whichever carts are sent.
  (Before, RESUME never moved: the cursor walked into carts clipped out of
  sight.)
- Find's keys, their views and targets, the grid's and RESUME's targets and
  entries are kept per layout or game.
- A layer not drawn is not sent: MENU's list while it is closed, the hero
  and the launch stage under another body, home's card unless it is the
  body, the split's rectangles on one screen, and the page under the
  attract. The shell keeps a field it is not sent, and a layer is sent whole
  whenever it shows.
- Marking the focus convention (`markIn` in `src/native/program.ts`) is
  remembered per part of the view, and a cart is copied whole.

## Preview

```sh
bun preview/scenarios.ts /tmp/gen                     # one Korri model per scenario
preview/render.sh SHELL FIXTURE_KORRID /tmp/gen OUT    # the real shell under Xvfb, at the six Caliper devices
surface-probe previews pico --sizes 640x480 --out OUT  # every preview state, headless
```

Each `*.preview.slint` also renders with `slint-viewer --component STATE --screenshot out.png FILE`.
