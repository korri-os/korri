# Velvet on the RG353M (haku), #484

2026-11-16. haku: RK3566, Mali-G52, Panfrost, 640×480 at 60 Hz, main `51765c77f`
system, FemtoVG on `renderD128` (Mesa `libgallium-26.2.3.so`, no llvmpipe), live
live catalog. The method is the Pro's (`../gpu484/README.md`, `measure-pro.sh`):
`measure.sh` restarts `korri-shell` with only Velvet, warms 12 s, then takes 31 `/proc`
samples a second apart. Navigation: Down into the row, then Right and Left with 400 ms
rests, through a temporary `ydotoold` with physical key codes. CPU is percent of one
core (p95 nearest rank over 30 intervals). Redraws/s and frame intervals are the shell's
`--stats` (5 s windows, the first one dropped). GPU is Panfrost's fdinfo engine time with
`device/profiling` on for the run and restored. `summarize.py DIR` prints the table.

| Run | Shell | Velvet |
| --- | --- | --- |
| `before` | deployed `anfxs6i1…-korri-shell` | deployed `y1baqimn…-korri-shell-surfaces` |
| `baseline` | the same | this branch's base, rebuilt (`nd5ii6a3…`) |
| `nodrift` | diagnostic build | base, with the stage's drift clock frozen (control only) |
| `after` | this branch (`9072z6g8…`) | this branch (`fgbyhr3b…`, equal to `packages/velvet` here) |

| State | Run | CPU mean / p95 | Redraws/s | Interval p50 / p95 / max, ms | GPU fragment / vertex-tiler busy |
| --- | --- | --- | --- | --- | --- |
| Idle | baseline | 41.8 / 53.4 % | 59.8–60.1 | 16.7 / 17.1–17.2 / 17.9–25.5 | 63.4 / 1.6 % |
| Idle | after | 23.7 / 26.5 % | 60.0–60.1 | 16.7 / 17.1–17.4 / 17.7–19.3 | 33.3 / 0.9 % |
| Navigation | baseline | 84.2 / 88.6 % | 48.8–50.8 | 16.7–16.8 / 31.8–35.7 / 45.6–61.3 | 58.5 / 3.5 % |
| Navigation | after | 50.7 / 53.4 % | 57.8–59.0 | 16.7 / 19.5–22.1 / 29.2–36.0 | 42.0 / 2.1 % |

`before` (the deployed package) measured the same as `baseline`: idle 41.5 / 44.8 %,
navigation 84.3 / 92.1 % and p95 29.5–37.4 ms.

## Causes

`profile.sh` is `measure.sh` with `KORRI_SHELL_PROFILE=1` (README, Development tools)
and `perf record -g` (99 Hz, DWARF) on the shell. `layers.py` reads its log.

1. **Idle redraws are the stage's drift, by design.** After 2 s without a press the hero
   pushes in and drifts (`VelvetStage`, `animation-tick()`, 18 s a way, 0.06 px a frame
   at 640 px). Each idle frame evaluated 8 bindings and walked 22 items: the hero's place.
   With only that clock frozen (`nodrift`), idle fell to 0.0–0.4 redraws/s and 3.5 % CPU.
   Nothing else asks for frames: korrid's ~1 model/s and the 1 s `Tick` give a new view
   about once a second, 1 to 5 ms on the logic thread. The drift is not stopped here: it
   is the owner's motion.
2. **Navigation lag is CPU time on the UI thread, not QuickJS, not the GPU.** Frames over
   16.7 ms miss vsync and show as 33 ms intervals. Before: a third of frames were slow,
   render CPU p50 11.4 ms, p95 33 ms, max 104 ms. Logic runs on its own thread
   (`update` 1.5 ms, `view` 4.0 ms, `setModel` 5.9 ms mean; diff 1.7 ms); applying a view
   on the UI thread was 0.9 ms. GPU fragment busy stayed under 60 %.
   - **FemtoVG layers redrawn into textures: 85 % of a slow frame** (22 ms of 26).
     A layer is a rounded clip or an opacity over children, drawn into its own texture;
     Panfrost pays a framebuffer switch, a new texture and its buffer mapping for each
     (`panfrost_bo_create`, `mmap`, `TexImage2D`: about 12 % of the UI thread). Per slow
     frame: the whole foreground (640×480, from the cache hint of an earlier candidate),
     the two tiles whose focus changed, each twice (the card's rounded clip, and the
     focus edge's fade), the badge drawn into a layer at opacity 0, and the title strip.
     The tile's grow redrew its card at a new fractional size every frame of the 240 ms
     motion. `profile/candidate3-navigation.profile.log`.
   - **The interpreted item walk: 13 % of all samples** (`item_geometry`,
     `eval_expression`): about 250 items a frame, 2.4 ms.
   - **A new view re-created rows that had not changed: 3.4 to 5 ms in the first frame
     after a press.** The binder converted a changed row with no diff, so a nested list
     in it (a tile's title lines) became a new model and its repeater built its rows again.
3. **The compositor.** Frame intervals sit on 16.7 ms when frames are short, so sway's
   GLES2 path keeps vsync. Whether it adds a frame of latency was not measured.

## What changed

| Change | Where | Effect, measured on haku |
| --- | --- | --- |
| The stage light and vignette are cached textures (`cache-rendering-hint`, `clip`) | `VelvetLight`, `VelvetScrim` | GPU fragment busy at idle 63 → 33 % |
| The foreground is one cached texture while the stage drifts, never while it moves | `VelvetBrowse`, `main.slint` | idle CPU 42 → 24 % |
| The off-stage pick page and the lobby clip; the peeking lobby is cached | `VelvetBrowse` | fewer items walked |
| The focus edge and the badge stand over the card, not in its rounded clip; each edge ring fades by itself; a badge at opacity 0 is not drawn | `VelvetTile`, `VelvetEdge` | the card's texture is no longer redrawn for a fade |
| The card's size and place are whole pixels | `VelvetTile` | the grow redraws the card on each new pixel, not every frame |
| A changed row keeps the models of its unchanged nested lists | `src/view.rs` | the first frame after a press no longer rebuilds title rows |

After (`profile/after-navigation.profile.log`): 7 % of frames over 16.7 ms (was 33 %),
render CPU p50 4.9 ms, p95 19.7 ms. What is left: the first frame after a press (21 ms
mean: the new title strip, the new light's texture, the two tiles, new marquee rows) and
the frames of the grow (two tile textures, 2 to 4 ms each).

## Pixels

`velvet_package.rs`'s `VELVET_FRAMES` over `home`, `home-no-art`, `row`, `row-no-art`,
`lobby`, `lobby-no-art`, `set`, `set-no-art` and `detail` at 640×480, 1280×720 and
480×800, base against this branch: the detail is byte-identical; the others differ only
at tile edges and their titles, by under a pixel of place, from the whole-pixel card. The
device screenshots in `before` and `after` show the same screen.

## Not measured

- The compositor's own latency (sway, `WLR_RENDERER=gles2`): only intervals, no
  input-to-photon time.
- GPU time per frame: fdinfo gives busy time per engine, not per frame.
