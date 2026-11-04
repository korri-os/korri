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

fn same_color(a: &PremultipliedRgbaColor, b: &PremultipliedRgbaColor) -> bool {
    bytemuck::bytes_of(a) == bytemuck::bytes_of(b)
}

fn same_extra(a: &SceneTextureExtra, b: &SceneTextureExtra) -> bool {
    a.dx == b.dx
        && a.dy == b.dy
        && a.off_x == b.off_x
        && a.off_y == b.off_y
        && a.colorize == b.colorize
        && a.alpha == b.alpha
        && a.rotation == b.rotation
}

/// The same pixels: the same bytes (the same memory, or equal content).
fn same_texture(a: &SceneTexture<'_>, b: &SceneTexture<'_>) -> bool {
    a.format == b.format
        && a.pixel_stride == b.pixel_stride
        && same_extra(&a.extra, &b.extra)
        && a.data.len() == b.data.len()
        && (core::ptr::eq(a.data.as_ptr(), b.data.as_ptr()) || a.data == b.data)
}

impl Op {
    /// Whether replaying `self` draws exactly what replaying `other` draws, on the same
    /// pixels. A cached layer is the same when it is the same recording (`Arc`), which the
    /// previous frame's ops keep alive, so its address is not reused meanwhile.
    pub(super) fn same(&self, other: &Op) -> bool {
        match (self, other) {
            (Op::Background(a), Op::Background(b)) => a == b,
            (Op::SetClip(a), Op::SetClip(b)) => a == b,
            (Op::Layer(a, ra), Op::Layer(b, rb)) => Arc::ptr_eq(a, b) && ra == rb,
            (Op::Texture(ga, a), Op::Texture(gb, b)) => ga == gb && same_texture(a, b),
            (Op::SharedTexture(ga, a), Op::SharedTexture(gb, b)) => {
                ga == gb && a.source_rect == b.source_rect && same_texture(&a.as_texture(), &b.as_texture())
            }
            (Op::Rectangle(ga, a), Op::Rectangle(gb, b)) => ga == gb && same_color(a, b),
            (Op::RoundedRectangle(ga, a), Op::RoundedRectangle(gb, b)) => {
                ga == gb
                    && a.radius == b.radius
                    && a.width == b.width
                    && same_color(&a.border_color, &b.border_color)
                    && same_color(&a.inner_color, &b.inner_color)
                    && a.left_clip == b.left_clip
                    && a.right_clip == b.right_clip
                    && a.top_clip == b.top_clip
                    && a.bottom_clip == b.bottom_clip
            }
            (Op::LinearGradient(ga, a), Op::LinearGradient(gb, b)) => {
                ga == gb
                    && same_color(&a.color1, &b.color1)
                    && same_color(&a.color2, &b.color2)
                    && a.start == b.start
                    && a.flags == b.flags
                    && a.left_clip == b.left_clip
                    && a.right_clip == b.right_clip
                    && a.top_clip == b.top_clip
                    && a.bottom_clip == b.bottom_clip
            }
            (Op::RadialGradient(ga, a), Op::RadialGradient(gb, b)) => {
                ga == gb
                    && a.stops == b.stops
                    && a.center_x == b.center_x
                    && a.center_y == b.center_y
                    && a.radius == b.radius
            }
            (Op::ConicGradient(ga, a), Op::ConicGradient(gb, b)) => {
                ga == gb && a.stops == b.stops && a.center_x == b.center_x && a.center_y == b.center_y
            }
            #[cfg(feature = "path")]
            (
                Op::FilledPath { path: pa, clip: ca, commands: a, color: la },
                Op::FilledPath { path: pb, clip: cb, commands: b, color: lb },
            ) => pa == pb && ca == cb && a == b && same_color(la, lb),
            #[cfg(feature = "path")]
            (
                Op::StrokedPath {
                    path: pa,
                    clip: ca,
                    commands: a,
                    color: la,
                    width: wa,
                    cap: capa,
                    join: joina,
                    miter_limit: ma,
                },
                Op::StrokedPath {
                    path: pb,
                    clip: cb,
                    commands: b,
                    color: lb,
                    width: wb,
                    cap: capb,
                    join: joinb,
                    miter_limit: mb,
                },
            ) => {
                pa == pb
                    && ca == cb
                    && a == b
                    && same_color(la, lb)
                    && wa == wb
                    && capa == capb
                    && joina == joinb
                    && ma == mb
            }
            _ => false,
        }
    }

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
    /// Pixel type, and the area's x range the arrays cover.
    kind: &'static str,
    x: core::ops::Range<i16>,
    before: Vec<u8>,
    after: Vec<u8>,
    /// Per pixel: whether `before` and `after` hold a replay's pixels there.
    valid: Vec<bool>,
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

    /// Draw the calls into `target` inside `clip`. Every pixel's result depends only on the
    /// pixel under it (no call reads its neighbours), so per line: when every pixel to draw
    /// has the pixel under it that it had when last drawn, it gets last time's result; else
    /// the line's part is replayed and remembered.
    fn replay<B: target_pixel_buffer::TargetPixelBuffer>(
        &self,
        target: &mut RenderToBuffer<'_, B>,
        clip: &PhysicalRect,
    ) {
        let Some(visible) = clip.intersection(&self.area) else { return };
        let kind = core::any::type_name::<B::TargetPixel>();
        let size = core::mem::size_of::<B::TargetPixel>();
        // Disjoint parts of the dirty region: a pixel is drawn once.
        for part in target.dirty_clips(&visible) {
            let (x0, x1) = (part.min_x(), part.max_x());
            let mut stale: Vec<i16> = Vec::new();
            for y in part.min_y()..part.max_y() {
                let index = (y - self.area.min_y()) as usize;
                let line = &mut target.buffer.line_slice(y as usize)[x0 as usize..x1 as usize];
                let snapshot = self.lines[index].lock().unwrap();
                let hit = snapshot.as_ref().is_some_and(|s| {
                    s.kind == kind && s.x.start <= x0 && x1 <= s.x.end && {
                        let from = (x0 - s.x.start) as usize;
                        let to = (x1 - s.x.start) as usize;
                        s.valid[from..to].iter().all(|v| *v)
                            && s.before[from * size..to * size] == *pixel_bytes(line)
                    }
                });
                if hit {
                    let s = snapshot.as_ref().unwrap();
                    let from = (x0 - s.x.start) as usize * size;
                    let len = line.len() * size;
                    pixel_bytes_mut(line).copy_from_slice(&s.after[from..from + len]);
                } else {
                    stale.push(y);
                }
            }
            let mut start = 0;
            while start < stale.len() {
                let mut end = start + 1;
                while end < stale.len() && stale[end] == stale[end - 1] + 1 {
                    end += 1;
                }
                let (top, bottom) = (stale[start], stale[end - 1] + 1);
                let before: Vec<Vec<u8>> = (top..bottom)
                    .map(|y| {
                        pixel_bytes(&target.buffer.line_slice(y as usize)[x0 as usize..x1 as usize])
                            .to_vec()
                    })
                    .collect();
                let run = PhysicalRect::new(
                    PhysicalPoint::new(x0, top),
                    PhysicalSize::new(x1 - x0, bottom - top),
                );
                self.draw(target, Some(run));
                for (y, before) in (top..bottom).zip(before) {
                    let after =
                        pixel_bytes(&target.buffer.line_slice(y as usize)[x0 as usize..x1 as usize]);
                    let index = (y - self.area.min_y()) as usize;
                    let mut snapshot = self.lines[index].lock().unwrap();
                    let width = self.area.width() as usize;
                    let fresh = !snapshot.as_ref().is_some_and(|s| s.kind == kind);
                    if fresh {
                        *snapshot = Some(LineSnapshot {
                            kind,
                            x: self.area.min_x()..self.area.max_x(),
                            before: alloc::vec![0; width * size],
                            after: alloc::vec![0; width * size],
                            valid: alloc::vec![false; width],
                        });
                    }
                    let s = snapshot.as_mut().unwrap();
                    let from = (x0 - s.x.start) as usize;
                    let to = (x1 - s.x.start) as usize;
                    s.before[from * size..to * size].copy_from_slice(&before);
                    s.after[from * size..to * size].copy_from_slice(after);
                    s.valid[from..to].iter_mut().for_each(|v| *v = true);
                }
                start = end;
            }
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
///
/// Compared with the previous frame's ops over the same region (`compare_with`), the ops
/// are held back while each is the same as the previous frame's at its place: the band
/// threads draw nothing yet. At the first one that differs, the held ops are published and
/// the frame streams on. A frame whose ops all match is the previous frame's image.
///
/// An opaque rectangle that covers the whole frame region (`cover_region`) hides every op
/// before it: those not published yet are dropped.
pub(super) struct Recorder {
    ops: Vec<Op>,
    stream: Option<Arc<OpStream>>,
    /// The previous frame's ops over the same region (`compare_with`).
    previous: Option<Vec<Op>>,
    /// Whether every op since the frame (or the last cover) began is `previous`'s at its
    /// place; the matching ops are `held`, not published.
    matching: bool,
    held: Vec<Op>,
    /// Whether a chunk went to the stream: published ops cannot be dropped any more.
    published: bool,
    /// The frame region's bounds, when an op covering them hides what is under it.
    region: Option<PhysicalRect>,
    /// The clip of the ops recorded last (`Op::SetClip`).
    clip: Option<PhysicalRect>,
}

impl Recorder {
    pub(super) fn new() -> Self {
        Self {
            ops: Vec::new(),
            stream: None,
            previous: None,
            matching: false,
            held: Vec::new(),
            published: false,
            region: None,
            clip: None,
        }
    }

    /// From now on, publish every full chunk to `stream`.
    pub(super) fn stream_to(&mut self, stream: Arc<OpStream>) {
        self.stream = Some(stream);
    }

    /// Hold the ops back while they are those of `previous`, the previous frame's over the
    /// same region.
    pub(super) fn compare_with(&mut self, previous: Vec<Op>) {
        self.previous = Some(previous);
        self.matching = true;
    }

    /// From now on, an opaque rectangle that covers `bounds` (the frame region's) drops the
    /// ops before it that are not published yet: it draws every pixel they drew.
    pub(super) fn cover_region(&mut self, bounds: PhysicalRect) {
        self.region = Some(bounds);
    }

    /// Whether `op` paints every pixel of the frame region, whatever was there.
    fn covers_region(&self, op: &Op) -> bool {
        let (Some(region), Op::Rectangle(geometry, color)) = (self.region, op) else {
            return false;
        };
        if color.alpha != 255 || region.is_empty() {
            return false;
        }
        let drawn = match self.clip {
            Some(clip) => geometry.intersection(&clip).unwrap_or_default(),
            None => *geometry,
        };
        drawn.contains_rect(&region)
    }

    pub(super) fn push(&mut self, op: Op) {
        if let Op::SetClip(clip) = &op {
            self.clip = *clip;
        }
        if !self.published && self.covers_region(&op) {
            // Everything recorded so far is painted over: keep only the clip it is under,
            // and compare again from there.
            self.ops.clear();
            self.held.clear();
            self.matching = self.previous.is_some();
            if let Some(clip) = self.clip {
                self.push_compared(Op::SetClip(Some(clip)));
            }
        }
        self.push_compared(op);
    }

    fn push_compared(&mut self, op: Op) {
        if self.matching {
            let previous = self.previous.as_ref().expect("matching compares with previous ops");
            if previous.get(self.held.len()).is_some_and(|p| p.same(&op)) {
                self.held.push(op);
                return;
            }
            self.matching = false;
            for held in core::mem::take(&mut self.held) {
                self.publish(held);
            }
        }
        self.publish(op);
    }

    fn publish(&mut self, op: Op) {
        self.ops.push(op);
        if self.ops.len() >= CHUNK {
            if let Some(stream) = &self.stream {
                self.published = true;
                stream.publish(core::mem::replace(&mut self.ops, Vec::with_capacity(CHUNK)), false);
            }
        }
    }

    /// The ops not published yet (all of them without a stream), and whether they are all
    /// the previous frame's: then nothing was published, and they are returned.
    pub(super) fn into_ops(mut self) -> (Vec<Op>, bool) {
        if self.matching {
            if self.previous.as_ref().is_some_and(|p| p.len() == self.held.len()) {
                return (self.held, true);
            }
            for held in core::mem::take(&mut self.held) {
                self.publish(held);
            }
        }
        (self.ops, false)
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

    /// How many chunks are published, and whether the last one is.
    fn status(&self) -> (usize, bool) {
        let state = self.state.lock().unwrap();
        (state.chunks.len(), state.done)
    }

    /// Wait until more than `published` chunks are published, or the stream is done when it
    /// was not.
    fn wait_beyond(&self, published: usize, done: bool) {
        let mut state = self.state.lock().unwrap();
        while state.chunks.len() <= published && state.done == done {
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
    /// The ops are all the previous frame's over the same region: its image.
    pub same_as_previous: bool,
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

/// How band threads share a frame's bands (`SoftwareRenderer::set_interleaved_bands`).
/// Interleaved (the default): a thread takes any band whose next chunk is published,
/// replays the chunks published so far into it, and moves on, so every band keeps up with
/// the walk. Whole: a thread takes a band and replays every chunk of the frame into it,
/// waiting for each; the bands not taken wait for the walk to end.
pub(super) static INTERLEAVED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// One band of a frame: its lines, its renderer's state between chunks, and the next
/// chunk it replays.
struct BandSlot<'a, T> {
    band: Band<'a, T>,
    /// The band's part of the region, its clip state, and its range cache; `None` once
    /// the band has no part of the region.
    state: Option<(PhysicalRegion, Option<PhysicalRegion>, Vec<core::ops::Range<i16>>)>,
    next: usize,
    finished: bool,
}

/// The bands of one frame, for the band threads and, once its walk ended, the UI thread.
pub(super) struct BandQueue<'a, T> {
    slots: Vec<std::sync::Mutex<BandSlot<'a, T>>>,
    /// Whole bands not yet taken (`INTERLEAVED` off).
    whole: std::sync::Mutex<Vec<usize>>,
    interleaved: bool,
    scale_factor: ScaleFactor,
}

// Sound as `SendBand` is: each band is replayed by one thread at a time (its mutex), and
// its lines are its own.
unsafe impl<T> Sync for BandQueue<'_, T> {}
unsafe impl<T> Send for BandQueue<'_, T> {}

impl<T: TargetPixel> BandSlot<'_, T> {
    /// Replay `chunk` into the band, as one renderer replaying every op in order would.
    fn replay(&mut self, chunk: &[Op], scale_factor: ScaleFactor) {
        let Some((dirty_region, unclipped, dirty_range_cache)) = self.state.take() else {
            return;
        };
        let mut renderer = RenderToBuffer {
            buffer: &mut self.band,
            dirty_range_cache,
            dirty_region,
            scale_factor,
            recording: None,
            unclipped,
        };
        for op in chunk {
            renderer.replay(op);
        }
        self.state = Some((renderer.dirty_region, renderer.unclipped, renderer.dirty_range_cache));
    }
}

impl<T: TargetPixel> BandQueue<'_, T> {
    /// Replay every chunk of `stream` into every band, with the other threads doing the same.
    pub(super) fn work(&self, stream: &OpStream) {
        if !self.interleaved {
            loop {
                let Some(i) = self.whole.lock().unwrap().pop() else { return };
                let mut slot = self.slots[i].lock().unwrap();
                while let Some(chunk) = stream.chunk(slot.next) {
                    slot.replay(&chunk, self.scale_factor);
                    slot.next += 1;
                }
                slot.finished = true;
            }
        }
        // Start where another thread is least likely to be.
        let start = THREAD_START.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        loop {
            let (published, done) = stream.status();
            let mut progressed = false;
            let mut unfinished = false;
            for k in 0..self.slots.len() {
                let i = (start + k) % self.slots.len();
                let Ok(mut slot) = self.slots[i].try_lock() else {
                    unfinished = true;
                    continue;
                };
                if slot.finished {
                    continue;
                }
                while slot.next < published {
                    let chunk = stream.chunk(slot.next).expect("published");
                    slot.replay(&chunk, self.scale_factor);
                    slot.next += 1;
                    progressed = true;
                }
                if done && slot.next == published {
                    slot.finished = true;
                } else {
                    unfinished = true;
                }
            }
            if !unfinished {
                return;
            }
            if !progressed {
                if done {
                    // Every chunk is out; the bands left are being replayed by the threads
                    // that hold them, which finish them.
                    return;
                }
                stream.wait_beyond(published, done);
            }
        }
    }
}

static THREAD_START: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Split `buffer` under `region` into bands several times more than `threads`, and start
/// `threads` threads replaying them; the queue goes back to the caller, whose own thread
/// helps once it has published the last chunk.
pub(super) fn stream_into_bands<'scope, 'env, T: TargetPixel>(
    scope: &'scope std::thread::Scope<'scope, 'env>,
    stream: &'scope OpStream,
    buffer: &'scope mut [T],
    pixel_stride: usize,
    region: &PhysicalRegion,
    scale_factor: ScaleFactor,
    threads: usize,
) -> Arc<BandQueue<'scope, T>> {
    let interleaved = INTERLEAVED.load(std::sync::atomic::Ordering::Relaxed);
    let bands = split_bands(buffer, pixel_stride, region, (threads + 1) * 2);
    let slots: Vec<_> = bands
        .into_iter()
        .map(|band| {
            let mut band = band.0;
            let state = band_renderer(&mut band, region, scale_factor)
                .map(|r| (r.dirty_region, r.unclipped, r.dirty_range_cache));
            std::sync::Mutex::new(BandSlot { finished: state.is_none(), band, state, next: 0 })
        })
        .collect();
    // Taken from the end: the top of the region first.
    let whole = (0..slots.len()).rev().collect();
    let queue = Arc::new(BandQueue {
        slots,
        whole: std::sync::Mutex::new(whole),
        interleaved,
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
