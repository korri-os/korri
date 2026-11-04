# Vendored Slint 1.18.1 crates, patched

Four crates from crates.io, version **1.18.1**, unchanged except for the
changes listed below: `i-slint-core`, `i-slint-renderer-software`,
`slint-interpreter` and `i-slint-backend-testing`. `../Cargo.toml` substitutes them through
`[patch.crates-io]`. Every other Slint crate is the published 1.18.1. The
whole difference is `slint.patch`. The public API is unchanged. Nothing in it
is specific to Korri.

Drop this directory once upstream Slint has an equivalent. To move to a new
Slint version, apply `slint.patch` to the new crates, run
`cargo test --release --features fixture --test layer_cache --test parallel_render --test text_reshape --test memory_probe`,
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
| `i-slint-renderer-software/lib.rs` | `SceneBuilder::draw_cached_layer`, `CachedLayer::shifted_at`, `LayerCache::shifted` | A layer at a new sub-pixel phase is blitted shifted by whole pixels (it may be moving) and its area is drawn again next frame; once drawn twice at the same position it is rendered again at its exact phase. A sliding layer is no longer re-rendered every frame; still frames stay pixel-identical. |
| `i-slint-renderer-software/lib.rs` | `SceneBuilder::visit_layer`, `LayerCache::{direct, rendering}`, `DirectLayer`, `SceneBuilder::nested_in_layer_image` | An image is rendered only from content that stayed the same for a frame. A layer whose content changed is drawn directly, and its image entry is marked clean with its dependencies kept, so the next frame tells whether it changed again. Once it stays the same for a frame, its area is drawn again and the image is rendered. A frame renders at most one image; a layer inside an image is drawn into it, not cached on its own. Same pixels; an animation inside a layer renders no image that the next frame throws away, and its end renders one image instead of every layer at once. |
| `i-slint-renderer-software/lib.rs` | `SoftwareRenderer::last_frame_phases` and `last_frame_layer_renders` (doc-hidden), `render_buffer_impl`, `LayerCache::renders` | Records the last render's dirty-walk and raster times, and the layer images it rendered (count, pixels, time), for `korri-shell bench`. |
| `i-slint-core/properties.rs` | `PropertyTracker::acknowledge_dirty` | Marks a tracker clean without evaluating: it keeps its dependencies, so it becomes dirty again when one changes again. |
| `i-slint-core/item_rendering.rs` | `ItemCache::{is_dirty, acknowledge_dirty}` | Whether an entry is dirty, and the above for an entry. |
| `i-slint-renderer-software/draw_functions.rs` | `draw_texture_line` | Direct-row fast path for an unscaled, unrotated, premultiplied texture at full opacity and a whole-pixel offset (cached layers): skip transparent, store opaque, blend the rest. |
| `i-slint-renderer-software/draw_functions.rs` | `fetch_blend_pixel` (premultiplied, alpha map) | An opaque premultiplied pixel is stored; a glyph pixel with no coverage is skipped. |
| `slint-interpreter/item_tree_vtable.rs` | `item_geometry` | A geometry that is a struct literal is evaluated field by field instead of as a `Struct` value (a hash map). The renderer's dirty walk asks every item for it every frame. |
| `i-slint-renderer-software/parallel.rs` (new), `mod parallel` | `Op`, `Recorded`, `RenderToBuffer::replay`, `replay_in_bands`, `default_threads`, `MIN_PARALLEL_PIXELS` | Banded rasterisation. The item walk stays on the UI thread and records each draw call (rectangle, texture, gradient, path) with its clip. Scoped threads replay the list, each into its own horizontal band of the buffer, so no two threads write the same line. The same draw functions run in the same order on every pixel, so the result equals one thread. A frame streams its calls (`Recorder`, `OpStream`, `stream_into_bands`, `CHUNK`): once the dirty region is known, one band thread fewer than the thread count starts, and each replays every chunk of 128 calls as the walk publishes it, so rasterisation runs while the walk goes on. The walk then draws into `LinesOnly`, a buffer with no pixels; the band threads own the real one. A layer image records all its calls, then replays them. |
| `i-slint-renderer-software/lib.rs` | `RenderToBuffer::recording`, every `ProcessScene` method, `render_buffer_inner`, `render` | With more than one thread and a dirty area of at least 16 Ki pixels, `render` records the frame and replays it in bands; otherwise it draws on one thread as before. |
| `i-slint-renderer-software/lib.rs` | `SoftwareRenderer::render_threads`, `set_render_threads` and `last_frame_replay` (doc-hidden) | The thread count: online cores (`available_parallelism`), at most 4, 1 on a single core. Tests and `korri-shell bench` set it. |
| `i-slint-renderer-software/lib.rs` | `SceneBuilder::render_layer_image`, `LayerCache::threads` | A layer image of at least 16 Ki pixels is recorded and replayed in bands the same way. |
| `i-slint-renderer-software/lib.rs` | `RenderToBuffer::{process_filled_path, process_stroked_path}`, `dirty_clips` | A path is drawn once per dirty rectangle, inside it. It was drawn over the whole clip, so its anti-aliased edges were blended again over pixels outside the dirty region. A band replays the same per-rectangle clips. |
| `i-slint-core/textlayout/sharedparley/shaping.rs`, `cache.rs`, `sharedparley.rs` | `ShapingInputs`, `CachedParagraphs::inputs`, `cached_paragraphs` | A text's cache entry keeps the inputs it was shaped from (text, font request, wrap, stroke, link colour, scale). A property is marked dirty whenever something it depends on changes, also when its value then comes out the same. So an invalidated entry reads its inputs again and keeps its paragraphs and line breaking when they are equal. Zone's cards (size animated, font size clamped) reshaped 43 texts a frame; now none. |
| `i-slint-core/item_rendering.rs` | `ItemCache::with_entry_mut` | Mutable access to one entry, for the above. |
| `slint-interpreter/eval.rs` | `invoke_function` | Evaluates the function body through a borrow instead of a deep copy of its expression tree on every call. |
| `slint-interpreter/item_tree_vtable.rs` | `item_geometry` | Borrows the compiled geometry expression instead of deep-copying it on every call. |
| `i-slint-backend-testing/search_api.rs` | `ElementHandle::is_visible` | Whether an element is inside the window and every clipping ancestor (`ItemRc::is_visible`): focus targets clipped out of a list are not drawn (`../src/targets.rs`). The crate's `[[test]]`s are not vendored. |
| `slint-interpreter/Cargo.toml` | `[[test]]` | The crate's own tests are not vendored. |
| `i-slint-core/properties.rs` | `DepEdge`, `Cutoff`, `sources_changed`, `mark_dependencies_maybe_dirty`, `Property::set_binding_comparing`, `PropertyTracker::{is_dirty, acknowledge_dirty}`, `set_equality_cutoff` (doc-hidden) | Equality cut-off. A changed property marks its direct dependents dirty and everything further only *maybe* dirty. Each dependency edge remembers the source binding and the version it read; a comparing binding bumps its version only when its value changed. Before a maybe-dirty binding or tracker is evaluated, its sources are brought up to date and compared, and it is not evaluated when none changed. Zone's band: the card that only moves keeps its layer and its ~120 bindings are not evaluated each snap frame. `set_equality_cutoff(false)` restores upstream propagation (tests, `KORRI_BENCH_NO_CUTOFF`). |
| `i-slint-core/rtti.rs`, `slint-interpreter/bindings.rs`, `globals.rs` | `set_binding` of native and interpreted properties | Use `set_binding_comparing`. Globals' bindings compare too. Animated and two-way bindings keep upstream behaviour (always "changed"). |
| `i-slint-core/properties/properties_animations.rs`, `change_tracker.rs` | `mark_dirty`, edges | An animation starts on a maybe-dirty original binding too; the change tracker uses the new edge type. |
| `i-slint-renderer-software/lib.rs`, `parallel.rs` | `CachedLayer`, `draw_cached_layer`, `render_layer_image_inner`, `ProcessScene::process_layer_ops`, `Op::{SetClip, translated, bounds}` | A cached layer holds its children's recorded draw calls at the screen offset they were drawn at, not an image. Drawing it replays them inside its clip, so every pixel is blended exactly as drawing the children directly (RGB565 included). At another offset, under opacity or rotation the layer is drawn directly; once it stays at a new offset its calls are recorded again. |
| `i-slint-renderer-software/parallel.rs` | `Op::Layer`, `LayerOps::{replay, draw}` | A cached layer is one op holding its calls. Each line's part of the dirty region whose target pixels under the layer are those of last time gets the pixels the calls drew then (no call reads a neighbouring pixel, so the same calls on the same pixels draw the same pixels: exact for every format); other parts are replayed and remembered, per pixel. Still layers are copied, in partial and whole frames alike. |
| `i-slint-core/window.rs` | `WindowPinnedFields::{instantiation_tracker, instantiated_for}`, `ensure_tree_instantiated` | The walk over every repeater and conditional before a frame runs only when something the last walk read changed, or the component or the popups did. It ran every frame, idle too (1 ms a frame on the RG353M). |
| `i-slint-renderer-software/parallel.rs`, `lib.rs` | `BandQueue`, `stream_into_bands`, `render` | A frame's region is split into several bands per thread; band threads take bands as they finish, and the UI thread takes the ones left once its walk ended, instead of waiting for fixed bands. |
| `i-slint-core/items.rs`, `window.rs` | `next_window_item`, `forget_window_items` | A tree's window item is looked up once per frame, not walked up for every text's font request. |
| `i-slint-renderer-software/lib.rs` | `PREPARED_FONTS`, `draw_glyph_run` | A font at a size is prepared (metrics parsed) once per frame, not for every glyph run. |
| `slint-interpreter/bindings.rs` | `install_repeater_model_bindings` | A number model (`for i in n`) evaluated to the same count returns the same model: a new one made the repeater drop and re-create every instance whenever the count's binding re-evaluated (Pico's press frame: instantiation 2.6 -> 0.3 ms on the workstation). |
| `i-slint-renderer-software/lib.rs` | `RenderToBuffer::dirty_clips` | The dirty region as non-overlapping rectangles, so a path is blended once where dirty rectangles overlap. |
| `i-slint-core/animations.rs`, `SoftwareRenderer::freeze_animation_tick` (doc-hidden) | `animation_tick` | Can return a frozen instant (the focus probe); property animations and timers keep the clock. |
| `i-slint-core/frame_profile.rs` (new), hooks in `sharedparley`, `vectorfont.rs`, `lib.rs` | `SoftwareRenderer::{set_frame_profile, take_frame_profile, last_walk_profile, last_frame_layer_visits}` (doc-hidden) | `KORRI_BENCH_PROFILE`: bindings, text measuring, shaping, line breaking, drawing, new glyphs by size, path and image draws, layer invalidations per frame. Off by default. |

## Verification

| Test (`../tests/`) | What it shows |
| --- | --- |
| `layer_cache.rs::a_cached_layer_matches_direct_rendering_through_every_change` | Two windows render the same tree under partial rendering (`ReusedBuffer`), one with the hint and one without. The pixels are equal for a still subtree, for an item moving under and over the layer, for a text and a colour change inside the layer, for a growing repeater, for whole and fractional layer moves (once settled: a moved layer is first shifted by whole pixels and drawn exactly the next frame), after a resize, and at scale factor 2. A one-pixel blit offset fails this test (checked by planting it). |
| `layer_cache.rs::a_clean_cached_layer_is_not_walked` | With 300 children in the layer, the direct window walks more than 300 items a frame and the cached window fewer than 20. A change inside is drawn directly (walked); two frames later its image is rendered, and then it is clean again. |
| `parallel_render.rs` | Windows on 1 thread and on 4 (and 3) threads render the same tree (gradients, a path, text, a cached layer, opacity, moves, a resize to 500x333, more draw calls than one chunk): every frame is pixel-identical, with 1, 2 and 3 band threads beside the walk. A band thread that skips a chunk fails it (checked by planting it). |
| `layer_cache.rs::a_cached_layer_matches_direct_rendering_in_rgb565`, `…_under_opacity_…` | The same, into RGB565 (text over a translucent child), and under opacity. |
| `equality_cutoff.rs` | Zone (modern, retro) and Pico through presses: every frame drawn whole is identical with the cut-off on and off; partial frames stay within the stale-edge bound either way. A band of three animated cards: every partial frame is identical on and off, and with the cut-off the card that only moves is drawn from its cache. |
| `probe_still.rs` | surface-probe on Zone with `--still`: an unchanged view applied again changes no pixel. |
| `text_reshape.rs` | Six texts whose width, text and font size depend on an animated size, with the text and size values unchanged: no text is shaped again over four size steps, and each frame equals a fresh window at that size. A new font size shapes all six again. Comparing nothing (always reshaping) fails it (checked by planting it). |
| `memory_probe.rs` | 20 rounds of compiling, creating, showing and dropping Zone and Pico grow by less than 48 KiB a round. The measured growth is 0 to 10 KiB a round; before the change it was 153 KiB a round. |

Zone on the workstation (Xvfb, 640x480, Zone marks its still parts with
the hint): 27.5–37.6 % of one core before, 6.2–8.1 % after
(`../evidence/cpu/`).

## Limits

- The software renderer's line-by-line mode (`render_by_line`, used by
  MCUs) does not cache layers. It draws them as before.
- A layer under opacity or rotation is drawn directly, as without the hint.
- A moving layer is drawn directly while it moves (a slide costs its walk).
- Partial redraws leave a few stale edge pixels next to some moving items
  (up to ~30 per frame in Zone; `tests/equality_cutoff.rs` bounds them at 64),
  with or without the cut-off; full redraws are exact. Not yet traced.
- A layer holds its children's draw calls (shared glyph and image data), not pixels.
- Banded rasterisation parallelises the replay only (beside the walk). The item walk, property
  evaluation and text shaping stay on the UI thread. A path's coverage mask
  is computed again in each band it crosses.
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
