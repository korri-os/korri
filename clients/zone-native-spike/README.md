# THROWAWAY SPIKE: one Zone state as a native Rust + Slint window

This is a prototype. It is not a product client, not a surface and not a
treaty. It has no korrid, no network and no input except Escape (quit).
Every value on screen is a hard-coded fixture.

## The question

Can a native Rust + Slint binary (software renderer, no browser) show one
state of the Zone surface so close to the real thing that the two are hard
to tell apart, with both of its subtle animations, cheaply enough for an
Anbernic RG35XX SP (640x480, 4x Cortex-A53, 1 GB)?

The state: Zone's GAMES page, state Ready (the default export of
`surfaces/zone/src/pages/ZoneGames.page.part.tsx`), Modern look.

## The answer (workstation only)

Yes on an x86_64 workstation, under X11 (Xvfb) and Wayland (headless sway).
See "Evidence". Nothing here was measured on the device.

## Run

| Variable | Effect |
| --- | --- |
| none | A 640x480 window. |
| `ZONE_SPIKE_SIZE=WxH` | Another window size, for example `1280x720`. |
| `ZONE_SPIKE_FULLSCREEN=1` | Full screen; the page is planned again for the size the compositor gives. |
| `ZONE_SPIKE_STATS=1` | On stderr: the time from start to the first frame, then frames a second every 5 s. |
| `ZONE_SPIKE_TIME_SCALE=N` | Runs the drift's clock N times fast (proof only). The nudge keeps real time. |
| `ZONE_SPIKE_LIVE=1` | Draws the still page live instead of baking it (comparison only; see "Cost"). |

Escape quits. winit picks Wayland when `WAYLAND_DISPLAY` is set, else X11.

## Build

The workstation has no cargo on `PATH`. From `clients/zone-native-spike`:

```sh
nix shell --inputs-from ../.. nixpkgs#cargo nixpkgs#rustc nixpkgs#gcc nixpkgs#clippy nixpkgs#rustfmt -c sh -c \
  'cargo fmt --check && cargo clippy --release --all-targets -- -D warnings && cargo test --release && cargo build --release'
```

Nix package (`package.nix`), from the checkout root. The command from the
brief uses the machine's `<nixpkgs>`; aarch64 goes to the remote builders:

```sh
nix build --impure --expr '(import <nixpkgs> { system = "aarch64-linux"; }).callPackage ./clients/zone-native-spike/package.nix {}'
nix build --impure --expr '(import <nixpkgs> { system = "x86_64-linux"; }).callPackage ./clients/zone-native-spike/package.nix {}'
```

The same with this repo's pinned nixpkgs (`flake.lock`, nixpkgs `4975466`):

```sh
nix build --impure --expr 'let pkgs = import (builtins.getFlake (toString ./.)).inputs.nixpkgs { system = "aarch64-linux"; }; in pkgs.callPackage ./clients/zone-native-spike/package.nix {}'
```

The binary links only libc, libm and libgcc_s. winit, xkbcommon and
fontique open Wayland, X11, xkbcommon and fontconfig with `dlopen`;
`package.nix` puts those libraries on the binary's RUNPATH, so the store
path runs as it is. fontconfig is optional: the font is embedded.

## How it is drawn

`src/plan.rs` ports the parts of Zone's plans this state needs, as a pure
function of the window's width and height:

- the art pixel (`src/plan/art-px.ts`): 2 px at 640x480, 3 px at 1280x720;
- the zone select (`src/plan/zone-select.ts` and the band's sums in
  `ZoneCardBand.css`): strips, card heights, three or five cards, the
  triangles and their room;
- the top strip's levels (`src/plan/top-strip.ts`): `keys` at 640 wide
  (keycaps only), `tight` at 1280 (keycaps with BACK and CONFIRM);
- every other length from its token in `src/zone-tokens.css` and the
  component stylesheets (cards, tags, banner, tabs, panels, facts line,
  action row, foot with its flex shrink and ellipsis).

Words are measured with harfrust (the shaper Slint already builds) on the
same Anton file Slint draws, so widths match Chromium's within a pixel
(`text.rs` test). The plan emits one flat list of primitives in paint order
(rectangles, polygons as `Path`, words, covers); `ui/zone.slint` only draws
them.

Each word is drawn as Zone's caps draw it: the face, the one-art-px ring on
all eight sides, and the one-step drop (the banner: a two-step orange drop
and the white band through rows 4 to 6). That is 14 `Text` items per word.

### The two animations

1. Drift (`src/plan/ground.ts`, `--zone-drift-big` 140 s clockwise,
   `--zone-drift-small` 200 s the other way, `games` ground colours). The
   rings never move, so Rust paints them once per size into two images:
   the ground with ring B (opaque), and ring A alone over its own box (with
   alpha). The three dots are small disc `Path`s between them, so ring A
   covers ring B's dot as Zone's canvas does. Their x and y follow
   `animation-tick()`; the software renderer draws a path at its
   fractional position, so the dots glide and never snap (Modern's
   `--zone-ground-fps: 60`, `renderZoneGroundSmooth`). Each dot stands half
   an art px in from its orbit point, as in Zone.
2. Nudge. The GAMES page's card triangles use `--zone-games-nudge-time:
   480ms` (`ZoneCard.css`), not the 600 ms `--zone-nudge-time` that the
   button and option triangles use. Modern adds `animation-direction:
   alternate` and `--zone-nudge-easing: ease-in-out`. So the margin goes 2
   to 4 art px in 480 ms, eased, and back. The spike uses one Slint
   property animation (`ease-in-out`, `direction: alternate`, endless) and
   two small triangle `Path`s.

### Cost

The still page is about 600 items (40 words x 14 copies, panels, covers).
Drawn live, every frame redraws each item that a moving dot or triangle
touches (Slint keeps at most three dirty rectangles and merges the rest).
So at start the spike draws the page once on a transparent window and keeps
it as one image (`Window::take_snapshot`), then drops the items. Each frame
then draws only, inside the small dirty rectangles: the ground image, the
ring A image, the page image, three dot paths and two triangle paths. The
page is baked again when the size changes.

Measured on the workstation (x86_64, release build, Xvfb, 10 s of
`/proc/<pid>/stat` CPU time, 62 frames a second):

| Case | CPU of one core |
| --- | --- |
| 640x480, page drawn live (`ZONE_SPIKE_LIVE=1`) | 25 to 30 % |
| 640x480, page baked | 7 % |
| 1280x720, page baked | 15 % |
| 640x480, page live, dots removed (a temporary experiment, not in the tree) | 8 % |

These are not device numbers. An A53 core is several times slower; if the
device cannot hold 60 frames a second, the next cut is to step the dots at
30 frames a second (they move 5 to 10 px a second, so steps stay under a
third of a pixel).

## Stats

`ZONE_SPIKE_STATS=1` uses winit's event hook (`unstable-winit-030`, a
feature of the same `slint` crate; it adds no crate). A frame is counted at
each `RedrawRequested`, which Slint answers with one software render. The
first line gives the time from `main` to the end of the first frame (and
when its draw began). The clock starts in `main`, so loading the binary is
not counted. Slint's software renderer has no rendering notifier.

## Fonts

Slint 1.18.1 does not read woff2: no source file in `i-slint-compiler`,
`i-slint-core`, `i-slint-common` or `i-slint-renderer-software` 1.18.1
mentions woff. Zone's Anton (`surfaces/zone/src/fonts/Anton-latin.woff2`,
OFL, `ui/fonts/Anton-OFL.txt`) was converted on the workstation with:

```sh
nix shell --impure --expr '(import <nixpkgs> {}).python3.withPackages (p: [ p.fonttools p.brotli ])' -c python3 -c "
from fontTools.ttLib import TTFont
f = TTFont('surfaces/zone/src/fonts/Anton-latin.woff2'); f.flavor = None
f.save('clients/zone-native-spike/ui/fonts/Anton-latin.ttf')"
```

`build.rs` embeds it (`EmbedFiles`); the software renderer draws it as
vectors at any size. Zone sizes Anton so its cap is the em of the face it
stands in (`zone-fonts.css` `size-adjust`): a word "C px tall" is Anton at
`C * 2048 / 1760`, placed by its cap (`text.rs`).

Covers: the four fixture pictures the band shows (`src/fixtures/
sample-art.ts`: SUMMIT, KNIGHT, BLOCKS, CAVERN) decoded from their data URLs
to `ui/art/*.png`.

## Proof

```sh
cd clients/zone-native-spike
# Only a cargo build needs this; the Nix build has the libraries on its RUNPATH.
export LD_LIBRARY_PATH="$(nix eval --raw --impure --expr 'let p = import <nixpkgs> {}; in p.lib.makeLibraryPath (with p; [ libx11 libxcursor libxrandr libxi libxcb libxkbcommon wayland fontconfig.lib ])')"
nix shell nixpkgs#xorg-server nixpkgs#xdotool nixpkgs#imagemagick -c ./proof.sh BINARY evidence
```

`proof.sh` starts Xvfb, runs the real window on the X11 backend, captures
it with `import -window`, and quits it with Escape.

## Evidence

`evidence/`, from the x86_64 Nix build:

| File | What |
| --- | --- |
| `caliper-reference-640x480.png` | Caliper, `ZONE_LOOK=modern`, device `rg353m`, state `default` |
| `caliper-reference-1280x720.png` | Caliper, device `screen-720p` |
| `native-640x480-t0.png`, `-t1.png` | the window, about 1 s apart: the triangles stand at different distances |
| `native-640x480-drift-a.png`, `-b.png` | `ZONE_SPIKE_TIME_SCALE=20`, about 1 s apart: every dot has moved along its ring |
| `native-1280x720.png` | the 720p layout: five cards, words on the hints |
| `side-by-side-640x480.png`, `side-by-side-1280x720.png` | reference left, native right |
| `wayland-sway-fullscreen-1280x720.png` | headless sway, `ZONE_SPIKE_FULLSCREEN=1`, captured with grim |
| `stats-640x480.log` | `ZONE_SPIKE_STATS=1` under Xvfb |

## What differs from the reference, and why

| Difference | Why |
| --- | --- |
| Capitals are about 1 px shorter at the top; edges are softer | Chromium hints Anton (FreeType, slight); Slint's software renderer draws it unhinted. Widths and baselines match. |
| Words stand up to 1 px left or right | Chromium places glyphs at sub-pixel positions; Slint rounds each run to whole pixels. |
| Ring edges differ by under a pixel | Zone's canvas arcs against an analytic one-pixel ramp in `ground.rs`. |
| Cover pixels are sampled a little differently | Both use nearest neighbour at a non-integer scale; the two pick different rows and columns. |
| Panel edges in the baked page lose a little antialiasing precision | The snapshot comes back un-premultiplied and is premultiplied again. |
| Dots and triangles stand elsewhere in a given capture | Both are clocks; the two captures are at different times. |

## Skipped

- Every other state of GAMES and every other page; the idle attract, the
  sheet, FIND's keyboard, focus moves and the snap animation.
- Top strip level `stacked` (under 312 art px wide, for example a portrait
  480x800): not ported. The page still plans, but the tabs do not wrap.
- The facts line does not wrap; spilled tags and the title on the facts
  line (both empty in this state) are not drawn.
- Reduced motion. Scale factors other than 1 are planned in logical px, but
  a scale change without a size change does not bake the page again.
- No device run, no device numbers.
