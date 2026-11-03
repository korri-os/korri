// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

//! Rasterising one frame on several threads.
//!
//! The item walk stays on the UI thread: the scene builder's draw calls are
//! recorded as [`Op`]s instead of being drawn. The frame's dirty region is
//! split into horizontal bands of buffer lines, and one scoped thread per band
//! replays every op through a [`RenderToBuffer`] clipped to its band. Bands own
//! disjoint lines, and each op draws a pixel exactly as the single-threaded
//! renderer does, so the result is the same image.
//!
//! A frame streams its ops: the band threads start once the dirty region is
//! known and replay each chunk of [`CHUNK`] ops as the UI thread publishes it,
//! so they draw while the UI thread is still walking the items. A layer image
//! records all its ops first and then replays them ([`replay_in_bands`]).

#![allow(unsafe_code)] // Two marker impls, justified where they are written.

use super::*;
use std::sync::Arc;

/// One recorded draw call. Every field is plain data, or shared data
/// (`SharedBufferData`, `SharedVector`, `'static` texture slices) that the
/// replay only reads: no reference count is touched off the UI thread.
pub(super) enum Op {
    /// The window background for the dirty region.
    Background(Color),
    Texture(PhysicalRect, SceneTexture<'static>),
    SharedTexture(PhysicalRect, SharedBufferCommand),
    Rectangle(PhysicalRect, PremultipliedRgbaColor),
    RoundedRectangle(PhysicalRect, RoundedRectangle),
    LinearGradient(PhysicalRect, LinearGradientCommand),
    RadialGradient(PhysicalRect, RadialGradientCommand),
    ConicGradient(PhysicalRect, ConicGradientCommand),
    #[cfg(feature = "path")]
    FilledPath {
        path: PhysicalRect,
        clip: PhysicalRect,
        commands: alloc::vec::Vec<path::Command>,
        color: PremultipliedRgbaColor,
    },
    #[cfg(feature = "path")]
    StrokedPath {
        path: PhysicalRect,
        clip: PhysicalRect,
        commands: alloc::vec::Vec<path::Command>,
        color: PremultipliedRgbaColor,
        width: f32,
        cap: i_slint_core::items::LineCap,
        join: i_slint_core::items::LineJoin,
        miter_limit: f32,
    },
}

/// Ops published to the band threads at a time.
pub(super) const CHUNK: usize = 128;

/// Where the scene builder records: the ops, and the stream that takes them in chunks.
pub(super) struct Recorder {
    ops: Vec<Op>,
    stream: Option<Arc<OpStream>>,
}

impl Recorder {
    pub(super) fn new() -> Self {
        Self { ops: Vec::new(), stream: None }
    }

    /// From now on, publish every full chunk to `stream`.
    pub(super) fn stream_to(&mut self, stream: Arc<OpStream>) {
        self.stream = Some(stream);
    }

    pub(super) fn push(&mut self, op: Op) {
        self.ops.push(op);
        if self.ops.len() >= CHUNK {
            if let Some(stream) = &self.stream {
                stream.publish(core::mem::replace(&mut self.ops, Vec::with_capacity(CHUNK)), false);
            }
        }
    }

    /// The ops not published yet: all of them without a stream.
    pub(super) fn into_ops(self) -> Vec<Op> {
        self.ops
    }
}

/// The chunks of one frame's ops, published by the UI thread and replayed by
/// every band thread in order.
pub(super) struct OpStream {
    state: std::sync::Mutex<StreamState>,
    published: std::sync::Condvar,
}

#[derive(Default)]
struct StreamState {
    chunks: Vec<Arc<Vec<Op>>>,
    done: bool,
}

// Sound for the same reason as `SharedOps`: band threads only read the ops,
// never clone or drop a reference-counted value in them. A band thread drops
// its `Arc` of a chunk while the stream still holds one, so every op is
// dropped on the UI thread, with the stream, after the band threads ended.
unsafe impl Sync for OpStream {}
unsafe impl Send for OpStream {}

impl OpStream {
    pub(super) fn new() -> Self {
        Self { state: Default::default(), published: std::sync::Condvar::new() }
    }

    /// Publish `ops`; `done` when they are the frame's last.
    pub(super) fn publish(&self, ops: Vec<Op>, done: bool) {
        let mut state = self.state.lock().unwrap();
        if !ops.is_empty() {
            state.chunks.push(Arc::new(ops));
        }
        state.done |= done;
        drop(state);
        self.published.notify_all();
    }

    /// The chunk at `index`, waiting until it is published; `None` after the last.
    fn chunk(&self, index: usize) -> Option<Arc<Vec<Op>>> {
        let mut state = self.state.lock().unwrap();
        loop {
            if let Some(chunk) = state.chunks.get(index) {
                return Some(chunk.clone());
            }
            if state.done {
                return None;
            }
            state = self.published.wait(state).unwrap();
        }
    }

    /// Every published op, for a frame drawn without band threads.
    pub(super) fn take_ops(&self) -> Vec<Op> {
        let mut state = self.state.lock().unwrap();
        let mut ops = Vec::new();
        for chunk in state.chunks.drain(..) {
            ops.extend(Arc::try_unwrap(chunk).ok().expect("no band thread holds a chunk"));
        }
        ops
    }
}

/// The ops of one frame and the region they draw.
pub(super) struct Recorded {
    pub ops: Vec<Op>,
    pub dirty_region: PhysicalRegion,
    pub scale_factor: ScaleFactor,
}

/// A frame whose dirty region is smaller than this is drawn on one thread:
/// starting threads costs more than it saves.
pub(super) const MIN_PARALLEL_PIXELS: usize = 16 * 1024;

/// How many threads rasterise a frame by default: the online cores, at most 4.
pub(super) fn default_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get().min(4))
}

/// A band of buffer lines `[first, first + lines)`, borrowed mutably from the frame buffer.
struct Band<'a, T> {
    data: &'a mut [T],
    pixel_stride: usize,
    first: usize,
    total_lines: usize,
    /// Returned for a line outside the band. Callers only read its length
    /// (path rendering asks line 0 for the width); nothing drawn there is kept.
    outside: Vec<T>,
}

impl<T: TargetPixel> target_pixel_buffer::TargetPixelBuffer for Band<'_, T> {
    type TargetPixel = T;

    fn line_slice(&mut self, line_number: usize) -> &mut [T] {
        let lines = self.data.len() / self.pixel_stride;
        if line_number < self.first || line_number >= self.first + lines {
            if self.outside.len() != self.pixel_stride {
                self.outside = alloc::vec![T::background(); self.pixel_stride];
            }
            return &mut self.outside;
        }
        let offset = (line_number - self.first) * self.pixel_stride;
        &mut self.data[offset..offset + self.pixel_stride]
    }

    fn num_lines(&self) -> usize {
        self.total_lines
    }
}

/// Shares the ops with the band threads. Sound because the replay only reads
/// them: it borrows texture slices and gradient stops and never clones or
/// drops a reference-counted value. The UI thread waits for every band.
struct SharedOps<'a>(&'a Recorded);
unsafe impl Sync for SharedOps<'_> {}

/// A band's lines, sent to its thread. Target pixels are plain values.
struct SendBand<'a, T>(Band<'a, T>);
unsafe impl<T> Send for SendBand<'_, T> {}

impl<B: target_pixel_buffer::TargetPixelBuffer> RenderToBuffer<'_, B> {
    /// Draw one recorded op, as the matching `ProcessScene` call would have.
    pub(super) fn replay(&mut self, op: &Op) {
        match op {
            Op::Background(color) => {
                let mut bg = <B::TargetPixel>::background();
                TargetPixel::blend(&mut bg, (*color).into());
                let bounds = self.dirty_region.bounding_rect();
                self.foreach_ranges(&bounds, |_, buffer, _, _| buffer.fill(bg));
            }
            Op::Texture(geometry, texture) => {
                let texture = SceneTexture { ..*texture };
                self.process_texture_impl(*geometry, texture);
            }
            Op::SharedTexture(geometry, command) => {
                self.process_texture_impl(*geometry, command.as_texture());
            }
            Op::Rectangle(geometry, color) => {
                let color = *color;
                self.foreach_ranges(geometry, |_, buffer, _, _| {
                    <B::TargetPixel>::blend_slice(buffer, color)
                });
            }
            Op::RoundedRectangle(geometry, rr) => {
                self.foreach_ranges(geometry, |line, buffer, left, right| {
                    draw_functions::draw_rounded_rectangle_line(
                        geometry,
                        PhysicalLength::new(line),
                        rr,
                        buffer,
                        left,
                        right,
                    );
                });
            }
            Op::LinearGradient(geometry, g) => {
                self.foreach_ranges(geometry, |line, buffer, left, _| {
                    draw_functions::draw_linear_gradient(
                        geometry,
                        PhysicalLength::new(line),
                        g,
                        buffer,
                        left,
                    );
                });
            }
            Op::RadialGradient(geometry, g) => {
                self.foreach_ranges(geometry, |line, buffer, left, right| {
                    draw_functions::draw_radial_gradient(
                        geometry,
                        PhysicalLength::new(line),
                        g,
                        buffer,
                        left,
                        right,
                    );
                });
            }
            Op::ConicGradient(geometry, g) => {
                self.foreach_ranges(geometry, |line, buffer, left, right| {
                    draw_functions::draw_conic_gradient(
                        geometry,
                        PhysicalLength::new(line),
                        g,
                        buffer,
                        left,
                        right,
                    );
                });
            }
            #[cfg(feature = "path")]
            Op::FilledPath { path, clip, commands, color } => {
                // As on one thread: inside the dirty region, here this band's part of it.
                for clip in self.dirty_clips(clip) {
                    path::render_filled_path(commands, path, &clip, *color, self.buffer);
                }
            }
            #[cfg(feature = "path")]
            Op::StrokedPath { path, clip, commands, color, width, cap, join, miter_limit } => {
                for clip in self.dirty_clips(clip) {
                    path::render_stroked_path(
                        commands,
                        path,
                        &clip,
                        *color,
                        *width,
                        *cap,
                        *join,
                        *miter_limit,
                        self.buffer,
                    );
                }
            }
        }
    }
}

/// `buffer`'s lines under `region`, split into at most `threads` bands.
fn split_bands<'a, T>(
    buffer: &'a mut [T],
    pixel_stride: usize,
    region: &PhysicalRegion,
    threads: usize,
) -> Vec<SendBand<'a, T>> {
    let total_lines = buffer.len() / pixel_stride;
    let bounds = region.bounding_rect();
    let (top, bottom) = (bounds.min_y().max(0) as usize, (bounds.max_y().max(0) as usize).min(total_lines));
    if top >= bottom {
        return Vec::new();
    }
    let threads = threads.clamp(1, bottom - top);
    let per_band = (bottom - top).div_ceil(threads);
    let mut bands = Vec::with_capacity(threads);
    let (_, mut rest) = buffer.split_at_mut(top * pixel_stride);
    let mut first = top;
    while first < bottom {
        let lines = per_band.min(bottom - first);
        let (band, tail) = rest.split_at_mut((lines * pixel_stride).min(rest.len()));
        bands.push(SendBand(Band { data: band, pixel_stride, first, total_lines, outside: Vec::new() }));
        rest = tail;
        first += lines;
    }
    bands
}

/// A renderer that draws into `band`, inside its part of `region`; `None` when the band has none.
fn band_renderer<'b, 'a, T: TargetPixel>(
    band: &'b mut Band<'a, T>,
    region: &PhysicalRegion,
    scale_factor: ScaleFactor,
) -> Option<RenderToBuffer<'b, Band<'a, T>>> {
    let rect = PhysicalRect::new(
        PhysicalPoint::new(i16::MIN / 2, band.first as i16),
        PhysicalSize::new(i16::MAX, (band.data.len() / band.pixel_stride) as i16),
    );
    let dirty_region = region.intersection(&rect);
    (dirty_region.count > 0).then(|| RenderToBuffer {
        buffer: band,
        dirty_range_cache: Vec::new(),
        dirty_region,
        scale_factor,
        recording: None,
    })
}

/// Start one thread per band of `buffer` under `region`; each replays the
/// chunks of `stream` as they are published, until the last.
pub(super) fn stream_into_bands<'scope, 'env, T: TargetPixel>(
    scope: &'scope std::thread::Scope<'scope, 'env>,
    stream: &'scope OpStream,
    buffer: &'scope mut [T],
    pixel_stride: usize,
    region: &PhysicalRegion,
    scale_factor: ScaleFactor,
    threads: usize,
) {
    for band in split_bands(buffer, pixel_stride, region, threads) {
        let region = region.clone();
        scope.spawn(move || {
            let band = band; // the whole `SendBand`, not its field
            let mut band = band.0;
            let Some(mut renderer) = band_renderer(&mut band, &region, scale_factor) else {
                return;
            };
            let mut index = 0;
            while let Some(chunk) = stream.chunk(index) {
                for op in chunk.iter() {
                    renderer.replay(op);
                }
                index += 1;
            }
        });
    }
}

/// Replay `recorded` into `buffer` on `threads` threads, one band of lines each.
pub(super) fn replay_in_bands<T: TargetPixel>(
    recorded: &Recorded,
    buffer: &mut [T],
    pixel_stride: usize,
    threads: usize,
) {
    let bands = split_bands(buffer, pixel_stride, &recorded.dirty_region, threads);
    let shared = SharedOps(recorded);
    let shared = &shared;
    let draw = move |band: SendBand<'_, T>| {
        let recorded = shared.0;
        let mut band = band.0;
        let Some(mut renderer) = band_renderer(&mut band, &recorded.dirty_region, recorded.scale_factor)
        else {
            return;
        };
        for op in &recorded.ops {
            renderer.replay(op);
        }
    };
    std::thread::scope(|scope| {
        let mut bands = bands.into_iter();
        let mine = bands.next();
        for band in bands {
            scope.spawn(move || draw(band));
        }
        if let Some(band) = mine {
            draw(band);
        }
    });
}
