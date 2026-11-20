# Brief: Velvet's sign-in QR code, style "Pleats"

Mode: product mode. Velvet's tokens (`ui/tokens.slint`) are the voice. The
flourish is spent in one place: the shape of the data modules.

## Problem

The sign-in card (Steam now, Tailscale next) draws its code as plain square
modules, ink on a House light plate (`src/steam.ts` `drawQr`,
`ui/organisms/VelvetProgramme.slint`). It works, but it looks like any QR code,
and it does not say which service it opens. The person must know at a glance
that this code is for Steam (or Tailscale) before they lift their phone.

## Direction

The code becomes a piece of Velvet: the dark modules are drawn as vertical
pleats, like the folds of a theatre curtain. Each vertical run of dark modules
is one capsule with round ends. The three finder patterns are rounded squares.
The centre holds the service's mark on a small Ink badge. The colours stay
Ink on House light, the same as today, so the plate still reads as the one
lit object on a dark stage. No new colour, no gradient, no motion.

## Token plan

| Element | Token | Value |
| --- | --- | --- |
| Plate (with quiet zone) | `Tokens.house-light` | `#f3eee6` |
| Data pleats, finders | `Tokens.ink` | `#0a0a0c` |
| Logo badge | `Tokens.ink` | `#0a0a0c` |
| Mark on the badge | `Tokens.house-light` | `#f3eee6` |
| Tailscale's faint dots | `Tokens.light-20` over the badge | `#f3eee633` |
| Plate corner | as today (square) | no change |

Never invert the code (light modules on a dark ground). zbar and many phone
scanners do not read an inverted code.

## Geometry (units are modules)

Let `n = size` (from `SurfaceQrCode`), `Q = 4` (the existing `QUIET`), and
the Path viewbox `n + 2Q` modules square, as today. Module `(x, y)` is dark
when `modules[y * n + x] == '1'`. Every coordinate below adds `Q` to `x` and
`y` before it goes into the path.

### 1. Excluded modules

A module is not drawn as data when it is in either set:

- Finder: `x < 7 && y < 7`, or `x >= n - 7 && y < 7`, or `x < 7 && y >= n - 7`.
- Plate: `k = largest odd integer <= 0.3 * n`, and at least 7.
  `c = (n - 1) / 2`. The module is in the plate when
  `|x - c| <= (k - 1) / 2` and `|y - c| <= (k - 1) / 2`.
  The plate takes whole modules only; never draw part of a module.

| Version | n | k | Plate share of code area |
| --- | --- | --- | --- |
| 4 | 33 | 9 | 7.4 % |
| 5 (both test codes) | 37 | 11 | 8.8 % |
| 6 | 41 | 11 | 7.2 % |
| 10 | 57 | 17 | 8.9 % |

Do not raise `k`. The study's margin test failed at 21 % and passed at 16 %.
The rule keeps the plate under 9 %, so a worn screen or a soft focus still
has error-correction budget left.

### 2. Data modules: pleats

For each column `x`, find each maximal vertical run of drawable dark modules
from row `y0` to row `y1` (both inclusive). An excluded module ends a run.
Draw one capsule per run:

- width 0.78 (inset 0.11 left and right), round ends of radius 0.39;
- top at `y0 + 0.04`, bottom at `y1 + 0.96`. Two runs in one column are at
  least 1.08 apart, and neighbouring columns are 0.22 apart;
- a run of one module is a short capsule (0.92 tall), not a circle.

Path commands for one run (before adding `Q`):

```
M {x+0.11} {y0+0.43}
A 0.39 0.39 0 0 1 {x+0.89} {y0+0.43}
V {y1+0.57}
A 0.39 0.39 0 0 1 {x+0.11} {y1+0.57}
Z
```

Alignment and timing patterns are data. They get no special shape.

### 3. Finder patterns

At `(0, 0)`, `(n - 7, 0)` and `(0, n - 7)`, three nested rounded squares:

| Part | Inset | Side | Corner radius | Fill |
| --- | --- | --- | --- | --- |
| Outer | 0 | 7 | 2.0 | Ink |
| Hole | 1 | 5 | 1.0 | Plate |
| Eye | 2 | 3 | 1.0 | Ink |

Draw all three as subpaths of the same Path and set
`fill-rule: FillRule.evenodd` (as `ui/atoms/VelvetShape.slint` does).
Pleats never overlap a finder or each other, so one Path with even-odd fill
can hold every ink shape of the code.

### 4. Logo badge and mark

- Badge: an Ink rounded square centred on the plate, side `k - 2` modules,
  corner radius 2 modules. The badge leaves one module of House light on
  every side inside the cleared plate. The light ring keeps the pleats from
  touching the badge.
- Mark: centred on the badge, 72 % of the badge side, in House light.
- Steam: the official Steam mark (the round badge with the piston), knocked
  out: the disc in House light, the piston in Ink.
- Tailscale: the official 3 x 3 dot mark. The middle row and the bottom centre
  dot in House light; the other five dots in `light-20`.
- The renders use hand-drawn stand-ins. Ship the official vector marks as
  path strings in the package. I did not check the trademark terms of either
  brand.

### 5. Quiet zone and size

- Quiet zone: 4 modules of House light on every side. This is the treaty's
  minimum (`SurfaceQrCode` doc) and ISO/IEC 18004's minimum. Never less.
- Minimum on screen: 4 px per module, and 0.5 mm per module on the device.
  For a version 5 code (45 modules with the quiet zone) that is 180 px and
  22.5 mm for the plate. The renders decode at 3.6 px per module (160 px), so
  4 px leaves a small margin. The 0.5 mm floor is inferred for a phone held
  20 to 30 cm away. Nobody tested it with a phone.
- Approximate plate side from the current plan:

| Window | Layout | Plate side, about | px per module (v5) |
| --- | --- | --- | --- |
| 1920 x 1080 | beside | 500 px | 11 |
| 1280 x 720 | beside | 330 px | 7.3 |
| 1240 x 1080 | stacked | 430 px | 9.6 |
| 640 x 480 | beside | 216 px | 4.8 |
| 320 x 240 | stacked | under 100 px | under 2.2 (fails) |

At 320 x 240 the plan gives the code too little room. When the plan gives
the plate less than the minimum, the code must take the panel's full width
and the note rows must move under it. A row must not be dropped. Prove this
with the 320 x 240 shot.

## How to draw it

1. In `src/steam.ts`, replace `drawQr`: walk the columns, not the rows, and
   emit one capsule per vertical run (section 2). Then append the three
   finders (section 3). Keep the memo (`lastQr`) and the input check: a grid
   that is not `size * size` characters of `0` and `1` still draws nothing.
2. Export `k` (or the badge's rectangle in viewbox units) next to the path,
   so the view does not compute it twice.
3. In `VelvetProgramme.slint`, keep the House light Rectangle and the Path.
   Set `fill-rule: FillRule.evenodd` on the Path. Arcs (`A`) already
   work in Velvet's paths (`ui/drawings.slint`). Over it, draw the badge as a
   Rectangle (Ink, radius 2 modules) and the mark as a Path in House light.
   Compute module size as `plate side / (size + 8)`.
4. The page decides the mark: the Steam page shows the Steam mark, the
   Tailscale page shows the Tailscale mark. The treaty sends no logo.
   Tailscale has no `SurfaceQrCode` in `contracts/generated/surface.ts` today.
5. When Steam turns the code, swap the path at once. No cross-fade: a phone
   that captures a frame in the middle of a fade sees two codes.

## Eye flow

| Lands first | Then | Then | Quiet | Primary action |
| --- | --- | --- | --- | --- |
| The service mark in the centre of the lit plate | the head (STEAM) | the kicker under the code | the note rows | Cancel (the only filled pill when shown) |

## Encodings

| Visual | What it encodes |
| --- | --- |
| Mark on the badge | Which service the code opens |
| Pleats | The code's data. The shape adds no meaning; it is the one flourish |
| Rounded finders | The code's corners, as the scanner needs them |

## Held back

| Considered | Why not |
| --- | --- |
| Service colour (Steam navy, a gel) | A saturated blue reads as corporate; Exit is Velvet's only saturated token |
| Circle finders, dot modules (the "Marquee bulbs" study) | Decodes as well, but dot codes are the default of every QR generator |
| Size gradient to the centre ("Spotlight") | Weakest contrast next to the mark; the effect is hard to see at real size |
| Rounded plate corners | The plate is square today; nothing gains from a change |
| Animation on code change | Breaks a scan in progress |
| Inverted code on the dark house | Many scanners do not read it |

## Stress set

| Case | What the card does |
| --- | --- |
| Blank: grid empty or malformed | No code. The existing problem row: "Korri sent a sign-in code this page cannot draw." |
| Loading or Working | No code; the existing busy row |
| Error (Problem) | No code; Korri's reason and its actions |
| Ideal | The code, mark, kicker |
| Partial: code without `detail` | Kicker only under the code |
| Smallest version (v1, n = 21) | k = 7, badge 5 modules. Not observed from Steam; it must still draw |
| Large version (v10, n = 57) | k = 17; the centre alignment pattern goes under the plate. Decodes in the study |
| Longest kicker | Wraps under the code in the panel's width, as today |
| Window under the minimum size | See section 5 |

## Copy

The surface invents no strings. Steam's head, kicker, detail and the note
"The code changes every few seconds. Scan the one shown now." stay as they
are in `src/steam.ts`. The Tailscale copy waits for Korri's model.

## Evidence

Renders, decode table and scripts: `/tmp/private-network-reach/qr/` at the time of the study (2026-11-19); they are not kept in the repository.
`render.py` holds the reference implementation of these rules (Python and
Pillow, supersampled 8x). Style key: `b-pleats`.

## The one thing that must not be lost

The code must scan at first try. Every rule above (the 9 % plate cap, the
4-module quiet zone, Ink on House light, the size floor) serves that. The
pleats are only allowed because the code still decodes at 160 px with them.
