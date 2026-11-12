# Velvet (native surface package)

A dark house, lit by the game in front of you. Velvet is the surface every
Korri product device opens (#424); Pico and Zone stay installed and
selectable. The owner's decisions are in #423; the direction, its motion
table and its capability map are in `docs/design/surfaces/velvet/DIRECTION.md`.

| Path | What |
| --- | --- |
| `main.slint` | The binding: `Surface` and `Overlay`. The only file that knows the view. It feeds the `Tokens` global the window's size, the plan, `still` and `touching`, and hands each page plain properties. |
| `ui/tokens.slint` | `global Tokens`: colour, type, shape, motion, and the few sizes that do not scale. Every raw colour and every pixel literal is here and nowhere else. |
| `ui/types.slint` | The view's structs. `Screen` is the plan (`src/plan.ts`), field for field. |
| `ui/atoms`, `ui/molecules`, `ui/organisms`, `pages/` | One component per file. Each has its preview states beside it, `<Name>.<layer>.preview.slint`, written by `preview/previews.ts`. |
| `ui/preview-frame.slint` | The window a preview state inherits. |
| `logic.ts` → `logic.js` | `init`, `update`, `view`, `overlay`. `bun run velvet:build` in `clients/shell`; `velvet-check` rebuilds it and fails on any difference. |
| `src/plan.ts` | The plan: layout as a pure function of (width, height). |
| `src/update.ts` | Every message. A row, a button and a key carry an `Act` (data); `perform` is the one place an act is done. |
| `src/view.ts` | The view, from the state and the model. |
| `src/browse.ts`, `src/lists.ts` | Home, the row, the lobby, a set, the detail; the lists home keeps over the one catalog window. |
| `src/pages.ts`, `src/settings.ts`, `src/curtain.ts`, `src/keys.ts` | Every other screen Korri publishes, as one list page; status and questions as the curtain; the keyboard. |
| `src/art.ts`, `src/poster.ts`, `src/colour.ts` | Which picture a tile shows, the pixel budget, which no-art card a game gets, the stage light. |
| `drawings/` | The no-art cards, drawn at build time. `draw.ts` is the geometry (a shape names a palette place, never a colour), `palettes.ts` the colours, `build.ts` writes the two files below. `bun packages/velvet/drawings/build.ts` in `clients/shell`; `velvet-check` runs it and fails on any difference. |
| `ui/drawings.slint`, `src/drawings.ts` | Generated. The view's side: `global Drawings`, every card's layers and every palette. The logic's side: each drawing's number, its title box and its ink rule. |
| `src/systems.ts`, `src/drawings-slots.ts` | The machines Velvet has a drawing for; the palette's places. Shared by the logic and the build. |
| `src/memo.ts` | The one rule for everything kept between calls: a kept answer is found by all it depends on, exactly, and forgetting it changes only time. Every module says there how to forget what it keeps. |
| `src/fit.ts`, `src/measure.ts`, `src/words.ts` | Fitting caps titles and fact lines from the fonts' advance tables; words about time and size. |
| `src/qr.ts` | The backup's QR code (own encoder, held equal to `qrcode` by test). |
| `fonts/` | Big Shoulders ExtraBold (display, caps), Archivo semi-condensed 400 and 700; their OFL texts; `fonts/source/` the scripts that cut them. |
| `test/` | The gates and tests (`bun test packages/velvet/test` in `clients/shell`). |
| `preview/` | `scenarios.ts` → `scenarios/*.json` (99 models with the presses to each screen), `fixtures/korri.json` (Korri's slices), `art/` (self-drawn test pictures and `build.py`), `previews.ts`, `shots.sh` (the real shell under Xvfb), `probe.sh` (the shell's probe over every scenario). |

## The plan

`plan(width, height)` (`src/plan.ts`) is the whole layout. The view sends it
as `screen`; `Tokens.screen` holds it; components read sizes from it. Named
thresholds decide structure, never a device:

| Name | Value | What changes |
| --- | --- | --- |
| `NARROW_RATIO` | width / height under 0.9 | The pick stacks under the hero's band; buttons stand one under the other; a list page's panel stands over its rows. |
| `PANEL_BESIDE_RATIO` | 1.25 and at least 560 px wide | A list page's panel (the backup's QR code) stands beside the rows. |
| `EMPTY_WIDE_RATIO` | 1.5 | The empty library's ghost tiles stand beside the invitation. |
| `MOST_COLUMNS` | 5 | The most tiles a set shows across (the pixel budget, below). |
| `SHELF_SLOTS` | 5 | Games on a lobby shelf before "See all". |

What yields first when the window gets small: the library's peek under the
row, then the row's height, then a logo's size, then lines of facts (three at
most), then a curtain's words (they page, "More above / More below"). A
button never yields: when a curtain cannot fit, it stands on the window's
foot and its words give way.

Proven at 1920x1080, 1240x1080, 640x480, 480x800, 320x240 and 1280x720, and
by a sweep from 320x240 to 2560x1440 (`test/plan.test.ts`).

## Screens

Every screen Korri publishes, and where its preview is. A scenario is
`preview/scenarios/<name>.json`; `preview/shots.sh` draws it through the real
shell, `VELVET_FRAMES` (below) through the test harness.

| Screen | Scenarios | Component previews |
| --- | --- | --- |
| Home: the pick | `home`, `home-no-art`, `home-fresh`, `home-stale`, `home-undated`, `home-one`, `home-menu-pill` | `pages/VelvetBrowse` (Home, HomeNoArt, OneGame), `VelvetPick`, `VelvetStage`, `VelvetMarquee` |
| The row under the pick | `row`, `row-no-art`, `row-all` | `VelvetBrowse.Row`, `VelvetTile` |
| The lobby | `lobby`, `lobby-no-art`, `lobby-not-started`, `lobby-peer`, `lobby-systems` | `VelvetBrowse.Lobby`, `VelvetLobby` |
| A set | `set`, `set-no-art`, `set-deep`, `set-system`, `set-peer`, `set-pill`, `set-sorted` | `pages/VelvetSet`, `VelvetGrid` |
| Sort and filter | `set-sort` | `VelvetSheet.Sort` |
| The detail (Cinema) | `detail`, `detail-no-art`, `detail-poster`, `detail-long-title`, `detail-two-places`, `detail-peer` | `pages/VelvetDetail`, `VelvetPick.Cinema` |
| A game's other actions | `detail-actions`, `detail-actions-asking` | `VelvetSheet.Actions` |
| No-art cards | `home-no-art`, `row-no-art`, `lobby-no-art`, `set-no-art`, `detail-no-art` | `VelvetPoster`, `VelvetTile.NoArt` |
| Empty library, loading, error | `empty`, `empty-no-store`, `loading`, `error` | `pages/VelvetHouse`, `VelvetEmpty` |
| The house menu | `menu`, `menu-invite` | `VelvetSheet.Menu` |
| Status: Busy, steps, progress | `busy-plain`, `busy-steps`, `busy-progress`, `running`, `ended`, `paused-stream` | `VelvetCurtain`, `VelvetSteps`, `VelvetBar` |
| Problem, Confirm, ChooseSave | `problem`, `problem-long`, `confirm`, `choose-save` | `VelvetCurtain` |
| Settings and every interaction | `settings`, `settings-choice`, `settings-text`, `settings-text-symbols`, `settings-secret`, `settings-destructive`, `settings-saving`, `settings-problem`, `settings-foot` | `pages/VelvetList.Settings`, `VelvetRow`, `VelvetKeyboard` |
| Wi-Fi and hidden networks | `wifi`, `wifi-password`, `wifi-hidden`, `wifi-failed`, `wifi-none` | `VelvetList.Wifi`, `VelvetSignal` |
| First boot | `first-boot`, `first-boot-wifi`, `first-boot-names`, `first-boot-starter`, `first-boot-ready` | `VelvetProgramme.FirstBoot` |
| Identity and backup | `identity`, `identity-last`, `identity-backup`, `identity-password`, `identity-working`, `identity-problem`, `identity-switched`, `identity-retired` | `VelvetList.Identity`, `VelvetProgramme.Backup`, `VelvetQr` |
| The store and its search | `store`, `store-none`, `store-search`, `store-results`, `store-offer`, `store-confirm`, `store-downloading` | `VelvetList.Store`, `VelvetProgramme.Store`, `.Offer` |
| Invites | `invites`, `invite-notice` | `VelvetList.Invites`, `VelvetNotice` |
| Saves, the runner, launch settings | `saves`, `saves-empty`, `saves-restore`, `runner`, `launch-settings`, `launch-changes`, `launch-keep`, `launch-streamed`, `launch-unreachable`, `launch-conflict`, `launch-loading`, `launch-empty` | `VelvetList.Saves`, `.Runner`, `.LaunchSettings` |
| The gameplay overlay | `overlay`, `overlay-groups`, `overlay-stream`, `overlay-asking` | `pages/VelvetPaused`, `VelvetPause` |

## Contract use

- `preferences`: `velvet.motion`, mark `device`, `full` or `reduced`; stored
  with `RememberPreference` from Settings › Velvet › Motion. Reduced lands
  every change at once (`Tokens.still`) and the stage never drifts.
  `displays: "main"`.
- One catalog window. Home needs several lists and has one window and one
  query at a time. `src/lists.ts` asks one after another, one question an
  update: `recent` from the start, `mostPlayed`, `recent` from the end,
  `korri` from the start and from the end, the pages and the games around
  the first one not yet started (below), one list a peer, then `recent`
  from the start again, the window Velvet rests on. An answer belongs to the
  question that asked for it: a page is known by the place in the order its
  cursor stood at (Korri gives no cursor back), a game's surroundings by the
  game. A pass is six questions in a small library and one more for each
  peer (six at most), up to three pages for peers, and the questions of
  "Not started". A set asks for a window of 60 and asks again `Around` the
  focused game near the window's edge.
- What a publication is. Velvet draws games that are not in the published
  window (the pick may be the game played most, long ago), and the treaty
  has no mark for "a game changed". So Velvet tells a publication by what it
  can see. Its own answer: the window answers the question out, or
  `focusedGame` names another game. The clock: `clockLabel` changed. Neither:
  Korri changed something out of sight, and the lists are read again, each
  once, which reads every game the home and the lobby draw; at most once in
  ten seconds of Korri's time, the next publication after that reading what
  came between. Its own answers and the clock ask nothing. A game in the
  published window is replaced at once in every list that holds it
  (`refresh`), with no question. An open set and the detail over it draw
  from the published window itself.
  What this cannot see: a change that arrives in the same publication as a
  clock change or as one of Velvet's own answers waits for the next
  publication that is neither. A revision mark on the catalog in the treaty
  would close that and save the pass ("Asked of the shell").
- Not started holds "the first one, in Korri's order" (contract section 5).
  korrid sorts `recent` by the last session and keeps Korri's order among
  equals, so the games with no session stand last there, in Korri's order.
  `recent` from the start shows where they begin when fewer games were
  played than it holds; `recent` from the end shows it when they all fit in
  it. Else Velvet reads on from both ends, a page of 200 each in turn, each
  page its own question: Korri's order from its start until a game with no
  session comes, `recent` back from its end until a played game comes. The
  first to show it ends both, so it costs twice the shorter way at most, and
  there is no limit of pages. Then `recent` is read around that game: the
  shelf, the row's last place, and where "See all" opens. Requests for a
  pass (`test/fresh.test.ts`): 80 played then 300 unplayed, 8; 80 then
  10,000, 8; 10,000 unplayed among the first of Korri's order, 7; every game
  unplayed, 6; one unplayed game, 6; 3,000 played then 500 unplayed, 12.
- "Now" is `model.publishedAt`. It can be a minute behind, so a session that
  ended after it is "Just now". Without it a fact is a date and the pick is
  the last game played. Dates are read in UTC (`getUTC*` only; a surface has
  no zone), and a save says how long ago it was made ("3 h 20 min ago"),
  never a clock time. `test/words.test.ts` runs the words under four `TZ`
  values and expects the same text.
- Launch settings (ADR 0066) are one list: where the game runs under the
  title, each setting once, a changed row's earlier value beside its value,
  and, while a row is changed, a last row for where the changes are kept.
  Every id goes to Korri unchanged.
- `FocusGame` whenever the game in focus changes; the detail's More and
  `options` list `model.focusedGame.actions`.
- Focus (contract 8.4): every target carries `focused`, `focusRegion` and a
  `focusId`; `focusPlan` names a region's exits only to regions with targets
  in that view. Regions: `menu`, `pick`, `row`, `shelf:<key>`, `pill`,
  `grid`, `detail`, `empty`, `rows`, `buttons`, `sheet`, `keys`, `curtain`.
- Touch (ADR 0055): a `VelvetTap` over every target reports its id through
  the root's one `activated`. A game tile takes the focus on the first tap
  and opens on the second. Shelves and the row drag by whole tiles, a set by
  whole rows (`dragged`). `Touched` hides the highlight until the next
  button. A touch screen has no Back button: while touch is in use a round
  chip stands in the lower right corner (`VelvetBack`, target `back`), and a
  tap beside a sheet or over the keyboard's field is Back too.
- `Tick`: the stage drifts after two seconds of rest and stops on any press.
- Images (ADR 0065): a picture is empty until it arrives. The no-art card is
  drawn under every picture, so there is never a hole. Only the detail cuts
  the hero to fill; everywhere else it keeps its shape, as a band across the
  top that falls off into the dark.
- A plugin's secret (`plugin-credential:<id>`, `sensitiveText`) is a Settings
  row: Configured or Not configured, edited on the keyboard with its text
  hidden, with "Clear key".

## The stage

Nothing on the stage is blurred, live or not; the shell's renderer has no
blur and the authoring gate forbids it. From the back:

1. House down, the window's ground.
2. The stage light (`VelvetLight`): two radial gradients in the focused
   game's colour, a large pool up and to the right and a low dim one at the
   left foot. Each is a square far larger than the window that fades to
   nothing inside itself. A second light cross-fades in when the game
   changes. The colour is the art's (`color` of the hero, else the poster),
   else one of twelve gels by the title, at a brightness that keeps House
   light at 4.5 to 1 on its core (`src/colour.ts`).
3. The hero, when there is one: a band across the top in its own shape,
   with a linear gradient that takes its foot into the dark; the detail
   cuts it to fill instead.
4. The vignette (`VelvetScrim`), one overlay the size of the window at
   every size: a radial fall-off towards the corners, a linear gradient
   from the foot, and on a wide window a linear gradient from the left
   edge, where the words stand. A narrow window (480x800) has the first
   two; its words stand under the picture.
5. The house lights (a flat veil whose weight says how far the person is
   from the game), and once, when Velvet opens, the lights coming up.

`the_backdrop_has_no_seam` (`clients/shell/tests/velvet_package.rs`) holds
every part to the window's edges: at twelve sizes, six of them odd, no
backdrop pixel differs from its neighbour by more than four levels.

## Limits

- **Pixels.** The shell keeps 24 MiB of decoded pixels for a surface. A
  stored poster is 600x900: 2.1 MiB. So a view asks for its pictures in
  order of importance (`take`, `src/art.ts`, into the count the view makes
  and hands down: the hero, the focused game's
  logo, then tiles from the focused one outwards, rows in sight first; in
  the library the shelves before the home's row, which has left the stage)
  and stops at 23 MiB. A tile over the budget shows its no-art card. A set is
  five tiles across at most and a shelf five games, so every tile in sight
  has its picture. The mockup's six across cannot have that while the shell
  hands a poster over at its stored size (see "Asked of the shell").
- **Calls.** Every `view` and every `update` over 10,000 games is under
  50 ms on the slowest device's clock, the first sight of a screen included:
  there is no allowance for a first call. The test
  (`view_calls_stay_well_inside_the_budget_on_a_slow_clock`) runs a tour
  twice on seven fresh engines and holds every single `view()`, and the
  longest `update()` of every step, to the budget by its median over the
  engines; it multiplies this workstation's CPU time by 28, the factor Zone
  measured. Two full runs of the test binary, the same logic: with this
  machine quiet a view was 5 to 22 ms there (medians; the worst single call
  24 ms), an update under 11 ms, and the update that keeps a window of 200
  games while the home reads its lists again 20 to 23 ms; with the machine
  busy (the logic took twice as long to load) the same calls were up to
  30 ms, 12 ms and 27 ms. The heaviest are the first sight of the lobby and
  of a shelf that comes into sight: the cards made there.
  - About half of a view call is its JSON, and that follows the number of
    values. A view holds what is on the stage and no more (the largest is
    under 13 kB): the shelves in sight, the set's rows in sight and one each
    way, for a tile only what its kind draws, for a line of facts only the
    facts it has.
  - The other half is making the view. A game's tile is made once and kept
    by the game's id while Korri says the same of the game; a tile at its
    place is kept too, so a move makes the two tiles the focus left and
    landed on and every other tile is the object the last view sent. What
    remains is the first sight of a card (its title fitted to its drawing's
    box): about 1 ms a card on that device.
  - A no-art card costs nothing to draw: it is a drawing and a palette by
    number (`drawings/`). `init` warms nothing.
  - Korri publishes with every move of the focus. A publication is read by
    its window's games (60), each against the one object kept for that game
    (`src/lists.ts`).
- **Motion.** A set's rows and the lobby's shelves move by whole steps. The
  software renderer has no blur, no shadow, no rotation, and does not clip a
  picture to a radius: tiles have square corners and a gradient shade.

## What the interpreter taught us

Slint 1.18.1, interpreted, with the shell's binder. The gates hold the ones
a pattern can see; `velvet_package.rs` holds the rest.

1. Animate only an element's own geometry, opacity or ground, and write the
   animated binding out from the row's own data, `Tokens`, its own geometry,
   and `in` properties that do not hang on row data. An animated binding that
   reads a property worked out in the component, or an `in` property bound
   to an expression over a row (a tile's `rests` was one), loses its value:
   the element stands at 0, or the interpreter panics at `rtti.rs:370`. What
   a tile needs for a motion is a field of the tile.
2. An animated place must not hang on a struct the view sends. The shell
   sets a struct again with every view that changes any field of it, and an
   animated binding under it starts again from zero when that happens twice
   before a frame. So the set's first row is a top-level number
   (`setFirstRow`), and so are the lobby's focused shelf (`shelfAt`) and
   home's depth (`homeDepth`): the shell sets a top-level property only when
   it changes. Moving each tile by itself instead loses the labels' place.
3. Every list is a top-level property of the window. A list inside a struct
   can miss an in-place row update. Lists inside list rows are fine.
4. A field the view leaves out of a struct keeps the value its list row had
   before; the shell converts from the previous value. So `focused` counts
   only on what is a target now (`focus-id` set), and a place with no tile
   says `shown: false`.
5. A list row is the same element for as long as it keeps its index. So a
   tile keeps its place in its list: shelves send every place (one out of
   sight is `{ shown: false }`), and a set's tiles stand in slots by place.
6. `row`, `col` and `top` are taken as property names. There are no hex
   integer literals. `viewport-*` is deprecated for `content-*`.
7. A motion starts when its place is first read, and a repeater makes its
   rows when the window draws: a test draws, steps the clock, and draws
   again before it reads where anything is.
8. A value set in `init` is the first value: nothing moves to it. A motion
   on arrival needs a change after the first frame (a `Timer`, as the house
   lights use).
9. A view that never changes is drawn once, and under X11 a window still on
   its way to the screen can lose that one frame: the empty library and the
   overlay stayed black until a press. The house lights (`t-open`, 700 ms)
   and the overlay panel's arrival draw the first view more than once.
10. A part with no `x` or `y` of its own stands in the middle of its parent.
    The vignette's left-edge gradient (three fifths of the window wide) had
    none, stood from a fifth of the width to four fifths, and left a seam
    down every wide stage. The fade for "more above" in a scrolled list had
    no `y` and lay across the middle of the list; the light on a card's head
    lay across its middle. A gate now refuses a gradient that names no `y`
    and does not fill its parent (`test/authoring-gate.test.ts`).
11. A part that starts on a fraction of a pixel and runs past the window's
    right edge leaves the window's last column undrawn. The stage's large
    gradients stand on whole pixels.

## Asked of the shell

Not Velvet's to change; reported to the coordinator.

- Pictures at the size they are drawn. A grid of posters needs a fifth of
  the pixels the shell decodes for it. With that, a set could be six or
  seven across with every picture.
- A mark in the treaty for "the catalog changed" (a revision on
  `SurfaceCatalog`, or on each game). Without it Velvet cannot tell a
  publication that changed a game out of its window from one that changed
  nothing it reads, and reads its lists again after any publication it
  cannot explain ("What a publication is"). A cursor echoed in the window
  would also let an answer be matched to its page without counting places.
- A treaty flag for "plays on another device". Velvet tells a peer's shelf
  by its section not being Korri's "Continue" or "This device".
- The fixture catalog orders `mostPlayed` by `playCount`; korrid orders it by
  time played; the treaty's comment says by recorded sessions.
- A struct field a view leaves out keeps its row's old value (4 above), and
  the probe's `selected_rect_is_visible` takes the first element whose
  `accessible-item-selected` is true, target or not. A package must gate
  that flag itself. The contract does not say so.
- Under X11 the shell does not draw again when its window is first shown
  (9 above).

## Checks

```
nix run .#velvet-check                 # the gates and tests, typecheck, freshness
cd clients/shell
bun test packages/velvet/test
bun run velvet:typecheck
cargo test --release --features fixture --test velvet_package
cargo run --release -- verify packages/velvet
bash packages/velvet/preview/probe.sh  <surface-probe> [MAX] [JOBS] [SCENARIOS...]
bash packages/velvet/preview/shots.sh  <korri-shell> <fixture-korrid> OUT [SIZES] [SCENARIOS...]
```

`probe.sh` runs the shell's own probe (where each press goes, and a tap on
every target) over scenarios' models. Over all 99 it takes hours; these 35
have the models that differ: `home home-no-art set-peer home-fresh home-one
home-stale empty error loading first-boot first-boot-starter wifi wifi-none
wifi-failed store store-results store-offer store-downloading identity
identity-backup identity-retired saves runner launch-settings launch-changes
launch-unreachable overlay
overlay-groups busy-steps problem-long confirm choose-save invites
settings-problem ended`.

`velvet_package.rs` draws every preview state at five sizes, plays every
scenario at six and checks the focused target is all in sight after every
press, holds the backdrop free of seams, holds a system's set to that
system's games, and times every `view()` and every step's longest
`update()` of a tour over 10,000 games.
`VELVET_FRAMES=DIR VELVET_SIZES=WxH,... VELVET_ONLY=a,b` writes scenarios'
last frames; `VELVET_PREVIEW_OUT=DIR` writes every preview state.

| Gate | Holds |
| --- | --- |
| `authoring-gate` | No raw colour or pixel literal outside the tokens; no blur, shadow or rotation; no saturated blue; animations only of an element's own geometry, opacity or ground; a gradient names its `y` or fills its parent; logic is pure, reads no local time, and imports only the treaty's types. |
| `atomic-gate` | One component a file, named as its file; imports point down; only `main.slint` binds. |
| `decomposition-gate` | Every component has its preview; none is unused; `Screen` is the plan, field for field. |
| `contrast-gate` | The house's pairs; a title and its facts on any game's light. |
| `touch-gate` | `activated` and `dragged` reach the root from every instance; the gesture rules; the way back. |
| `scenarios` | Every scenario at six sizes: the focus convention, the pixel budget, the view's size, the view's keys; no negative number and no empty kind stands in for "none". |
| `refresh` | A game Korri changes shows on the stage, the row, the lobby and the detail, in the published window or not (the reviewer's case: heroes for the pick outside it); one pass of questions after a publication Velvet cannot explain, none after its own answers or the clock; at most one pass in ten seconds; a game is one object in every list that holds it; the pixel budget goes to what is in sight. |
| `fresh` | Not started with 0, 1, 3, 60, 200, 300 and 10,000 unplayed games, among the first of Korri's order and after every played one: the first one in the row, on the shelf and first in the set; each page asked once with its own cursor; the number of questions in each. |
| `memo` | Every binding in `src/` that can change is named in an `onForget`; each selection answers the same with its memory warm, warm from another order, and empty; `update` and `view` at every step of every scenario and of generated plays. |
| `poster` | The generated drawings and palettes: every card names ones that are there, its title fits its box, its ink stands out, and a card in a view is two numbers and its title. |
| `plan`, `browse`, `touch`, `motion`, `words`, `qr` | The plan's invariants over a sweep of sizes; the owner's decisions as behaviour; launch settings as one list; the rest. |
