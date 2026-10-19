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
| Raw hex colours and px literals only in `src/zone-tokens.css` and `src/fonts/` | `test/atomic-gate.test.ts` and `test/authoring-gate.test.ts` |
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
  zone-motion.css     the one-bounce snap, the triangle nudge, the ring drift
  zone.css            reset, screen scope and reduced motion; imports nothing
  fonts/              Anton-Regular.ttf and OFL.txt
  plan/               plan(width, height) per page: pure, thresholds named
  state/              navigation model, messages, requests; pure
  zone-host.ts        the one door to Korri
  use-zone-program.ts the runtime: holds a model, runs update, performs requests
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
