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
  entering means: PLAY, CONTINUE, PLAY ON a named device, then OPTS. for
  GAME OPTS.
- **Three tabs** across the top: GAMES, OPTIONS, SAVE DATA. The active tab
  wears the one yellow-on-blue hexagonal banner. OPTIONS holds settings and Wi-Fi. SAVE
  DATA holds identity.
- A **foot strip**: TIME as a stage timer (Korri's `clockLabel`), the Wi-Fi
  badge, the device name tag and the build string.
- **Keycap hints** in the top corners: B BACK left, A CONFIRM right.
- **Focus** is never ambiguous: a pink edge plus two red triangles on a card,
  a pink offset underline on a button, a pink bar and a lift on a tab.
- **One palette per screen group** (GAMES, OPTIONS, SAVE DATA, GAME OPTS.,
  PAUSED, the title card, the tally), sampled from Mania's menus and
  switched by `data-ground`. No blends, no blur, no soft shadows, no smooth
  gradients: flat colours, hard-stop stripes and dither only. A drop edge is
  a hard offset in the ground's shade. (Supersedes "eight colours".)
- **Two faces on one art-pixel grid**: Zone Caps, Zone's own outlined pixel
  capitals, for every word Zone writes; Jersey 10 for every fact and
  sentence Korri publishes, in its own case. (Supersedes "one face:
  Anton".)

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
| Every lettering pair reaches contrast; outlined lettering is judged face against outline, on every screen group's ground | `test/contrast-gate.test.ts` |
| Only the ground draws on a canvas or runs a frame clock; gradients are hard-stop stripes or dither; no word breaks inside itself | `test/authoring-gate.test.ts` |
| Zone Caps is the grid, built: coverage, metrics, outlines and a reproducible build | `test/zone-caps-font.test.ts` |
| Layout is `plan(width, height)` on the art grid (`planArt(wArt, hArt)`), thresholds are named constants in art px, tested one art pixel either side | `test/plan-zone-select.test.ts` |

`test/architecture-lint.test.ts` runs oxlint over `test/lint-tripwires`, where
each planted violation is marked `// VIOLATION <rule>`, and requires exactly
those reports.

## Layout

```
src/
  zone-tokens.css     the only file with a raw colour or pixel value; every
                      value is a literal custom property with a knob comment
  zone-fonts.css      Zone Caps and Jersey 10, embedded from src/fonts/
                      (SIL OFL 1.1)
  zone-motion.css     the one-bounce snap, the triangle nudge, the ring drift;
                      imported by each component whose stylesheet names one
  zone.css            reset, screen scope and reduced motion; imports nothing
  fonts/              zone-caps.grid (Zone Caps, drawn by hand), its build
                      ZoneCaps.woff2 and .ttf, ZoneCaps-OFL.txt;
                      Jersey10-latin.woff2 and its OFL.txt
  fixtures/           the fixture host and model, and the part frame
  plan/               plan(width, height) per page: pure, thresholds named
  state/              every model, messages, requests, and surface.ts, which
                      composes them and routes each message; pure
  zone-host.ts        the one door to Korri
  use-zone-program.ts the runtime: holds a model, runs update, performs requests
  use-zone-games.ts   Zone's program: every model under one useZoneProgram,
                      host buttons, idle and its cycle, and the GAMES page's
                      props
  use-zone-plan.ts    measures the root and returns plan(width, height)
  use-zone-idle.ts    the idle timer, a subscription
  ZoneSurface.tsx     the composition root: the only file below the host door
                      that reads the treaty's model; picks the page
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
- `optionsGroup`: where OPTIONS lands when `system` took the person there
  (its SYSTEM group)
- `layers`: a stack over the page, each one of
  `gameopts | keyboard | busy | problem | confirm | overlay | onboarding | idle`.
  An empty stack is `none`. Back pops the top layer; a host status pushes
  `busy`, `problem` or `confirm`; OPTS. in the action row (`ChoseGameOpts`)
  and its shortcut `options` push `gameopts`; 30 s without input pushes
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
- **OPTS. is the last button of the action row.** It opens GAME OPTS. as
  `options` does, so a person with d-pad, confirm and back alone reaches
  Korri's game actions (C28). It is never lit by default; B from GAME
  OPTS. returns the cursor to it.
- **GAME OPTS. is a header (GAME OPTS.) over a group titled with the game.**
  Each enabled action is a row whose value is RUN (red when destructive);
  a disabled action is a read-only row whose value is Korri's description,
  so the reason is on screen. ZoneOptionRow has no dimmed look for a
  disabled row; that is a gap in the molecule, not decided here. A plain
  action and YES close GAME OPTS. The question's headline is the action's
  label, its line the description, its yellow title the game; NO has the
  cursor first and B is NO.
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
- **Zone reads no key.** `useZoneHorizontal` only marks the element and
  listens for `korri-semantic-direction`; it has no key handler, so the
  portal's keyboard adapter keeps its repeat and release handling. In
  Caliper, where no portal stands in front, `src/fixtures/
  fixture-directions.ts` turns an arrow key on a focused stepper into the
  portal's event (the PAUSED, Paused control and OPTIONS parts call it),
  and the Paused control part carries a Caliper check that dispatches the
  event. `use-zone-idle` keeps its root `keydown` listener to restart the
  timer and wake: it reads no key and prevents nothing. The authoring gate
  fails a file outside `src/fixtures` that reads `event.key`, `.key ===`,
  `keyCode` or an arrow key's name, or calls `preventDefault` in a key
  handler.
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
  pages land; `tab` already drives which one shows. Superseded: the wire
  stage mounts both pages.
- **FIND shows only while the catalog is Ready.** There is nothing to find
  while it loads, fails or is empty.
- **`ZonePartFrame` gained a `handheld` shape** (640 by 480, as tokens) for
  the Narrow state on the 720p device.
- **The intrinsic ladder was rendered with two Caliper devices added for the
  run and not committed** (480x800, 1280x300), so every other part keeps its
  two devices.

### OPTIONS and RUN WITH (the options worker)

- **Shared row plan in `src/plan/`.** Pages may not import `src/state`, so
  what OPTIONS shows (`src/plan/options.ts`: groups, rows, what A does on
  each, the keyboard and dialog types) and what RUN WITH shows
  (`src/plan/options-run-with.ts`) are pure functions there. The state
  models and the pages both read them, so they cannot disagree about which
  rows exist. The page takes `korri` (the model slices) and `view` (the
  state model's value, structurally) and reports through plain callbacks.
- **Rows are strings.** ZoneOptionRow takes a string value, and it is not
  this worker's to change. So Wi-Fi bars are glyphs (`▂▄▆▁`) followed by
  the strength, LOCK and KNOWN are words, CONNECTING is the focused row's
  description beside the Saving caret (the caret alone is the value), and
  the connection row reads SSID, bars and % as text. ZoneSignal shows the
  same facts in the foot. An issue is suggested to let a row take a node.
- **A on a choice steps it forward**, as right does. The real host delivers
  left and right to a control only through `korri-semantic-direction`,
  which ZoneOptionRow listens to while focused; A keeps every choice
  reachable as well.
- **A secret's keyboard starts empty.** Korri's value for a secret is its
  word about it ("Configured"), never the secret. DONE on an empty secret
  closes without a request; clearing is the clear key alone.
- **A secret's row shows Korri's status word as given.** "Configured" or
  "Not configured" is a status, not the secret, so it is not turned into
  dots. Dots are only for what the person types on the keyboard.
- **Placeholders.** Korri's `interaction.placeholder` (text and
  sensitiveText) stands dimmed (`--zone-placeholder-opacity`, 0.6) in the
  keyboard's slab after the caret while the draft is empty, and in a row's
  value while Korri's value is empty. On first boot the NAME keyboard shows
  it too. At 0.6 the dimmed ink still reaches about 5.7:1 on the white slab
  and about 7:1 on the black panel; the contrast gate does not compute
  blended pairs.
- **Destructive without Korri's words** asks NO / YES under the row's (or
  action's) label, with the action's description as the line when there is
  one. Zone writes no sentence of its own.
- **SYSTEM and WI-FI are omitted when empty.** No device actions, no SYSTEM
  heading; no `network`, no WI-FI. `focusSystemGroup(state)` sets the
  cursor to the SYSTEM key; it resolves to the group's first row, or the
  page's first row when there is no SYSTEM. It leaves an open keyboard or
  dialog alone. `optionsLayer(state)` tells the wire stage what is open, so
  B and menu go to `update` (`PressedBack`, `PressedMenu`) while it is.
- **An inert device action** is drawn read-only with the value UNAVAILABLE
  (ZoneOptionRow cannot dim), its description under it when focused.
- **NoAdapter keeps the connection row**, then NO WI-FI ADAPTER: a wired
  device still says it is wired.
- **The network in use, and any network while Korri is joining one, does
  nothing on A** (as Pico). A rejected key reopens the keyboard once per
  failure; Korri's reason stands in red in the keyboard's heading, since
  the keyboard covers the row.
- **Facts keep their case.** `ZoneCaps` has a verbatim mode, and
  `ZoneOptionRow` takes `verbatimLabel` and `verbatimValue`. Every value
  Korri publishes on an OPTIONS row keeps its own case (a version, a
  path, "Configured"), and so do a Korri label that reads as a path
  (starts with `/` or `~`, `isPathLabel` in `src/plan/options.ts`), RUN
  WITH's program and build, SSIDs, and keys on SAVE DATA's cards and
  dialogs. Labels, Zone's own words and Korri's sentences stay in caps.
  Zone cannot tell a path from a label any other way: the treaty marks
  neither, so the leading `/` or `~` is the rule.
- **No body banner.** The active tab wears the one yellow-on-blue banner
  and names the page, as DIRECTION uses the banner once. OPTIONS and SAVE
  DATA draw none in their bodies; the rows and the band take the room.
  RUN WITH, which has no tabs, keeps its own.
- **RUN WITH keeps case for programs and builds** (a build is a package
  path) and for SSIDs on OPTIONS; Zone's own words there are written in
  caps. Its state tag: Ready blue, Busy yellow, Stale and Warnings orange,
  Conflict and Error red, Loading white. Mint is the ground, so SAVED is
  yellow. A saved choice with no route (Stale) is drawn as its own route:
  SAVED, the runner id, UNAVAILABLE. Korri's message wraps rather than cuts.
  Back does nothing on RUN WITH: the way out is the host's own action, and
  Zone never infers which action that is. The page traps the cursor
  (`data-block-exit`) and has no tabs.
- **Parts run the real update.** `src/fixtures/fixture-options.ts` holds
  the fixtures and a small live program around `src/state`'s update; a
  part imports that, never `src/state`. With no Korri behind it the program
  plays Korri's part in the smallest way (a changed value lands, a join
  starts connecting). A part reads the frame once for the plan's strips
  and band.
- **Pages awaiting their mount.** The decomposition gate requires every
  page to be reached from ZoneSurface, which only the wire stage edits. It
  now has an `AWAITING_MOUNT` list (ZoneOptions, ZoneRunWith), checked both
  ways like `AWAITING_PAGE`.

### OPTIONS and RUN WITH, refined (options): decided without asking

The refinement brief, section 4, for OPTIONS, Wi-Fi and RUN WITH. These
supersede the bullets above where they disagree: rows are no longer
strings only, LOCK and KNOWN are gone, placeholders and disabled rows are
no longer dimmed with opacity, and Korri's sentences are no longer caps.

- **The band is the scroll.** OPTIONS' body is the sky band
  (`--zone-options-band`), the full width of the screen with a 3-art-px
  black lip over and under it, and the lips stand outside the scroll box,
  so a row the band cannot hold is cut by a lip, never by the tabs. RUN
  WITH has the same band. 4 art px of ground stand between the band and
  each strip, so the yellow ground and its rings frame it as Mania's do.
- **Group titles hold.** A group title is the black wedge flush with the
  band's left edge (caps-2, caps-1 when it must, `useZoneFit`), its words
  starting where the rows' words start. It is `position: sticky` at the
  top of the band, on a band-coloured row (`--zone-option-group-backing`,
  which OPTIONS sets; other holders keep none), so rows pass under it and
  are cut on one straight line. While a group's last rows are under its
  title the title can stand right over the next one; accepted, it lasts
  one title's height of scroll.
- **The row measures itself** (`src/use-zone-option-row.ts`, a DOM
  mechanic like `useZoneFit`): it stacks label over value at or under 224
  art px (`ZONE_OPTION_ROW_STACK_AT` in `src/plan/options-row.ts`: the
  brief's 240-art-px screen less the band's 8-art-px inset each side), so
  GAME OPTS., first boot and RUN WITH get the reflow too. 480x800 stacks;
  640x480 does not. Stacked, the pink drop hangs under the value panel,
  the row's foot.
- **Cells on the grid.** A cell is 14 art px for one line (cap 10, 2 over
  and under), its slant 13 (45 degrees, from `zoneStairClip`); a cell that
  wraps keeps the one-line slant, stepped over its height
  (`zoneOptionClip`), so a slant never meets its words. Caps lines in a row
  are 12 art px (1.2 of the cap), so wrapped lines stand 2 apart.
- **Two-tone caps without the copies.** Row labels, Zone's own value
  words, group titles' fallback and RUN WITH's title draw the white ink's
  lavender lower rows with a hard-stop gradient clipped to the text, one
  tile per line, because they wrap (ZoneCaps' copies are one line). On a
  black panel there is no ring to lose. Group titles use `ZoneCaps` itself.
- **Cuts.** A fact that does not fit is cut by the row: a path (starts
  with `/` or `~`) in the middle, keeping its last segment whole and as
  much of the start as fits; when the last segment alone is too long, its
  tail from a `-` or `.` ("…retroarch-1.19.1"). Anything else ends in "…"
  (CSS). The whole fact is written on the focused row's own line under its
  panels while it is cut, as generated content (no second copy in the DOM;
  the words stay in the cell for assistive tech). A path on that line may
  wrap after each `/`. A caps label wraps between words; a single word
  longer than the line ends in "…" and shows whole the same way.
- **Wi-Fi words.** `zoneWifiWords`: LOCKED or OPEN, then SAVED when Korri
  knows the key; `zoneNetworkValue` ("72% LOCKED SAVED") is the one
  function OPTIONS and first boot call. First boot used to say one word
  (SAVED over LOCKED over OPEN); both now say the security word and SAVED.
  OPTIONS draws bars, then the strength in Jersey 10, then the words in
  caps; its WI-FI rows give the value the larger share
  (`--zone-options-wifi-label-share`), and SSIDs are facts (Jersey 10).
- **Bars** are 2/4/6/8 art px, 2 wide, 1 apart, lit white, unlit
  `--zone-track`, no opacity. ZoneSignal's strength is Jersey 10, white,
  no longer mint. ZoneSignal takes a new optional `panel` (default on):
  off, the OPTIONS connection row draws the readouts in its own black
  value panel instead of a panel inside a panel.
- **Saving** keeps the value and blinks a red pixel caret at the value
  panel's end (500 ms each way, `steps(1)`). CONNECTING is Zone's word
  under the row (caps, `descriptionCaps`).
- **Steel, not opacity.** A disabled row and a placeholder are drawn in
  `--zone-steel` (#84A2B5, Mania's OPTIONS ring), never with alpha.
- **Problems and warnings** are one look: a black panel flush at its
  start with a 2-art-px `#F75131` bar, leaning at its end, the words white
  in Jersey 10 as written. ZoneWarningLines draws RUN WITH's warnings and
  the join keyboard's rejected-key reason; a settings problem under its row
  draws the same.
- **Choice triangles** are 4 by 7 art px pixel stairs inside the value
  panel's ends, nudging in two steps; only rows that step (choices) wear
  them, so a text or action row's focus is the pink drop alone.
- **A veil under a layer.** While the keyboard or the NO / YES dialog is
  open, the band behind it wears a 1-art-px black checker (conic hard
  stops), so the sky dialog and keyboard stand clear of the sky band.
- **RUN WITH's banner stands where the tabs stand** (the shell's `tabs`
  slot: RUN WITH has none), so the band takes the body. The game's title
  is caps-2 on a black panel (caps-1 when it must); the message is Jersey
  10 on a black panel; a route gives its build the larger share
  (`--zone-run-with-label-share`, 0.45). READY was already the
  `#5A71E7` tag. C11's acceptance test now reads the page's whole shell,
  since the banner left `.zone-run-with`.
- **Row props added** (ZoneOptionRow; ZoneOptionGroupRow carries the
  first): `descriptionCaps` (the description is Zone's own word). Every
  other prop is as it was.

### SAVE DATA page (C16 to C20)

- **The page owns the shell, the binding owns its slots.** `ZoneSaveData`
  draws `ZoneShell` itself, as the brief says, but takes the tabs, the hints
  and the foot as a `frame` of slots. The GAMES worker owns the tab strip and
  the navigation; the stage that mounts the pages fills the frame once for
  every page. The part fills it with `ZoneTabStrip` (SAVE DATA active),
  `ZoneKeycap` and `ZoneFoot`, sized by `plan()` of the frame's window.
- **A page may not import `src/state`, so it declares what it reads.**
  `ZoneSaveDataFlow` and `ZoneSaveDataEvent` mirror the identity model's
  `IdentityFlow` and `IdentityMessage`; `test/zone-save-data.test.tsx` holds
  them assignable, so the binding passes the model's value and dispatches the
  page's events unchanged.
- **The part runs the real model through a fixture store.** The atomic gate
  counts parts as shapes, so the store that runs `update` and answers each
  request as Korri would lives in `src/fixtures/fixture-save-data.ts`, beside
  the fixture host. The part imports the store, never `src/state`.
- **`AWAITING_MOUNT` in the decomposition gate.** "Every page is reached from
  the surface binding" cannot pass while `ZoneSurface.tsx` belongs to another
  worker. `ZoneSaveData` is listed there with the same rule as
  `AWAITING_PAGE`: an entry the binding already reaches fails.
- **`AWAITING_PAGE` loses five entries, not two.** The page and its organisms
  import `ZoneSaveCard`, `ZoneQrWell`, `ZoneKeyboard`, `ZoneDialog` and
  `ZoneBusyStrip`; the gate fails on any consumed entry, so all five go.
- **Card words.** BACK UP reads THIS KEY in its well; the two SWITCH cards
  read FROM BACKUP and BUNKER and carry no tag (MANIA MODE's NEW cards have
  no foot either); NOT AVAILABLE is the well of one card headed IDENTITY.
  Tags: NEW is orange (`open`), BACKED UP mint (`saved`), DELETE red
  (`remote`). A key in a card or a dialog keeps its own case: it is a fact
  Korri publishes, not a label (superseded the upper-case rule).
- **The bunker step opens with `bunker://` typed** on the URI layer. Every
  NIP-46 bunker URI starts with it; DONE with only the prefix goes nowhere.
- **Whitespace is stripped from a typed backup secret.** NIP-49 backups are
  bech32 and hold none; the slab shows the secret in groups of four, so a
  person copying it by hand may type the gaps.
- **Back steps back one step, NO abandons.** B on a keyboard step returns to
  the step before it with its text kept; B on a dialog returns to the step
  that opened it. NO on the trust loss or either delete question ends the
  flow and sends nothing. B or DISMISS / OK on Korri's problem or switched
  dialog, and B on a finished backup, call `dismissIdentityStatus`. Starting
  a flow over a finished backup dismisses it first. A republished model never
  dismisses (the loop Pico's test guards against).
- **Dialog words.** Problem: headline IDENTITY over Korri's message verbatim,
  DISMISS. Switched: SWITCHED with the new key shortened as its yellow
  title line (in its own case), OK. Disposition:
  LOCAL RECORDS, one line, TRANSFER / DELETE with TRANSFER under the cursor.
  Trust: SWITCH KEY? over the brief's line, NO under the cursor. The two
  delete questions carry the key shortened as their yellow title line.
- **One cursor on screen.** While a keyboard or a dialog is up, or the
  cursor rests on the backup's slab, the band is unmarked: no pink edge, no
  card triangles, no edge triangles. When Korri's backup arrives the cursor
  goes to the slab (`FocusedSecret`), which also lets the QR code take the
  whole well; with the cursor back on BACK UP the QR stands clear of the
  card's triangles so a reader sees every module.
- **The slab stands beside the band when the page is wider than tall by
  1.25** (`ZONE_SAVE_SLAB_BESIDE_ASPECT`), and under it, two shares to one,
  in a column. The band's height is what makes the QR code scannable, so
  width pays for the slab wherever it can. On a strip 4 times wider than
  tall (`ZONE_SAVE_ROW_BESIDE_ASPECT`, 1280x300) a retired key's row
  stands at the band's end. Both are aspect-ratio container queries, held
  equal to the constants by `test/plan-save-data.test.ts`.
- **Edge triangles show the band runs on.** Cut cards alone failed at
  1280x720, where five cards ended flush with both edges; a red triangle at
  each edge points the way more cards lie and dims on a side with none.
- **The keyboard reports focus through its organism.** `ZoneKeyboard` and
  `ZoneDialog` draw the cursor from a prop and have no focus callback;
  `ZoneSaveEntry` and the page hear React's bubbling focus event and name the
  key (`zoneKeyId`'s scheme) or the button index. A focus callback on the two
  molecules would be cleaner.
- **Retired keys are deleted, never backed up, from this page.** The brief's
  flow offers DELETE only; `exportIdentityBackup(password, retiredPublicKey)`
  is in the treaty but has no control here yet.

### The treaty: Confirm, progress, download (C31 to C33)

The surface treaty gained a `Confirm` status, `Busy.progress` and
`SurfaceGame.download` while Zone was built (commit 1a593ed07). The brief
fixed the look of each; these are the choices it left open.

- **Confirm is its own layer.** The status model shows `confirm`, and
  navigation's stack holds a `confirm` layer while Korri asks. Like Busy and
  Problem it covers the page: menu, options and system do nothing under it,
  and Back answers nothing. Today the portal keeps every host button to
  itself while its download gate has a status, and sends Back to the gate
  (`clients/portal/src/surface/SurfaceRoot.tsx`, `acquisition.back()`), so
  Zone never has to guess which action declines.
- **The question opens on Korri's first enabled action.** "The first
  focused" is the first of the host's order; an inert button cannot hold
  DOM focus, so a question whose first action is disabled opens on the
  next enabled one. With no action enabled (or none at all) nothing takes
  the cursor and the page stays inert until Korri publishes again.
- **ZoneDialog gained `id`, `disabled` and `destructive` on an action.**
  Korri's labels may repeat, so a button is keyed by the action's id. An
  inert action is drawn dimmed and does nothing; a destructive one has red
  caps, as the busy strip already marks it.
- **The meter's words are DONE / TOTAL.** A slash, no word: the two sizes
  stand at the bar's right end as SPECIAL STAGE's score stands at its bar.
  They sit beside the bar, not on it, so they never hide the fill; when
  the strip is too narrow for both they drop under it, right-aligned.
  Without a total the size done stands alone. With nothing done and no
  total (the portal's unpacking, `progress: { done: 0 }`) there are no
  figures at all: "0 B" would read as stuck.
- **Korri's detail and Zone's figures both show.** The portal's detail
  already names the sizes ("612 MB of 1.2 GB · 6.4 MB/s · 1:30 left") and
  is drawn verbatim; the figures are Zone's own reading of `progress`, in
  the brief's format, so they can differ in rounding (612 MB against
  612.0 MB). Zone does not parse or hide Korri's words.
- **Bytes are decimal.** `formatZoneBytes` (`src/plan/bytes.ts`) counts
  1 KB as 1000 B, as the portal's detail does, so a card's 1.5 GB and
  Korri's question agree. Under 1 KB it writes whole bytes (512 B), since
  "0 KB" would hide a real count. From 10 MB it keeps the decimal even when
  it is 0 (1.0 GB), so the figure's width does not jump. A value that
  rounds to 1000 of a unit is written in the next (999 999 B is 1 MB). GB
  is the largest unit.
- **Anton has no tabular figures.** Its GSUB lists `aalt c2sc case ccmp dlig
  dnom frac liga locl numr ordn sinf smcp subs sups zero`, no `tnum`, and
  its "1" is 677 units wide against 1012 for every other digit. So the
  size done gets room of its own (`--zone-meter-figures`, 8.5ch,
  right-aligned) and the readout keeps its width as it counts.
- **The bar is red over blue.** The fill is red under an orange lip (a
  hard stop at 34%), as SPECIAL STAGE's bar is red over a darker edge; the
  track is blue, as PAUSED's range meter is. Both lean as every panel
  leans, and a full fill is exactly the track's shape.
- **No end is stripes over the whole bar.** Red diagonal stripes on the
  blue track slide one tile along, over and over (`zone-sweep`, a fourth
  movement added to `zone-motion.css`). Under reduced motion they stand
  still. A single sliding stripe would stand still as a short fill and read
  as a share; stripes over the whole bar read as "under way, no end" moving
  or not.
- **The meter is a `progressbar`** named by Korri's kicker. With a total it
  states the whole percent done and the figures as its value text; without
  one it states no value. The drawn figures are `aria-hidden`, since the
  bar's value text already says them.
- **The DOWNLOAD tag is orange with black ink.** A game card wears blue
  (system), yellow (HERE), red (a device) and white (CONTINUE); orange is
  OPEN, which is only ever on a save card or the running strip. It closes
  the row, after CONTINUE. The row fits its caps as one, so DOWNLOAD 1.5 GB
  makes every tag on that card smaller; on a side card with two places the
  caps reach the fit's floor.
- **DOWNLOAD is the last fact.** The facts line keeps its order and adds
  DOWNLOAD <size> (or the word alone). A narrow screen cuts the line at its
  end, so DOWNLOAD is the first fact to go there; the card's tag still
  carries it.
- **Extra part states.** Beside the brief's list, the status layer has
  `BusyProgress` (the meter over the page) and the busy strip has
  `ProgressUnpacking` and `ProgressNarrow`; the tag atom has `Download`.

## Layers: status, PAUSED, first boot

Three shapes, each driven by a pure model, built ahead of their mount in
`ZoneSurface.tsx`. Each takes Korri's slice and its model's value as plain
props and reports through callbacks; the binding turns a callback into the
model's message, and the model's `update` returns `ZoneRequest`s.

| Shape | Model (`src/state/`) | Korri slice | Capability |
|---|---|---|---|
| `ui/organisms/ZoneStatusLayer` | `status.ts` (`updateStatus`) | `status`, the catalog's titles | C7 to C10, C31, C32 |
| `pages/ZonePaused` (+ `ZonePausedControl`) | `overlay.ts` (`updateOverlay`) | `presentation` when `gameplay-overlay` | C24 |
| `pages/ZoneOnboarding` (+ `ZoneControllerCard`) | `onboarding.ts` (`updateOnboarding`) | `onboarding`, `network`, `settings`, `settingsStatus` | C25 |

- **Status layer.** Takes the page as its `page` prop and covers it:
  nothing while Browsing; Busy drops the busy strip over the dimmed, inert
  page with the game's title in yellow above it; Running is a small strip
  at the top (kicker, OPEN, title) that leaves the page usable; Problem is
  the blue dialog, DISMISS lit before RETRY; Confirm is the same dialog
  with Korri's own actions in its order, the first lit, Back answering
  nothing. Busy with Korri's progress draws a red slanted meter under the
  detail. Korri's Problem never offers
  a reload. C10 is RELOAD in the catalog's Error dialog on GAMES: the
  page reports `ChoseReload` and navigation asks `Reload`. The SYSTEM
  group lists Korri's own device actions and has no reload of its own.
- **PAUSED.** The red band with Korri's title (PAUSED when absent), the
  ground greyed and striped as the game's stand-in, the yellow wedge of
  flat controls then groups. Left and right arrive as the portal's
  `korri-semantic-direction` event on rows marked
  `data-korri-horizontal-control`, never as keys.
- **First boot.** CONTROLLERS, WI-FI, NAME, IDENTITY under a strip of four
  tags; NEXT on the first three, FINISH on the last. The controller cards'
  grid is `src/plan/onboarding.ts`, measured on the stage.
- **Focus.** Each model holds the lit control; `src/use-zone-layer-focus.ts`
  moves DOM focus to it when a layer opens or the model moves it, and the
  status layer gives focus back to the page when it closes.
- **Parts** run the real models through `src/fixtures/layer-program.ts`, a
  fixture program with a pretend Korri (it applies values, joins networks,
  saves the name) and the fixture host recording every request.

### Decided without asking: the layers

- **`dismissOnSuccess` sends no dismiss.** The brief said such a control
  closes the overlay after invoke. The portal's overlay controller already
  returns to the game after one succeeds
  (`clients/portal/src/overlay/live-overlay-controller.ts`, the
  `dismissOnSuccess` branch), so a second `dismissGameplayOverlay` from Zone
  would return before the outcome and close the overlay on a failure too.
  Zone invokes, marks the overlay `leaving` (further presses wait for
  Korri's next model) and lets the host close it.
- **A range is invoked on every step**, as the brief says; Pico debounces
  instead. The value shows at once in yellow (`moving`) and Korri's value
  replaces it when it catches up, the cursor leaves, or Korri disables it.
- **A on a toggle flips it, A on a choice steps it forward**; A on a range
  does nothing. A choice wraps at its ends; a range stops at min and max and
  its end triangle dims.
- **Destructive applies to every interaction kind.** The question names the
  control as headline, its description as the line, and the value it would
  take in yellow.
- **Disabled controls stay focusable** and wear the underline when focused,
  so the reason under them can be reached and read.
- **Destructive labels are red** on PAUSED's rows, as the busy strip's
  destructive action is. Mania's EXIT is white; the treaty asks to mark it.
- **The status layer takes the page as a `page` prop**, so it can dim it
  and make it `inert` while Busy or Problem covers it. Running does neither.
  Busy shows the game's title (C7 "the game it belongs to") on its own panel
  in yellow, the colour the problem dialog states a game in.
- **Busy with no action takes no focus**; the page goes inert, so focus
  waits on nothing until Korri publishes the next status. Back never
  cancels busy work.
- **Problem's game** is Korri's `gameTitle`, else the catalog's title for
  its `gameId`.
- **Onboarding's device-name lookup** is by id: the first settings item,
  in any group, with the id `device-name`, which is the id the portal
  publishes (`clients/portal/src/surface/settings-model.ts`, label "Name").
  The label is Korri's word and is not read. Without a `text` interaction
  the row shows Korri's value read-only and A on it does nothing; absent,
  the row is read-only and empty.
- **No identity call at first boot.** The treaty asks for none: the device
  already has its own identity. START on the NEW SAVE card is the player's
  choice, held in the model, and FINISH waits for it. Restoring a save is
  SAVE DATA's.
- **WI-FI joins what needs no password at once.** `onboardingJoin(networkId)`
  sends `connectWifi` for a saved or open network; a secured network Korri
  has no key for is handed over as `join` in the model, for the OPTIONS
  join flow to pick up when it is wired. A SCAN row (`scanWifi`) was added
  beside the brief's SKIP. NEXT on WI-FI lights once the device is online
  (or Korri cannot manage the network); SKIP always goes on.
- **Network rows keep the SSID's case**, as OPTIONS does (`verbatimLabel`),
  and the name row keeps Korri's name's case. Controller names keep their
  case on the cards.
- **Zero controllers show one WAITING card**, COMPETITION's empty seat.
  Seats are numbered in heard order; 1P blue, 2P red, 3P yellow, 4P mint.
- **The keyboard's lit key follows DOM focus** through `FocusedKey`
  (superseded the last-press rule).
- **The save card's START is a click on a wrapper.** `ZoneSaveCard` has no
  press callback; the host's confirm is a real click and bubbles to it.
- **Parts reach the models through a fixture.** The atomic gate forbids a
  part under `ui/` or `pages/` to import `src/state`; the scenarios import
  `src/fixtures/layer-program.ts`, which does. Shapes never do.
- **Gate lists.** `ZoneBusyStrip`, `ZoneDialog`, `ZoneSaveCard`,
  `ZoneKeycap`, `ZoneActionRow`, `ZoneOptionGroup` and `ZoneKeyboard` left
  `AWAITING_PAGE`, since the layers consume them. `ZoneStatusLayer` joined
  it, and a new `AWAITING_BINDING` names `ZonePaused` and `ZoneOnboarding`,
  because `ZoneSurface.tsx` is not this worker's to edit. The entries go
  when the binding mounts them.
- **Tokens** are in the `/* ---- layers ---- */` block. The controller-card
  plan's widest and gap equal `--zone-controller-card-max` and
  `--zone-card-gap-wide` (held by `test/zone-onboarding.test.tsx`).

## The wire stage: Zone as a whole

`ZoneSurface.tsx` is the composition root. It runs one program
(`useZoneSurface` in `src/use-zone-games.ts`, on `useZoneProgram`) over
`src/state/surface.ts`, which holds every model side by side (navigation,
OPTIONS, RUN WITH, SAVE DATA's identity, the status layer, PAUSED, first
boot) and routes each message to the model it belongs to. A page reports in
its own model's words; Korri publishing goes to every model; the host's four
buttons are routed by what is shown.

What takes the screen, first match wins (`zoneShown`):

| Shown | When |
|---|---|
| PAUSED (`ZonePaused`) | Korri presents `gameplay-overlay` |
| OPTIONS' join keyboard | first boot handed a secured network to OPTIONS' join flow |
| first boot (`ZoneOnboarding`) | `model.onboarding` exists |
| RUN WITH (`ZoneRunWith`) | Korri's `runnerChoice` is open |
| the tab | GAMES (home), OPTIONS, SAVE DATA |

`ZoneStatusLayer` is drawn over whichever it is.

| Button | What it does |
|---|---|
| back | Korri's Problem first (dismiss); Busy is never cancelled by it, Confirm never answered by it. Then the shown page's own rule; on OPTIONS and SAVE DATA, when the page has nothing to close, to the GAMES tab; on GAMES, navigation's rule; then nothing |
| menu | the cursor to the tabs, on every tab. OPTIONS' keyboard commits instead; a dialog or an identity step holds the cursor |
| options | GAME OPTS. on GAMES, a shortcut for OPTS. in the action row; nothing on OPTIONS or SAVE DATA |
| system | OPTIONS with the cursor on its SYSTEM group (`focusSystemGroup`) |

PAUSED, first boot and RUN WITH have no tabs on screen, so menu, options
and system do nothing there.

### Decided without asking: the wire stage

- **RUN WITH shows while Korri's runner choice is open, whoever opened
  it.** The treaty has no call that opens a runner choice for a game. The
  portal opens it itself on a launch and on its own game action (RUNNERS ON
  THIS DEVICE, which GAME OPTS. lists as Korri's), and while it is open the
  portal sends Back to its own cancel and no other button to Zone. So the
  page follows `model.runnerChoice`. GAME OPTS. has no RUN WITH row of its
  own (see Polish); C11 is proved through Korri's action.
- **Navigation took two messages of the wire stage's**, in
  `ZoneNavMessage` beside `ZoneMessage` (so `messages.ts` is untouched):
  `FocusedPage` (the cursor left the tabs for OPTIONS' rows or SAVE DATA's
  cards, so `menu` knows to bring it back) and `LandedOptions` (clears
  `optionsGroup` once `focusSystemGroup` has run). `system` now also puts
  `zone` off the tabs, so the cursor lands on the SYSTEM row, not a tab.
- **The binding moves DOM focus to a tab on OPTIONS and SAVE DATA** when
  navigation says the cursor is on the tabs (a tab chosen, `menu`). Those
  pages do not do it themselves; GAMES does.
- **Idle belongs to the zone select.** `IdleElapsed` is dropped unless
  GAMES is shown, so the title card never covers OPTIONS, SAVE DATA, RUN
  WITH, PAUSED or first boot.
- **First boot's secured network** goes to OPTIONS' join flow: the wire
  stage presses that network's row in the OPTIONS model, its keyboard rises,
  and the OPTIONS page is shown (keyboard up, cursor trapped) until DONE
  sends `connectWifi(networkId, password)` or B puts it down; first boot
  comes back on the WI-FI step.
- **PAUSED's model starts over each time Korri presents the overlay**, so
  the cursor opens on Korri's first control.
- **PAUSED is wrapped in a dialog named for the game** (`role="dialog"`,
  Korri's title or PAUSED). The portal's own test renders every overlay
  surface and finds its overlay by that role and name.
- **The foot's device tag** shows the value of Korri's device-name setting,
  found as first boot finds it (by its id), editable or not. Absent, the
  tag is empty.
- **Ground.** The root draws ZoneGround under GAMES, OPTIONS, SAVE DATA and
  RUN WITH; PAUSED and first boot draw their own.
- **Wide art.** A game with no cover but wide art wears the wide art,
  letterboxed in the cover well like any other art (`zoneBandCards` reads
  `coverArtUrl ?? wideArtUrl`). A cover wins when both exist, since Zone's
  cards are portrait. The GAMES part's `WideArtOnly` state shows it (C2).
- **Portal.** Zone is registered as `catalog` and `gameplay-overlay`, aliased
  like Pico in `tsconfig.json`, `vite.config.ts` and `test/happydom.ts`. The
  device bundle (`clients/portal/nix/package.nix`) carries Zone's source,
  installs its `qrcode` as Pico's is, ships Anton's OFL beside Pico's font
  licences and checks `zone-screen` is in the CSS; its dependency hash moved.
- **Nix.** One task, `nix run .#zone-check` (tests, typecheck, lint), the
  shape of `pico-check`. `portal-check` and `portal-bundle` install Zone.
- **Two assertions in `test/zone-games.test.tsx`** that looked for the
  OPTIONS and SAVE DATA placeholder panel now look for the pages.

## Capability map

Every line names Zone's own answer and the test in
`test/zone-navigation-acceptance.test.tsx` that reaches it from home with
d-pad, confirm and back, through `ZoneSurface` and a host that records calls.

```
C1  GAMES band: three stripe wells and LOADING, Korri's message under GAMES with RELOAD, NO GAMES, the cards — test: zone-navigation-acceptance "C1 catalog states"
C2  ZoneCardBand cards: title, subtitle tag, a section spine per section, cover art, else wide art letterboxed, else the stripe well with initials — test: zone-navigation-acceptance "C2 browse"
C3  facts line LAST PLAYED, PLAYS, TIME under the band; CONTINUE for a resumable game — test: zone-navigation-acceptance "C3 game facts"
C4  action row: A on a one-place card launches; a choice drops into PLAY HERE / PLAY ON <device> — test: zone-navigation-acceptance "C4 launch"
C5  GAME OPTS. (OPTS. at the end of the action row; options is its shortcut): Korri's actions as rows, a disabled one with its reason, NO / YES before a destructive one — test: zone-navigation-acceptance "C5 per-game actions"
C6  FIND key (menu twice) and the keyboard filtering the band, FIND: <draft> kept, B lets go — test: zone-navigation-acceptance "C6 find"
C7  ZoneStatusLayer busy strip: kicker, detail, the game in yellow, Korri's CANCEL; the page inert — test: zone-navigation-acceptance "C7 busy"
C8  ZoneStatusLayer running strip: kicker, OPEN, the game's title; the page stays usable — test: zone-navigation-acceptance "C8 running"
C9  ZoneStatusLayer problem dialog: kicker, reason, Korri's game title, DISMISS and RETRY; B dismisses — test: zone-navigation-acceptance "C9 problem"
C10 RELOAD in the catalog's error dialog under GAMES — test: zone-navigation-acceptance "C10 reload"
C11 RUN WITH page: state tag, message, warnings, a row per route (program, build, SAVED), Korri's actions — test: zone-navigation-acceptance "C11 runner choice"
C12 OPTIONS: Korri's groups under their titles, rows with label and value, the description under the focused row — test: zone-navigation-acceptance "C12 groups and items"
C13 OPTIONS rows: choice steps left/right, text and secret on the keyboard, CLEAR, Korri's confirmation dialog — test: zone-navigation-acceptance "C13 interactions"
C14 OPTIONS: the Saving caret on the row, Korri's message in red and a DISMISS row — test: zone-navigation-acceptance "C14 saving and problem"
C15 OPTIONS SYSTEM group (system button): Korri's device actions, NO / YES before a destructive one — test: zone-navigation-acceptance "C15 device actions"
C16 SAVE DATA BACK UP card: password keyboard, the busy strip, the QR code in the well and the secret on its slab — test: zone-navigation-acceptance "C16 export backup"
C17 SAVE DATA SWITCH FROM BACKUP: secret, password, LOCAL RECORDS TRANSFER / DELETE, SWITCH KEY? — test: zone-navigation-acceptance "C17 switch from backup"
C18 SAVE DATA SWITCH BUNKER: the URI keyboard from bunker://, disposition, SWITCH KEY? — test: zone-navigation-acceptance "C18 switch by bunker"
C19 SAVE DATA RETIRED cards with DELETE: DELETE THIS KEY? then IS IT BACKED UP? — test: zone-navigation-acceptance "C19 retired keys"
C20 SAVE DATA: Working as the busy strip, Problem headed IDENTITY with DISMISS, SWITCHED with the key and OK — test: zone-navigation-acceptance "C20 identity status"
C21 the foot's Wi-Fi badge and OPTIONS' CONNECTION row: SSID, bars and %, WIRED, OFFLINE — test: zone-navigation-acceptance "C21 connection"
C22 OPTIONS WI-FI: networks strongest first with LOCK and KNOWN, SCAN, SCANNING…, the caret while joining, the failure in red, the keyboard back on a rejected key, NO WI-FI ADAPTER — test: zone-navigation-acceptance "C22 wi-fi list"
C23 OPTIONS WI-FI: a locked network's password keyboard, an open one at once, HIDDEN NETWORK name then password — test: zone-navigation-acceptance "C23 join"
C24 PAUSED: Korri's title on the red band, controls then groups in the yellow wedge, toggle/choice/range on left and right, reasons, NO / YES, B back to the game — test: zone-navigation-acceptance "C24 overlay"
C25 first boot: CONTROLLERS, WI-FI (OPTIONS' join keyboard for a locked network), NAME, IDENTITY's NEW SAVE and FINISH — test: zone-navigation-acceptance "C25 first boot"
C26 the foot: TIME as Korri's clock, the device's name, the build string, on every tab — test: zone-navigation-acceptance "C26 clock and build"
C27 idle: after 30 s the attract for the focused game (its title card, then its stage), the next every 6 s, on GAMES only; any input wakes it on the same card — test: zone-navigation-acceptance "C27 idle", "attract wakes to the same card"
C28 options = GAME OPTS., menu = the tabs, system = OPTIONS' SYSTEM group; every tab by d-pad and A, home by B — test: zone-navigation-acceptance "C28 input actions"
C29 the one slanted keyboard: a 40-character backup secret, capitals and symbols as layers — test: zone-navigation-acceptance "C29 text entry"
C30 the pink edge and underline on whatever holds DOM focus, on every page and layer; reduced motion stops all motion — test: zone-navigation-acceptance "C30 focus and motion"
C31 ZoneStatusLayer question: the blue ZoneDialog with Korri's kicker, detail and the game in yellow, one button per host action in its order, the first lit, Back answering nothing; the page inert and dimmed — test: zone-navigation-acceptance "C31 confirm"
C32 ZoneBusyStrip meter: SPECIAL STAGE's red slanted bar, its share and DONE / TOTAL; with no total the size so far and sweeping stripes; without progress, the strip as before — test: zone-navigation-acceptance "C32 progress"
C33 ZoneCard's orange DOWNLOAD tag with the size, and DOWNLOAD <size> at the end of the GAMES facts line — test: zone-navigation-acceptance "C33 download"
```

### Polish (decided without asking)

- **The shell measures its own width for the top strip.** The brief wants
  the strip's ladder as `planTopStrip(width)` with CSS following a data
  attribute, and `ZoneSurface.tsx` (which passes `strips` and `band`) is
  the wire worker's. So `useZoneTopStrip` observes the shell's box and
  writes `data-top` on it, as `use-zone-fit` writes `--zone-fit`: no
  re-render, no state. A box of width 0 (a test DOM) keeps no attribute.
  The shell is the container whose width matters, so this also holds for
  a shell drawn in a pane.
- **The top strip's ladder** is `inline` (620 and up), `tight` (530: gaps,
  margins, panel padding and type at their least), `keys` (400: hints keep
  only the round keycap; `role="img"` with `aria-label` "B Back" keeps the
  word), then `stacked` (two rows; the hints take their words back while
  the row is 240 wide or more). Thresholds measured in Chromium at the
  narrow type with today's tab words; a longer tab word needs them moved.
- **The tabs and the foot fit their type to their row.** In two rows the
  tabs still want about 300 px at the least type, so `ZoneTabStrip` uses
  `useZoneFit` (to half) and the banner follows the fitted tab type; a tab
  only shrinks and truncates at the fit's floor. The foot overlapped
  itself at 360, so `ZoneFoot` fits its type the same way after the SSID
  has given way. At 260 the foot reaches the floor and the build string is
  tiny; that is the weakest size.
- **The title card's circle moves under the title on a card taller than
  wide** (`ZONE_TITLE_STACK_ASPECT`, 1, in `src/plan/title-card.ts`, held
  equal to the container query by test). The title wraps only between
  words (`word-break: keep-all`, balanced lines) and `useZoneFit` shrinks
  it until its longest word fits. `useZoneFit` gained an optional floor;
  the title's is a quarter, not half, so a single word wider than the card
  stays whole (tiny) instead of cut. A column's title type follows 11% of
  the width, not 7.5%, since the column has height to spare.
- **Narrow bands draw hairline spines, and every band counts its spines.**
  On the three-card band a section divider is a 6 px stripe in a black
  hairline frame (`--zone-section-hairline`); its caption stays as the
  separator's accessible name. The caption's facts survive on the cards'
  own tags (HERE, the device, CONTINUE). Both bands reserve room for the
  most dividers a side of the focus ever holds (`bandSpinesPerSide`), the
  same number for every focus, so the cards never change size as the
  cursor crosses a section. Cost: on a sectioned catalog the cards are a
  little smaller at width-bound sizes (about 5% at 640x480, about 15% at
  1280x720 when a side can hold two full spines).
- **Korri's backup takes the band's room.** BackupReady replaces the band
  with the QR code (the largest square left after the slab's least room,
  `--zone-save-slab-least` 220 px across or `--zone-save-slab-least-block`
  130 px down) and the slab beside it, or under it when the page is taller
  than wide (`ZONE_SAVE_BACKUP_BESIDE_ASPECT`, 1, which replaces the old
  1.25 band-and-slab rule). The cards come back when B dismisses the
  backup, with the cursor where it was; that is one press, and the B hint
  says so. The well keeps a hairline of black, not 10 px, round the
  code's own four-module quiet zone. Measured in Chromium, the code is
  317 px at 640x480 (5.2 px a module), 168 px at 1280x300 (2.75 px a
  module), 454 px at 480x800 and 502 px at 1280x720. This supersedes the
  QR in BACK UP's well, the `data-clear` padding and BackupReadyOnCard.
- **A retired key offers BACK UP and DELETE.** Supersedes "Retired keys
  are deleted, never backed up". While a retired card has the cursor a
  `ZoneActionRow` under the band offers both, BACK UP lit. A on the card
  drops into the row on BACK UP (`flow: retired`), as A on a game with a
  choice of places does; it sends nothing. BACK UP runs this key's
  password step with `retiredPublicKey`, and Korri's backup shows as
  C16's does. B steps back from either action's first step to the row,
  and from the row to the card. The card loses its red DELETE tag, since
  A no longer deletes. The band keeps the row's room whenever Korri lists
  a retired key, so it does not jump; on a strip 4 times wider than tall
  the row stands at the band's end instead of under it. The backup view
  does not say which key it holds; the person chose it one step before.
- **ZoneKeyboard and ZoneDialog report focus themselves.** Supersedes the
  glyph and index reading noted under GAMES, SAVE DATA and OPTIONS:
  `ZoneKeyboard` calls `onFocusKey(id)` (the `zoneKeyId`) and `ZoneDialog`
  calls `onFocusAction(index)` from each button's `onFocus`. GAMES,
  OPTIONS, SAVE DATA, PAUSED, `ZoneStatusLayer`'s problem dialog,
  `ZoneOptionsEntry` and `ZoneSaveEntry` use them. GAMES moved DOM focus to
  a key by reading its glyph; each key now carries `data-key-id`, and the
  page finds it by that. `ZoneBusyStrip` calls `onFocusAction(index)` too,
  and first boot's keyboard reports `FocusedKey` to its model, so every
  keyboard, two-way dialog and busy strip follows DOM focus
  (`test/zone-focus-callbacks.test.tsx` checks each use passes its
  callback).
- **OPTIONS and GAME OPTS. use ZoneOptionRow's `valueNode` and
  `disabled`.** Supersedes "Rows are strings" and "An inert device action
  ... UNAVAILABLE" and GAME OPTS.' description-as-value. The plan marks a
  Wi-Fi row with a `signal` and an inert row `disabled`; the page draws
  the connection row as `ZoneSignal` (at the row's type, through
  `--zone-signal-type`) and a network row as `ZoneBars` then its words
  (72% LOCK KNOWN). The words stay the row's value and accessible name; no
  glyph bars remain. `ZoneBars` is a new atom, and `ZoneSignal` draws its
  bars with it. A disabled row keeps its verb (GO, RUN), dimmed, and
  `ZoneOptionRow` now shows a disabled row's description whether or not
  it is focused, so Korri's reason is always on screen.
- **GAME OPTS. has no RUN WITH row.** Supersedes "RUN WITH is a row whose
  value is CHOOSE". The treaty has no host call that opens a runner
  choice: the RUN WITH page shows only when Korri itself publishes
  `model.runnerChoice` (usually after the person runs Korri's own game
  action for runners, which GAME OPTS. lists). The row could never do
  what its label said, so the row, its message and their tests are gone.
- **The status layer's dim covers the page's own layers.** The page under
  `ZoneStatusLayer` now sits in `.zone-status-layer-content`, its own
  stacking context (`isolation: isolate`, `z-index: 0`) inside the
  isolated `.zone-status-layer-page`, and the sheet is `z-index: 1` over
  it. Before, FIND (`z-index: 5` on the GAMES page) painted through the
  dim. The GAMES part's `BusyOverHome` state shows it.

## The pixel refinement: foundation (decided without asking)

The refinement brief (`docs/design/surfaces/zone/REFINEMENT.md`) supersedes
the "eight colours" and "one face: Anton" rules above. This section records
the shared pieces the foundation worker built and every choice it made
without asking. The four screen workers build on them.

### The shared pieces, for the screen workers

| Piece | Where | How to use it |
|---|---|---|
| The art pixel | `src/plan/art-px.ts` (`zoneArt`, `zoneArtPx`, `ZONE_ART_ROWS` 240) | `--zone-px = max(1, floor(min(W, H) / 240))` CSS px, a whole number. The root writes it (`useZonePlan`: `data-px` and `--zone-px` on `.zone-screen`); a part outside the root derives the same number in `zone.css`, a `ZonePartFrame` in its own `.zone-part-frame-screen`. A hook that measures its own box reads it with `zoneArtPxOf(element)` (`src/use-zone-plan.ts`). |
| Lengths | `src/zone-tokens.css` | Every length token is a bare number of art px. Use it as `calc(var(--zone-slant) * var(--zone-px))`; never re-declare a token with a length (`--zone-panel-pad: var(--zone-strip-gap-least)`, not its `calc`). Only the part frames' sizes stay in CSS px. |
| Plans | `src/plan/zone-select.ts` (`plan`, `planArt`), `top-strip.ts`, `onboarding.ts` | Thresholds and lengths are art px. `plan(width, height)` divides a CSS size first; `planArt(wArt, hArt, px)` takes the grid. `ZonePlan` carries `px`, `wArt`, `hArt`. |
| Type | `--zone-caps-1/2/3` (10/20/30), `--zone-fact` (18.6667) | Zone Caps' em is its cap, so a font-size of `calc(var(--zone-caps-2) * var(--zone-px))` draws caps-2. Type tokens are multiples of 10. A fact (Jersey 10) is `--zone-fact / --zone-caps-1` ems of the caps around it, which puts one Jersey pixel on one art pixel among caps-1. |
| Lettering | `ZoneCaps` (`src/ui/atoms/ZoneCaps.tsx`) | `ink`: `white` (default), `hud`, `banner`, `ink`, `danger`; `size`: the strip names or `caps-1/2/3`; `fact` sets Korri's words in Jersey 10 in their own case (replaces `verbatim`). The white ink takes its face from the colour around it; a component that tints its text sets `--zone-caps-shade: currentColor` with the colour so the lower rows follow (already added to every rule that sets a colour). |
| The fit | `useZoneFit` (`src/use-zone-fit.ts`) | Steps whole caps only (3, 2, 1), snapping a share-of-width size down to the step under it; writes `--zone-fit` and `data-zone-fit`; at caps-1 it marks `data-zone-fit-floor`, where the stylesheet wraps between words or truncates with "…". The old half-size floor argument is gone. |
| Stairs | `src/plan/stair-clip.ts` | `zoneStairClip(wArt, hArt, { left, right })` (edges `straight`, `slash`, `backslash`, `point`), `zoneStairWedge(size, corner)` for the dialog's and PAUSED's wedges, `zoneStairCircle(d)` for a pixel disc. `useZoneStairClip(ref, edges)` measures a box and writes `--zone-panel-shape`, `--zone-panel-run` and `--zone-panel-point` on it. `ZonePanel`, `ZoneBanner` and the active tab use it; the dialog wedges do not yet. |
| Grounds | `zone-tokens.css` "the grounds" | `data-ground` = `games`, `options`, `savedata`, `gameopts`, `paused`, `titlecard`, `tally` sets `--zone-ground`, `--zone-ring-a`, `--zone-ring-b`, `--zone-shade`, `--zone-ground-rings`. The root sets it from the page shown; `ZoneShell` and `ZoneGround` take a `ground` prop for a subtree (the GAME OPTS. sheet, the title card, a part). The group extras of 2c are tokens in the refinement block (`--zone-stripe-light`, `--zone-stripe-teal`, `--zone-title-red/blue/green`, `--zone-grey-dark/light`, `--zone-sky`, `--zone-track`, `--zone-danger-shade`). |
| The ground | `ZoneGround`, `src/use-zone-ground.ts`, `src/plan/ground.ts` | A canvas `wArt` by `hArt` at exactly `--zone-px` a pixel, `image-rendering: pixelated`; `renderZoneGround(painter, t, palette, wArt, hArt)` is pure and seekable. |
| Chrome | `ZoneBanner`, `ZoneKeycap`, `ZoneUnderline` | The banner is caps-2 banner ink on `#0020B5` with stair points and the red/green lip with four notches (it hangs under the body and takes no room). The keycap is a 15-px pixel disc with a black rim, the letter in B `#EF285A` / A `#639F3D`, the word in white ink on a black tail cut at 45 degrees 3 art px past it. The pink drop is 2 art px, 3 when its control is focused (`--zone-underline-depth: var(--zone-drop-focus)`). |

### Decided without asking: the foundation

- **Zone Caps' em is its cap.** One art px is 100 units and the em is 1000
  (10 art px), so `caps-1/2/3` are font-sizes of 10, 20 and 30 art px. The
  ascent holds 3 px of accent headroom (1300), the descent 3 px (300).
  `line-height: 1` puts the cap exactly in the line and the baseline on a
  whole art px. `ZoneCaps` pads its box for the accents (0.3em), the tails
  (0.2em) and the ring and drop (3 art px) and gives the room back with
  negative margins, so its layout box is the line.
- **Glyph shapes.** Two corners also notched on B, P and R's bowls and on
  the digits that curve (2, 3, 5), beyond the brief's list. `/` and `%`
  step two rows a column: a 1:1 stair ten rows tall needs eleven columns.
  `#`, `@` and `&` use 1-px strokes where 2 px cannot fit in six columns.
  Accented capitals are composed from a base and a mark drawn once on the
  grid (`compose` lines); the marks over I are 4 px wide and overhang its
  gap. Æ, Œ and Ĳ are 9 wide like M and W. Ð/Đ share one glyph. Every
  Latin-1 and Latin Extended-A capital is drawn; ŉ and ĸ (no capital
  form) are not.
- **The build needs brotli.** `scripts/build-zone-caps.py`'s nix-shell line
  adds `python3Packages.brotli` to the brief's `fonttools`: fontTools
  cannot write woff2 without it. It writes `.ttf` too, which the test reads
  with its own small TrueType reader. The reproducibility test reruns the
  script into a temporary folder when `nix-shell` is on the PATH and skips
  (saying so in its name) where it is not.
- **Jersey 10's size.** Jersey 10 draws one pixel per 75 units of a
  1400-unit em (its `H` is 525 by 750), so the brief's 14 px x `--zone-px`
  would put a Jersey pixel on 0.75 of an art pixel. `--zone-fact` is
  18.6667 art px instead: one Jersey pixel per art pixel, cap 10, x-height
  8, the same cap as caps-1. A fact sets its line at 0.75 of its em (14 art
  px), which lands its baseline on a whole art px.
- **Jersey 10's files.** Subset with pyftsubset to Basic Latin, Latin-1,
  Latin Extended-A and -B, Latin Extended Additional, general punctuation,
  the block elements and a few symbols (`Jersey10-latin.woff2`, 18 KB). Its
  licence keeps the upstream name, `src/fonts/OFL.txt`, so the portal's
  bundle, which copies that file, still builds (it now ships Jersey's OFL
  under the name `anton-OFL.txt`; see "Not mine to change").
- **`verbatim` became `fact`.** Zone Caps has no lower case, so a name
  kept in its own case can only be Jersey 10; the two callers moved. The
  rows, dialogs and save cards that mark a fact with `data-case="verbatim"`
  set it in Jersey 10 at the caps' height too.
- **The two-tone face is a copy over the words.** `background-clip: text`
  paints the text shadow over the fill in Chromium, so the ring and drop
  are a generated copy under the words and the lower rows a copy over them
  clipped to the last three of ten; both copies have an empty alternative
  (`content: attr(data-text) / ""`), so a screen reader reads the words
  once.
- **The art-px conversion.** Each length token was divided by the art
  pixel of the device it was tuned for: by 2 for most (640x480 keeps its
  sizes and 720p grows 1.5x, as Mania's canvas does), by 3 for the
  720p-only strip heights, rounded, with the brief's numbers where it gives
  them (drop 2 and 3, underline 2, bars 2 wide and 8 tall). Type tokens
  went to caps-1, -2 or -3 by role.
- **Plan thresholds on the art grid.** Wide is `wArt >= 333` (the brief's
  example). Tall strips are `hArt >= 320`: every landscape screen holds 240
  to 479 art rows, so 640x480 and 1280x720 both plan as short now (the same
  canvas height), and a portrait screen is tall. The facts join the action
  strip below 240 art rows, which only a box under 240 CSS px tall reaches;
  1280x300 (one CSS px an art px, 300 rows) keeps the facts row. The tab
  strip is 36 art px short and 40 tall, room for the caps-2 banner and its
  lip.
- **The top strip ladder, measured in Chromium at the new type**: one row
  needs 483 art px, 446 at the least gaps, 339 with keycaps alone; the
  thresholds are 490, 450, 345 and 170 (words in two rows). At 1280x720
  (426) the hints keep only their keycaps; at 640x480 (320) the hints take
  a row of their own with their words.
- **Tab sub-lines are read, not drawn.** Zone Caps has no size under caps-1,
  and a caps-1 sub-line would double each tab's width; SETTINGS + WI-FI and
  IDENTITY stay in the tab's accessible name. Supersedes "Tab subtitle
  lines".
- **The tabs shrink only at caps-1.** The strip's fit marks
  `data-zone-fit-floor`; only then may a tab shrink and truncate
  (`--zone-tab-shrink`). At 480x800 the three tabs need 286 art px of 240,
  so SAVE DATA truncates there.
- **Panels lean at the measured 45 degrees.** The slant is the panel's
  height less one row; until a panel is measured (a test DOM) it is a
  smooth parallelogram of 1.5em.
- **A panel's drop is the ground's shade** (`--zone-panel-edge:
  var(--zone-shade)`), and the old blue cap edge (`--zone-cap-edge-colour`)
  is black, so the blue lettering edge is gone everywhere without touching
  the pages. `--zone-drop-edge` and `--zone-drop-edge-focus` were renamed
  `--zone-drop` and `--zone-drop-focus`, the brief's names.
- **PAUSED's ground is the light grey** (`#8C8C8C`), the dark grey its
  rings: the build string's black reaches 6:1 on it, 3:1 on the dark one.
- **First boot and FIND stand on the GAMES ground; RUN WITH and OPTIONS'
  join keyboard on OPTIONS'.** GAME OPTS. leaves the root on GAMES: its
  sheet is a panel of its own (`ground="gameopts"`) for the GAMES worker.
- **The ground's rings** stand as shares of the canvas (ring A across the
  middle, ring B upper right) and carry two dots and one; the old SVG's
  bands, arc, marks and every ground opacity are gone with their tokens.
  Supersedes "Ground opacities stay" and "The ground's Caliper spill".
- **The banner's lip** is a fixed polygon: a 2-art-px bar and four notches
  4 wide and 2 deep (45 degree stairs) at its centre; `--zone-banner-lip`
  names its 4-px height.
- **The keycap's letter** stands 2 art px under the rim's top, the nearest
  whole pixel to centre (a 10-px cap in a 15-px disc).
- **Korri's sentences may break a word only at a line's end, with a
  hyphen** (`overflow-wrap: break-word` with `hyphens: auto`): the busy
  strip's kicker and detail, the dialog's line, RUN WITH's message and
  warnings, PAUSED's notes. Every other rule that broke anywhere now keeps
  words whole.
- **The canvas allowance** in the authoring gate names two files,
  `ui/organisms/ZoneGround.tsx` and `use-zone-ground.ts`; a second test
  fails if either stops drawing, so the allowance cannot outlive its use.
- **The contrast gate parses the grounds**: each `data-ground` block is
  judged on its own, and the gate fails if a group the brief names has no
  block.

### Known gaps, left for the screen workers

- **Fractional type.** Sizes that are shares still land between art
  pixels where no fit snaps them: the foot's readouts (timer label 0.74,
  device 0.6, badge 0.56, build 0.4 of the foot type), a tag's 0.58 of the
  card title (the tag row's fit snaps the row, not each tag), the section
  caption's share, the save card's well text (`min(14cqw, 30cqh)`), the
  title card's act word and PAUSED's group labels (0.75). Each needs a caps
  step or a fact.
- **The dialog wedges and PAUSED's wedge** are still smooth polygons;
  `zoneStairWedge` is ready for them.
- **Rows' own text** (option rows, dialog lines, the busy strip) is Zone
  Caps through the page font but not `ZoneCaps`, so it has no ink ring; the
  rows still draw their own one-art-px edge in black.
- **Korri's sentences are still upper-cased Zone Caps**, not Jersey 10 as
  written: that is each page's change (reason, detail, description,
  message, warnings).
- **Not mine to change: the portal bundle.** `clients/portal/nix/
  package.nix` copies `surfaces/zone/src/fonts/OFL.txt` as
  `anton-OFL.txt`; it should copy `OFL.txt` as `jersey10-OFL.txt` and add
  `ZoneCaps-OFL.txt`. Suggested issue.

## The GAMES refinement (decided without asking: games)

The games worker refined GAMES, GAME OPTS., FIND, the idle attract and the
launch moments (REFINEMENT.md sections 2d, 3 and 4). Each choice the brief
left open is recorded here.

| Piece | Where |
|---|---|
| The launch title card (M3) | `ui/organisms/ZoneLaunchCard`, model `launch` in `state/navigation.ts` with `state/tally.ts` |
| The results tally (M5) | `ui/organisms/ZoneTally`, model `session` and `tally`, words in `plan/tally.ts` |
| The idle attract (M6) | `ui/organisms/ZoneAttract` over `ui/molecules/ZoneTitleCard` |
| The moments' clock | `use-zone-moment.ts` (`useZoneMoment`, `window.seek(t)`, `window.play()`); frames in `plan/motion.ts` |
| Words that wrap only between words | `ui/atoms/ZoneWords`, fitted by `use-zone-words-fit.ts` |
| Acceptance | `zone-navigation-acceptance` "title card on launch", "results tally", "attract wakes to the same card" |

- **The launch card waits for Korri, and a press before Korri answers lets
  go of it.** It opens on PLAY or CONTINUE (A on a one-place card too),
  with the title, the system and the act as they stood then. Until Korri's
  status names the game (Busy, Confirm or Running, or one of those naming
  no game) it is only Zone's announcement: the page under it stays live, so
  a move or a host button lets go of it and does what it does. A host that
  never answers therefore never strands it. Once named, the page goes inert
  and the card holds every press until the status names the game no more.
  A Problem before Korri answers sends it out too.
- **Both moments leave by moving.** `leaving` is part of the model; the
  binding removes the card after M3's 400 ms way out (`TitleCardLeft`) and
  the tally after M5's 300 ms (`TallyLeft`). While leaving they hold no
  press.
- **An absent playCount is none.** The treaty says playCount is absent
  together with lastPlayedAt, that is, no session recorded. So the act is
  `(playCount ?? 0) + 1`, and a first session (absent, then 1) is a rise of
  exactly one and gets a tally, with TIME counted from 0 seconds. Any other
  absence makes TIME or TOTAL absent, never invented.
- **The session is noted at the first Running for a game**, kept through
  Busy, Problem and Confirm, replaced by another game's Running, and read
  at the next Browsing. A tally opens only while GAMES is shown with none
  of Zone's own layers over it; on another tab no tally shows (it would
  hold presses on a page that does not draw it).
- **The tally is one button.** A on it is a press like the host's
  buttons; the first skips the count-up (`counted`), the next closes it.
  The count-up's end (`TallyCounted`) is the runtime's timer, so a press
  after it closes at once. A direction moves no DOM focus (nothing else is
  focusable), so on the device it neither skips nor closes; B, menu,
  options, system and A do. Bars: TIME's bar is this session's share of
  the new total; PLAYS and TOTAL run full. The values count with CSS
  counters from the clock's whole numbers, so the whole value is also
  there as text for a screen reader.
- **The busy strip and Confirm stand on the card because they belong to
  the status layer**, which draws over the page that holds the card. The
  status layer's veil also lies over the card while Busy; the layers
  worker owns it. `ZoneLaunchCard` has a `foot` slot for a strip of its
  own, empty today. Suggested: the veil skips a page that shows a launch.
- **The attract's stage stands under its card the whole turn.** A turn's
  first frames sweep the next card in over the game's art, never over the
  page. A game with no art gets the card's light, blue and teal stripes as
  its stage. PLAYS shows the dashes when Korri has none. Under reduced
  motion the card stands at rest and nothing blinks; the turn still moves
  on every 6 s (that is content, not motion).
- **The moments are a seekable clock beside the ground's.** The authoring
  gate's frame-clock allowance names `use-zone-moment.ts` too: the brief
  asks for a `render(t)` a reviewer can hold, which a transition is not.
  It writes custom properties only, and paints one rest frame under
  reduced motion. Parts that show a moment hold it at a named time
  (`fixtures/fixture-moments.ts`), so a still is the same every run.
- **M1 is the band's track on the clock; the cards' sizes stay a 220 ms
  transition.** The track's two counts (focus and dividers) are
  interpolated together, from wherever the track stands, so a move that
  starts mid-move does not jump. The track's translate is rounded to whole
  art px. The triangles nudge 2 art px in two steps every 480 ms.
- **M2 has no outgoing half.** The binding swaps the page at once, so
  there is nothing left to slide out. A chosen tab marks the ground it
  leaves (`markZoneTransition`); the arriving shell (a new page, or a
  changed `page` on the same shell) wipes that colour away from the top
  right in 4-art-px stairs, drops the banner 8 art px and slides the body
  in from the right, all inside 500 ms. A page that mounts for any other
  reason (power-on, a part) arrives still. Rows are not staggered one by
  one: the body moves as one, since the shell cannot know a page's rows.
- **Tags never lose a letter.** A tag's type is rounded down to a whole
  caps step (also for the save card, the controller card and the running
  strip, whose holders ask for shares). Each tag starts at its word's
  width (`flex: 1 1 auto`, was equal shares). A row that still cuts a tag
  at caps-1 hides its last tags one by one, keeping the system first, then
  the places, down to none when even the system's tag would be cut. The
  focused card reports how many it kept, and the facts
  line shows the rest (not DOWNLOAD, which is a fact there already). The
  spill is written on the next task, outside the resize notification, so
  no observer loop.
- **Titles step and wrap with `ZoneWords`.** Each word is its own
  ZoneCaps, so a line breaks only between words and every word keeps its
  ring and two-tone face. `useZoneWordsFit` adds up the words' letters and
  gaps (a scroll width misses words that run out past a right-aligned
  line's start, and counts the ring room). A focused card's word longer
  than the card stands whole past its edge; a side card with such a word
  draws no title at all (its cover and its name still say which game it
  is) and shows it whole once focused. No title ever loses a letter. A title of several words is read once, whole,
  from a hidden copy, and its drawn words are hidden from a screen reader.
- **The facts line is one strip with a one-line lean.** A wrapped line
  keeps 45 degree ends one line tall, so wrapping never widens the ends and
  starts a loop. Each spilled tag and each fact is an item of one run.
  Absent: LAST PLAYED --, PLAYS --, TIME --'--"--. Values are Korri's
  facts in Jersey 10; the dashes are Zone's words in Zone Caps.
- **The top strip, measured again.** With each hint's column its own
  width (`minmax(max-content, 1fr)`) and the tabs' padding as space
  (`--zone-games-tab-pad-y-least`), one row needs 472, 414 and 307 art px;
  the thresholds are 476, 420 and 312, and two rows keep the words down to
  152. 1280x720 keeps BACK and CONFIRM; 640x480 is one row of keycaps
  (two rows before; the yield order puts words before rows); 480x800 is
  two rows, its tabs wrapping onto a row of their own. A tab never
  shrinks or truncates. The inactive tab draws its own stair panel so its
  padding can yield; ZonePanel's is fixed.
- **GAME OPTS. is ZoneOptionRow rows under its own head.** The page draws
  the head (GAME OPTS. caps-2 on black, the game in Jersey 10 on blue) and
  maps Korri's actions to ZoneOptionRow itself, since ZoneOptionGroup
  always draws a title. The rows keep that molecule's focus, disabled and
  destructive looks; the triangles flanking the value are not drawn
  (ZoneOptionRow is the options worker's).
- **The keyboard's letters.** A key whose character has a capital is
  lower case and is set in Jersey 10; digits, capitals, symbols and the
  control words are Zone Caps at caps-1. The count is HUD ink. The FIND
  draft holds `ZONE_FIND_MAX` (40), so the FIND keyboard shows `n/40`.
- **Static** is three hard-stop checkers at prime tile sizes (3, 5, 7 art
  px) jumped to five places 100 ms apart. It repeats every 500 ms; true
  noise would need a canvas. NO GAMES is one card that cannot be chosen,
  the words on its white header bar over a well of static.
- **The foot on the grid.** TIME is HUD ink, the clock caps-1, the device's
  name and the build in Jersey 10. The build is no longer dimmed (an
  opacity is a blend) and yields with "…" after the SSID; at 640x480 it
  shows `korri 0.4.1 · …`.
- **Not done.** PLAYS ×07 (Could). The keyboard's keys still lean by the
  slant token, not 45 degrees. The card triangles are smooth polygons, not
  stairs. Live takes at 1280x720 failed on this machine (the recorder fell
  behind); every move has a seek-built take at both sizes instead.

## SAVE DATA and first boot, refined (decided without asking: refine-savedata)

SAVE DATA reads as MANIA MODE's save select and first boot as COMPETITION.
New pieces: `ZoneEmblem` (Zone's own 16x16 drawings), `ZoneStepStrip`,
`ZoneVersus`, and `use-zone-save-band-fit.ts`. Tokens are in the
`refine-savedata` block.

- **The emblems are grids in code.** `ZONE_EMBLEMS` in
  `src/ui/atoms/ZoneEmblem.tsx` holds five 16x16 drawings, one letter a
  pixel; the black outline is computed (every empty pixel beside a filled
  one), so a drawing stays inside a one-pixel margin. Key = BACK UP and
  first boot's START; an arrow going into a card = FROM BACKUP; a silver
  link through a gold one = BUNKER; the key in greys = RETIRED and NOT
  AVAILABLE (no identity at all reads as a key that does not work here).
  A fifth, `pad`, stands in a heard controller card where COMPETITION puts
  a portrait; it replaces the initials well, which read as a seat number
  ("8P" for 8BitDo). The emblem takes the largest whole multiple of 16 art
  px that half its box allows.
- **The save card's anatomy.** A 4-art-px black frame; the header pale
  teal (`#9CE7C6`) with the yellow, red and blue stripes and the white bar,
  the label in `ink` caps; the window pale teal with a teal (`#5AA2B5`)
  diagonal, the emblem over the word on a black tag. The window's word is
  one element drawn in Zone Caps with a hard-stop two-tone fill clipped to
  the letters, not `ZoneCaps`: `ZoneCaps` never wraps, and Zone's words
  must wrap between words at caps-1 (FROM / BACKUP on a narrow card). On a
  black tag the ring would not show, so nothing is lost. A key keeps
  Jersey 10 and breaks only after its ellipsis (`<wbr>`).
- **The save card is 0.7 as wide as tall** (`--zone-save-card-aspect`,
  was 0.6), the least that sets FROM BACKUP on one caps-1 line at the
  card's cap of 133 art px. Both sides round down to whole art px.
- **One header step for the whole band.** Per-card fitting left SWITCH at
  caps-2 beside BACK UP at caps-1. `useZoneSaveBandFit` tries caps-2 on
  every header at once, then caps-1; a header that still overflows
  truncates. A card standing alone (first boot) fits its own header, so
  NEW is caps-2 there, as the brief asks.
- **The status is a bar across the foot** (`.zone-save-card-foot`), in the
  status's colour with dark ink on the light ones. Supersedes "Tags: NEW is
  orange …" in the SAVE DATA section: the words and colours stay, the tag
  shape goes.
- **The page names its ground.** `ZoneSaveData` passes `ground="savedata"`
  to its shell, so the cards' shade and every layer over them read the
  teal set even where a binding forgets to; the root already sets it.
- **The checker is the QR well's own background**, `earned` only on
  Korri's BackupReady: blue `#2E4BD6` and white, 8-art-px squares, centred
  on the paper. The paper (the SVG, with its 4-module quiet zone inside the
  viewBox) stands at least one square in from every side and is opaque, so
  the checker never reaches the quiet zone. The paper is a whole number of
  CSS px a module, not of art px: on the art grid 640x480 would drop from
  4 px to 2 px a module. Measured: 640x480 is 244x244 px (61 modules at
  4 px, quiet zone 16 px); 1280x300 is 183x183 px (3 px, quiet zone
  12 px); 1280x720 is 427x427 px (7 px).
- **The secret is Jersey 10 in whole CSS px a Jersey pixel**, not whole art
  px. 162 characters must be on screen at once for copying by hand; at one
  Jersey pixel an art pixel they do not fit beside the QR at 640x480 or
  1280x720. So the slab steps by `--zone-fact` px: 1280x300 lands on the
  grid (1 px), 640x480 at 1 px of a 2-px grid, 1280x720 at 2 px of 3. The
  one place on these screens that leaves the art grid, for a reason a
  reader needs. When one line has to break at its middle every line does,
  so the groups keep even columns. The slab's widest is its own token
  (`--zone-save-slab-widest`, 400 art px): the old widest (320) was set
  for Zone Caps and cut a Jersey line at 1280x300.
- **First boot's strip is the banner and four pips.** `ZoneStepStrip`
  draws the one banner with the step's name and hangs four diamond pips
  (5 art px inside a 1-px black outline) one lip and 4 art px under it;
  they take no room in the top strip. The group reads "Setup steps"; the
  pips read "Step 2 of 4". The step words lose their numbers ("1
  Controllers" became CONTROLLERS). Supersedes "under a strip of four
  tags" in the layers section.
- **Seats wear one black corner tab**, its left edge a 45-degree stair,
  1P to 4P in white caps-1. Supersedes "1P blue, 2P red, 3P yellow, 4P
  mint".
- **VS needs two seats, both heard.** The brief says "exactly 2 heard" and
  "with 3 or 4 seats none"; Korri may list a third, unheard controller, so
  the rule is: Korri lists exactly two controllers and both are heard.
  `planControllerCards(w, h, count, versus)` spends VS's room on the axis
  the cards stand along (80 art px across in a row, 28 down in a column).
- **A waiting card keeps an empty name bar**, so every seat is one height,
  as COMPETITION's WAITING seat is.
- **Controller names are one Jersey line** ending in "…": a two-line clamp
  cut a long single word mid-letter. The triangles stand aside on a card
  under 90 art px (`ZONE_CONTROLLER_ARROWS_LEAST`, written by the page as
  `--zone-seat-arrows`) so the name keeps their room. The full name stays
  in the card's label.
- **First boot's identity card** is headed NEW (was NEW SAVE, per the
  brief's copy list), START under the key, READY as its foot bar once
  chosen. The C25 acceptance step moves to `button("New")`.
- **Wi-Fi words on first boot already read SAVED, LOCKED and OPEN**, the
  set the options worker is moving OPTIONS to; nothing to switch here. If
  that branch renames them, `wifiRows` in `ZoneOnboarding.tsx` is the one
  place to follow.
- **The WI-FI step's group keeps its WI-FI title** under a banner that
  now also says WI-FI. The brief allows no new strings; a second word for
  the group would be one. Left as is, noted as a duplicate.
