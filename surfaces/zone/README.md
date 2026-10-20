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
