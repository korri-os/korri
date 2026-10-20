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
- `zone`: where focus is on the games page: `tabs | cards | actions`
- `card`, `action`: indices
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
- **The idle layer.** Reserved in the stack now, pushed by an `IdleElapsed`
  message after `ZONE_IDLE_AFTER_MS` (30 000). Any input pops it. The title
  card cycle it will show is not built yet.
- **System jump target.** `system` sets the tab to `options` and records
  `focusGroup: "system"`; the OPTIONS page, when it lands, reads it.
- **Host door is a plain function pair** (`performZoneRequest`,
  `subscribeZoneHostButtons`), as in Pico. The oxlint rule names `host`, so
  a renamed host parameter would get past it; review must catch that.
- **Catalog titles for now.** `ZoneSurface` lists the catalog's game titles
  as plain text in the shell's body until the card band lands, so the
  package renders end to end with a real `SurfaceModel`.

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
