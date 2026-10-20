# Zone

Korri's zone select. A mint ground with orbit rings and two diagonal bands;
black parallelogram panels with white chunky caps; a horizontal band of game
cards with the chosen one centred and larger; red triangles pointing at it.
The register is Sonic Mania's menus. Every word is a verb or a label in caps.

Zone is a **surface**, not a theme. It owns its pages, components, words and
stylesheet, and it is built to ship from another repository unchanged.

## What Zone is

- A **zone select**. Games are stages. The band scrolls sideways; the focused
  card is the one you are about to enter. The action row under it says what
  entering means: PLAY, CONTINUE, PLAY ON a named device.
- **Three tabs** across the top: GAMES, OPTIONS, SAVE DATA. GAMES wears the
  one yellow-on-blue hexagonal banner. OPTIONS holds settings and Wi-Fi. SAVE
  DATA holds identity.
- A **foot strip**: TIME as a stage timer (Korri's `clockLabel`), the Wi-Fi
  badge, the device name tag and the build string.
- **Keycap hints** in the top corners: B BACK left, A CONFIRM right.
- **Focus** is never ambiguous: a pink edge plus two red triangles on a card,
  a pink offset underline on a button, a pink bar and a lift on a tab.
- **Eight colours** and nothing else: mint, black, white, yellow, blue,
  orange, red, pink. No gradients, no blur, no soft shadows. A drop edge is
  a hard offset in a palette colour.
- **One face**: Anton. All caps.

## What Zone never does

- Never imports anything from Korri except the treaty,
  `contracts/surface/korri-surface.ts`, for types only. Its only other
  dependency is `qrcode`, a pure encoder for the identity backup.
- Never invents a fact. Absent data stays absent: no cover becomes a stripe
  well with initials; no clock hides the timer; no network hides the badge.
- Never interprets a failure. Korri's `kicker` and `reason` are shown
  verbatim. They are the only sentences Zone draws.
- Never removes a control at any size. Width decides how many cards show;
  height decides how tall the strips are. Everything stays reachable.
- Never translates a device. The host delivers DOM focus, clicks and the four
  semantic actions; Zone reacts to focus.
- Never keeps React state. Every value is in a model under `src/state/`.

## Architecture rules

The gates under `test/` enforce these. Read them before adding a file; each
one has been seen to fail on a planted defect.

| Rule | Where it is enforced |
|---|---|
| Pure state models in `src/state/`: a model, a `Message` union, an `update`, and a `Request` union | `.oxlintrc.json` (no React, DOM, clock or storage in `src/state`) |
| One door to the host: only `src/zone-host.ts` calls a host method or subscribes to `host.input` | `.oxlintrc.json` `no-restricted-properties` |
| No `useState` or `useReducer` anywhere except `src/use-zone-program.ts`; refs for DOM mechanics are allowed | `.oxlintrc.json` `no-restricted-imports` |
| Every `switch` over a union is exhaustive | `.oxlintrc.json` `switch-exhaustiveness-check` |
| One component per file; every component has a `*.part.tsx` beside it; layers point down | `test/decomposition-gate.test.ts` |
| Every part default-exports a function and a name; no inline styles; nothing imported outside the treaty, React and `qrcode` | `test/authoring-gate.test.ts` |
| Each component imports its own CSS; no `@import`; a stylesheet styles only classes its component renders | `test/css-ownership-gate.test.ts` |
| Raw hex colours and px literals only in `src/zone-tokens.css` and `src/fonts/`; every token literal, with a knob hint | `test/atomic-gate.test.ts`, `test/authoring-gate.test.ts` |
| No blur, blend, soft shadow or smooth gradient; a `linear-gradient` is a hard-stop stripe | `test/authoring-gate.test.ts` |
| Shapes read no state: nothing under `src/ui/` or `src/pages/` imports `src/state/` or reads the host | `test/atomic-gate.test.ts` |
| Every component file has exactly one component | `test/atomic-gate.test.ts` |
| White caps on black, yellow on blue, and every other lettering pair reach contrast | `test/contrast-gate.test.ts` |
| Layout is `plan(width, height)`, thresholds are named constants, tested one pixel either side | `test/plan-zone-select.test.ts` |

`test/architecture-lint.test.ts` runs oxlint over `test/lint-tripwires`, where
each planted violation is marked `// VIOLATION <rule>`, and requires exactly
those reports.

## Layout

```
src/
  zone-tokens.css     the only file with a raw colour or pixel value; every
                      value is a literal custom property with a knob comment
  zone-fonts.css      Anton, embedded from src/fonts/ (SIL OFL 1.1)
  zone-motion.css     the one-bounce snap, the triangle nudge, the ring drift;
                      imported by each component whose stylesheet names one
  zone.css            reset, screen scope and reduced motion; imports nothing
  fonts/              Anton-Regular.ttf and OFL.txt
  fixtures/           the fixture host and model, and the part frame
  plan/               plan(width, height) per page: pure, thresholds named
  state/              navigation model, messages, requests; pure
  zone-host.ts        the one door to Korri
  use-zone-program.ts the runtime: holds a model, runs update, performs requests
  use-zone-games.ts   the home program: navigation, host buttons, idle and its
                      cycle, and the GAMES page's props
  use-zone-plan.ts    measures the root and returns plan(width, height)
  use-zone-idle.ts    the idle timer, a subscription
  ZoneSurface.tsx     the composition root: the only file that reads the treaty
  ui/atoms|molecules|organisms|templates
  pages/
```

Each component imports its sibling `<Name>.css` from its `.tsx` and carries
its `<Name>.<layer>.part.tsx` beside it. Components are controlled: a value
comes in as a prop and a change goes out through a callback.

## Navigation model

`src/state/navigation.ts` holds one value:

- `tab`: `games | options | savedata`
- `zone`: where focus is on the games page: `tabs | find | cards | actions`
- `card`, `action`: indices; `card` counts the band, which is the catalog
  through the find draft
- `find`: the find draft (the band's filter), its caret and the keyboard's
  layer and focused key
- `opts`: the focused GAME OPTS. row and the destructive action awaiting
  NO / YES
- `idleCard`: the game the idle title card shows, in catalog order
- `optionsGroup`, `optionsRunWith`: where OPTIONS lands when `system` or RUN
  WITH took the person there
- `layers`: a stack over the page, each one of
  `gameopts | keyboard | busy | problem | overlay | onboarding | idle`.
  An empty stack is `none`. Back pops the top layer; a host status pushes
  `busy` or `problem`; `options` pushes `gameopts`; 30 s without input pushes
  `idle` (the idle cycle itself is built later).

`menu` jumps to the tabs. `system` jumps to the SYSTEM group under OPTIONS.
Everything is reachable with d-pad, confirm and back alone.

## Caliper

```sh
bun install
bun run caliper      # http://localhost:5173/__caliper/
caliper-render --url http://localhost:5173 --part src/ui/templates/ZoneShell.template.part.tsx --state '*' --device '*'
```

Devices: `rg353m` (640x480) and `screen-720p` (1280x720), the two sizes the
mockup was proven at.

## Verification

```sh
bun test
bun run typecheck
bun run lint
```

## Decided without asking

- **`zone.css` does not `@import` the other three.** The brief asked for it;
  the CSS ownership gate (ported from Pico) forbids a CSS import chain because
  Vite would read those files outside Caliper's take overlay. `src/index.ts`
  imports tokens, fonts and `zone.css`; each component imports
  `zone-motion.css` itself when its stylesheet names a keyframe.
- **The plan's constants live twice.** `src/plan/zone-select.ts` is the one
  function; `zone-tokens.css` repeats every constant (including the two
  thresholds) so Caliper shows them as knobs. `test/plan-zone-select.test.ts`
  holds the two equal, so they cannot drift. Turning a knob in Caliper moves
  the CSS only; the TS plan does not read CSS.
- **The root measures, the shell is a shape.** `useZonePlan` reads the root
  with a ResizeObserver through `useSyncExternalStore` (a measurement, not
  state) and `ZoneSurface` hands the plan's two discrete answers to
  `ZoneShell` as `strips` and `band` props. The shell never measures.
- **Parallelogram slant is a length, not an angle.** `clip-path` polygons take
  lengths, and Mania's lean looks the same on short and tall panels. The
  stripe under a card title keeps an angle token (`--zone-stripe-angle`).
- **Ground opacities stay.** The rings and bands show through at the mockup's
  opacities, as tokens. They are the ground's own drawing with no edge to
  blur; everything with lettering on it is solid.
- **Required layers grow with the work.** The decomposition gate requires a
  template and an organism part today (`REQUIRED_LAYERS`); the worker that
  lands the first page, molecule or atom adds it to the list.
- **Idle resets on model change and on DOM input.** A host button that
  changes nothing (menu while already on the tabs) does not restart the
  timer. Recorded in `use-zone-idle.ts`.
- **The ground's Caliper spill is accepted as Review, not declared.** Its SVG
  children extend past the box by design and the SVG clips them; declaring
  forty per-edge exceptions would hide real findings. Its determinism check
  is `Inconclusive` because the rings drift.

- **Device name tag.** The treaty has no device-name field. The foot's
  device tag will show the device name only when Korri publishes one as a
  settings item; until then the slot is drawn with a fixture value in parts
  and is empty in the product. No invented name.
- **Mockup drop shadow.** The mockup's card shadow is black at 45% opacity.
  The brief says no soft shadows and eight colours; the card's drop edge is
  therefore a hard offset in solid blue (`--zone-drop-edge`), the same blue
  offset the panels wear. Ring and band opacities on the ground stay, since
  they are part of the ground's own drawing and have no edge to blur.
- **Tab subtitle lines.** The mockup draws "SETTINGS + WI-FI" and "IDENTITY"
  under two tabs. Kept: they are labels, not sentences.
- **Third gate scope.** "No px literal outside tokens" allows `0`, and `1px`
  only as the value of a token. Font files and `zone-fonts.css` (which names
  the TTF) are exempt from the hex and px check.
- **Caliper device for 1280x720.** No stock Caliper device has that
  viewport, so `vite.config.ts` declares `screen-720p` with estimated
  millimetres.
- **The idle layer.** Pushed by an `IdleElapsed` message after
  `ZONE_IDLE_AFTER_MS` (30 000). Any input pops it. It shows ZoneTitleCard
  for the focused game and moves to the next every `ZONE_IDLE_CYCLE_MS`
  (4 000).
- **System jump target.** `system` sets the tab to `options` and records
  `focusGroup: "system"`; the OPTIONS page, when it lands, reads it.
- **Host door is a plain function pair** (`performZoneRequest`,
  `subscribeZoneHostButtons`), as in Pico. The oxlint rule names `host`, so
  a renamed host parameter would get past it; review must catch that.
- **Catalog titles for now.** Superseded: `ZoneSurface` renders the GAMES
  page as home.

### GAMES page (decided without asking)

- **FIND sits at the body's top right, under the A hint, not in the action
  row.** Korri moves focus by geometry (`clients/portal/src/input/
  spatial-focus.ts`); a key under the band is never the next stop right of
  SAVE DATA, because the side cards are nearer. Under the A hint it is, and
  `menu` twice reaches it as well. The top strip at 640 px has no room for a
  fourth control.
- **A on a one-place card launches.** Its one button is lit as the default,
  so A on the card is A on that button. With a choice of places, A drops
  into the row and launches nothing (the brief's rule).
- **One published location is one place.** The button says PLAY (or
  CONTINUE) and carries that location's id, since the treaty requires the id
  whenever locations are published. The tag names the location; HERE is only
  drawn for a game with no published location (as the mockup and the
  fixture do) and for the first of a choice.
- **A resumable game with a choice says CONTINUE HERE / CONTINUE ON ZAO**
  (DIRECTION C4), not PLAY HERE.
- **The first of a choice is HERE**, as the brief says, trusting the
  treaty's "Korri orders the local device first". If a game is published
  only on remote devices that label is wrong; the treaty has no way to tell.
- **Find keeps the cursor on its game.** A new draft keeps the focused game
  when it still matches, else the first match. A draft that matches nothing
  shows NO GAMES with the cursor on FIND. B on the band while a filter is
  kept lets go of it, on the same game; B on the keyboard deletes, and on an
  empty draft drops the keyboard and the filter. The draft holds 40
  characters.
- **GAME OPTS. is a header (GAME OPTS.) over a group titled with the game.**
  Each enabled action is a row whose value is RUN (red when destructive);
  a disabled action is a read-only row whose value is Korri's description,
  so the reason is on screen. ZoneOptionRow has no dimmed look for a
  disabled row; that is a gap in the molecule, not decided here. A plain
  action and YES close GAME OPTS. The question's headline is the action's
  label, its line the description, its yellow title the game; NO has the
  cursor first and B is NO.
- **RUN WITH is a row whose value is CHOOSE.** It dispatches
  `OpenedRunWith`, which closes GAME OPTS., opens OPTIONS and records
  `optionsRunWith: gameId` for the OPTIONS page to read and clear. B from
  OPTIONS forgets it.
- **The catalog's error dialog is headed GAMES.** Korri's `Error` carries
  only a message; the headline names the place that failed rather than
  inventing a sentence, and the message is the body.
- **LOADING uses ZoneBusyStrip, the error ZoneDialog.** Both left
  `AWAITING_PAGE`, since the gate fails while a consumed component is listed.
  The layers worker uses them too.
- **Facts are a date, a count and hours.** LAST PLAYED is the local day
  (19 Oct 2026); a relative "today" would need a clock. TIME is whole hours
  and minutes (3h 12m). The line keeps its height when a game has none, so
  the band does not jump.
- **Short screens: `ZONE_SHORT_HEIGHT` (400, token `--zone-short-height`).**
  Below it, on a wide screen, the facts join the action strip at note size;
  on any screen the find keyboard rises over everything below the tabs,
  because the body cannot hold its keys. The band is hidden while typing
  there; DONE shows the result.
- **The action strip grows when its row wraps** (two places on a narrow
  screen); the band gives the room.
- **A section is a spine in the band.** `ZoneSectionCard` (a new molecule):
  a narrow black card, side-card height, caption up it, stripe down it, no
  focus. `ZoneCardBand` takes `section` on a card and writes
  `--zone-band-dividers` so its centring sum stays exact. Its caption is set
  in the molecule, not ZoneCaps, so the ellipsis runs up the spine.
- **The band clips instead of hiding overflow.** `overflow: clip` makes it
  never a scroll box, so the host's `scrollIntoView` after a focus move
  cannot scroll it under the track's translation.
- **A on a card had no way out.** `ZoneCard` gained `onPress` and
  `ZoneCardBand` `onChoose`.
- **Waking keeps the exact focus.** `use-zone-idle` no longer wakes on
  `focusin` (a focus move is already a message, and `update` treats any
  message while idle as a wake) and wakes on a key one task later, after the
  host has turned it into a move. The page does not report the focus moves
  it makes itself to follow the model, so its own sync never wakes idle.
- **The page moves DOM focus to the model's target** after `menu`, `back`
  or a layer, through `data-games-region` wrappers. ZoneKeyboard and
  ZoneDialog report no focus of their own, so the page reads a focused key
  from its glyph and a dialog button from its index.
- **The home program lives in `src/use-zone-games.ts`**, beside the other
  runtime hooks, as Pico's `use-pico-navigation.ts` does. The page part runs
  the real `update` through it, since nothing under `src/pages/` may import
  `src/state/`.
- **OPTIONS and SAVE DATA show their tab's label on a panel** until their
  pages land; `tab` already drives which one shows.
- **FIND shows only while the catalog is Ready.** There is nothing to find
  while it loads, fails or is empty.
- **`ZonePartFrame` gained a `handheld` shape** (640 by 480, as tokens) for
  the Narrow state on the 720p device.
- **The intrinsic ladder was rendered with two Caliper devices added for the
  run and not committed** (480x800, 1280x300), so every other part keeps its
  two devices.
