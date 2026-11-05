# Zone's attract: what each frame costs (workstation, 2026-11-05)

`korri-shell bench --packages packages --surface zone --size 640x480 --frames 2700 --games 40`
(`KORRI_BENCH_FRAMES=1`): 30 s with no press, then the attract (a game every
6 s: its title card until 2.9 s, then its cover pans 32 art px left until
5.6 s, then holds). Frames 1802-2522 are 12 s of attract.

## Per frame (µs, mean over 720 frames; threads as on a device: 4)

| | still screen | attract |
| --- | --- | --- |
| frame total | 143 | 459 |
| prepare (instantiate) | 47 | 89 |
| dirty walk | 33 | 107 |
| rasterisation (UI thread wall) | 42 | 235 |
| frames drawn | 142 of 700 | 212 of 720 |
| pixels drawn a second | 16 555 | 3 521 173 |

Within one 6 s turn (frames drawn of 30 per half second, pixels a drawn frame):

| turn | drawn | px a drawn frame | µs a frame |
| --- | --- | --- | --- |
| 0.0-0.5 s (title card enters) | 11 | 73 961 | 650 |
| 0.5-2.5 s (title card holds) | 5-6 | 1 100-1 700 | 244-279 |
| 3.0-5.5 s (cover pans) | 12 | 307 200 | 562-708 |
| 5.5-6.0 s (cover holds) | 7 | 132 471 | 322 |

What changes per frame during the pan: the cover's x (−32 a × progress, about
0.4 px a frame). 18 of 30 frames round to the same draw calls and are neither
drawn nor presented (vendored `Recorder`); the other 12 redraw and present the
whole 640x480 screen.

## Where the CPU goes in the attract (perf, 12 s)

| | share of samples |
| --- | --- |
| `draw_texture_line` (the scaled cover, band threads and UI thread) | 30.5 % |
| path rasterising (zeno, `render_path_with_style`) | about 11 % |
| band replay bookkeeping | 5.6 % |
| interpreter, property tracking, malloc | about 10 % |

## Can the panning cover be a cached layer that moves (blitted)?

No, not pixel-identical. A clipped texture's source offset is kept to 1/16
source pixel from the clip edge (`SceneTextureExtra::off_x`, `Fixed<u16, 4>`),
not from the image's origin. A 128 px image scaled to 704 px and moved one
whole pixel left samples other source columns than the frame before, shifted:
5 columns of 640 differ in 7 of 8 shifts and 35 in every 8th (measured over 64
shifts). A blit would draw other pixels than today. Making the offset exact
would itself change today's pixels.

## Done: colour textures drawn a run at a time (pixel-identical)

`fill_scaled_runs` (vendor/README.md): each source pixel is read and converted
once per run of target pixels. `perf stat` over the same 12 s of attract,
instructions in user space (two runs each):

| | instructions |
| --- | --- |
| a pixel at a time (`KORRI_BENCH_NO_RUNS=1`) | 1.562 G, 1.563 G |
| runs | 1.447 G, 1.447 G |

7 % fewer instructions. Cycles on this x86 core did not change measurably
(the writes to the frame dominate there); an in-order Cortex-A55 follows
instructions more closely, so the RG353M cut is expected at about 5 % of the
attract's 424 Mcycles/s (inferred, not measured).

## Not done (Zone package, recommended)

The GAMES page stays under the attract and keeps animating (the ground, the
band's marks): each frame walks it and records its draw calls before the
attract's opaque stage drops them. A trial build with `visible: !root.attract`
on ZoneGames' `shell` cut the attract's instructions from 1.447 G to 1.242 G
(14 %) and its CPU time by about a quarter. It is pixel-identical only if the
title card and the stage paint every pixel opaque from the attract's first
frame; that is not checked here, and needs a frame comparison test across the
three looks before it lands.
