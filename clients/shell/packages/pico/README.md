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

## Launch settings (ADR 0066)

A game's launch settings are one list, the same for a game that runs here and
a game that another device runs. Pico reads `groups`, `changes`, `keep` and
`runsOn` (`src/pico-launch-view.ts`) and nothing of the old places: no layer
spines and no runner scope tabs.

- One card holds every setting once. The first row of a setting section
  stands under the section's caption (VIDEO, AUDIO, STREAM).
- A chip at the card's top says where the game runs, in Korri's words. On a
  short, wide screen (1024x240) the floor row says it instead, beside the
  lead: it has the width and is always in sight (`launchWhereOnFloor` in
  `src/native/panel.ts`). The status bar shows the game's title whole or not
  at all; when it cannot, the card says the title whole over the chip.
- A changed row says its earlier value under its plate, after a yellow WAS
  chip, focused or not.
- Only the focused row says where its value stays (`source`) and Korri's
  `description`. Those words wrap. The focused row also says whole, under
  its plate, a label or value the plate had to cut.
- A row with no `interaction` is a fact: it takes the cursor, and its
  description says why it cannot change.
- The keep row shows only while `changes` is not empty. It stands under the
  body and over the floor, outside the scroll, so it is always in sight and
  never covers a row. Its plate is yellow: the screen's one filled control.
  A press opens Pico's list of the places on the one in force; the place
  chosen goes to `ChangeSetting` with the keep row's id.
- The floor's lead is Korri's first action that is not Close: RELOAD, or
  RETRY when Korri reports an error. Back runs Close.
- An error that still carries rows (the device that runs the game cannot be
  reached) keeps the list: Korri's message stands on a red plate at the
  card's top, the cursor starts on RETRY, and this device's own rows still
  change. Loading, a conflict, and an error with no rows are Pico's cards, as
  before.
- A bar down the body's right edge says the list holds more than it shows.
- Focus: `rows`, then `keep`, then the `floor`. The list also names the row
  the cursor is on (`panel.cursor`), so a row far out of sight comes back
  into it.

At 320x240 the body holds about one row with its lines. The card's first
lines (the title, where the game runs, Korri's message) then stand above the
first row and leave the view when the cursor is on a row; a finger scrolls
them back. Under an error the cursor starts on RETRY with them in view.

`test/fixtures/launch-settings.ts` holds hand-written models that follow the
treaty (korrid does not publish this model yet). `test/launch-settings.test.ts`
covers the view, the focus rules and the scroll rule.
`bun preview/launch-previews.ts` writes the page's preview states from the
views `logic.ts` makes of those models.

## Layout

| Path | What |
|---|---|
| `main.slint` | The binding: the one file that knows the view. `Surface` (catalog and setup) and `Overlay` (pause menu, see-through over the running game: ADR 0053). |
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
- **Touch (ADR 0055).** A tap or click on a focus target reports its
  `focusId` through the root's one `activated(target, at, released)` callback
  (`ui/atoms/PicoTap.slint`, threaded up every layer); the shell sends
  `Activated`, and `update` moves the cursor there as a d-pad move would, then
  does what Confirm does. An id the screen shown does not list (stale, or a
  disabled control) does nothing; on attract the first tap only wakes. The
  hints (`hint:a`, `hint:b`, `hint:left`, `hint:right`) are targets in their
  own `hints` region that the cursor never walks onto: A runs Confirm, B runs
  Back, and ◀▶ ADJUST is two targets, `hint:left` and `hint:right`, one per
  half. A hint not drawn now is stale. A range in the
  pause menu takes the finger's place on its cells (`at`), snapped to Korri's
  step and sent when that value changes. `Touched` (the shell sends it as any
  finger goes down) hides the focus highlight (`touching`, read as
  `Tokens.ring`) until the next `Pressed`, which also brings the focused
  control back into sight; `focused` stays on the real focus, and `Touched`
  wakes nothing by itself. A native cancel completes no gesture. PicoScroll and PicoRail are Flickables:
  a drag scrolls them and moves no focus. The shelf, RESUME and both results
  racks follow a sideways drag or fling item by item (`ui/atoms/PicoSlide.slint`)
  and send `dragged(region, by)`; `Dragged` walks the cursor `by` targets in
  that region with the d-pad's move path, so `enter` and catalog paging run as
  a walk does.
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
  `results`, `search`, `play`, `actions`, `spines`, `rows`, `keep`, `answers`,
  `list`, `pack`, `go`. Inside a region a press moves to the nearest target
  ahead. Leaving one follows `picoExits` in `src/pico-display-plan.ts`,
  which the plan computes per size (spines beside the rows or above them).
  `view()` emits the convention: `focusRegion` on every entry that is a
  target, `focusActive` on a region's active one (the chosen cart, the open
  tab, the chosen order), and `focusPlan`. An entry that is no target drops
  `focused`. The shell strips these keys before Slint.
- **Status strip (#378).** Over a running game the overlay shows Korri's
  facts on a black plate top right, beside the card: the time, the battery
  (PicoBattery, cells of five), round trip, video bitrate and dropped frames
  (`picoStatusStrip` in `src/pico-overlay-view.ts`). It takes one line, two,
  or one fact a line, whichever fits; when the card takes the whole width it
  goes to the foot. A fact Korri does not give shows `--`, quiet; a measured
  0 is 0. It is no focus target. The viewer's "Stream controls" group (#379)
  is drawn as any other group; the list shows a scroll bar when it holds
  more than it shows, and a long value ends in "…" before its label.
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
| `pico-touch` | Activated moves focus and confirms; stale ids and attract; the highlight hides and shows; hints; ranges at the finger; Dragged walks and pages as the d-pad does. |
| `focus-visible` | Every focus target the logic reaches (breadth-first over every button, on 21 models and one with a 1920x1080 display, with `Resized` at each size) is drawn inside the window at those five sizes. `test/walk-plan.ts` runs the same walk on the logic alone first: each model walks, at each size, only as far as its last target no earlier model reached, and the probes run side by side (`PICO_WALK_JOBS`, `PICO_FOCUS_STATES`). |

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
the worst call of each kind. The scenario is deterministic, so the
collection lands in the same call in every run: the test runs it three
times, once collecting inside calls as the shell does and twice collecting
between calls, and fails when half or more of a kind's calls held a
collection. The median is then each call's own cost (#426). `long_lists_keep_the_cursor_in_sight` (same
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
