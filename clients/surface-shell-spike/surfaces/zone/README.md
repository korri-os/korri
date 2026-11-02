# THROWAWAY SPIKE: Zone as a runtime-loaded surface package

Zone's GAMES page, Modern look, as plain files the surface shell loads at
run time (`../../CONTRACT.md`). No Rust: `main.slint` draws everything,
`logic.js` runs Zone's own state code. `logic.ts` was deleted with the web surfaces and the old TS treaty (2026-11-02); `logic.js` is a frozen bundle.
It starts from `clients/zone-native-spike/`.

| File | What |
| --- | --- |
| `main.slint` | `export component Surface inherits Window`. No callbacks, no key handling, no `init`. Imports only `fonts/Anton-latin.ttf`. |
| `logic.js` (frozen bundle) | `globalThis.surface = { init, update, view }`, pure. Bundled with the contract's bun command. |
| `fonts/` | Anton (OFL), converted from Zone's woff2 by the native spike. |
| `preview/` | The proof without the shell: scripts, view JSON and images. |

## The logic

`logic.ts` imports Zone's real program, `surfaces/zone/src/state/surface.ts`
(`zoneSurfaceStart`, `update`, `zoneShown`) and `state/navigation.ts` and
`state/games.ts`. The bundle holds 20 of Zone's modules, 106 KB. A grep
of `logic.js` finds no `Date`, timer, `Math.random`, `window` or `document`.

- `init` is Zone's start in the Modern look, then the Caliper part's
  opening (`ZoneGames.page.part.tsx`, `Ready`: `FocusedCard 1`). So the
  first frame is the reference state, with Hollow Knight focused.
- `update` is a thin adapter. In the browser the host moves DOM focus and
  the page reports `Focused…` facts. Here the adapter turns
  `{ _tag: "Pressed", action }` into the same facts and hands them to
  Zone's `update`:

  | Cursor on | Press | Zone message |
  | --- | --- | --- |
  | the band | left, right | `Games / FocusedCard card±1` (Zone clamps it) |
  | the band | up | `Games / FocusedTab` on the shown tab |
  | the band | down | `Games / FocusedAction 0` |
  | the action row | left, right | `Games / FocusedAction action±1` |
  | the action row | up | `Games / FocusedCard card` |
  | the tabs | left, right | `Games / FocusedTab` on the next tab |
  | the tabs | down | `Games / FocusedCard card` |
  | the tabs | confirm | `Games / ChoseTab games` (GAMES only) |
  | FIND | left, up / down | `FocusedTab savedata` / `FocusedCard` |
  | anywhere | back, menu | `PressedBack`, `PressedMenu` |

  Confirm on a card or a button, `options` (GAME OPTS.), and confirm on
  OPTIONS or SAVE DATA do nothing. This package does not draw the launch
  title card, GAME OPTS., the keyboard or the other pages, and the shell
  performs no requests.
- `view` builds one JSON object from the state and the model: the tabs,
  the band's cards (title, cover as `{ "$image": coverArtUrl }`, initials
  and stripe set for a card with no art, tags), the focused card, the
  facts line, the action row (PLAY / CONTINUE / PLAY HERE / PLAY ON …,
  then OPTS., with `lit` and `focused`), the clock, the Wi-Fi badge and
  the build. Words come in Zone's own case; `main.slint` sets Zone's
  words in capitals with `to-uppercase()`, as Zone's caps do, and keeps
  Korri's names (`Switch`, `zao`, the SSID) as written.

  The view's shape is spike-only (the contract says so). It follows the
  props Zone's page reads (`use-zone-games.ts`, `ZoneGames.tsx`). Four
  small helpers are copied from `.tsx` files, because those files import
  CSS and React: `launchLabel`, `cardTags` with `placesOf`, `zoneBarsLit`,
  `initialsOf` with `stripeSetOf`.
- The facts line's day is computed without `Date`, in UTC. Zone formats
  it in the person's time zone. The fixture's instants fall on the same
  day in every zone from -11 to +11.

## How it is drawn

`main.slint` ports the native spike's `src/plan.rs` to properties of the
window's width and height: the art pixel, the zone select (three or five
cards, strips, triangles), the top strip's levels (`keys` at 640 wide,
`tight` at 1280), and every length from its Zone token. Words are measured
by Slint itself (`Text.preferred-width` on the same Anton file), so the
widths match the native spike's harfrust measure.

- Words: each ink is its own component, written out copy by copy, as
  Zone's caps draw them (`ZoneCaps.css`): the face, the ring on all eight
  neighbours, the one-step drop (the banner: a two-step orange drop and
  the white band through rows 4 to 6). A white word is 16 Slint items.
- Lists of unknown length that need sums (the tabs, the facts, the action
  row, the tag rows) use fixed slots: three tabs, five facts, four
  buttons, five tags. Each slot measures itself and the parent sums the
  widths.
- The tag row drops its last tags when the row cannot hold them all, by
  `plan.rs`'s rule (Chromium's 1.5 px cut). The action row wraps onto a
  second line when its buttons do not fit (`ZoneActionRow.css`), and the
  band gives up the room (`ZoneGames.css`). Spelunky's PLAY HERE,
  PLAY ON ZAO and OPTS. need this at 640x480.
- The stripe under each title is two polygons cut along CSS's -62 degree
  gradient line. Slint's own hard-stop gradient draws a dark seam at each
  stop.
- A card with no cover gets the six stripe sets and its initials
  (`ZoneCoverWell.css`), drawn as a Slint gradient.

### Motion

1. **Drift** (`ground.ts`, `renderZoneGroundSmooth`). The two rings are
   bordered, round `Rectangle`s that never move: an antialiased annulus,
   6 art px thick, ring B under ring A. The three dots are small disc
   `Path`s, so they stand at sub-pixel positions. Ring A's dots turn once
   in 140 s clockwise, ring B's once in 200 s the other way, from
   `animation-tick()`. Each dot stands half an art px in from its orbit
   point, as Zone's canvas draws it. The clock is stepped to
   `ground-fps` (Zone's `zoneGroundFrame`; Modern's `--zone-ground-fps`
   is 60).
2. **Nudge**. The band's triangles go 2 to 4 art px and back, 480 ms each
   way (`--zone-games-nudge-time`). The buttons' inline triangles take
   600 ms (`--zone-nudge-time`). Both use CSS's `ease-in-out`,
   `cubic-bezier(0.42, 0, 0.58, 1)`, computed from `animation-tick()`
   with four Newton steps on the curve, so no `init` handler is needed to
   start an animation. At an end of the band, the triangle with nothing
   beyond it is the inert steel and stands still.
3. **Snap** (M1, `ZoneCardBand.tsx`, `--zone-bounce`). When the focused
   card changes, a float `f` follows it in 220 ms on
   `cubic-bezier(0.34, 1.56, 0.64, 1)`: one overshoot. The track's
   translation centres `f`, as `--zone-band-focus` does. For a move of
   one card, the old and the new card trade sizes and triangle room on
   the same curve, as CSS's two transitions do. A second float, linear
   over the same 220 ms, tells which way the band travels. For a longer
   move (a held direction that repeats faster than the snap), every card
   takes a share of `f`. The positions stay continuous, but the card
   sizes do not overshoot then. A card past the visible count disappears
   at once, as Zone's `data-beyond` does. Modern rounds the travel to
   1/64 px. A focused tab lifts 2 art px in 180 ms on the same bounce.

## Cost

Each frame moves only the three dots and the two band triangles (and two
inline triangles when a button is focused). The rings, cards and words
change only when a property changes.

Measured on the workstation (x86_64, slint-viewer 1.18.1, software
renderer, winit on X11 under Xvfb, CPU from `/proc/<pid>/stat` over 10 s,
start state, `preview/live.sh`):

| Case | CPU of one core |
| --- | ---: |
| 640x480 | 26 % |
| 1280x720 | 30 % |
| 640x480, dots stepped at 30 frames a second (`ground-fps: 30`) | 20 to 21 % |
| 640x480, dots still, triangles only | 10 to 11 % |
| 640x480, triangles only, words without ring and drop copies | 6 to 7 % |
| 640x480, an empty window with two moving triangles | 1 % |

For comparison, the native spike measured 7 % (640) and 15 % (1280) with
its page baked into one image by Rust, and 25 to 30 % with the page drawn
live. A package cannot bake: `take_snapshot` is Rust.

Where the time goes, from `perf` on the viewer:

- **The tree walk.** Each frame, Slint's partial renderer visits every
  item to find what changed, through the interpreter's vtables. The cost
  grows with the item count (several hundred items here; estimated, not
  counted), not with what moves.
  The ring and drop copies are about half of it. So only the visible
  cards exist (an `if` inside the `for`), each ink is its own component
  with no wrapper items, and an empty fact or button slot holds no words.
- **Redraws under the dots.** A dot moves under the page, and every word
  copy, cover or panel over it is redrawn in its small rectangle. At the
  start state, one dot is under the OPTIONS tab, one under the facts line
  and one under the Hollow Knight card.
- **The interpreter's function calls.** The first nudge, written as
  nested `pure function` calls, took a whole core: each call copied the
  curve into itself again. Each Newton step is now a property of its own
  (`Breath`). Calls are kept out of everything that runs each frame.

The next cuts, not taken, because each changes what Zone draws:
`ground-fps: 30` (about 5 points; the dots then step about 0.3 px at a
time), or fewer ring and drop copies per word (about 4 points). Neither is
measured on the RG35XX SP. An A53 core is several times slower than this
workstation.

## Proof

```sh
cd clients/surface-shell-spike/surfaces/zone
bun preview/run-logic.ts            # view JSON: start, right, right
SLINT_VIEWER=/path/to/slint-viewer nix shell nixpkgs#imagemagick -c preview/render.sh
SLINT_VIEWER=/path/to/slint-viewer nix shell nixpkgs#xorg-server nixpkgs#xorg.xwd nixpkgs#xdotool nixpkgs#imagemagick \
  -c preview/live.sh 640x480        # motion captures and CPU; add --snap for the snap burst
```

`run-logic.ts` runs `logic.js` as the shell does: a bare `node:vm` context
with nothing in it. It checks that the script leaves no global except
`surface`, then prints the view after `init` and after each of two presses
of right. With `--write` it decodes each cover from its data URL to
`preview/art/<n>.png` and writes `preview/view-<step>.json` with the covers
as file paths. `gen-preview.ts` turns each view into a throwaway wrapper
(`preview/<step>.slint`) that sets the Surface's properties as the shell
would. The wrappers and `art/` are generated and ignored by git.

slint-viewer 1.18.1 is not in nixpkgs (it has 1.14.1). It was built from
crates.io:

```sh
nix-shell -p cargo rustc gcc pkg-config fontconfig libxkbcommon --run \
  'cargo install slint-viewer --version 1.18.1 --locked --root /tmp/slint-viewer-1.18.1 \
     --no-default-features --features backend-winit,renderer-software,renderer-femtovg'
```

`--screenshot` renders with Slint's own software renderer (no Skia in this
build) and needs no display.

### Images

| File | What |
| --- | --- |
| `start-640x480.png`, `start-1280x720.png` | `init`: Hollow Knight focused |
| `right-1-*.png` | one press of right: Tetris (no facts recorded: Zone's dashes) |
| `right-2-*.png` | two presses: Spelunky, two places, so PLAY HERE and PLAY ON ZAO, nothing lit; the row wraps at 640 |
| `side-by-side-640x480.png`, `side-by-side-1280x720.png` | Caliper reference left, this package right |
| `focus-640x480.png` | the cursor elsewhere: OPTIONS tab (up, right), FIND (menu, menu), PLAY (down), PLAY ON ZAO (right, right, down, right) |
| `motion-640x480-a.png`, `-b.png`, `-diff.png` | the live window under Xvfb, 1.5 s apart, and what changed: the three dots and the two triangles |
| `motion-1280x720-a.png`, `-b.png`, `-diff.png` | the same at 1280x720 |
| `snap-640x480.png` | 16 captures about 22 ms apart while the focus moves from Tetris to Hollow Knight |

## What differs from the reference

Compared with `clients/zone-native-spike/evidence/caliper-reference-*.png`
(ImageMagick `compare -fuzz 10%`): 5.4 % of the pixels differ at 640x480
and 5.7 % at 1280x720. The native spike measured 3.4 % and 4.4 %. Read
side by side, the layout, the words and the colours match. What differs:

| Difference | Why |
| --- | --- |
| Glyph edges are softer, and capitals are about 1 px shorter at the top | Chromium hints Anton; Slint's software renderer draws it unhinted (as in the native spike). |
| Words stand up to 1 px left or right; card edges differ by up to 1 px | Sub-pixel placement and rounding differ between Chromium and Slint. |
| Ring edges differ by under a pixel | Zone's canvas arcs against a bordered, rounded Rectangle. |
| Cover pixels are sampled a little differently | Both use nearest neighbour at a non-integer scale, and they pick different rows. |
| Dots and triangles stand elsewhere in a given capture | Both are clocks. |

## Skipped

- Every other page and layer: OPTIONS, SAVE DATA, GAME OPTS., the find
  keyboard, the launch title card, the idle attract, Korri's status
  layers, the Loading, Error and Empty catalogs, GET GAMES, section
  dividers.
- Tags the focused card's row drops are not spilled onto the facts line.
  A title too long for its card is not moved to the facts line either.
- The facts line does not wrap; more than five facts, four buttons or
  five tags are not drawn.
- The top strip's `stacked` level (under 312 art px wide).
- Reduced motion.
- When the shell first sets `focused-card`, the band snaps once from card
  0 (the property's default) to card 1. A wrapper that binds the value at
  creation, as the previews do, shows no snap.
- No device run, no device numbers.
