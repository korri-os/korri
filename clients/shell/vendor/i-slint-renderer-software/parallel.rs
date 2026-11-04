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
#[derive(Clone)]
pub(super) enum Op {
    /// The window background for the dirty region.
    Background(Color),
    /// Restrict the ops that follow to this rectangle, or lift the restriction.
    SetClip(Option<PhysicalRect>),
    /// A cached layer's draw calls, inside the clip (`LayerOps::replay`).
    Layer(Arc<LayerOps>, PhysicalRect),
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

impl Op {
    /// The pixels the op can touch.
    pub(super) fn bounds(&self) -> PhysicalRect {
        match self {
            Op::Layer(_, clip) => *clip,
            Op::Background(_) | Op::SetClip(_) => {
                PhysicalRect::new(PhysicalPoint::new(i16::MIN / 2, i16::MIN / 2), PhysicalSize::new(i16::MAX, i16::MAX))
            }
            Op::Texture(geometry, _)
            | Op::SharedTexture(geometry, _)
            | Op::Rectangle(geometry, _)
            | Op::RoundedRectangle(geometry, _)
            | Op::LinearGradient(geometry, _)
            | Op::RadialGradient(geometry, _)
            | Op::ConicGradient(geometry, _) => *geometry,
            #[cfg(feature = "path")]
            Op::FilledPath { clip, .. } | Op::StrokedPath { clip, .. } => *clip,
        }
    }
}

/// A cached layer's draw calls (screen coordinates, recorded where the layer stands), and per
/// line of its area the target pixels before and after they were last replayed there.
///
/// When the frame's dirty region spans the layer's visible width, a line whose target pixels
/// before the layer are those it had last time gets the pixels the calls drew then: the same
/// calls on the same pixels draw the same pixels, so this is exact for every pixel format. A
/// line with other pixels under it is drawn by replaying the calls, and remembered. A whole
/// screen redrawn every frame (a fresh buffer) so copies a still layer instead of drawing it.
pub(super) struct LayerOps {
    ops: Vec<Op>,
    area: PhysicalRect,
    lines: Vec<std::sync::Mutex<Option<LineSnapshot>>>,
}

struct LineSnapshot {
    /// Pixel type and the x range the pixels cover.
    kind: &'static str,
    x: core::ops::Range<i16>,
    before: Vec<u8>,
    after: Vec<u8>,
}

// Sound for the reason `SharedOps` is: band threads only read the calls (they never clone
// or drop a reference-counted value in them), and each band locks only its own lines.
unsafe impl Sync for LayerOps {}
unsafe impl Send for LayerOps {}

/// The bytes of a run of target pixels (plain values without padding: RGB565, RGB8, RGBA8).
fn pixel_bytes<T>(pixels: &[T]) -> &[u8] {
    // Safety: target pixels are plain integer structs; reading them as bytes is fine.
    unsafe { core::slice::from_raw_parts(pixels.as_ptr().cast(), core::mem::size_of_val(pixels)) }
}

fn pixel_bytes_mut<T>(pixels: &mut [T]) -> &mut [u8] {
    // Safety: as above, and any bytes written come from pixels of the same type.
    unsafe {
        core::slice::from_raw_parts_mut(pixels.as_mut_ptr().cast(), core::mem::size_of_val(pixels))
    }
}

impl LayerOps {
    pub(super) fn new(ops: Vec<Op>, area: PhysicalRect) -> Self {
        let lines = (0..area.height().max(0)).map(|_| std::sync::Mutex::new(None)).collect();
        Self { ops, area, lines }
    }

    /// Draw the calls into `target` inside `clip`.
    fn replay<B: target_pixel_buffer::TargetPixelBuffer>(
        &self,
        target: &mut RenderToBuffer<'_, B>,
        clip: &PhysicalRect,
    ) {
        let Some(visible) = clip.intersection(&self.area) else { return };
        let bounds = target.dirty_region.bounding_rect();
        let Some(drawn) = visible.intersection(&bounds) else { return };
        // Copies only where the dirty region covers the layer's visible width in one rectangle.
        let whole_width = target.dirty_region.iter_box().count() == 1
            && drawn.min_x() == visible.min_x()
            && drawn.max_x() == visible.max_x();
        if !whole_width {
            self.draw(target, Some(drawn));
            return;
        }
        let kind = core::any::type_name::<B::TargetPixel>();
        let x = drawn.min_x()..drawn.max_x();
        let (x0, x1) = (x.start as usize, x.end as usize);
        let mut stale: Vec<i16> = Vec::new();
        for y in drawn.min_y()..drawn.max_y() {
            let line = &mut target.buffer.line_slice(y as usize)[x0..x1];
            let index = (y - self.area.min_y()) as usize;
            let snapshot = self.lines[index].lock().unwrap();
            match snapshot.as_ref() {
                Some(s) if s.kind == kind && s.x == x && s.before == pixel_bytes(line) => {
                    pixel_bytes_mut(line).copy_from_slice(&s.after);
                }
                _ => stale.push(y),
            }
        }
        // Draw the other lines, a run at a time, and remember them.
        let mut start = 0;
        while start < stale.len() {
            let mut end = start + 1;
            while end < stale.len() && stale[end] == stale[end - 1] + 1 {
                end += 1;
            }
            let (top, bottom) = (stale[start], stale[end - 1] + 1);
            let before: Vec<Vec<u8>> = (top..bottom)
                .map(|y| pixel_bytes(&target.buffer.line_slice(y as usize)[x0..x1]).to_vec())
                .collect();
            let run = PhysicalRect::new(
                PhysicalPoint::new(x.start, top),
                PhysicalSize::new(x.end - x.start, bottom - top),
            );
            self.draw(target, Some(run));
            for (y, before) in (top..bottom).zip(before) {
                let after = pixel_bytes(&target.buffer.line_slice(y as usize)[x0..x1]).to_vec();
                let index = (y - self.area.min_y()) as usize;
                *self.lines[index].lock().unwrap() =
                    Some(LineSnapshot { kind, x: x.clone(), before, after });
            }
            start = end;
        }
    }

    /// Replay the calls inside `clip`.
    fn draw<B: target_pixel_buffer::TargetPixelBuffer>(
        &self,
        target: &mut RenderToBuffer<'_, B>,
        clip: Option<PhysicalRect>,
    ) {
        let Some(clip) = clip else { return };
        target.replay(&Op::SetClip(Some(clip)));
        for op in &self.ops {
            if op.bounds().intersects(&clip) {
                target.replay(op);
            }
        }
        target.replay(&Op::SetClip(None));
    }
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
            Op::Layer(layer, clip) => layer.replay(self, clip),
            Op::SetClip(Some(clip)) => {
                let whole = self.unclipped.take().unwrap_or_else(|| self.dirty_region.clone());
                self.dirty_region = whole.intersection(clip);
                self.unclipped = Some(whole);
                self.dirty_range_cache.clear();
            }
            Op::SetClip(None) => {
                if let Some(whole) = self.unclipped.take() {
                    self.dirty_region = whole;
                    self.dirty_range_cache.clear();
                }
            }
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
        unclipped: None,
    })
}

/// The bands of one frame not yet taken, for the band threads and, once its walk ended, the
/// UI thread: a band is taken whole and replays every chunk of the stream.
pub(super) struct BandQueue<'a, T> {
    bands: std::sync::Mutex<Vec<SendBand<'a, T>>>,
    region: PhysicalRegion,
    scale_factor: ScaleFactor,
}

// Sound as `SendBand` is: each band is taken by one thread, and its lines are its own.
unsafe impl<T> Sync for BandQueue<'_, T> {}
unsafe impl<T> Send for BandQueue<'_, T> {}

impl<T: TargetPixel> BandQueue<'_, T> {
    /// Take bands until none is left; replay every chunk of `stream` into each.
    pub(super) fn work(&self, stream: &OpStream) {
        loop {
            let Some(band) = self.bands.lock().unwrap().pop() else { return };
            let mut band = band.0;
            let Some(mut renderer) = band_renderer(&mut band, &self.region, self.scale_factor)
            else {
                continue;
            };
            let mut index = 0;
            while let Some(chunk) = stream.chunk(index) {
                for op in chunk.iter() {
                    renderer.replay(op);
                }
                index += 1;
            }
        }
    }
}

/// Split `buffer` under `region` into bands several times more than `threads`, and start
/// `threads` threads taking them; the queue goes back to the caller, whose own thread takes
/// the bands still left once it has published the last chunk. Balanced by what is left, not
/// by rows: a thread that finishes early takes another band.
pub(super) fn stream_into_bands<'scope, 'env, T: TargetPixel>(
    scope: &'scope std::thread::Scope<'scope, 'env>,
    stream: &'scope OpStream,
    buffer: &'scope mut [T],
    pixel_stride: usize,
    region: &PhysicalRegion,
    scale_factor: ScaleFactor,
    threads: usize,
) -> Arc<BandQueue<'scope, T>> {
    let mut bands = split_bands(buffer, pixel_stride, region, (threads + 1) * 2);
    // Taken from the end: the top of the region first.
    bands.reverse();
    let queue = Arc::new(BandQueue {
        bands: std::sync::Mutex::new(bands),
        region: region.clone(),
        scale_factor,
    });
    for _ in 0..threads {
        let queue = queue.clone();
        scope.spawn(move || queue.work(stream));
    }
    queue
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
