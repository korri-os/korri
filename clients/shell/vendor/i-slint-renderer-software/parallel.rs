// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

//! Rasterising one frame on several threads.
//!
//! The item walk stays on the UI thread: the scene builder's draw calls are
//! recorded as [`Op`]s instead of being drawn. The frame's dirty region is then
//! split into horizontal bands of buffer lines, and one scoped thread per band
//! replays every op through a [`RenderToBuffer`] clipped to its band. Bands own
//! disjoint lines, and each op draws a pixel exactly as the single-threaded
//! renderer does, so the result is the same image.

#![allow(unsafe_code)] // Two marker impls, justified where they are written.

use super::*;

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

/// Replay `recorded` into `buffer` on `threads` threads, one band of lines each.
pub(super) fn replay_in_bands<T: TargetPixel>(
    recorded: &Recorded,
    buffer: &mut [T],
    pixel_stride: usize,
    threads: usize,
) {
    let total_lines = buffer.len() / pixel_stride;
    let bounds = recorded.dirty_region.bounding_rect();
    let (top, bottom) = (bounds.min_y().max(0) as usize, (bounds.max_y().max(0) as usize).min(total_lines));
    if top >= bottom {
        return;
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
    let shared = SharedOps(recorded);
    let shared = &shared;
    let draw = move |band: SendBand<'_, T>| {
        let recorded = shared.0;
        let mut band = band.0;
        let rect = PhysicalRect::new(
            PhysicalPoint::new(i16::MIN / 2, band.first as i16),
            PhysicalSize::new(i16::MAX, (band.data.len() / band.pixel_stride) as i16),
        );
        let mut renderer = RenderToBuffer {
            buffer: &mut band,
            dirty_range_cache: Vec::new(),
            dirty_region: recorded.dirty_region.intersection(&rect),
            scale_factor: recorded.scale_factor,
            recording: None,
        };
        if renderer.dirty_region.count == 0 {
            return;
        }
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
