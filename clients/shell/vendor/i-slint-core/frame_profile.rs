// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

//! Where one frame's text and drawing time goes, for benchmarks.
//!
//! Off by default: nothing is timed until [`set_enabled`] turns it on for the calling thread.
//! The counters are per thread; the renderer's UI thread is the one that shapes, breaks lines
//! and rasterises glyphs. [`take`] returns the counts since the last call and resets them.

use std::cell::{Cell, RefCell};
use std::thread_local;
use std::vec::Vec;
use std::time::{Duration, Instant};

/// How often something ran and how long it took altogether.
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct Counter {
    pub count: u64,
    pub time: Duration,
}

/// The counters. Times nest: `text_measure` and `text_draw` include the `shape` and
/// `line_break` they caused; `text_draw` includes `glyph_raster` and `glyph_run_setup`.
#[derive(Default, Clone, Debug, PartialEq)]
pub struct FrameProfile {
    /// Shaping a text (a `TextLayoutCache` miss).
    pub shape: Counter,
    /// Breaking and aligning a shaped text's lines (the retained breaking did not apply).
    pub line_break: Counter,
    /// Measuring a text for its preferred size (`text_size`): the layout queries.
    pub text_measure: Counter,
    /// Drawing a text item, from its layout to its glyphs' draw calls.
    pub text_draw: Counter,
    /// Rasterising a glyph that the glyph cache did not hold.
    pub glyph_raster: Counter,
    /// Preparing a font for one glyph run (parsing its metrics).
    pub glyph_run_setup: Counter,
    /// Glyphs drawn (from the cache or rasterised).
    pub glyphs: u64,
    /// Drawing an image item (scaling and clipping it into draw calls).
    pub image_draw: Counter,
    /// Property bindings evaluated: `count` is every evaluation, `time` the outermost ones
    /// only (it includes what they caused, a text measure for example).
    pub binding: Counter,
    /// Drawing a path item (building and filling or stroking it).
    pub path_draw: Counter,
    /// Drawing rectangles and bordered rectangles.
    pub rect_draw: Counter,
    /// Drawing a cached layer (`visit_layer`, including recording its calls when it has none).
    pub layer_draw: Counter,
    /// Making sure every repeater and conditional is instantiated before a frame.
    pub instantiate: Counter,
    /// Pixel sizes at which glyphs were rasterised, with their glyph count.
    pub raster_sizes: Vec<(i16, u64)>,
}

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static PROFILE: RefCell<FrameProfile> = RefCell::new(FrameProfile::default());
    static BINDING_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Turns profiling on or off for the calling thread.
pub fn set_enabled(enabled: bool) {
    ENABLED.with(|e| e.set(enabled));
}

/// Whether profiling is on for the calling thread.
#[inline]
pub fn enabled() -> bool {
    ENABLED.with(Cell::get)
}

/// The counters since the last call; resets them.
pub fn take() -> FrameProfile {
    PROFILE.with(|p| core::mem::take(&mut *p.borrow_mut()))
}

/// Runs `f`, adding its time to the counter `field` selects when profiling is on.
#[inline]
pub fn time<R>(field: fn(&mut FrameProfile) -> &mut Counter, f: impl FnOnce() -> R) -> R {
    if !enabled() {
        return f();
    }
    let started = Instant::now();
    let result = f();
    let elapsed = started.elapsed();
    PROFILE.with(|p| {
        let mut p = p.borrow_mut();
        let counter = field(&mut p);
        counter.count += 1;
        counter.time += elapsed;
    });
    result
}

/// Changes the counters when profiling is on.
#[inline]
pub fn record(f: impl FnOnce(&mut FrameProfile)) {
    if enabled() {
        PROFILE.with(|p| f(&mut p.borrow_mut()));
    }
}

/// Evaluates a property binding: counts it, and times it when it is the outermost one.
#[inline]
pub fn binding<R>(f: impl FnOnce() -> R) -> R {
    if !enabled() {
        return f();
    }
    let depth = BINDING_DEPTH.with(|d| {
        let depth = d.get();
        d.set(depth + 1);
        depth
    });
    let started = (depth == 0).then(Instant::now);
    let result = f();
    BINDING_DEPTH.with(|d| d.set(depth));
    let elapsed = started.map(|s| s.elapsed());
    PROFILE.with(|p| {
        let mut p = p.borrow_mut();
        p.binding.count += 1;
        if let Some(elapsed) = elapsed {
            p.binding.time += elapsed;
        }
    });
    result
}
