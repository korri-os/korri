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

- **Catalog titles for now.** `ZoneSurface` lists the catalog's game titles
  as plain text in the shell's body until the card band lands, so the
  package renders end to end with a real `SurfaceModel`.

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
  which ZoneOptionRow does not listen to yet; A keeps every choice reachable.
- **A secret's keyboard starts empty.** Korri's value for a secret is its
  word about it ("Configured"), never the secret. DONE on an empty secret
  closes without a request; clearing is the clear key alone.
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
- **The body banner yields.** The brief puts a ZoneBanner reading OPTIONS
  in the body although the active tab already wears one; its type follows
  the body's height (`4cqb`, clamped) so a short screen gives it little.
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
  (`remote`). A key in a card or a dialog is upper case like every Zone word:
  bech32 and hex are case-insensitive, so the shortened key still compares.
- **NOT AVAILABLE keeps the banner.** "One card and nothing else" is read as
  the body's content; the banner is the page's title and stays so the page
  does not jump between states.
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
  DISMISS. Switched: SWITCHED over the new key shortened, OK. Disposition:
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
  tall (`ZONE_SAVE_BANNER_FOLD_ASPECT`, 1280x300) the banner folds: the tab
  strip already names the page. Both are aspect-ratio container queries,
  held equal to the constants by `test/plan-save-data.test.ts`.
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

## Layers: status, PAUSED, first boot

Three shapes, each driven by a pure model, built ahead of their mount in
`ZoneSurface.tsx`. Each takes Korri's slice and its model's value as plain
props and reports through callbacks; the binding turns a callback into the
model's message, and the model's `update` returns `ZoneRequest`s.

| Shape | Model (`src/state/`) | Korri slice | Capability |
|---|---|---|---|
| `ui/organisms/ZoneStatusLayer` | `status.ts` (`updateStatus`) | `status`, the catalog's titles | C7 to C10 |
| `pages/ZonePaused` (+ `ZonePausedControl`) | `overlay.ts` (`updateOverlay`) | `presentation` when `gameplay-overlay` | C24 |
| `pages/ZoneOnboarding` (+ `ZoneControllerCard`) | `onboarding.ts` (`updateOnboarding`) | `onboarding`, `network`, `settings`, `settingsStatus` | C25 |

- **Status layer.** Takes the page as its `page` prop and covers it:
  nothing while Browsing; Busy drops the busy strip over the dimmed, inert
  page with the game's title in yellow above it; Running is a small strip
  at the top (kicker, OPEN, title) that leaves the page usable; Problem is
  the blue dialog, DISMISS lit before RETRY. `reloadRequest()` in
  `status.ts` is C10, for the catalog's Error state and the SYSTEM group.
- **PAUSED.** The red band with Korri's title (PAUSED when absent), the
  ground greyed and striped as the game's stand-in, the yellow wedge of
  flat controls then groups. Left and right arrive as the portal's
  `korri-semantic-direction` event on rows marked
  `data-korri-horizontal-control` (and as arrow keys without a portal).
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
- **Onboarding's device-name lookup** is by label: the first settings item,
  in any group, whose label reads "Device name" (case and outer space
  ignored) and whose interaction is `text` (`deviceNameSetting` in
  `onboarding.ts`). Korri publishes it as `device-name` today; Zone does not
  rely on the id. Absent, the row is read-only.
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
- **Network rows show SSIDs in caps**: `ZoneOptionRow` has no verbatim case.
  Controller names keep their case on the cards.
- **Zero controllers show one WAITING card**, COMPETITION's empty seat.
  Seats are numbered in heard order; 1P blue, 2P red, 3P yellow, 4P mint.
- **The keyboard's lit key follows the last press**, as its own part does:
  `ZoneKeyboard` reports no key focus, and editing it was out of scope.
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
  `--zone-save-band-share` and `--zone-save-slab-share` are now unused and
  left in the savedata block for its owner to remove.
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
