# Vendored Slint 1.18.1 crates, patched

Two crates from crates.io, version **1.18.1**, unchanged except for the
changes listed below: `i-slint-core` and `i-slint-renderer-software`.
`../Cargo.toml` substitutes them through `[patch.crates-io]`. Every other
Slint crate is the published 1.18.1. The whole difference is
`layer-cache.patch` (about 250 changed lines). The public API is unchanged.
Nothing in it is specific to Korri.

Drop this directory once upstream Slint has an equivalent. To move to a new
Slint version, apply `layer-cache.patch` to the new crates, run
`cargo test --release --features fixture --test layer_cache --test memory_probe`,
and run the Zone CPU proof (`../README.md`, Performance).

## Why

The native shell runs on an RG35XX SP: one Cortex-A53 core per surface and no
GPU renderer. Zone's GAMES page animates three dots and two triangles at 60
frames a second, under several hundred still items (words drawn as 14 text
copies each). Slint's software renderer re-drew every still word the dots
passed under, and walked every item each frame to find what changed. Two
standard mechanisms did not reach the software renderer:

1. `cache-rendering-hint: true`. The compiler lowers it to a `Layer` item, but
   the software renderer's `visit_layer` was "Not supported" and drew the
   children every time.
2. Font registration. The interpreter registers a file's imported fonts each
   time a component is created. The software renderer's swash cache keeps every
   registered font blob for the life of the thread, so each switch between
   surfaces grew the process by the size of the package's font files.

## Changes, by function

| File | Function or item | Change |
| --- | --- | --- |
| `i-slint-core/partial_renderer.rs` | `PartialRenderingCachedData` | New field `cached_layer`: the item is a `Layer` that the actual renderer drew from its own cache last time. |
| `i-slint-core/partial_renderer.rs` | `ItemRenderer for PartialRenderer::visit_layer` | Previously forwarded by macro. It still forwards under the item's tracker, then records whether the renderer returned `ContinueRenderingWithoutChildren`. |
| `i-slint-core/partial_renderer.rs` | `PartialRenderer::compute_dirty_regions` | A layer that was drawn from a cache, is not dirty and did not move returns `SkipChildren`. Its tracker covers every property its children's rendering read, so nothing under it can have changed. |
| `i-slint-core/partial_renderer.rs` | `PartialRendererCache::walked_items`, `PartialRenderingState::walked_items` (doc-hidden) | Counts the items one dirty-region computation visited. Tests use it to show that a clean layer is skipped. |
| `i-slint-renderer-software/lib.rs` | `CachedLayer`, `LayerCache` | A layer's image (premultiplied RGBA), its origin relative to the item, and the sub-pixel phase it was rendered at. Kept in an `ItemCache`, so it is invalidated by property tracking. |
| `i-slint-renderer-software/lib.rs` | `SoftwareRenderer` fields, `Default`, `set_repaint_buffer_type`, `set_window_adapter`, `free_graphics_resources` | Owns the layer cache. Clears it with the other caches and drops a destroyed tree's entries. |
| `i-slint-renderer-software/lib.rs` | `SoftwareRenderer::render_buffer_impl` | Clears the layer cache when the scale factor or the window size changes, and gives the cache to the `SceneBuilder`. |
| `i-slint-renderer-software/lib.rs` | `prepare_scene` (line-by-line rendering) | Passes no layer cache. Layers there draw their children directly, as before. |
| `i-slint-renderer-software/lib.rs` | `SceneBuilder::new`, `SceneBuilder` field `layer_cache` | Takes the optional cache. |
| `i-slint-renderer-software/lib.rs` | `SceneBuilder::visit_layer` (new; it used the trait's "Not supported" default) | With `cache-rendering-hint` and a cache, it draws the layer from the cache and returns `ContinueRenderingWithoutChildren`. Otherwise its behaviour is unchanged. |
| `i-slint-renderer-software/lib.rs` | `SceneBuilder::draw_cached_layer` (new) | Gets the image or renders it, then blits the visible part with the current clip, opacity and rotation. A layer moved by a fraction of a physical pixel is rendered again at the new phase, not shifted. |
| `i-slint-renderer-software/lib.rs` | `SceneBuilder::render_layer_image` (new) | Renders the item's children, unrotated and at full opacity, through a nested `SceneBuilder` into a buffer that covers the item and all its children. |
| `i-slint-renderer-software/lib.rs` | `SoftwareRenderer::dirty_region_walked_items` (doc-hidden) | Exposes the walk count for tests. |
| `i-slint-renderer-software/draw_functions.rs` | `fetch_blend_pixel`, premultiplied RGBA at full opacity | Skips fully transparent source pixels. Blending one leaves the target unchanged, and a cached layer is mostly transparent. |
| `i-slint-renderer-software/fonts/systemfonts.rs` | `register_font_from_path`, `REGISTERED_FONT_FILES` | Registers each unchanged font file (same canonical path, size and modification time) once per thread. A changed file is registered again. |

## Verification

| Test (`../tests/`) | What it shows |
| --- | --- |
| `layer_cache.rs::a_cached_layer_matches_direct_rendering_through_every_change` | Two windows render the same tree under partial rendering (`ReusedBuffer`), one with the hint and one without. The pixels are equal for a still subtree, for an item moving under and over the layer, for a text and a colour change inside the layer, for a growing repeater, for whole and fractional layer moves, after a resize, and at scale factor 2. A one-pixel blit offset fails this test (checked by planting it). |
| `layer_cache.rs::a_clean_cached_layer_is_not_walked` | With 300 children in the layer, the direct window walks more than 300 items a frame and the cached window fewer than 20. A change inside walks it once, and then it is clean again. |
| `memory_probe.rs` | 20 rounds of compiling, creating, showing and dropping Zone and Pico grow by less than 48 KiB a round. The measured growth is 0 to 10 KiB a round; before the change it was 153 KiB a round. |

Zone on the workstation (Xvfb, 640x480, Zone marks its still parts with
the hint): 27.5–37.6 % of one core before, 6.2–8.1 % after
(`../evidence/cpu/`).

## Limits

- The software renderer's line-by-line mode (`render_by_line`, used by
  MCUs) does not cache layers. It draws them as before.
- A cached layer is drawn without rotation inside it and without the layer's
  own opacity applied twice. Opacity is applied once, when the image is
  blitted, which is group opacity. That is what the GPU renderers' layers do.
  Without the hint, the software renderer applied it to each child.
- The image covers the item and all its children. A very large layer costs its
  area in memory: 4 bytes a physical pixel.
- Font files: a font registered by path stays registered until the process
  ends. Unregistering it would not return its memory while the swash cache
  holds the blob.

## Draft upstream issue

Not filed. Filing is the owner's decision.

> **Software renderer: implement `cache-rendering-hint`, skip the dirty-region walk under clean cached layers, and stop re-registering imported fonts**
>
> On a Cortex-A53 with the software renderer, a page with a few small
> animated items over several hundred still `Text` items spends most of its
> frame time in two places. First, `PartialRenderer::compute_dirty_regions`
> walks every item each frame. Second, the still items the animation passes
> under are drawn again. `cache-rendering-hint: true` is lowered to a `Layer`,
> but `SceneBuilder` keeps the default `visit_layer` ("Not supported").
>
> Proposal, about 250 lines, no API change:
> 1. `SceneBuilder::visit_layer` renders the layer's children once into a
>    premultiplied image held in an `ItemCache` on the `SoftwareRenderer`. It
>    renders again when a tracked property changes, on scale-factor or size
>    change, or when the layer moves by a sub-pixel fraction. Then it blits the
>    image and returns `ContinueRenderingWithoutChildren`. The line-by-line
>    path keeps today's behaviour.
> 2. `PartialRenderer` records which layers the renderer drew from its cache.
>    `compute_dirty_regions` returns `SkipChildren` for such a layer when its
>    tracker is clean and it did not move.
> 3. `fetch_blend_pixel` skips fully transparent premultiplied source pixels.
> 4. `register_font_from_path` registers an unchanged file once.
>    `SWASH_FONTS` otherwise keeps a blob for every repeated registration.
>    This grows an interpreter-based app that creates components repeatedly
>    by the font file's size each time.
>
> Measured: one interpreted page went from 27.5 % to 6.2–6.7 % of one x86 core
> at 60 fps. Tests compare cached and direct rendering pixel by pixel through
> partial redraws, property changes, sub-pixel moves, resize and scale
> changes, and count the items walked.
