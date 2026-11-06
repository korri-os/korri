# Window-item cache mismatch

Baseline measurements: 40 separate invocations of
`cargo test --locked --release --features fixture --test layer_previews zone_previews_are_byte_identical_cached_and_direct`
per commit, with `CARGO_BUILD_JOBS=4`, separate worktrees and separate target directories.
The two batches ran concurrently. The only test instrumentation copied its temporary
directory on failure before the existing assertion; comparison and rendering were unchanged.
`measurements.tsv` records all 80 outcomes.

| Commit | Failures | Rate |
| --- | ---: | ---: |
| `4c6dd8d4f` (pool change's parent) | 6/40 | 15% |
| `049749b77` (pool change) | 4/40 | 10% |

This establishes that the pool change did not introduce the failure. These samples do not
establish a change in failure rate.

## Saved pixels

`cached/` and `direct/` are the four differing PNG pairs from parent run 2
(`preview-failure.log`). Coordinates below are zero-based, half-open bounds.
The counts compare decoded RGB pixels, not PNG bytes.

| Size | Changed pixels | Bounds `(left, top, right, bottom)` |
| --- | ---: | --- |
| 640x480 | 14,282 | (230, 102, 410, 288) |
| 480x800 | 6,336 | (179, 300, 301, 408) |
| 1280x720 | 16,138 | (545, 215, 737, 415) |
| 1920x1080 | 16,138 | (865, 395, 1057, 595) |

Only the focused Hollow Knight card changes: the title's font size and wrapping, and
therefore the cover's height, initials and diagonal stripes. In this run direct rendering
chooses the oversized title and clips its right-hand letters; it does not omit a band or
an entire layer. At (387, 102) in the 640x480 pair, cached is RGB (247, 219, 8), direct is
(0, 0, 0). The 320x240 images were equal.

## Cause and regression

`i-slint-core/items.rs::next_window_item` keyed its cache by a raw item-tree address,
retaining only a weak reference to the ancestor window. Text measurement also calls it
between frames, including while the next component is instantiated. After a repeater row
is destroyed, its address can identify a new row while the old entry remains. The cached
window may have expired, so `WindowItem::resolved_font_request` returns an entirely default
font request, ignoring the text's explicit font properties. Clearing the cache before
rendering is too late: the incorrect measurement is already retained by layout bindings.

Temporary measurement logging reproduced the preview failure: Hollow Knight's large-title
measurement used a default font request and returned 82.740234 px, rather than the explicit
Anton font's 253.54546 px. The smaller value selects the oversized title.

`tests/window_item_cache.rs` isolates this without rendering: create a window with a
repeated text row, measure it, hide and drop it, then repeat. Before the fix it fails on
the second window, measuring 103 px instead of 391 px (`regression-before.log`).
The fix keeps a weak reference to the source tree alongside its cached window, reserving
the source allocation until the cache is cleared. Failed lookups are not retained while
a tree is still being attached. Layer caching, band scheduling and the preview assertion
are unchanged.
