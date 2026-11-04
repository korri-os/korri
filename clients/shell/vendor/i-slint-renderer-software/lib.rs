// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

// cSpell: ignore frontmost
#![doc = include_str!("README.md")]
#![doc(html_logo_url = "https://slint.dev/logo/slint-logo-square-light.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(unsafe_code)]
#![cfg_attr(slint_nightly_test, feature(non_exhaustive_omitted_patterns_lint))]
#![cfg_attr(slint_nightly_test, warn(non_exhaustive_omitted_patterns))]
#![no_std]
#![warn(missing_docs)]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

mod draw_functions;
mod fixed;
mod fonts;
mod minimal_software_window;
#[cfg(feature = "path")]
mod path;
mod scene;
#[cfg(feature = "std")]
mod parallel;

use self::fonts::GlyphRenderer;
pub use self::minimal_software_window::MinimalSoftwareWindow;
use self::scene::*;
use alloc::rc::{Rc, Weak};
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};
use core::pin::Pin;
use euclid::Length;
use fixed::Fixed;
#[cfg(feature = "std")]
use i_slint_core::api::PlatformError;
#[cfg(feature = "std")]
use i_slint_core::graphics::Rgba8Pixel;
use i_slint_core::graphics::rendering_metrics_collector::{RefreshMode, RenderingMetricsCollector};
use i_slint_core::graphics::{BorderRadius, SharedImageBuffer, SharedPixelBuffer};
use i_slint_core::item_rendering::HasFont;
use i_slint_core::item_rendering::{
    CachedRenderingData, ItemRenderer, PlainOrStyledText, RenderBorderRectangle, RenderImage,
    RenderRectangle,
};
use i_slint_core::item_tree::ItemTreeWeak;
use i_slint_core::items::{ItemRc, RenderingResult, TextOverflow, TextWrap};
use i_slint_core::lengths::{
    LogicalBorderRadius, LogicalLength, LogicalPoint, LogicalRect, LogicalSize, LogicalVector,
    PhysicalPx, PointLengths, RectLengths, ScaleFactor, SizeLengths,
};
use i_slint_core::partial_renderer::{DirtyRegion, PartialRenderingState};
use i_slint_core::renderer::RendererSealed;
use i_slint_core::textlayout::{AbstractFont, FontMetrics, TextParagraphLayout};
use i_slint_core::window::{WindowAdapter, WindowInner};
use i_slint_core::{Brush, Color, ImageInner, StaticTextures};
#[allow(unused)]
use num_traits::Float;
use num_traits::NumCast;

pub use draw_functions::{PremultipliedRgbaColor, Rgb565BigEndianPixel, Rgb565Pixel, TargetPixel};

type PhysicalLength = euclid::Length<i16, PhysicalPx>;
type PhysicalRect = euclid::Rect<i16, PhysicalPx>;
type PhysicalSize = euclid::Size2D<i16, PhysicalPx>;
type PhysicalPoint = euclid::Point2D<i16, PhysicalPx>;
type PhysicalBorderRadius = BorderRadius<i16, PhysicalPx>;

pub use i_slint_core::partial_renderer::RepaintBufferType;

use fonts::with_font;

/// This enum describes the rotation that should be applied to the contents rendered by the software renderer.
///
/// Argument to be passed in [`SoftwareRenderer::set_rendering_rotation`].
#[non_exhaustive]
#[derive(Default, Copy, Clone, Eq, PartialEq, Debug)]
pub enum RenderingRotation {
    /// No rotation
    #[default]
    NoRotation,
    /// Rotate 90° to the right
    Rotate90,
    /// 180° rotation (upside-down)
    Rotate180,
    /// Rotate 90° to the left
    Rotate270,
}

impl RenderingRotation {
    fn is_transpose(self) -> bool {
        matches!(self, Self::Rotate90 | Self::Rotate270)
    }
    fn mirror_width(self) -> bool {
        matches!(self, Self::Rotate270 | Self::Rotate180)
    }
    fn mirror_height(self) -> bool {
        matches!(self, Self::Rotate90 | Self::Rotate180)
    }
    /// Angle of the rotation in degrees
    pub fn angle(self) -> f32 {
        match self {
            RenderingRotation::NoRotation => 0.,
            RenderingRotation::Rotate90 => 90.,
            RenderingRotation::Rotate180 => 180.,
            RenderingRotation::Rotate270 => 270.,
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct RotationInfo {
    orientation: RenderingRotation,
    screen_size: PhysicalSize,
}

/// Extension trait for euclid type to transpose coordinates (swap x and y, as well as width and height)
trait Transform {
    /// Return a copy of Self whose coordinate are swapped (x swapped with y)
    #[must_use]
    fn transformed(self, info: RotationInfo) -> Self;
}

impl<T: Copy + NumCast + core::ops::Sub<Output = T>> Transform for euclid::Point2D<T, PhysicalPx> {
    fn transformed(mut self, info: RotationInfo) -> Self {
        if info.orientation.mirror_width() {
            self.x = T::from(info.screen_size.width).unwrap() - self.x - T::from(1).unwrap()
        }
        if info.orientation.mirror_height() {
            self.y = T::from(info.screen_size.height).unwrap() - self.y - T::from(1).unwrap()
        }
        if info.orientation.is_transpose() {
            core::mem::swap(&mut self.x, &mut self.y);
        }
        self
    }
}

impl<T: Copy> Transform for euclid::Size2D<T, PhysicalPx> {
    fn transformed(mut self, info: RotationInfo) -> Self {
        if info.orientation.is_transpose() {
            core::mem::swap(&mut self.width, &mut self.height);
        }
        self
    }
}

impl<T: Copy + NumCast + core::ops::Sub<Output = T>> Transform for euclid::Rect<T, PhysicalPx> {
    fn transformed(self, info: RotationInfo) -> Self {
        let one = T::from(1).unwrap();
        let mut origin = self.origin.transformed(info);
        let size = self.size.transformed(info);
        if info.orientation.mirror_width() {
            origin.y = origin.y - (size.height - one);
        }
        if info.orientation.mirror_height() {
            origin.x = origin.x - (size.width - one);
        }
        Self::new(origin, size)
    }
}

impl<T: Copy> Transform for BorderRadius<T, PhysicalPx> {
    fn transformed(self, info: RotationInfo) -> Self {
        match info.orientation {
            RenderingRotation::NoRotation => self,
            RenderingRotation::Rotate90 => {
                Self::new(self.bottom_left, self.top_left, self.top_right, self.bottom_right)
            }
            RenderingRotation::Rotate180 => {
                Self::new(self.bottom_right, self.bottom_left, self.top_left, self.top_right)
            }
            RenderingRotation::Rotate270 => {
                Self::new(self.top_right, self.bottom_right, self.bottom_left, self.top_left)
            }
        }
    }
}

/// This trait defines a bi-directional interface between Slint and your code to send lines to your screen, when using
/// the [`SoftwareRenderer::render_by_line`] function.
///
/// * Through the associated `TargetPixel` type Slint knows how to create and manipulate pixels without having to know
///   the exact device-specific binary representation and operations for blending.
/// * Through the `process_line` function Slint notifies you when a line can be rendered and provides a callback that
///   you can invoke to fill a slice of pixels for the given line.
///
/// See the [`render_by_line`](SoftwareRenderer::render_by_line) documentation for an example.
pub trait LineBufferProvider {
    /// The pixel type of the buffer
    type TargetPixel: TargetPixel;

    /// Called once per line, you will have to call the render_fn back with the buffer.
    ///
    /// The `line` is the y position of the line to be drawn.
    /// The `range` is the range within the line that is going to be rendered (eg, within the dirty region)
    /// The `render_fn` function should be called to render the line, passing the buffer
    /// corresponding to the specified line and range.
    fn process_line(
        &mut self,
        line: usize,
        range: core::ops::Range<usize>,
        render_fn: impl FnOnce(&mut [Self::TargetPixel]),
    );
}

#[cfg(not(cbindgen))]
const PHYSICAL_REGION_MAX_SIZE: usize = DirtyRegion::MAX_COUNT;
// cbindgen can't understand associated const correctly, so hardcode the value
#[cfg(cbindgen)]
pub const PHYSICAL_REGION_MAX_SIZE: usize = 3;
const _: () = {
    assert!(PHYSICAL_REGION_MAX_SIZE == 3);
    assert!(DirtyRegion::MAX_COUNT == 3);
};

/// Represents a rectangular region on the screen, used for partial rendering.
///
/// The region may be composed of multiple sub-regions.
#[derive(Clone, Debug, Default)]
#[repr(C)]
pub struct PhysicalRegion {
    rectangles: [euclid::Box2D<i16, PhysicalPx>; PHYSICAL_REGION_MAX_SIZE],
    count: usize,
}

impl PhysicalRegion {
    fn iter_box(&self) -> impl Iterator<Item = euclid::Box2D<i16, PhysicalPx>> + '_ {
        (0..self.count).map(|x| self.rectangles[x])
    }

    fn bounding_rect(&self) -> PhysicalRect {
        if self.count == 0 {
            return Default::default();
        }
        let mut r = self.rectangles[0];
        for i in 1..self.count {
            r = r.union(&self.rectangles[i]);
        }
        r.to_rect()
    }

    /// Returns the size of the bounding box of this region.
    pub fn bounding_box_size(&self) -> i_slint_core::api::PhysicalSize {
        let bb = self.bounding_rect();
        i_slint_core::api::PhysicalSize { width: bb.width() as _, height: bb.height() as _ }
    }
    /// Returns the origin of the bounding box of this region.
    pub fn bounding_box_origin(&self) -> i_slint_core::api::PhysicalPosition {
        let bb = self.bounding_rect();
        i_slint_core::api::PhysicalPosition { x: bb.origin.x as _, y: bb.origin.y as _ }
    }

    /// Returns an iterator over the rectangles in this region.
    /// Each rectangle is represented by its position and its size.
    /// They do not overlap.
    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (i_slint_core::api::PhysicalPosition, i_slint_core::api::PhysicalSize)> + '_
    {
        let mut line_ranges = Vec::<core::ops::Range<i16>>::new();
        let mut begin_line = 0;
        let mut end_line = 0;
        core::iter::from_fn(move || {
            loop {
                match line_ranges.pop() {
                    Some(r) => {
                        return Some((
                            i_slint_core::api::PhysicalPosition {
                                x: r.start as _,
                                y: begin_line as _,
                            },
                            i_slint_core::api::PhysicalSize {
                                width: r.len() as _,
                                height: (end_line - begin_line) as _,
                            },
                        ));
                    }
                    None => {
                        begin_line = end_line;
                        end_line = match region_line_ranges(self, begin_line, &mut line_ranges) {
                            Some(end_line) => end_line,
                            None => return None,
                        };
                        line_ranges.reverse();
                    }
                }
            }
        })
    }

    fn intersection(&self, clip: &PhysicalRect) -> PhysicalRegion {
        let mut res = Self::default();
        let clip = clip.to_box2d();
        let mut count = 0;
        for i in 0..self.count {
            if let Some(r) = self.rectangles[i].intersection(&clip) {
                res.rectangles[count] = r;
                count += 1;
            }
        }
        res.count = count;
        res
    }
}

#[test]
fn region_iter() {
    let mut region = PhysicalRegion::default();
    assert_eq!(region.iter().next(), None);
    region.rectangles[0] =
        euclid::Box2D::from_origin_and_size(euclid::point2(1, 1), euclid::size2(2, 3));
    region.rectangles[1] =
        euclid::Box2D::from_origin_and_size(euclid::point2(6, 2), euclid::size2(3, 20));
    region.rectangles[2] =
        euclid::Box2D::from_origin_and_size(euclid::point2(0, 10), euclid::size2(10, 5));
    assert_eq!(region.iter().next(), None);
    region.count = 1;
    let r = |x, y, width, height| {
        (
            i_slint_core::api::PhysicalPosition { x, y },
            i_slint_core::api::PhysicalSize { width, height },
        )
    };

    let mut iter = region.iter();
    assert_eq!(iter.next(), Some(r(1, 1, 2, 3)));
    assert_eq!(iter.next(), None);
    drop(iter);

    region.count = 3;
    let mut iter = region.iter();
    assert_eq!(iter.next(), Some(r(1, 1, 2, 1))); // the two first rectangle could have been merged
    assert_eq!(iter.next(), Some(r(1, 2, 2, 2)));
    assert_eq!(iter.next(), Some(r(6, 2, 3, 2)));
    assert_eq!(iter.next(), Some(r(6, 4, 3, 6)));
    assert_eq!(iter.next(), Some(r(0, 10, 10, 5)));
    assert_eq!(iter.next(), Some(r(6, 15, 3, 7)));
    assert_eq!(iter.next(), None);
}

/// Computes what are the x ranges that intersects the region for specified y line.
///
/// This uses a mutable reference to a Vec so that the memory is re-used between calls.
///
/// Returns the y position until which this range is valid
fn region_line_ranges(
    region: &PhysicalRegion,
    line: i16,
    line_ranges: &mut Vec<core::ops::Range<i16>>,
) -> Option<i16> {
    line_ranges.clear();
    let mut next_validity = None::<i16>;
    for geom in region.iter_box() {
        if geom.is_empty() {
            continue;
        }
        if geom.y_range().contains(&line) {
            match &mut next_validity {
                Some(val) => *val = geom.max.y.min(*val),
                None => next_validity = Some(geom.max.y),
            }
            let mut tmp = Some(geom.x_range());
            line_ranges.retain_mut(|it| {
                if let Some(r) = &mut tmp {
                    if it.end < r.start {
                        true
                    } else if it.start <= r.start {
                        if it.end >= r.end {
                            tmp = None;
                            return true;
                        }
                        r.start = it.start;
                        false
                    } else if it.start <= r.end {
                        if it.end <= r.end {
                            false
                        } else {
                            it.start = r.start;
                            tmp = None;
                            true
                        }
                    } else {
                        core::mem::swap(it, r);
                        true
                    }
                } else {
                    true
                }
            });
            if let Some(r) = tmp {
                line_ranges.push(r);
            }
            continue;
        } else if geom.min.y >= line {
            match &mut next_validity {
                Some(val) => *val = geom.min.y.min(*val),
                None => next_validity = Some(geom.min.y),
            }
        }
    }
    // check that current items are properly sorted
    debug_assert!(line_ranges.windows(2).all(|x| x[0].end < x[1].start));
    next_validity
}

mod target_pixel_buffer;

#[cfg(feature = "experimental")]
pub use target_pixel_buffer::{
    DrawRectangleArgs, DrawTextureArgs, TargetPixelBuffer, TexturePixelFormat,
};

#[cfg(not(feature = "experimental"))]
use target_pixel_buffer::TexturePixelFormat;

struct TargetPixelSlice<'a, T> {
    data: &'a mut [T],
    pixel_stride: usize,
}

/// A `Layer` (`cache-rendering-hint: true`) rendered once into an image: its children,
/// premultiplied, over transparency.
#[cfg(feature = "std")]
struct CachedLayer {
    /// The layer's physical screen offset when its children's draw calls were recorded. The
    /// calls are replayed only at exactly that offset: each draws the pixels drawing the
    /// children there directly would (positions, edges and blending alike), for every
    /// target pixel format. Elsewhere the layer is drawn directly, and recorded again once
    /// it stays at a new offset.
    at: euclid::Point2D<f32, PhysicalPx>,
    /// Where the layer was last drawn at another offset than `at`, if it was.
    shifted_at: Cell<Option<euclid::Point2D<f32, PhysicalPx>>>,
    /// The screen area the children draw in, in physical pixels.
    area: euclid::Rect<i32, PhysicalPx>,
    /// The children's draw calls, in screen coordinates.
    ops: Vec<parallel::Op>,
}

/// Rendered layers, invalidated through the property tracking of `ItemCache` when anything the
/// children's rendering read changes, and cleared when the scale factor or window size changes.
#[cfg(feature = "std")]
#[derive(Default)]
struct LayerCache {
    items: i_slint_core::item_rendering::ItemCache<Option<Rc<CachedLayer>>>,
    /// Screen areas of layers drawn shifted this frame: drawn again next frame, so a layer that
    /// stopped moving is rendered at its exact phase.
    shifted: RefCell<DirtyRegion>,
    /// This frame's layer images: how many were rendered, their pixels, and the time.
    renders: Cell<(u32, u64, std::time::Duration)>,
    /// Whether this frame rendered a layer image: a frame renders one, so that the end of an
    /// animation does not render every layer at once.
    rendering: Cell<bool>,
    /// Threads a layer image is rasterised with: the renderer's `render` threads this frame.
    threads: Cell<usize>,
    /// This frame's cached layers: invalidated (content changed, drawn directly), drawn
    /// directly while waiting for their turn to render an image, and drawn from an image.
    visits: Cell<(u32, u32, u32)>,
    /// Layers drawn directly because their content changed (`visit_layer`).
    direct: RefCell<std::collections::HashMap<(usize, u32), DirectLayer>>,
}

/// A layer drawn directly because its content changed: the item, its last screen area, and
/// whether the change was seen this frame. Once its content stays the same for a frame, its
/// area is drawn again, and `visit_layer` renders its image.
#[cfg(feature = "std")]
struct DirectLayer {
    item: i_slint_core::item_tree::ItemWeak,
    area: LogicalRect,
    changed_this_frame: bool,
}

#[cfg(feature = "std")]
fn layer_key(item_rc: &ItemRc) -> (usize, u32) {
    (&(**item_rc.item_tree()) as *const _ as *const u8 as usize, item_rc.index())
}

#[cfg(feature = "std")]
impl core::ops::Deref for LayerCache {
    type Target = i_slint_core::item_rendering::ItemCache<Option<Rc<CachedLayer>>>;
    fn deref(&self) -> &Self::Target {
        &self.items
    }
}

impl<'a, T: TargetPixel> target_pixel_buffer::TargetPixelBuffer for TargetPixelSlice<'a, T> {
    type TargetPixel = T;

    fn line_slice(&mut self, line_number: usize) -> &mut [Self::TargetPixel] {
        let offset = line_number * self.pixel_stride;
        &mut self.data[offset..offset + self.pixel_stride]
    }

    fn num_lines(&self) -> usize {
        self.data.len() / self.pixel_stride
    }
}

/// A Renderer that do the rendering in software
///
/// The renderer can remember what items needs to be redrawn from the previous iteration.
///
/// There are two kind of possible rendering
///  1. Using [`render()`](Self::render()) to render the window in a buffer
///  2. Using [`render_by_line()`](Self::render()) to render the window line by line. This
///     is only useful if the device does not have enough memory to render the whole window
///     in one single buffer
pub struct SoftwareRenderer {
    repaint_buffer_type: Cell<RepaintBufferType>,
    /// This is the area which was dirty on the previous frame.
    /// Only used if repaint_buffer_type == RepaintBufferType::SwappedBuffers
    prev_frame_dirty: Cell<DirtyRegion>,
    partial_rendering_state: PartialRenderingState,
    maybe_window_adapter: RefCell<Option<Weak<dyn i_slint_core::window::WindowAdapter>>>,
    rotation: Cell<RenderingRotation>,
    rendering_metrics_collector: Option<Rc<RenderingMetricsCollector>>,
    #[cfg(feature = "systemfonts")]
    text_layout_cache: sharedparley::TextLayoutCache,
    #[cfg(feature = "std")]
    layer_cache: LayerCache,
    /// The window size the layer cache was filled for.
    #[cfg(feature = "std")]
    layer_cache_size: Cell<PhysicalSize>,
    /// The last render's phases: dirty-region walk and rasterisation.
    #[cfg(feature = "std")]
    last_phases: Cell<(std::time::Duration, std::time::Duration)>,
    /// Threads `render` rasterises with (`parallel`).
    #[cfg(feature = "std")]
    render_threads: Cell<usize>,
    /// Of the last raster phase: the banded replay (`parallel`).
    #[cfg(feature = "std")]
    last_replay: Cell<std::time::Duration>,
    /// The last render's frame profile of the dirty-region walk, when profiling is on.
    #[cfg(feature = "std")]
    walk_profile: RefCell<i_slint_core::frame_profile::FrameProfile>,
}

impl Default for SoftwareRenderer {
    fn default() -> Self {
        Self {
            partial_rendering_state: Default::default(),
            prev_frame_dirty: Default::default(),
            maybe_window_adapter: Default::default(),
            rotation: Default::default(),
            rendering_metrics_collector: RenderingMetricsCollector::new("software"),
            repaint_buffer_type: Default::default(),
            #[cfg(feature = "systemfonts")]
            text_layout_cache: Default::default(),
            #[cfg(feature = "std")]
            layer_cache: Default::default(),
            #[cfg(feature = "std")]
            layer_cache_size: Default::default(),
            #[cfg(feature = "std")]
            last_phases: Default::default(),
            #[cfg(feature = "std")]
            render_threads: Cell::new(parallel::default_threads()),
            #[cfg(feature = "std")]
            last_replay: Default::default(),
            #[cfg(feature = "std")]
            walk_profile: Default::default(),
        }
    }
}

#[cfg(feature = "testing")]
impl SoftwareRenderer {
    /// Returns a reference to the text layout cache for testing purposes.
    pub fn text_layout_cache(&self) -> &sharedparley::TextLayoutCache {
        &self.text_layout_cache
    }
}

impl SoftwareRenderer {
    /// Create a new Renderer
    pub fn new() -> Self {
        Default::default()
    }

    /// Create a new SoftwareRenderer.
    ///
    /// The `repaint_buffer_type` parameter specify what kind of buffer are passed to [`Self::render`]
    pub fn new_with_repaint_buffer_type(repaint_buffer_type: RepaintBufferType) -> Self {
        let self_ = Self::default();
        self_.repaint_buffer_type.set(repaint_buffer_type);
        self_
    }

    /// Change the what kind of buffer is being passed to [`Self::render`]
    ///
    /// This may clear the internal caches
    pub fn set_repaint_buffer_type(&self, repaint_buffer_type: RepaintBufferType) {
        if self.repaint_buffer_type.replace(repaint_buffer_type) != repaint_buffer_type {
            self.partial_rendering_state.clear_cache();
            #[cfg(feature = "std")]
            self.layer_cache.clear_all();
        }
    }

    /// How many items the last partial render visited to compute its dirty region.
    #[doc(hidden)]
    pub fn dirty_region_walked_items(&self) -> usize {
        self.partial_rendering_state.walked_items()
    }

    /// The last render's time in the dirty-region walk (property and layout
    /// evaluation included) and in rasterisation.
    #[doc(hidden)]
    #[cfg(feature = "std")]
    pub fn last_frame_phases(&self) -> (std::time::Duration, std::time::Duration) {
        self.last_phases.get()
    }

    /// The last render's layer images: how many were rendered, their pixels (a nested layer's
    /// also in its parent's), and the time (part of the raster phase).
    #[doc(hidden)]
    #[cfg(feature = "std")]
    pub fn last_frame_layer_renders(&self) -> (u32, u64, std::time::Duration) {
        self.layer_cache.renders.get()
    }

    /// Turns the calling thread's frame profile (`i_slint_core::frame_profile`) on or off:
    /// text shaping, line breaking, measuring and drawing, glyph rasterisation and image draws,
    /// timed. For benchmarks; off by default.
    #[doc(hidden)]
    #[cfg(feature = "std")]
    pub fn set_frame_profile(&self, enabled: bool) {
        i_slint_core::frame_profile::set_enabled(enabled);
    }

    /// Turns the property system's equality cut-off on (the default) or off for the calling
    /// thread (`i_slint_core::properties::set_equality_cutoff`). Off, a binding that depends on
    /// a changed property is always evaluated again, as upstream. For tests and benchmarks.
    #[doc(hidden)]
    pub fn set_equality_cutoff(&self, enabled: bool) {
        i_slint_core::properties::set_equality_cutoff(enabled);
    }

    /// Makes `animation-tick()` return `tick` milliseconds on the calling thread, or follow
    /// the clock again with `None` (`i_slint_core::animations::freeze_animation_tick`).
    /// Property animations and timers keep following the clock. For focus probes.
    #[doc(hidden)]
    pub fn freeze_animation_tick(&self, tick: Option<u64>) {
        i_slint_core::animations::freeze_animation_tick(tick);
    }

    /// The calling thread's frame profile since the last call; resets it. After a render:
    /// its raster phase (the walk's part is in [`Self::last_walk_profile`]).
    #[doc(hidden)]
    #[cfg(feature = "std")]
    pub fn take_frame_profile(&self) -> i_slint_core::frame_profile::FrameProfile {
        i_slint_core::frame_profile::take()
    }

    /// The frame profile of the last render's dirty-region walk, when profiling is on. The
    /// profile from before the render up to the walk's end, so take the profile right before
    /// rendering.
    #[doc(hidden)]
    #[cfg(feature = "std")]
    pub fn last_walk_profile(&self) -> i_slint_core::frame_profile::FrameProfile {
        self.walk_profile.take()
    }

    /// The last render's cached layers (`cache-rendering-hint`): invalidated (drawn directly
    /// because their content changed), drawn directly while another layer rendered its image,
    /// and drawn from their image (part of the raster phase).
    #[doc(hidden)]
    #[cfg(feature = "std")]
    pub fn last_frame_layer_visits(&self) -> (u32, u32, u32) {
        self.layer_cache.visits.get()
    }

    /// Returns the kind of buffer that must be passed to  [`Self::render`]
    pub fn repaint_buffer_type(&self) -> RepaintBufferType {
        self.repaint_buffer_type.get()
    }

    /// Set how the window need to be rotated in the buffer.
    ///
    /// This is typically used to implement screen rotation in software
    ///
    /// **Note:** This only affects rendering. Input events must still be given to
    /// Slint in logical (un-rotated) coordinates.
    pub fn set_rendering_rotation(&self, rotation: RenderingRotation) {
        self.rotation.set(rotation)
    }

    /// Return the current rotation. See [`Self::set_rendering_rotation()`]
    pub fn rendering_rotation(&self) -> RenderingRotation {
        self.rotation.get()
    }

    /// Render the window to the given frame buffer.
    ///
    /// The renderer uses a cache internally and will only render the part of the window
    /// which are dirty. The `extra_draw_region` is an extra region which will also
    /// be rendered. (eg: the previous dirty region in case of double buffering)
    /// This function returns the region that was rendered.
    ///
    /// The pixel_stride is the size (in pixels) between two lines in the buffer.
    /// It is equal `width` if the screen is not rotated, and `height` if the screen is rotated by 90°.
    /// The buffer needs to be big enough to contain the window, so its size must be at least
    /// `pixel_stride * height`, or `pixel_stride * width` if the screen is rotated by 90°.
    ///
    /// Returns the physical dirty region for this frame, excluding the extra_draw_region,
    /// in the window frame of reference. It is affected by the screen rotation.
    pub fn render(&self, buffer: &mut [impl TargetPixel], pixel_stride: usize) -> PhysicalRegion {
        #[cfg(feature = "std")]
        self.layer_cache.threads.set(self.render_threads.get());
        #[cfg(feature = "std")]
        if self.render_threads.get() > 1 {
            // Walk the items here and record the draw calls. Once the dirty region is known,
            // band threads start and rasterise the recorded calls while the walk goes on
            // (`parallel`); a small region is drawn here once the walk ended.
            let threads = self.render_threads.get();
            let lines = buffer.len() / pixel_stride.max(1);
            let stream = std::sync::Arc::new(parallel::OpStream::new());
            let shared = &*stream;
            let mut scratch = LinesOnly::like(buffer, pixel_stride, lines);
            let (region, started) = std::thread::scope(|scope| {
                let mut unclaimed = Some(&mut *buffer);
                let mut start = |region: &PhysicalRegion, scale_factor: ScaleFactor| {
                    let pixels: usize = region
                        .iter()
                        .map(|(_, size)| size.width as usize * size.height as usize)
                        .sum();
                    if pixels < parallel::MIN_PARALLEL_PIXELS {
                        return None;
                    }
                    let buffer = unclaimed.take().expect("one frame starts its bands once");
                    // The UI thread keeps recording: one band thread fewer than threads.
                    let bands = (threads - 1).max(1);
                    parallel::stream_into_bands(
                        scope,
                        shared,
                        buffer,
                        pixel_stride,
                        region,
                        scale_factor,
                        bands,
                    );
                    Some(stream.clone())
                };
                let (region, recorded) =
                    self.render_buffer_inner(&mut scratch, true, Some(&mut start));
                drop(start);
                let started = std::time::Instant::now();
                if let Some(recorded) = recorded {
                    match unclaimed.take() {
                        None => shared.publish(recorded.ops, true),
                        Some(buffer) => {
                            let mut ops = shared.take_ops();
                            ops.extend(recorded.ops);
                            let recorded = parallel::Recorded { ops, ..recorded };
                            parallel::replay_in_bands(&recorded, buffer, pixel_stride, 1);
                        }
                    }
                } else {
                    shared.publish(Vec::new(), true);
                }
                (region, started)
            });
            // The scope waited for the band threads: their tail counts as replay.
            let replay = started.elapsed();
            let (walk, record) = self.last_phases.get();
            self.last_phases.set((walk, record + replay));
            self.last_replay.set(replay);
            return region;
        }
        self.render_buffer_impl(&mut TargetPixelSlice { data: buffer, pixel_stride })
    }

    /// Of the last raster phase, the time replaying the recorded draw calls in bands; zero
    /// when the frame was drawn on one thread without recording.
    #[doc(hidden)]
    #[cfg(feature = "std")]
    pub fn last_frame_replay(&self) -> std::time::Duration {
        self.last_replay.get()
    }

    /// How many threads `render` rasterises a frame with. The default is the
    /// online cores, at most 4; 1 draws on the calling thread only.
    #[doc(hidden)]
    #[cfg(feature = "std")]
    pub fn set_render_threads(&self, threads: usize) {
        self.render_threads.set(threads.max(1));
    }

    /// Render the window to the given frame buffer.
    ///
    /// The renderer uses a cache internally and will only render the part of the window
    /// which are dirty. The `extra_draw_region` is an extra region which will also
    /// be rendered. (eg: the previous dirty region in case of double buffering)
    /// This function returns the region that was rendered.
    ///
    /// The buffer's line slices need to be wide enough to if the `width` of the screen and the line count the `height`,
    /// or the `height` and `width` swapped if the screen is rotated by 90°.
    ///
    /// Returns the physical dirty region for this frame, excluding the extra_draw_region,
    /// in the window frame of reference. It is affected by the screen rotation.
    #[cfg(feature = "experimental")]
    pub fn render_into_buffer(&self, buffer: &mut impl TargetPixelBuffer) -> PhysicalRegion {
        self.render_buffer_impl(buffer)
    }

    fn render_buffer_impl(
        &self,
        buffer: &mut impl target_pixel_buffer::TargetPixelBuffer,
    ) -> PhysicalRegion {
        self.render_buffer_inner(buffer, false, None).0
    }

    /// Render, or with `record` walk the items and return the frame's draw
    /// calls instead of drawing them.
    ///
    /// While recording, `start_bands` is called once the dirty region is known; when it
    /// started band threads, it returns their stream, and the recorded calls go to it.
    fn render_buffer_inner(
        &self,
        buffer: &mut impl target_pixel_buffer::TargetPixelBuffer,
        #[allow(unused)] record: bool,
        #[allow(unused)] mut start_bands: Option<&mut StartBands<'_>>,
    ) -> (PhysicalRegion, Option<parallel::Recorded>) {
        #[cfg(feature = "std")]
        let mut recorded = None;
        #[cfg(not(feature = "std"))]
        let recorded = None;
        let pixels_per_line = buffer.line_slice(0).len();
        let num_lines = buffer.num_lines();
        let buffer_pixel_count = num_lines * pixels_per_line;

        let Some(window) = self.maybe_window_adapter.borrow().as_ref().and_then(|w| w.upgrade())
        else {
            return (Default::default(), None);
        };
        let window_inner = WindowInner::from_pub(window.window());
        let factor = ScaleFactor::new(window_inner.scale_factor());
        let rotation = self.rotation.get();
        let (size, background) = if let Some(window_item) =
            window_inner.window_item().as_ref().map(|item| item.as_pin_ref())
        {
            (
                (LogicalSize::from_lengths(window_item.width(), window_item.height()).cast()
                    * factor)
                    .cast(),
                window_item.background(),
            )
        } else if rotation.is_transpose() {
            (euclid::size2(num_lines as _, pixels_per_line as _), Brush::default())
        } else {
            (euclid::size2(pixels_per_line as _, num_lines as _), Brush::default())
        };
        if size.is_empty() {
            return (Default::default(), None);
        }
        #[cfg(feature = "std")]
        {
            self.layer_cache.clear_cache_if_scale_factor_changed(window.window());
            if self.layer_cache_size.replace(size) != size {
                self.layer_cache.clear_all();
            }
        }
        assert!(
            if rotation.is_transpose() {
                pixels_per_line >= size.height as usize
                    && buffer_pixel_count
                        >= (size.width as usize * pixels_per_line + size.height as usize)
                            - pixels_per_line
            } else {
                pixels_per_line >= size.width as usize
                    && buffer_pixel_count
                        >= (size.height as usize * pixels_per_line + size.width as usize)
                            - pixels_per_line
            },
            "buffer of size {} with {pixels_per_line} pixels per line is too small to handle a window of size {size:?}",
            buffer_pixel_count
        );
        let buffer_renderer = SceneBuilder::new(
            size,
            factor,
            window_inner,
            RenderToBuffer {
                buffer,
                dirty_range_cache: Vec::new(),
                dirty_region: Default::default(),
                scale_factor: factor,
                #[cfg(feature = "std")]
                recording: None,
                #[cfg(feature = "std")]
                unclipped: None,
            },
            rotation,
            #[cfg(feature = "systemfonts")]
            &self.text_layout_cache,
            #[cfg(feature = "std")]
            Some(&self.layer_cache),
        );
        let mut renderer = self.partial_rendering_state.create_partial_renderer(buffer_renderer);
        #[cfg(feature = "std")]
        if record {
            renderer.actual_renderer.processor.recording = Some(parallel::Recorder::new());
        }
        let window_adapter = renderer.window_adapter.clone();

        let region = window_inner
            .draw_contents(|components, post_render| {
                let logical_size = (size.cast() / factor).cast();
                #[cfg(feature = "std")]
                let walk_started = std::time::Instant::now();

                match self.repaint_buffer_type.get() {
                    RepaintBufferType::NewBuffer => {
                        renderer.dirty_region = LogicalRect::from_size(logical_size).into();
                        self.partial_rendering_state.clear_cache();
                    }
                    RepaintBufferType::ReusedBuffer => {
                        self.partial_rendering_state.apply_dirty_region(
                            &mut renderer,
                            components,
                            logical_size,
                            None,
                        );
                    }
                    RepaintBufferType::SwappedBuffers => {
                        let dirty_region_for_this_frame =
                            self.partial_rendering_state.apply_dirty_region(
                                &mut renderer,
                                components,
                                logical_size,
                                Some(self.prev_frame_dirty.take()),
                            );
                        self.prev_frame_dirty.set(dirty_region_for_this_frame);
                    }
                }

                #[cfg(feature = "std")]
                let (walk, raster_started) = (walk_started.elapsed(), std::time::Instant::now());
                #[cfg(feature = "std")]
                if i_slint_core::frame_profile::enabled() {
                    *self.walk_profile.borrow_mut() = i_slint_core::frame_profile::take();
                }
                #[cfg(feature = "std")]
                {
                    self.layer_cache.renders.set(Default::default());
                    self.layer_cache.visits.set(Default::default());
                    self.layer_cache.rendering.set(false);
                }
                let rotation = RotationInfo { orientation: rotation, screen_size: size };
                let screen_rect = PhysicalRect::from_size(size);
                let mut i = renderer.dirty_region.iter().filter_map(|r| {
                    (r.cast() * factor)
                        .to_rect()
                        .round_out()
                        .cast()
                        .intersection(&screen_rect)?
                        .transformed(rotation)
                        .into()
                });
                let dirty_region = PhysicalRegion {
                    rectangles: core::array::from_fn(|_| i.next().unwrap_or_default().to_box2d()),
                    count: renderer.dirty_region.iter().count(),
                };
                drop(i);

                renderer.actual_renderer.processor.dirty_region = dirty_region.clone();
                #[cfg(feature = "std")]
                if let Some(recorder) = &mut renderer.actual_renderer.processor.recording {
                    if let Some(stream) = start_bands.as_mut().and_then(|start| start(&dirty_region, factor)) {
                        recorder.stream_to(stream);
                    }
                    recorder.push(parallel::Op::Background(background.color()));
                } else if !renderer
                    .actual_renderer
                    .processor
                    .buffer
                    .fill_background(&background, &dirty_region)
                {
                    let mut bg = TargetPixel::background();
                    TargetPixel::blend(&mut bg, background.color().into());
                    renderer.actual_renderer.processor.foreach_ranges(
                        &dirty_region.bounding_rect(),
                        |_, buffer, _, _| {
                            buffer.fill(bg);
                        },
                    );
                }
                #[cfg(not(feature = "std"))]
                if !renderer
                    .actual_renderer
                    .processor
                    .buffer
                    .fill_background(&background, &dirty_region)
                {
                    let mut bg = TargetPixel::background();
                    // TODO: gradient background
                    TargetPixel::blend(&mut bg, background.color().into());
                    renderer.actual_renderer.processor.foreach_ranges(
                        &dirty_region.bounding_rect(),
                        |_, buffer, _, _| {
                            buffer.fill(bg);
                        },
                    );
                }

                let partial = self.repaint_buffer_type.get() != RepaintBufferType::NewBuffer;
                for (component, origin) in components {
                    if let Some(component) = ItemTreeWeak::upgrade(component) {
                        i_slint_core::item_rendering::render_component_items(
                            &component,
                            if partial { &mut renderer } else { &mut renderer.actual_renderer },
                            *origin,
                            &window_adapter,
                        );
                    }
                }

                if partial {
                    post_render(&mut renderer);
                } else {
                    post_render(&mut renderer.actual_renderer);
                }

                #[cfg(feature = "std")]
                {
                    // Layers drawn shifted are drawn again next frame, at their exact phase
                    // if they stopped moving.
                    let mut shifted = self.layer_cache.shifted.take();
                    // A layer drawn directly whose content did not change since is drawn again
                    // next frame, to render its image. One that changed is drawn again anyway.
                    let mut check_again = false;
                    for layer in self.layer_cache.direct.borrow_mut().values_mut() {
                        let changed = core::mem::take(&mut layer.changed_this_frame);
                        check_again |= changed;
                        let stable = !changed
                            && layer
                                .item
                                .upgrade()
                                .is_some_and(|item| self.layer_cache.is_dirty(&item) != Some(true));
                        if stable {
                            shifted.add_rect(layer.area);
                        }
                    }
                    // A layer that changed this frame: the next frame tells whether it stays.
                    if check_again {
                        window.window().request_redraw();
                    }
                    if shifted.iter().next().is_some() {
                        self.partial_rendering_state.mark_dirty_region(shifted);
                        window.window().request_redraw();
                    }
                }
                #[cfg(feature = "std")]
                self.last_phases.set((walk, raster_started.elapsed()));
                self.measure_frame_rendered(&mut renderer);
                #[cfg(feature = "std")]
                if let Some(recorder) = renderer.actual_renderer.processor.recording.take() {
                    recorded = Some(parallel::Recorded {
                        ops: recorder.into_ops(),
                        dirty_region: dirty_region.clone(),
                        scale_factor: factor,
                    });
                }

                dirty_region
            })
            .unwrap_or_default();
        (region, recorded)
    }

    fn measure_frame_rendered(&self, renderer: &mut dyn ItemRenderer) {
        if let Some(metrics) = &self.rendering_metrics_collector {
            let prev_frame_dirty = self.prev_frame_dirty.take();
            let m = i_slint_core::graphics::rendering_metrics_collector::RenderingMetrics {
                dirty_region: Some(prev_frame_dirty.clone()),
                ..Default::default()
            };
            self.prev_frame_dirty.set(prev_frame_dirty);
            metrics.measure_frame_rendered(renderer, m);
            if metrics.refresh_mode() == RefreshMode::FullSpeed {
                self.partial_rendering_state.force_screen_refresh();
            }
        }
    }

    /// Render the window, line by line, into the line buffer provided by the [`LineBufferProvider`].
    ///
    /// The renderer uses a cache internally and will only render the part of the window
    /// which are dirty, depending on the dirty tracking policy set in [`SoftwareRenderer::new`]
    /// This function returns the physical region that was rendered considering the rotation.
    ///
    /// The [`LineBufferProvider::process_line()`] function will be called for each line and should
    ///  provide a buffer to draw into.
    ///
    /// As an example, let's imagine we want to render into a plain buffer.
    /// (You wouldn't normally use `render_by_line` for that because the [`Self::render`] would
    /// then be more efficient)
    ///
    /// ```rust
    /// # use i_slint_renderer_software::{LineBufferProvider, SoftwareRenderer, Rgb565Pixel};
    /// # fn xxx<'a>(the_frame_buffer: &'a mut [Rgb565Pixel], display_width: usize, renderer: &SoftwareRenderer) {
    /// struct FrameBuffer<'a>{ frame_buffer: &'a mut [Rgb565Pixel], stride: usize }
    /// impl<'a> LineBufferProvider for FrameBuffer<'a> {
    ///     type TargetPixel = Rgb565Pixel;
    ///     fn process_line(
    ///         &mut self,
    ///         line: usize,
    ///         range: core::ops::Range<usize>,
    ///         render_fn: impl FnOnce(&mut [Self::TargetPixel]),
    ///     ) {
    ///         let line_begin = line * self.stride;
    ///         render_fn(&mut self.frame_buffer[line_begin..][range]);
    ///         // The line has been rendered and there could be code here to
    ///         // send the pixel to the display
    ///     }
    /// }
    /// renderer.render_by_line(FrameBuffer{ frame_buffer: the_frame_buffer, stride: display_width });
    /// # }
    /// ```
    pub fn render_by_line(&self, line_buffer: impl LineBufferProvider) -> PhysicalRegion {
        let Some(window) = self.maybe_window_adapter.borrow().as_ref().and_then(|w| w.upgrade())
        else {
            return Default::default();
        };
        let window_inner = WindowInner::from_pub(window.window());
        let component_rc = window_inner.component();
        let component = i_slint_core::item_tree::ItemTreeRc::borrow_pin(&component_rc);
        if let Some(window_item) = i_slint_core::items::ItemRef::downcast_pin::<
            i_slint_core::items::WindowItem,
        >(component.as_ref().get_item_ref(0))
        {
            let factor = ScaleFactor::new(window_inner.scale_factor());
            let size = LogicalSize::from_lengths(window_item.width(), window_item.height()).cast()
                * factor;
            render_window_frame_by_line(
                window_inner,
                window_item.background(),
                size.cast(),
                self,
                line_buffer,
            )
        } else {
            PhysicalRegion { ..Default::default() }
        }
    }
}

#[doc(hidden)]
impl RendererSealed for SoftwareRenderer {
    #[cfg(feature = "systemfonts")]
    fn text_layout_cache(&self) -> Option<&sharedparley::TextLayoutCache> {
        Some(&self.text_layout_cache)
    }

    fn text_size(
        &self,
        text_item: Pin<&dyn i_slint_core::item_rendering::RenderString>,
        item_rc: &i_slint_core::item_tree::ItemRc,
        max_width: Option<LogicalLength>,
        text_wrap: TextWrap,
    ) -> LogicalSize {
        let Some(scale_factor) = self.scale_factor() else {
            return LogicalSize::default();
        };
        let font_request = text_item.font_request(item_rc);
        // Evaluate text() before borrowing font_context: the binding can
        // re-enter text_size for other elements and would panic on a second
        // borrow_mut().
        let content = text_item.text();
        #[cfg(feature = "systemfonts")]
        let Some(slint_ctx) = self.slint_context() else {
            return Default::default();
        };
        let font = {
            #[cfg(feature = "systemfonts")]
            let mut font_ctx = slint_ctx.font_context().borrow_mut();
            fonts::match_font(
                &font_request,
                scale_factor,
                #[cfg(feature = "systemfonts")]
                &mut font_ctx,
            )
        };

        #[cfg(feature = "systemfonts")]
        if uses_parley(&font) {
            return sharedparley::text_size(
                self,
                text_item,
                item_rc,
                max_width,
                text_wrap,
                Some(&self.text_layout_cache),
            )
            .unwrap_or_default();
        }

        let max_lines = text_item.line_limit();
        let string = match &content {
            PlainOrStyledText::Plain(string) => alloc::borrow::Cow::Borrowed(string.as_str()),
            PlainOrStyledText::Styled(styled_text) => {
                i_slint_core::styled_text::get_raw_text(styled_text)
            }
        };
        let (longest_line_width, height) = with_font!(&font, |font| {
            let layout = fonts::text_layout_for_font(font, &font_request, scale_factor);
            layout.text_size(
                &string,
                max_width.map(|max_width| (max_width.cast() * scale_factor).cast()),
                text_wrap,
                max_lines,
            )
        });
        (PhysicalSize::from_lengths(longest_line_width, height).cast() / scale_factor).cast()
    }

    // Mirrors the font selection in text_size(), so both widths always come from the
    // same font as what is rendered. The bitmap path measures word breaks only, which is
    // all the caller asks for: char-wrap has no minimum and never reaches here.
    fn text_content_widths(
        &self,
        text_item: Pin<&dyn i_slint_core::item_rendering::RenderString>,
        item_rc: &i_slint_core::item_tree::ItemRc,
    ) -> Option<i_slint_core::renderer::ContentWidths> {
        let scale_factor = self.scale_factor()?;
        let font_request = text_item.font_request(item_rc);
        // Evaluate text() before borrowing font_context, as in text_size().
        let content = text_item.text();
        #[cfg(feature = "systemfonts")]
        let slint_ctx = self.slint_context()?;
        let font = {
            #[cfg(feature = "systemfonts")]
            let mut font_ctx = slint_ctx.font_context().borrow_mut();
            fonts::match_font(
                &font_request,
                scale_factor,
                #[cfg(feature = "systemfonts")]
                &mut font_ctx,
            )
        };

        #[cfg(feature = "systemfonts")]
        if uses_parley(&font) {
            return sharedparley::text_content_widths(
                self,
                text_item,
                item_rc,
                Some(&self.text_layout_cache),
            );
        }

        let max_lines = text_item.line_limit();
        let string = match &content {
            PlainOrStyledText::Plain(string) => alloc::borrow::Cow::Borrowed(string.as_str()),
            PlainOrStyledText::Styled(styled_text) => {
                i_slint_core::styled_text::get_raw_text(styled_text)
            }
        };
        let (min, max) = with_font!(&font, |font| {
            fonts::text_layout_for_font(font, &font_request, scale_factor)
                .content_widths(&string, max_lines)
        });
        Some(i_slint_core::renderer::ContentWidths {
            min: (min.cast() / scale_factor).cast(),
            max: (max.cast() / scale_factor).cast(),
        })
    }

    // Bitmap fonts fall back to text_size() with the same font.
    fn text_line_height(
        &self,
        font_request: i_slint_core::graphics::FontRequest,
    ) -> Option<LogicalLength> {
        #[cfg(feature = "systemfonts")]
        {
            let scale_factor = self.scale_factor()?;
            let slint_ctx = self.slint_context()?;
            let mut font_ctx = slint_ctx.font_context().borrow_mut();
            let font = fonts::match_font(&font_request, scale_factor, &mut font_ctx);
            if uses_parley(&font) {
                return sharedparley::text_line_height(&mut font_ctx, &font_request);
            }
        }
        #[cfg(not(feature = "systemfonts"))]
        let _ = font_request;
        None
    }

    fn char_size(
        &self,
        text_item: Pin<&dyn i_slint_core::item_rendering::HasFont>,
        item_rc: &i_slint_core::item_tree::ItemRc,
        ch: char,
    ) -> LogicalSize {
        let Some(scale_factor) = self.scale_factor() else {
            return LogicalSize::default();
        };
        let font_request = text_item.font_request(item_rc);
        #[cfg(feature = "systemfonts")]
        let Some(slint_ctx) = self.slint_context() else {
            return Default::default();
        };
        let font = {
            #[cfg(feature = "systemfonts")]
            let mut font_ctx = slint_ctx.font_context().borrow_mut();
            fonts::match_font(
                &font_request,
                scale_factor,
                #[cfg(feature = "systemfonts")]
                &mut font_ctx,
            )
        };

        #[cfg(feature = "systemfonts")]
        if uses_parley(&font) {
            let mut font_ctx = slint_ctx.font_context().borrow_mut();
            return sharedparley::char_size(&mut font_ctx, text_item, item_rc, ch)
                .unwrap_or_default();
        }

        let (longest_line_width, height) = with_font!(&font, |font| {
            let mut buf = [0u8, 0u8, 0u8, 0u8];
            let layout = fonts::text_layout_for_font(font, &font_request, scale_factor);
            layout.text_size(ch.encode_utf8(&mut buf), None, TextWrap::NoWrap, None)
        });
        (PhysicalSize::from_lengths(longest_line_width, height).cast() / scale_factor).cast()
    }

    fn font_metrics(
        &self,
        font_request: i_slint_core::graphics::FontRequest,
    ) -> i_slint_core::items::FontMetrics {
        let Some(scale_factor) = self.scale_factor() else {
            return i_slint_core::items::FontMetrics::default();
        };
        #[cfg(feature = "systemfonts")]
        let Some(slint_ctx) = self.slint_context() else {
            return Default::default();
        };
        #[cfg(feature = "systemfonts")]
        let mut font_ctx = slint_ctx.font_context().borrow_mut();
        let font = fonts::match_font(
            &font_request,
            scale_factor,
            #[cfg(feature = "systemfonts")]
            &mut font_ctx,
        );

        #[cfg(feature = "systemfonts")]
        if uses_parley(&font) {
            return sharedparley::font_metrics(&mut font_ctx, font_request);
        }

        // `Font` forwards the metrics to the concrete font itself, so this needs no dispatch.
        let ascent: LogicalLength = (font.ascent().cast() / scale_factor).cast();
        let descent: LogicalLength = (font.descent().cast() / scale_factor).cast();
        let x_height: LogicalLength = (font.x_height().cast() / scale_factor).cast();
        let cap_height: LogicalLength = (font.cap_height().cast() / scale_factor).cast();

        i_slint_core::items::FontMetrics {
            ascent: ascent.get() as _,
            descent: descent.get() as _,
            x_height: x_height.get() as _,
            cap_height: cap_height.get() as _,
        }
    }

    fn text_input_byte_offset_for_position(
        &self,
        text_input: Pin<&i_slint_core::items::TextInput>,
        item_rc: &ItemRc,
        pos: LogicalPoint,
    ) -> (usize, i_slint_core::items::TextCursorAffinity) {
        let Some(scale_factor) = self.scale_factor() else {
            return Default::default();
        };
        let font_request = text_input.font_request(item_rc);
        #[cfg(feature = "systemfonts")]
        let Some(slint_ctx) = self.slint_context() else {
            return Default::default();
        };
        let font = {
            #[cfg(feature = "systemfonts")]
            let mut font_ctx = slint_ctx.font_context().borrow_mut();
            fonts::match_font(
                &font_request,
                scale_factor,
                #[cfg(feature = "systemfonts")]
                &mut font_ctx,
            )
        };

        #[cfg(feature = "systemfonts")]
        if uses_parley(&font) {
            return sharedparley::text_input_byte_offset_for_position(
                self,
                text_input,
                item_rc,
                pos,
                Some(&self.text_layout_cache),
            );
        }

        let visual_representation = text_input.visual_representation();

        let pos = (pos.cast() * scale_factor)
            .clamp(euclid::point2(0., 0.), euclid::point2(i16::MAX, i16::MAX).cast())
            .cast();

        let byte_offset = with_font!(&font, |font| {
            let layout = fonts::text_layout_for_font(font, &font_request, scale_factor);
            let paragraph = text_input_query_paragraph(
                text_input,
                &visual_representation.text,
                layout,
                scale_factor,
            );
            paragraph.byte_offset_for_position((pos.x_length(), pos.y_length()))
        });

        (
            visual_representation.map_byte_offset_from_visual_text_to_actual_text(byte_offset),
            i_slint_core::items::TextCursorAffinity::NextCharacter,
        )
    }

    // Answers for the accessibility tree what `draw_text_input` decides for drawing; both
    // ask `uses_parley`, so they cannot describe a layout that isn't the one drawn.
    #[cfg(feature = "systemfonts")]
    fn text_input_has_parley_layout(
        &self,
        text_input: Pin<&i_slint_core::items::TextInput>,
        item_rc: &ItemRc,
    ) -> bool {
        let (Some(scale_factor), Some(slint_ctx)) = (self.scale_factor(), self.slint_context())
        else {
            return false;
        };
        let font_request = text_input.font_request(item_rc);
        let font = {
            let mut font_ctx = slint_ctx.font_context().borrow_mut();
            fonts::match_font(&font_request, scale_factor, &mut font_ctx)
        };

        uses_parley(&font)
    }

    fn text_input_cursor_rect_for_byte_offset(
        &self,
        text_input: Pin<&i_slint_core::items::TextInput>,
        item_rc: &ItemRc,
        byte_offset: usize,
        affinity: i_slint_core::items::TextCursorAffinity,
    ) -> LogicalRect {
        #[cfg(not(feature = "systemfonts"))]
        let _ = affinity;
        let Some(scale_factor) = self.scale_factor() else {
            return LogicalRect::default();
        };
        let font_request = text_input.font_request(item_rc);
        #[cfg(feature = "systemfonts")]
        let Some(slint_ctx) = self.slint_context() else {
            return Default::default();
        };
        let font = {
            #[cfg(feature = "systemfonts")]
            let mut font_ctx = slint_ctx.font_context().borrow_mut();
            fonts::match_font(
                &font_request,
                scale_factor,
                #[cfg(feature = "systemfonts")]
                &mut font_ctx,
            )
        };

        #[cfg(feature = "systemfonts")]
        if uses_parley(&font) {
            return sharedparley::text_input_cursor_rect_for_byte_offset(
                self,
                text_input,
                item_rc,
                byte_offset,
                affinity,
                Some(&self.text_layout_cache),
            );
        }

        let visual_representation = text_input.visual_representation();

        let (cursor_position, band_offset, cursor_height) = with_font!(&font, |font| {
            let layout = fonts::text_layout_for_font(font, &font_request, scale_factor);
            let paragraph = text_input_query_paragraph(
                text_input,
                &visual_representation.text,
                layout,
                scale_factor,
            );

            let cursor_position = paragraph.cursor_pos_for_byte_offset(byte_offset);
            let (band_offset, cursor_height) = paragraph.layout.cursor_band();
            (cursor_position, band_offset, cursor_height)
        });

        (PhysicalRect::new(
            PhysicalPoint::from_lengths(cursor_position.0, cursor_position.1 + band_offset),
            PhysicalSize::from_lengths(
                (text_input.text_cursor_width().cast() * scale_factor).cast(),
                cursor_height,
            ),
        )
        .cast()
            / scale_factor)
            .cast()
    }

    fn free_graphics_resources(
        &self,
        component: i_slint_core::item_tree::ItemTreeRef,
        items: &mut dyn Iterator<Item = Pin<i_slint_core::items::ItemRef<'_>>>,
    ) -> Result<(), i_slint_core::platform::PlatformError> {
        #[cfg(feature = "systemfonts")]
        self.text_layout_cache.component_destroyed(component);
        #[cfg(feature = "std")]
        {
            self.layer_cache.component_destroyed(component);
            let destroyed = &*component as *const _ as *const u8 as usize;
            self.layer_cache.direct.borrow_mut().retain(|key, _| key.0 != destroyed);
        }
        self.partial_rendering_state.free_graphics_resources(component, items);
        Ok(())
    }

    fn mark_dirty_region(&self, region: DirtyRegion) {
        self.partial_rendering_state.mark_dirty_region(region);
    }

    fn register_bitmap_font(&self, font_data: &'static i_slint_core::graphics::BitmapFont) {
        fonts::register_bitmap_font(font_data);
    }

    #[cfg(feature = "systemfonts")]
    fn register_font_from_memory(
        &self,
        data: &'static [u8],
    ) -> Result<(), std::boxed::Box<dyn std::error::Error>> {
        let ctx = self.slint_context().ok_or("slint platform not initialized")?;
        ctx.font_context().borrow_mut().register_static_font(data);
        Ok(())
    }

    #[cfg(all(feature = "systemfonts", not(target_arch = "wasm32")))]
    fn register_font_from_path(
        &self,
        path: &std::path::Path,
    ) -> Result<(), std::boxed::Box<dyn std::error::Error>> {
        let ctx = self.slint_context().ok_or("slint platform not initialized")?;
        self::fonts::systemfonts::register_font_from_path(
            &mut ctx.font_context().borrow_mut().collection,
            path,
        )
    }

    fn set_window_adapter(&self, window_adapter: &Rc<dyn WindowAdapter>) {
        *self.maybe_window_adapter.borrow_mut() = Some(Rc::downgrade(window_adapter));
        #[cfg(feature = "systemfonts")]
        self.text_layout_cache.clear_all();
        #[cfg(feature = "std")]
        self.layer_cache.clear_all();
        self.partial_rendering_state.clear_cache();
    }

    fn window_adapter(&self) -> Option<Rc<dyn WindowAdapter>> {
        self.maybe_window_adapter
            .borrow()
            .as_ref()
            .and_then(|window_adapter| window_adapter.upgrade())
    }

    // Rendering to a second pixel format monomorphizes the pipeline twice; keep it out of MCU builds.
    #[cfg(feature = "std")]
    fn take_snapshot(&self) -> Result<SharedPixelBuffer<Rgba8Pixel>, PlatformError> {
        let Some(window_adapter) =
            self.maybe_window_adapter.borrow().as_ref().and_then(|w| w.upgrade())
        else {
            return Err(
                "SoftwareRenderer's screenshot called without a window adapter present".into()
            );
        };

        let window = window_adapter.window();
        let size = window.size();

        if size.width == 0 || size.height == 0 {
            // Nothing to render
            return Err("take_snapshot() called on window with invalid size".into());
        };

        // Render into a premultiplied buffer so that windows with a transparent
        // or semi-transparent background end up with the right alpha in the
        // snapshot. PremultipliedRgbaColor::background() is (0,0,0,0), so
        // anything the window doesn't paint stays fully transparent.
        let mut premul = SharedPixelBuffer::<PremultipliedRgbaColor>::new(size.width, size.height);

        let old_repaint_buffer_type = self.repaint_buffer_type();
        // ensure that caches are clear
        self.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
        self.render(premul.make_mut_slice(), size.width as usize);
        self.set_repaint_buffer_type(old_repaint_buffer_type);

        let mut target_buffer_with_alpha =
            SharedPixelBuffer::<Rgba8Pixel>::new(premul.width(), premul.height());
        for (target_pixel, source_pixel) in
            target_buffer_with_alpha.make_mut_slice().iter_mut().zip(premul.as_slice().iter())
        {
            // Un-premultiply: straight RGBA is what the public API exposes (and
            // what PNG encoders expect). Round half up to keep `255 * a / a == 255`.
            let a = source_pixel.alpha;
            if a == 0 {
                *target_pixel = Rgba8Pixel::new(0, 0, 0, 0);
            } else {
                let unp = |c: u8| ((c as u32 * 255 + (a as u32 / 2)) / a as u32).min(255) as u8;
                *target_pixel = Rgba8Pixel::new(
                    unp(source_pixel.red),
                    unp(source_pixel.green),
                    unp(source_pixel.blue),
                    a,
                );
            }
        }
        Ok(target_buffer_with_alpha)
    }

    fn supports_transformations(&self) -> bool {
        false
    }
}

/// Only the vector font path can hand off to parley, so this is only ever asked under
/// `systemfonts`; without it there is nothing to disable.
///
/// Read once: this sits on the per-item, per-frame text path, and every other `SLINT_`
/// switch is resolved once as well. Setting the variable after the first text is laid
/// out therefore has no effect.
#[cfg(feature = "systemfonts")]
fn parley_disabled() -> bool {
    static DISABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *DISABLED.get_or_init(|| std::env::var_os("SLINT_SOFTWARE_RENDERER_PARLEY_DISABLED").is_some())
}

/// Whether this font is laid out by parley rather than by the local text layout.
///
/// Every text method has to agree on this: the accessibility tree describes the layout
/// `text_input_has_parley_layout` reports, and it has to be the one that was drawn.
#[cfg(feature = "systemfonts")]
fn uses_parley(font: &fonts::Font) -> bool {
    matches!(font, fonts::Font::VectorFont(_)) && !parley_disabled()
}

/// The paragraph layout the cursor and hit-testing queries measure against.
///
/// `string` is the text input's visual representation, which the caller owns because the
/// returned layout borrows from it.
///
/// Note that this does not pass on the text input's `single_line`, matching what these
/// queries did before they shared this helper. `draw_text_input` does pass it on.
fn text_input_query_paragraph<'a, Font>(
    text_input: Pin<&i_slint_core::items::TextInput>,
    string: &'a str,
    layout: i_slint_core::textlayout::TextLayout<'a, Font>,
    scale_factor: ScaleFactor,
) -> TextParagraphLayout<'a, Font>
where
    Font: AbstractFont + i_slint_core::textlayout::TextShaper<Length = PhysicalLength>,
{
    TextParagraphLayout {
        string,
        layout,
        max_width: (text_input.width().cast() * scale_factor).cast(),
        max_height: (text_input.height().cast() * scale_factor).cast(),
        horizontal_alignment: text_input.horizontal_alignment(),
        vertical_alignment: text_input.vertical_alignment(),
        wrap: text_input.wrap(),
        overflow: TextOverflow::Clip,
        single_line: false,
        max_lines: None,
    }
}

fn render_window_frame_by_line(
    window: &WindowInner,
    background: Brush,
    size: PhysicalSize,
    renderer: &SoftwareRenderer,
    mut line_buffer: impl LineBufferProvider,
) -> PhysicalRegion {
    let mut scene = prepare_scene(window, size, renderer);

    let to_draw_tr = scene.dirty_region.bounding_rect();

    let mut background_color = TargetPixel::background();
    // FIXME gradient
    TargetPixel::blend(&mut background_color, background.color().into());

    while scene.current_line < to_draw_tr.origin.y_length() + to_draw_tr.size.height_length() {
        for r in &scene.current_line_ranges {
            line_buffer.process_line(
                scene.current_line.get() as usize,
                r.start as usize..r.end as usize,
                |line_buffer| {
                    let offset = r.start;

                    let items = &scene.items[0..scene.current_items_index];
                    // A span that opaquely covers the whole range hides
                    // everything behind it, background included. Draw from the
                    // frontmost such span and drop the rest.
                    let first_cover = items.iter().position(|span| {
                        span.pos.x <= r.start
                            && span.pos.x + span.size.width >= r.end
                            && scene.is_guaranteed_opaque(&span.command)
                    });
                    let items = match first_cover {
                        Some(i) => &items[..=i],
                        None => {
                            line_buffer.fill(background_color);
                            items
                        }
                    };
                    for span in items.iter().rev() {
                        debug_assert!(scene.current_line >= span.pos.y_length());
                        debug_assert!(
                            scene.current_line < span.pos.y_length() + span.size.height_length(),
                        );
                        if span.pos.x >= r.end {
                            continue;
                        }
                        let begin = r.start.max(span.pos.x);
                        let end = r.end.min(span.pos.x + span.size.width);
                        if begin >= end {
                            continue;
                        }

                        let extra_left_clip = begin - span.pos.x;
                        let extra_right_clip = span.pos.x + span.size.width - end;
                        let range_buffer =
                            &mut line_buffer[(begin - offset) as usize..(end - offset) as usize];

                        match span.command {
                            SceneCommand::Rectangle { color } => {
                                TargetPixel::blend_slice(range_buffer, color);
                            }
                            SceneCommand::Texture { texture_index } => {
                                let texture = &scene.vectors.textures[texture_index as usize];
                                draw_functions::draw_texture_line(
                                    &PhysicalRect { origin: span.pos, size: span.size },
                                    scene.current_line,
                                    texture,
                                    range_buffer,
                                    extra_left_clip,
                                    extra_right_clip,
                                );
                            }
                            SceneCommand::SharedBuffer { shared_buffer_index } => {
                                let texture = scene.vectors.shared_buffers
                                    [shared_buffer_index as usize]
                                    .as_texture();
                                draw_functions::draw_texture_line(
                                    &PhysicalRect { origin: span.pos, size: span.size },
                                    scene.current_line,
                                    &texture,
                                    range_buffer,
                                    extra_left_clip,
                                    extra_right_clip,
                                );
                            }
                            SceneCommand::RoundedRectangle { rectangle_index } => {
                                let rr =
                                    &scene.vectors.rounded_rectangles[rectangle_index as usize];
                                draw_functions::draw_rounded_rectangle_line(
                                    &PhysicalRect { origin: span.pos, size: span.size },
                                    scene.current_line,
                                    rr,
                                    range_buffer,
                                    extra_left_clip,
                                    extra_right_clip,
                                );
                            }
                            SceneCommand::LinearGradient { linear_gradient_index } => {
                                let g =
                                    &scene.vectors.linear_gradients[linear_gradient_index as usize];

                                draw_functions::draw_linear_gradient(
                                    &PhysicalRect { origin: span.pos, size: span.size },
                                    scene.current_line,
                                    g,
                                    range_buffer,
                                    extra_left_clip,
                                );
                            }
                            SceneCommand::RadialGradient { radial_gradient_index } => {
                                let g =
                                    &scene.vectors.radial_gradients[radial_gradient_index as usize];
                                draw_functions::draw_radial_gradient(
                                    &PhysicalRect { origin: span.pos, size: span.size },
                                    scene.current_line,
                                    g,
                                    range_buffer,
                                    extra_left_clip,
                                    extra_right_clip,
                                );
                            }
                            SceneCommand::ConicGradient { conic_gradient_index } => {
                                let g =
                                    &scene.vectors.conic_gradients[conic_gradient_index as usize];
                                draw_functions::draw_conic_gradient(
                                    &PhysicalRect { origin: span.pos, size: span.size },
                                    scene.current_line,
                                    g,
                                    range_buffer,
                                    extra_left_clip,
                                    extra_right_clip,
                                );
                            }
                        }
                    }
                },
            );
        }

        if scene.current_line < to_draw_tr.origin.y_length() + to_draw_tr.size.height_length() {
            scene.next_line();
        }
    }
    scene.dirty_region
}

fn prepare_scene(
    window: &WindowInner,
    size: PhysicalSize,
    software_renderer: &SoftwareRenderer,
) -> Scene {
    let factor = ScaleFactor::new(window.scale_factor());
    let prepare_scene = SceneBuilder::new(
        size,
        factor,
        window,
        PrepareScene { scale_factor: factor, ..Default::default() },
        software_renderer.rotation.get(),
        #[cfg(feature = "systemfonts")]
        &software_renderer.text_layout_cache,
        // Line-by-line rendering draws layers' children directly.
        #[cfg(feature = "std")]
        None,
    );
    let mut renderer =
        software_renderer.partial_rendering_state.create_partial_renderer(prepare_scene);
    let window_adapter = renderer.window_adapter.clone();

    let mut dirty_region = PhysicalRegion::default();
    window.draw_contents(|components, post_render| {
        let logical_size = (size.cast() / factor).cast();

        match software_renderer.repaint_buffer_type.get() {
            RepaintBufferType::NewBuffer => {
                // NewBuffer always redraws the full screen, so skip dirty region
                // tracking to avoid unbounded growth of the partial rendering cache.
                renderer.dirty_region = LogicalRect::from_size(logical_size).into();
                software_renderer.partial_rendering_state.clear_cache();
            }
            RepaintBufferType::ReusedBuffer => {
                software_renderer.partial_rendering_state.apply_dirty_region(
                    &mut renderer,
                    components,
                    logical_size,
                    None,
                );
            }
            RepaintBufferType::SwappedBuffers => {
                let dirty_region_for_this_frame =
                    software_renderer.partial_rendering_state.apply_dirty_region(
                        &mut renderer,
                        components,
                        logical_size,
                        Some(software_renderer.prev_frame_dirty.take()),
                    );
                software_renderer.prev_frame_dirty.set(dirty_region_for_this_frame);
            }
        }

        let rotation =
            RotationInfo { orientation: software_renderer.rotation.get(), screen_size: size };
        let screen_rect = PhysicalRect::from_size(size);
        let mut i = renderer.dirty_region.iter().filter_map(|r| {
            (r.cast() * factor)
                .to_rect()
                .round_out()
                .cast()
                .intersection(&screen_rect)?
                .transformed(rotation)
                .into()
        });
        dirty_region = PhysicalRegion {
            rectangles: core::array::from_fn(|_| i.next().unwrap_or_default().to_box2d()),
            count: renderer.dirty_region.iter().count(),
        };
        drop(i);

        let partial = software_renderer.repaint_buffer_type.get() != RepaintBufferType::NewBuffer;
        for (component, origin) in components {
            if let Some(component) = ItemTreeWeak::upgrade(component) {
                i_slint_core::item_rendering::render_component_items(
                    &component,
                    if partial { &mut renderer } else { &mut renderer.actual_renderer },
                    *origin,
                    &window_adapter,
                );
            }
        }

        if partial {
            post_render(&mut renderer);
        } else {
            post_render(&mut renderer.actual_renderer);
        }
    });

    software_renderer.measure_frame_rendered(&mut renderer);

    let prepare_scene = renderer.into_inner();

    /* // visualize dirty regions
    let mut prepare_scene = prepare_scene;
    for rect in dirty_region.iter() {
        prepare_scene.processor.process_rounded_rectangle(
            euclid::rect(rect.0.x as _, rect.0.y as _, rect.1.width as _, rect.1.height as _),
            RoundedRectangle {
                radius: BorderRadius::default(),
                width: Length::new(1),
                border_color: Color::from_argb_u8(128, 255, 0, 0).into(),
                inner_color: PremultipliedRgbaColor::default(),
                left_clip: Length::default(),
                right_clip: Length::default(),
                top_clip: Length::default(),
                bottom_clip: Length::default(),
            },
        )
    } // */

    Scene::new(prepare_scene.processor.items, prepare_scene.processor.vectors, dirty_region)
}

trait ProcessScene {
    /// Draw a cached layer's recorded draw calls, moved by `delta`, inside `clip`.
    #[cfg(feature = "std")]
    fn process_layer_ops(
        &mut self,
        ops: &[parallel::Op],
        delta: euclid::Vector2D<i16, PhysicalPx>,
        clip: PhysicalRect,
    );
    fn process_scene_texture(&mut self, geometry: PhysicalRect, texture: SceneTexture<'static>);
    fn process_target_texture(
        &mut self,
        texture: &target_pixel_buffer::DrawTextureArgs,
        clip: PhysicalRect,
    );
    fn process_rectangle(&mut self, _: &target_pixel_buffer::DrawRectangleArgs, clip: PhysicalRect);

    fn process_simple_rectangle(&mut self, geometry: PhysicalRect, color: PremultipliedRgbaColor);
    fn process_rounded_rectangle(&mut self, geometry: PhysicalRect, data: RoundedRectangle);
    fn process_linear_gradient(&mut self, geometry: PhysicalRect, gradient: LinearGradientCommand);
    fn process_radial_gradient(&mut self, geometry: PhysicalRect, gradient: RadialGradientCommand);
    fn process_conic_gradient(&mut self, geometry: PhysicalRect, gradient: ConicGradientCommand);
    #[cfg(feature = "path")]
    fn process_filled_path(
        &mut self,
        path_geometry: PhysicalRect,
        clip_geometry: PhysicalRect,
        commands: alloc::vec::Vec<path::Command>,
        color: PremultipliedRgbaColor,
    );
    #[cfg(feature = "path")]
    fn process_stroked_path(
        &mut self,
        path_geometry: PhysicalRect,
        clip_geometry: PhysicalRect,
        commands: alloc::vec::Vec<path::Command>,
        color: PremultipliedRgbaColor,
        stroke_width: f32,
        stroke_line_cap: i_slint_core::items::LineCap,
        stroke_line_join: i_slint_core::items::LineJoin,
        stroke_miter_limit: f32,
    );
}

fn process_rectangle_impl(
    processor: &mut dyn ProcessScene,
    args: &target_pixel_buffer::DrawRectangleArgs,
    clip: &PhysicalRect,
    scale_factor: ScaleFactor,
) {
    let geom = args.geometry();
    let Some(clipped) = geom.intersection(&clip.cast()) else { return };
    let geom_w = geom.width();
    let geom_h = geom.height();
    let to_clipped_center = |cx: f32, cy: f32| {
        (geom.min_x() + cx - clipped.min_x(), geom.min_y() + cy - clipped.min_y())
    };

    let color = if let Brush::LinearGradient(g) = &args.background {
        let angle = g.angle() + args.rotation.angle();
        let tan = angle.to_radians().tan().abs();
        let start = if !tan.is_finite() {
            255.
        } else {
            let h = tan * geom.width();
            255. * h / (h + geom.height())
        } as u8;
        let mut angle = angle as i32 % 360;
        if angle < 0 {
            angle += 360;
        }
        let mut stops = g
            .stops()
            .copied()
            .map(|mut s| {
                s.color = alpha_color(s.color, args.alpha);
                s
            })
            .peekable();
        let mut idx = 0;
        let stop_count = g.stops().count();
        while let (Some(mut s1), Some(mut s2)) = (stops.next(), stops.peek().copied()) {
            let mut flags = 0;
            if (angle % 180) > 90 {
                flags |= 0b1;
            }
            if angle <= 90 || angle > 270 {
                core::mem::swap(&mut s1, &mut s2);
                s1.position = 1. - s1.position;
                s2.position = 1. - s2.position;
                if idx == 0 {
                    flags |= 0b100;
                }
                if idx == stop_count - 2 {
                    flags |= 0b010;
                }
            } else {
                if idx == 0 {
                    flags |= 0b010;
                }
                if idx == stop_count - 2 {
                    flags |= 0b100;
                }
            }

            idx += 1;

            let (adjust_left, adjust_right) = if (angle % 180) > 90 {
                (
                    (geom.width() * s1.position).floor() as i16,
                    (geom.width() * (1. - s2.position)).ceil() as i16,
                )
            } else {
                (
                    (geom.width() * (1. - s2.position)).ceil() as i16,
                    (geom.width() * s1.position).floor() as i16,
                )
            };

            let gr = LinearGradientCommand {
                color1: s1.color.into(),
                color2: s2.color.into(),
                start,
                flags,
                top_clip: Length::new(
                    (clipped.min_y() - geom.min_y() - (geom.height() * s1.position).floor()) as i16,
                ),
                bottom_clip: Length::new(
                    (geom.max_y() - clipped.max_y() - (geom.height() * (1. - s2.position)).ceil())
                        as i16,
                ),
                left_clip: Length::new((clipped.min_x() - geom.min_x()) as i16 - adjust_left),
                right_clip: Length::new((geom.max_x() - clipped.max_x()) as i16 - adjust_right),
            };

            let act_rect = clipped.round().cast();
            let size_y = act_rect.height_length() + gr.top_clip + gr.bottom_clip;
            let size_x = act_rect.width_length() + gr.left_clip + gr.right_clip;
            if size_x.get() == 0 || size_y.get() == 0 {
                // the position are too close to each other
                // FIXME: For the first or the last, we should draw a plain color to the end
                continue;
            }

            processor.process_linear_gradient(act_rect, gr);
        }
        Color::default()
    } else if let Brush::RadialGradient(g) = &args.background {
        let (cx, cy) = g.center_or_default_scaled(geom_w, geom_h, scale_factor.get());
        let (center_x, center_y) = to_clipped_center(cx, cy);
        let radius = g.radius_or_default_scaled(geom_w, geom_h, scale_factor.get());

        let radial_grad = RadialGradientCommand {
            stops: g
                .stops()
                .map(|s| {
                    let mut stop = *s;
                    stop.color = alpha_color(stop.color, args.alpha);
                    stop
                })
                .collect(),
            center_x,
            center_y,
            radius,
        };

        processor.process_radial_gradient(clipped.cast(), radial_grad);
        Color::default()
    } else if let Brush::ConicGradient(g) = &args.background {
        let (cx, cy) = g.center_or_default_scaled(geom_w, geom_h, scale_factor.get());
        let (center_x, center_y) = to_clipped_center(cx, cy);
        let conic_grad = ConicGradientCommand {
            stops: g
                .stops()
                .map(|s| {
                    let mut stop = *s;
                    stop.color = alpha_color(stop.color, args.alpha);
                    stop
                })
                .collect(),
            center_x,
            center_y,
        };

        processor.process_conic_gradient(clipped.cast(), conic_grad);
        Color::default()
    } else {
        alpha_color(args.background.color(), args.alpha)
    };

    let mut border_color =
        PremultipliedRgbaColor::from(alpha_color(args.border.color(), args.alpha));
    let color = PremultipliedRgbaColor::from(color);
    let mut border = PhysicalLength::new(args.border_width as _);
    if border_color.alpha == 0 {
        border = PhysicalLength::new(0);
    } else if border_color.alpha < 255 {
        // Find a color for the border which is an equivalent to blend the background and then the border.
        // In the end, the resulting of blending the background and the color is
        // (A + B) + C, where A is the buffer color, B is the background, and C is the border.
        // which expands to (A*(1-Bα) + B*Bα)*(1-Cα) + C*Cα = A*(1-(Bα+Cα-Bα*Cα)) + B*Bα*(1-Cα) + C*Cα
        // so let the new alpha be: Nα = Bα+Cα-Bα*Cα, then this is A*(1-Nα) + N*Nα
        // with N = (B*Bα*(1-Cα) + C*Cα)/Nα
        // N being the equivalent color of the border that mixes the background and the border
        // In pre-multiplied space, the formula simplifies further N' = B'*(1-Cα) + C'
        let b = border_color;
        let b_alpha_16 = b.alpha as u16;
        border_color = PremultipliedRgbaColor {
            red: ((color.red as u16 * (255 - b_alpha_16)) / 255) as u8 + b.red,
            green: ((color.green as u16 * (255 - b_alpha_16)) / 255) as u8 + b.green,
            blue: ((color.blue as u16 * (255 - b_alpha_16)) / 255) as u8 + b.blue,
            alpha: (color.alpha as u16 + b_alpha_16 - (color.alpha as u16 * b_alpha_16) / 255)
                as u8,
        }
    }

    let radius = PhysicalBorderRadius {
        top_left: args.top_left_radius as _,
        top_right: args.top_right_radius as _,
        bottom_right: args.bottom_right_radius as _,
        bottom_left: args.bottom_left_radius as _,
        _unit: Default::default(),
    };

    if !radius.is_zero() {
        // Add a small value to make sure that the clip is always positive despite floating point shenanigans
        const E: f32 = 0.00001;

        processor.process_rounded_rectangle(
            clipped.round().cast(),
            RoundedRectangle {
                radius,
                width: border,
                border_color,
                inner_color: color,
                top_clip: PhysicalLength::new((clipped.min_y() - geom.min_y() + E) as _),
                bottom_clip: PhysicalLength::new((geom.max_y() - clipped.max_y() + E) as _),
                left_clip: PhysicalLength::new((clipped.min_x() - geom.min_x() + E) as _),
                right_clip: PhysicalLength::new((geom.max_x() - clipped.max_x() + E) as _),
            },
        );
        return;
    }

    if color.alpha > 0
        && let Some(r) =
            geom.round().cast().inflate(-border.get(), -border.get()).intersection(clip)
    {
        processor.process_simple_rectangle(r, color);
    }

    if border_color.alpha > 0 {
        let mut add_border = |r: PhysicalRect| {
            if let Some(r) = r.intersection(clip) {
                processor.process_simple_rectangle(r, border_color);
            }
        };
        let b = border.get();
        let g = geom.round().cast();
        add_border(euclid::rect(g.min_x(), g.min_y(), g.width(), b));
        add_border(euclid::rect(g.min_x(), g.min_y() + g.height() - b, g.width(), b));
        add_border(euclid::rect(g.min_x(), g.min_y() + b, b, g.height() - b - b));
        add_border(euclid::rect(g.min_x() + g.width() - b, g.min_y() + b, b, g.height() - b - b));
    }
}

/// Starts a frame's band threads once its dirty region is known (`render_buffer_inner`).
#[cfg(feature = "std")]
type StartBands<'a> =
    dyn FnMut(&PhysicalRegion, ScaleFactor) -> Option<std::sync::Arc<parallel::OpStream>> + 'a;
#[cfg(not(feature = "std"))]
type StartBands<'a> = dyn FnMut(&PhysicalRegion, ScaleFactor) -> Option<()> + 'a;

/// A buffer with lines of the right width and count and no pixels: what the walk renders
/// into while it records (`render`), so that the band threads own the real buffer.
#[cfg(feature = "std")]
struct LinesOnly<T> {
    line: Vec<T>,
    lines: usize,
}

#[cfg(feature = "std")]
impl<T: TargetPixel> LinesOnly<T> {
    fn like(_buffer: &[T], width: usize, lines: usize) -> Self {
        Self { line: alloc::vec![T::background(); width], lines }
    }
}

#[cfg(feature = "std")]
impl<T: TargetPixel> target_pixel_buffer::TargetPixelBuffer for LinesOnly<T> {
    type TargetPixel = T;
    fn line_slice(&mut self, _line_number: usize) -> &mut [T] {
        &mut self.line
    }
    fn num_lines(&self) -> usize {
        self.lines
    }
}

struct RenderToBuffer<'a, TargetPixelBuffer> {
    buffer: &'a mut TargetPixelBuffer,
    dirty_range_cache: Vec<core::ops::Range<i16>>,
    dirty_region: PhysicalRegion,
    scale_factor: ScaleFactor,
    /// When set, draw calls are recorded here instead of drawn (`parallel`).
    #[cfg(feature = "std")]
    recording: Option<parallel::Recorder>,
    /// While a cached layer's draw calls are replayed: the dirty region outside its clip.
    #[cfg(feature = "std")]
    unclipped: Option<PhysicalRegion>,
}

impl<B: target_pixel_buffer::TargetPixelBuffer> RenderToBuffer<'_, B> {
    /// The dirty region inside `clip`, as rectangles that do not overlap: a path drawn once
    /// per rectangle blends each pixel once, also where the region's rectangles overlap.
    fn dirty_clips(&self, clip: &PhysicalRect) -> Vec<PhysicalRect> {
        let rects: Vec<PhysicalRect> =
            self.dirty_region.iter_box().filter_map(|b| clip.intersection(&b.to_rect())).collect();
        if rects.len() < 2 {
            return rects;
        }
        let mut edges: Vec<i16> = rects.iter().flat_map(|r| [r.min_y(), r.max_y()]).collect();
        edges.sort_unstable();
        edges.dedup();
        let mut out = Vec::new();
        for band in edges.windows(2) {
            let (top, bottom) = (band[0], band[1]);
            let mut spans: Vec<(i16, i16)> = rects
                .iter()
                .filter(|r| r.min_y() <= top && r.max_y() >= bottom)
                .map(|r| (r.min_x(), r.max_x()))
                .collect();
            spans.sort_unstable();
            let mut merged: Vec<(i16, i16)> = Vec::new();
            for (start, end) in spans {
                match merged.last_mut() {
                    Some(last) if start <= last.1 => last.1 = last.1.max(end),
                    _ => merged.push((start, end)),
                }
            }
            for (start, end) in merged {
                out.push(PhysicalRect::new(
                    PhysicalPoint::new(start, top),
                    PhysicalSize::new(end - start, bottom - top),
                ));
            }
        }
        out
    }

    fn foreach_ranges(
        &mut self,
        geometry: &PhysicalRect,
        mut f: impl FnMut(i16, &mut [B::TargetPixel], i16, i16),
    ) {
        let mut line = geometry.min_y();
        while let Some(mut next) =
            region_line_ranges(&self.dirty_region, line, &mut self.dirty_range_cache)
        {
            next = next.min(geometry.max_y());
            for r in &self.dirty_range_cache {
                if geometry.origin.x >= r.end {
                    continue;
                }
                let begin = r.start.max(geometry.origin.x);
                let end = r.end.min(geometry.origin.x + geometry.size.width);
                if begin >= end {
                    continue;
                }
                let extra_left_clip = begin - geometry.origin.x;
                let extra_right_clip = geometry.origin.x + geometry.size.width - end;

                let region = PhysicalRect {
                    origin: PhysicalPoint::new(begin, line),
                    size: PhysicalSize::new(end - begin, next - line),
                };

                for l in region.y_range() {
                    f(
                        l,
                        &mut self.buffer.line_slice(l as usize)
                            [region.min_x() as usize..region.max_x() as usize],
                        extra_left_clip,
                        extra_right_clip,
                    );
                }
            }
            if next == geometry.max_y() {
                break;
            }
            line = next;
        }
    }

    fn process_texture_impl(&mut self, geometry: PhysicalRect, texture: SceneTexture<'_>) {
        self.foreach_ranges(&geometry, |line, buffer, extra_left_clip, extra_right_clip| {
            draw_functions::draw_texture_line(
                &geometry,
                PhysicalLength::new(line),
                &texture,
                buffer,
                extra_left_clip,
                extra_right_clip,
            );
        });
    }
}

impl<B: target_pixel_buffer::TargetPixelBuffer> ProcessScene for RenderToBuffer<'_, B> {
    #[cfg(feature = "std")]
    fn process_layer_ops(
        &mut self,
        ops: &[parallel::Op],
        delta: euclid::Vector2D<i16, PhysicalPx>,
        clip: PhysicalRect,
    ) {
        // Only the calls that reach the clip and, when drawing now, the dirty region.
        let reach = match &self.recording {
            Some(_) => Some(clip),
            None => clip.intersection(&self.dirty_region.bounding_rect()),
        };
        let Some(reach) = reach else { return };
        let moved = ops.iter().filter_map(|op| {
            let op = op.translated(delta);
            op.bounds().intersects(&reach).then_some(op)
        });
        if let Some(recorder) = &mut self.recording {
            recorder.push(parallel::Op::SetClip(Some(clip)));
            for op in moved {
                recorder.push(op);
            }
            recorder.push(parallel::Op::SetClip(None));
        } else {
            self.replay(&parallel::Op::SetClip(Some(clip)));
            for op in moved {
                self.replay(&op);
            }
            self.replay(&parallel::Op::SetClip(None));
        }
    }

    fn process_scene_texture(&mut self, geometry: PhysicalRect, texture: SceneTexture<'static>) {
        #[cfg(feature = "std")]
        if let Some(ops) = &mut self.recording {
            ops.push(parallel::Op::Texture(geometry, texture));
            return;
        }
        self.process_texture_impl(geometry, texture);
    }

    fn process_target_texture(
        &mut self,
        texture: &target_pixel_buffer::DrawTextureArgs,
        clip: PhysicalRect,
    ) {
        #[cfg(feature = "std")]
        if let Some(ops) = &mut self.recording {
            // As `PrepareScene` keeps it: a static slice, or the shared buffer.
            let Some((extra, geometry)) = SceneTextureExtra::from_target_texture(texture, &clip)
            else {
                return;
            };
            match &texture.data {
                target_pixel_buffer::TextureDataContainer::Static(data) => {
                    let pixel_stride = (data.byte_stride / data.pixel_format.bpp()) as u16;
                    ops.push(parallel::Op::Texture(
                        geometry,
                        SceneTexture { data: data.data, format: data.pixel_format, pixel_stride, extra },
                    ));
                }
                target_pixel_buffer::TextureDataContainer::Shared { buffer, source_rect } => {
                    ops.push(parallel::Op::SharedTexture(
                        geometry,
                        SharedBufferCommand { buffer: buffer.clone(), source_rect: *source_rect, extra },
                    ));
                }
            }
            return;
        }
        if self.buffer.draw_texture(texture, &self.dirty_region.intersection(&clip)) {
            return;
        }

        let Some((texture, geometry)) = SceneTexture::from_target_texture(texture, &clip) else {
            return;
        };

        self.process_texture_impl(geometry, texture);
    }

    fn process_rectangle(
        &mut self,
        args: &target_pixel_buffer::DrawRectangleArgs,
        clip: PhysicalRect,
    ) {
        // Recording: the rectangle's primitives come back here and are recorded.
        #[cfg(feature = "std")]
        let recording = self.recording.is_some();
        #[cfg(not(feature = "std"))]
        let recording = false;
        if !recording && self.buffer.draw_rectangle(args, &self.dirty_region.intersection(&clip)) {
            return;
        }

        let scale_factor = self.scale_factor;
        process_rectangle_impl(self, args, &clip, scale_factor);
    }

    fn process_rounded_rectangle(&mut self, geometry: PhysicalRect, rr: RoundedRectangle) {
        #[cfg(feature = "std")]
        if let Some(ops) = &mut self.recording {
            ops.push(parallel::Op::RoundedRectangle(geometry, rr));
            return;
        }
        self.foreach_ranges(&geometry, |line, buffer, extra_left_clip, extra_right_clip| {
            draw_functions::draw_rounded_rectangle_line(
                &geometry,
                PhysicalLength::new(line),
                &rr,
                buffer,
                extra_left_clip,
                extra_right_clip,
            );
        });
    }

    fn process_simple_rectangle(&mut self, geometry: PhysicalRect, color: PremultipliedRgbaColor) {
        #[cfg(feature = "std")]
        if let Some(ops) = &mut self.recording {
            ops.push(parallel::Op::Rectangle(geometry, color));
            return;
        }
        self.foreach_ranges(&geometry, |_line, buffer, _extra_left_clip, _extra_right_clip| {
            <B::TargetPixel>::blend_slice(buffer, color)
        });
    }

    fn process_linear_gradient(&mut self, geometry: PhysicalRect, g: LinearGradientCommand) {
        #[cfg(feature = "std")]
        if let Some(ops) = &mut self.recording {
            ops.push(parallel::Op::LinearGradient(geometry, g));
            return;
        }
        self.foreach_ranges(&geometry, |line, buffer, extra_left_clip, _extra_right_clip| {
            draw_functions::draw_linear_gradient(
                &geometry,
                PhysicalLength::new(line),
                &g,
                buffer,
                extra_left_clip,
            );
        });
    }
    fn process_radial_gradient(&mut self, geometry: PhysicalRect, g: RadialGradientCommand) {
        #[cfg(feature = "std")]
        if let Some(ops) = &mut self.recording {
            ops.push(parallel::Op::RadialGradient(geometry, g));
            return;
        }
        self.foreach_ranges(&geometry, |line, buffer, extra_left_clip, extra_right_clip| {
            draw_functions::draw_radial_gradient(
                &geometry,
                PhysicalLength::new(line),
                &g,
                buffer,
                extra_left_clip,
                extra_right_clip,
            );
        });
    }
    fn process_conic_gradient(&mut self, geometry: PhysicalRect, g: ConicGradientCommand) {
        #[cfg(feature = "std")]
        if let Some(ops) = &mut self.recording {
            ops.push(parallel::Op::ConicGradient(geometry, g));
            return;
        }
        self.foreach_ranges(&geometry, |line, buffer, extra_left_clip, extra_right_clip| {
            draw_functions::draw_conic_gradient(
                &geometry,
                PhysicalLength::new(line),
                &g,
                buffer,
                extra_left_clip,
                extra_right_clip,
            );
        });
    }

    #[cfg(feature = "path")]
    fn process_filled_path(
        &mut self,
        path_geometry: PhysicalRect,
        clip_geometry: PhysicalRect,
        commands: alloc::vec::Vec<path::Command>,
        color: PremultipliedRgbaColor,
    ) {
        #[cfg(feature = "std")]
        if let Some(ops) = &mut self.recording {
            ops.push(parallel::Op::FilledPath { path: path_geometry, clip: clip_geometry, commands, color });
            return;
        }
        // Only inside the dirty region: drawing the path over unchanged pixels
        // would blend its anti-aliased edges twice.
        for clip in self.dirty_clips(&clip_geometry) {
            path::render_filled_path(&commands, &path_geometry, &clip, color, self.buffer);
        }
    }

    #[cfg(feature = "path")]
    fn process_stroked_path(
        &mut self,
        path_geometry: PhysicalRect,
        clip_geometry: PhysicalRect,
        commands: alloc::vec::Vec<path::Command>,
        color: PremultipliedRgbaColor,
        stroke_width: f32,
        stroke_line_cap: i_slint_core::items::LineCap,
        stroke_line_join: i_slint_core::items::LineJoin,
        stroke_miter_limit: f32,
    ) {
        #[cfg(feature = "std")]
        if let Some(ops) = &mut self.recording {
            ops.push(parallel::Op::StrokedPath {
                path: path_geometry,
                clip: clip_geometry,
                commands,
                color,
                width: stroke_width,
                cap: stroke_line_cap,
                join: stroke_line_join,
                miter_limit: stroke_miter_limit,
            });
            return;
        }
        for clip in self.dirty_clips(&clip_geometry) {
            path::render_stroked_path(
                &commands,
                &path_geometry,
                &clip,
                color,
                stroke_width,
                stroke_line_cap,
                stroke_line_join,
                stroke_miter_limit,
                self.buffer,
            );
        }
    }
}

#[derive(Default)]
struct PrepareScene {
    items: Vec<SceneItem>,
    vectors: SceneVectors,
    scale_factor: ScaleFactor,
}

impl ProcessScene for PrepareScene {
    #[cfg(feature = "std")]
    fn process_layer_ops(
        &mut self,
        _ops: &[parallel::Op],
        _delta: euclid::Vector2D<i16, PhysicalPx>,
        _clip: PhysicalRect,
    ) {
        // Line-by-line rendering has no layer cache.
    }

    fn process_scene_texture(&mut self, geometry: PhysicalRect, texture: SceneTexture<'static>) {
        let texture_index = self.vectors.textures.len() as u16;
        self.vectors.textures.push(texture);
        self.items.push(SceneItem {
            pos: geometry.origin,
            size: geometry.size,
            z: self.items.len() as u16,
            command: SceneCommand::Texture { texture_index },
        });
    }

    fn process_target_texture(
        &mut self,
        texture: &target_pixel_buffer::DrawTextureArgs,
        clip: PhysicalRect,
    ) {
        let Some((extra, geometry)) = SceneTextureExtra::from_target_texture(texture, &clip) else {
            return;
        };
        match &texture.data {
            target_pixel_buffer::TextureDataContainer::Static(texture_data) => {
                let texture_index = self.vectors.textures.len() as u16;
                let pixel_stride =
                    (texture_data.byte_stride / texture_data.pixel_format.bpp()) as u16;
                self.vectors.textures.push(SceneTexture {
                    data: texture_data.data,
                    format: texture_data.pixel_format,
                    pixel_stride,
                    extra,
                });
                self.items.push(SceneItem {
                    pos: geometry.origin,
                    size: geometry.size,
                    z: self.items.len() as u16,
                    command: SceneCommand::Texture { texture_index },
                });
            }
            target_pixel_buffer::TextureDataContainer::Shared { buffer, source_rect } => {
                let shared_buffer_index = self.vectors.shared_buffers.len() as u16;
                self.vectors.shared_buffers.push(SharedBufferCommand {
                    buffer: buffer.clone(),
                    source_rect: *source_rect,
                    extra,
                });
                self.items.push(SceneItem {
                    pos: geometry.origin,
                    size: geometry.size,
                    z: self.items.len() as u16,
                    command: SceneCommand::SharedBuffer { shared_buffer_index },
                });
            }
        }
    }

    fn process_rectangle(
        &mut self,
        args: &target_pixel_buffer::DrawRectangleArgs,
        clip: PhysicalRect,
    ) {
        let scale_factor = self.scale_factor;
        process_rectangle_impl(self, args, &clip, scale_factor);
    }

    fn process_simple_rectangle(&mut self, geometry: PhysicalRect, color: PremultipliedRgbaColor) {
        let size = geometry.size;
        if !size.is_empty() {
            let z = self.items.len() as u16;
            let pos = geometry.origin;
            self.items.push(SceneItem { pos, size, z, command: SceneCommand::Rectangle { color } });
        }
    }

    fn process_rounded_rectangle(&mut self, geometry: PhysicalRect, data: RoundedRectangle) {
        let size = geometry.size;
        if !size.is_empty() {
            let rectangle_index = self.vectors.rounded_rectangles.len() as u16;
            self.vectors.rounded_rectangles.push(data);
            self.items.push(SceneItem {
                pos: geometry.origin,
                size,
                z: self.items.len() as u16,
                command: SceneCommand::RoundedRectangle { rectangle_index },
            });
        }
    }

    fn process_linear_gradient(&mut self, geometry: PhysicalRect, gradient: LinearGradientCommand) {
        let size = geometry.size;
        if !size.is_empty() {
            let gradient_index = self.vectors.linear_gradients.len() as u16;
            self.vectors.linear_gradients.push(gradient);
            self.items.push(SceneItem {
                pos: geometry.origin,
                size,
                z: self.items.len() as u16,
                command: SceneCommand::LinearGradient { linear_gradient_index: gradient_index },
            });
        }
    }
    fn process_radial_gradient(&mut self, geometry: PhysicalRect, gradient: RadialGradientCommand) {
        let size = geometry.size;
        if !size.is_empty() {
            let radial_gradient_index = self.vectors.radial_gradients.len() as u16;
            self.vectors.radial_gradients.push(gradient);
            self.items.push(SceneItem {
                pos: geometry.origin,
                size,
                z: self.items.len() as u16,
                command: SceneCommand::RadialGradient { radial_gradient_index },
            });
        }
    }
    fn process_conic_gradient(&mut self, geometry: PhysicalRect, gradient: ConicGradientCommand) {
        let size = geometry.size;
        if !size.is_empty() {
            let conic_gradient_index = self.vectors.conic_gradients.len() as u16;
            self.vectors.conic_gradients.push(gradient);
            self.items.push(SceneItem {
                pos: geometry.origin,
                size,
                z: self.items.len() as u16,
                command: SceneCommand::ConicGradient { conic_gradient_index },
            });
        }
    }

    #[cfg(feature = "path")]
    fn process_filled_path(
        &mut self,
        _path_geometry: PhysicalRect,
        _clip_geometry: PhysicalRect,
        _commands: alloc::vec::Vec<path::Command>,
        _color: PremultipliedRgbaColor,
    ) {
        // Path rendering is not supported in line-by-line mode (PrepareScene/render_by_line)
        // Only works with buffer-based rendering (RenderToBuffer)
    }

    #[cfg(feature = "path")]
    fn process_stroked_path(
        &mut self,
        _path_geometry: PhysicalRect,
        _clip_geometry: PhysicalRect,
        _commands: alloc::vec::Vec<path::Command>,
        _color: PremultipliedRgbaColor,
        _stroke_width: f32,
        _stroke_line_cap: i_slint_core::items::LineCap,
        _stroke_line_join: i_slint_core::items::LineJoin,
        _stroke_miter_limit: f32,
    ) {
        // Path rendering is not supported in line-by-line mode (PrepareScene/render_by_line)
        // Only works with buffer-based rendering (RenderToBuffer)
    }
}

struct SceneBuilder<'a, T> {
    processor: T,
    state_stack: Vec<RenderState>,
    current_state: RenderState,
    scale_factor: ScaleFactor,
    window: &'a WindowInner,
    rotation: RotationInfo,
    #[cfg(feature = "systemfonts")]
    text_layout_cache: &'a sharedparley::TextLayoutCache,
    /// Where `Layer`s with `cache-rendering-hint` keep their image. `None` renders their
    /// children directly.
    #[cfg(feature = "std")]
    layer_cache: Option<&'a LayerCache>,
    /// This builder renders a layer's image: the layers nested in it render theirs too.
    #[cfg(feature = "std")]
    nested_in_layer_image: bool,
}

impl<'a, T: ProcessScene> SceneBuilder<'a, T> {
    fn new(
        screen_size: PhysicalSize,
        scale_factor: ScaleFactor,
        window: &'a WindowInner,
        processor: T,
        orientation: RenderingRotation,
        #[cfg(feature = "systemfonts")] text_layout_cache: &'a sharedparley::TextLayoutCache,
        #[cfg(feature = "std")] layer_cache: Option<&'a LayerCache>,
    ) -> Self {
        Self {
            processor,
            state_stack: Vec::new(),
            current_state: RenderState {
                alpha: 1.,
                offset: LogicalPoint::default(),
                clip: LogicalRect::new(
                    LogicalPoint::default(),
                    (screen_size.cast() / scale_factor).cast(),
                ),
            },
            scale_factor,
            window,
            rotation: RotationInfo { orientation, screen_size },
            #[cfg(feature = "systemfonts")]
            text_layout_cache,
            #[cfg(feature = "std")]
            layer_cache,
            #[cfg(feature = "std")]
            nested_in_layer_image: false,
        }
    }

    /// Draw a `Layer`'s children from its cached image, rendering the image first if the
    /// cache has none or anything it depends on changed.
    #[cfg(feature = "std")]
    fn draw_cached_layer(
        &mut self,
        cache: &'a LayerCache,
        item_rc: &ItemRc,
        size: LogicalSize,
    ) -> bool {
        let scale = self.scale_factor;
        let at = self.current_state.offset.cast::<f32>() * scale;
        let render = || self.render_layer_image(cache, item_rc, size);
        let mut layer = cache.get_or_update_cache_entry(item_rc, render);
        // At another offset the recorded calls would not place every edge exactly as drawing
        // directly does: draw directly while the layer moves; once it is drawn twice at the
        // same offset, record its calls again there.
        if let Some(cached) = layer.as_ref().filter(|layer| layer.at != at) {
            if cached.shifted_at.get() != Some(at) {
                cached.shifted_at.set(Some(at));
                return false;
            }
            cache.release(item_rc);
            layer = cache.get_or_update_cache_entry(item_rc, || {
                self.render_layer_image(cache, item_rc, size)
            });
        }
        let Some(layer) = layer else { return true };
        let screen_clip = (self.current_state.clip.translate(self.current_state.offset.to_vector()).cast()
            * scale)
            .round_out()
            .cast::<i32>();
        let Some(clipped) = layer.area.intersection(&screen_clip) else { return true };
        self.processor.process_layer_ops(&layer.ops, euclid::vec2(0, 0), clipped.cast());
        true
    }

    /// Record a `Layer`'s children's draw calls, unrotated and at full opacity, over an area
    /// that covers the layer and every child, at the given sub-pixel phase.
    #[cfg(feature = "std")]
    fn render_layer_image(
        &self,
        cache: &'a LayerCache,
        item_rc: &ItemRc,
        size: LogicalSize,
    ) -> Option<Rc<CachedLayer>> {
        let started = std::time::Instant::now();
        cache.rendering.set(true);
        let image = self.render_layer_image_inner(cache, item_rc, size);
        let (count, pixels, time) = cache.renders.get();
        let area = image.as_ref().map_or(0, |l| l.area.size.width as u64 * l.area.size.height as u64);
        cache.renders.set((count + 1, pixels + area, time + started.elapsed()));
        image
    }

    #[cfg(feature = "std")]
    fn render_layer_image_inner(
        &self,
        cache: &'a LayerCache,
        item_rc: &ItemRc,
        size: LogicalSize,
    ) -> Option<Rc<CachedLayer>> {
        let window_adapter = self.window.window_adapter();
        let scale = self.scale_factor;
        let bounds = i_slint_core::item_rendering::item_children_bounding_rect(item_rc, &window_adapter)
            .union(&LogicalRect::from_size(size));
        let at = self.current_state.offset.cast::<f32>() * scale;
        let area = (bounds.cast::<f32>() * scale).translate(at.to_vector()).round_out().cast::<i32>();
        if area.is_empty() || area.size.width > i16::MAX as i32 || area.size.height > i16::MAX as i32 {
            return None;
        }
        let screen = self.rotation.screen_size;
        let mut lines =
            LinesOnly::<PremultipliedRgbaColor>::like(&[], screen.width as usize, screen.height as usize);
        let ops = {
            let mut dirty_region = PhysicalRegion::default();
            dirty_region.rectangles[0] = euclid::Box2D::new(
                euclid::point2(0, 0),
                euclid::point2(screen.width, screen.height),
            );
            dirty_region.count = 1;
            let mut nested = SceneBuilder::new(
                screen,
                scale,
                self.window,
                RenderToBuffer {
                    buffer: &mut lines,
                    dirty_range_cache: Vec::new(),
                    dirty_region,
                    scale_factor: scale,
                    recording: Some(parallel::Recorder::new()),
                    unclipped: None,
                },
                RenderingRotation::NoRotation,
                #[cfg(feature = "systemfonts")]
                self.text_layout_cache,
                Some(cache),
            );
            // Recorded where the children are drawn now, clipped to their bounds only: the
            // replay clips to the layer's clip of the frame it is drawn in.
            nested.nested_in_layer_image = true;
            nested.current_state.offset = self.current_state.offset;
            nested.current_state.clip = bounds;
            i_slint_core::item_rendering::render_item_children(
                &mut nested,
                &item_rc.item_tree(),
                item_rc.index() as isize,
                &window_adapter,
            );
            nested.processor.recording.take().map(|recorder| recorder.into_ops()).unwrap_or_default()
        };
        Some(Rc::new(CachedLayer { at, shifted_at: Cell::new(None), area, ops }))
    }

    #[cfg(feature = "path")]
    fn draw_path_inner(
        &mut self,
        path: Pin<&i_slint_core::items::Path>,
        self_rc: &ItemRc,
        size: LogicalSize,
    ) {
        let geom = LogicalRect::from(size);
        if !self.should_draw(&geom) {
            return;
        }

        // Get the fitted path events from the Path item
        let Some((offset, path_iterator)) = path.fitted_path_events(self_rc) else {
            return;
        };

        let physical_geom_f32 =
            geom.translate(self.current_state.offset.to_vector()).cast() * self.scale_factor;
        let rounded_geom = physical_geom_f32.round();
        let physical_geom = rounded_geom.cast().transformed(self.rotation);

        let rotation = RotationInfo {
            orientation: self.rotation.orientation,
            screen_size: rounded_geom.size.cast::<i16>() + euclid::size2(1, 1),
        };

        let offset = offset * self.scale_factor + (physical_geom_f32.origin - rounded_geom.origin);

        // Convert to zeno commands
        let zeno_commands =
            path::convert_path_data_to_zeno(path_iterator, rotation, self.scale_factor, offset);

        let physical_clip =
            (self.current_state.clip.translate(self.current_state.offset.to_vector()).cast()
                * self.scale_factor)
                .round()
                .cast::<i16>()
                .transformed(self.rotation);

        // Clip the geometry - early return if nothing to draw
        let Some(clipped_geom) = physical_geom.intersection(&physical_clip) else {
            return;
        };

        // Draw fill if specified
        let fill_brush = path.fill();
        if !fill_brush.is_transparent() {
            let fill_color = self.alpha_color(fill_brush.color());
            if fill_color.alpha() > 0 {
                self.processor.process_filled_path(
                    physical_geom,
                    clipped_geom,
                    zeno_commands.clone(),
                    fill_color.into(),
                );
            }
        }

        // Draw stroke if specified
        let stroke_brush = path.stroke();
        let stroke_width = path.stroke_width();
        if !stroke_brush.is_transparent() && stroke_width.get() > 0.0 {
            let stroke_color = self.alpha_color(stroke_brush.color());
            if stroke_color.alpha() > 0 {
                let physical_stroke_width = (stroke_width.cast() * self.scale_factor).get();
                let stroke_line_cap = path.stroke_line_cap();
                let stroke_line_join = path.stroke_line_join();
                let stroke_miter_limit = path.stroke_miter_limit();
                self.processor.process_stroked_path(
                    physical_geom,
                    clipped_geom,
                    zeno_commands,
                    stroke_color.into(),
                    physical_stroke_width,
                    stroke_line_cap,
                    stroke_line_join,
                    stroke_miter_limit,
                );
            }
        }
    }

    fn should_draw(&self, rect: &LogicalRect) -> bool {
        !rect.size.is_empty()
            && self.current_state.alpha > 0.01
            && self.current_state.clip.intersects(rect)
    }

    fn draw_image_impl(
        &mut self,
        image_inner: &ImageInner,
        fit: i_slint_core::graphics::FitResult,
        colorize: Color,
    ) {
        #[cfg(feature = "std")]
        i_slint_core::frame_profile::time(
            |p| &mut p.image_draw,
            || self.draw_image_impl_inner(image_inner, fit, colorize),
        );
        #[cfg(not(feature = "std"))]
        self.draw_image_impl_inner(image_inner, fit, colorize);
    }

    fn draw_image_impl_inner(
        &mut self,
        image_inner: &ImageInner,
        i_slint_core::graphics::FitResult {
            clip_rect: source_rect,
            source_to_target_x,
            source_to_target_y,
            size: fit_size,
            offset: image_fit_offset,
            tiled,
        }: i_slint_core::graphics::FitResult,
        colorize: Color,
    ) {
        let global_alpha_u16 = (self.current_state.alpha * 255.) as u16;
        let offset =
            self.current_state.offset.cast() * self.scale_factor + image_fit_offset.to_vector();

        let physical_clip =
            (self.current_state.clip.translate(self.current_state.offset.to_vector()).cast()
                * self.scale_factor)
                .round()
                .cast()
                .transformed(self.rotation);

        match image_inner {
            ImageInner::None => (),
            ImageInner::StaticTextures(StaticTextures {
                data,
                textures,
                size,
                original_size,
                ..
            }) => {
                let adjust_x = size.width as f32 / original_size.width as f32;
                let adjust_y = size.height as f32 / original_size.height as f32;
                let source_to_target_x = source_to_target_x / adjust_x;
                let source_to_target_y = source_to_target_y / adjust_y;
                let source_rect =
                    source_rect.cast::<f32>().scale(adjust_x, adjust_y).round().to_box2d().cast();

                for t in textures.as_slice() {
                    let t_rect = t.rect.to_box2d();
                    // That's the source rect in the whole image coordinate
                    let Some(src_rect) = t_rect.intersection(&source_rect) else { continue };

                    let target_rect = if tiled.is_some() {
                        euclid::Rect::new(offset, fit_size).round().cast::<i32>()
                    } else {
                        // The slice maps onto the fit rect `offset ..= offset + fit_size`;
                        // this texture only covers `src_rect` of the slice's `source_rect`, so
                        // inset each edge by the uncovered source amount. Edges reaching the
                        // slice boundary keep the exact fit rect, so abutting slices share a
                        // seamless edge (scaling each from its source extent drifted apart).
                        let inset = |a: i32, b: i32, s2t: f32| (a - b) as f32 * s2t;
                        euclid::Box2D::<f32, PhysicalPx>::new(
                            euclid::point2(
                                offset.x
                                    + inset(src_rect.min.x, source_rect.min.x, source_to_target_x),
                                offset.y
                                    + inset(src_rect.min.y, source_rect.min.y, source_to_target_y),
                            ),
                            euclid::point2(
                                offset.x + fit_size.width
                                    - inset(source_rect.max.x, src_rect.max.x, source_to_target_x),
                                offset.y + fit_size.height
                                    - inset(source_rect.max.y, src_rect.max.y, source_to_target_y),
                            ),
                        )
                        .round()
                        .to_rect()
                        .cast::<i32>()
                    };
                    let target_rect = target_rect.transformed(self.rotation).round();

                    let Some(clipped_target) = physical_clip.intersection(&target_rect) else {
                        continue;
                    };

                    let pixel_stride = t.rect.width() as usize;
                    let core::ops::Range { start, end } = compute_range_in_buffer(
                        &PhysicalRect::from_untyped(
                            &src_rect.to_rect().translate(-t.rect.origin.to_vector()).cast(),
                        ),
                        pixel_stride,
                    );
                    let bpp = t.format.bpp();

                    let color = if colorize.alpha() > 0 { colorize } else { t.color };
                    let alpha = if colorize.alpha() > 0 || t.format == TexturePixelFormat::AlphaMap
                    {
                        color.alpha() as u16 * global_alpha_u16 / 255
                    } else {
                        global_alpha_u16
                    } as u8;

                    let tiling = tiled.map(|tile_o| {
                        let src_o = src_rect.min - source_rect.min;
                        let gap = (src_o) + (source_rect.max - src_rect.max);
                        target_pixel_buffer::TilingInfo {
                            offset_x: ((src_o.x as f32 - tile_o.x as f32) * source_to_target_x)
                                .round() as _,
                            offset_y: ((src_o.y as f32 - tile_o.y as f32) * source_to_target_y)
                                .round() as _,
                            scale_x: 1. / source_to_target_x,
                            scale_y: 1. / source_to_target_y,
                            gap_x: (gap.x as f32 * source_to_target_x).round() as _,
                            gap_y: (gap.y as f32 * source_to_target_y).round() as _,
                        }
                    });

                    let t = target_pixel_buffer::DrawTextureArgs {
                        data: target_pixel_buffer::TextureDataContainer::Static(
                            target_pixel_buffer::TextureData::new(
                                &data.as_slice()[t.index..][start * bpp..end * bpp],
                                t.format,
                                pixel_stride * bpp,
                                src_rect.size().cast(),
                            ),
                        ),
                        colorize: (color.alpha() > 0).then_some(color),
                        alpha,
                        dst_x: target_rect.origin.x as _,
                        dst_y: target_rect.origin.y as _,
                        dst_width: target_rect.size.width as _,
                        dst_height: target_rect.size.height as _,
                        rotation: self.rotation.orientation,
                        tiling,
                    };

                    self.processor.process_target_texture(&t, clipped_target.cast());
                }
            }

            ImageInner::NineSlice(..) => unreachable!(),
            _ => {
                let target_rect =
                    euclid::Rect::new(offset, fit_size).round().cast().transformed(self.rotation);
                let Some(clipped_target) = physical_clip.intersection(&target_rect) else {
                    return;
                };

                let orig = image_inner.size().cast::<f32>();
                let svg_target_size = if tiled.is_some() {
                    euclid::size2(orig.width * source_to_target_x, orig.height * source_to_target_y)
                        .round()
                        .cast()
                } else {
                    target_rect.size.cast()
                };
                if let Some(buffer) = image_inner.render_to_buffer(Some(svg_target_size)) {
                    let buf_size = buffer.size().cast::<f32>();

                    let alpha = if colorize.alpha() > 0 {
                        colorize.alpha() as u16 * global_alpha_u16 / 255
                    } else {
                        global_alpha_u16
                    } as u8;

                    let tiling = tiled.map(|tile_o| target_pixel_buffer::TilingInfo {
                        offset_x: (tile_o.x as f32 * -source_to_target_x).round() as _,
                        offset_y: (tile_o.y as f32 * -source_to_target_y).round() as _,
                        scale_x: 1. / source_to_target_x,
                        scale_y: 1. / source_to_target_y,
                        gap_x: 0,
                        gap_y: 0,
                    });

                    let t = target_pixel_buffer::DrawTextureArgs {
                        data: target_pixel_buffer::TextureDataContainer::Shared {
                            buffer: SharedBufferData::SharedImage(buffer),
                            source_rect: PhysicalRect::from_untyped(
                                &source_rect
                                    .cast::<f32>()
                                    .scale(
                                        buf_size.width / orig.width,
                                        buf_size.height / orig.height,
                                    )
                                    .round()
                                    .cast(),
                            ),
                        },
                        colorize: (colorize.alpha() > 0).then_some(colorize),
                        alpha,
                        dst_x: target_rect.origin.x as _,
                        dst_y: target_rect.origin.y as _,
                        dst_width: target_rect.size.width as _,
                        dst_height: target_rect.size.height as _,
                        rotation: self.rotation.orientation,
                        tiling,
                    };

                    self.processor.process_target_texture(&t, clipped_target.cast());
                } else {
                    unimplemented!("The image cannot be rendered")
                }
            }
        };
    }

    fn draw_text_paragraph<Font>(
        &mut self,
        paragraph: &TextParagraphLayout<'_, Font>,
        physical_clip: euclid::Rect<f32, PhysicalPx>,
        offset: euclid::Vector2D<f32, PhysicalPx>,
        color: Color,
        selection: Option<SelectionInfo>,
    ) where
        Font: AbstractFont
            + i_slint_core::textlayout::TextShaper<Length = PhysicalLength>
            + GlyphRenderer,
    {
        let slint_context = self.window.context();
        paragraph
            .layout_lines::<()>(
                |glyphs, line_x, line_y, _, sel| {
                    let baseline_y =
                        line_y + paragraph.layout.half_leading() + paragraph.layout.font.ascent();
                    if let (Some(sel), Some(selection)) = (sel, &selection) {
                        let (band_offset, band_height) = paragraph.layout.cursor_band();
                        let geometry = euclid::rect(
                            line_x.get() + sel.start.get(),
                            (line_y + band_offset).get(),
                            (sel.end - sel.start).get(),
                            band_height.get(),
                        );
                        if let Some(clipped_src) = geometry.intersection(&physical_clip.cast()) {
                            let geometry =
                                clipped_src.translate(offset.cast()).transformed(self.rotation);
                            let args = target_pixel_buffer::DrawRectangleArgs::from_rect(
                                geometry.cast(),
                                selection.selection_background.into(),
                            );
                            self.processor.process_rectangle(&args, geometry);
                        }
                    }
                    let scale_delta = paragraph.layout.font.scale_delta();
                    for positioned_glyph in glyphs {
                        let Some(glyph) = paragraph
                            .layout
                            .font
                            .render_glyph(positioned_glyph.glyph_id, slint_context)
                        else {
                            continue;
                        };

                        let gl_x = PhysicalLength::new((-glyph.x).truncate() as i16);
                        let gl_y = PhysicalLength::new(glyph.y.truncate() as i16);
                        let target_rect = PhysicalRect::new(
                            PhysicalPoint::from_lengths(
                                line_x + positioned_glyph.x - gl_x,
                                baseline_y - gl_y - glyph.height,
                            ),
                            glyph.size(),
                        )
                        .cast();

                        let color = match &selection {
                            Some(s) if s.selection.contains(&positioned_glyph.text_byte_offset) => {
                                s.selection_color
                            }
                            _ => color,
                        };

                        let Some(clipped_target) = physical_clip.intersection(&target_rect) else {
                            continue;
                        };

                        let data = match &glyph.alpha_map {
                            fonts::GlyphAlphaMap::Static(data) => {
                                if glyph.sdf {
                                    let geometry = clipped_target.translate(offset).round();
                                    let origin =
                                        (geometry.origin - offset.round()).round().cast::<i16>();
                                    let off_x = origin.x - target_rect.origin.x as i16;
                                    let off_y = origin.y - target_rect.origin.y as i16;
                                    let pixel_stride = glyph.pixel_stride;
                                    let mut geometry = geometry.cast();
                                    if geometry.size.width > glyph.width.get() - off_x {
                                        geometry.size.width = glyph.width.get() - off_x
                                    }
                                    if geometry.size.height > glyph.height.get() - off_y {
                                        geometry.size.height = glyph.height.get() - off_y
                                    }
                                    let source_size = geometry.size;
                                    if source_size.is_empty() {
                                        continue;
                                    }

                                    let delta32 = Fixed::<i32, 8>::from_fixed(scale_delta);
                                    let normalize = |x: Fixed<i32, 8>| {
                                        if x < Fixed::from_integer(0) {
                                            x + Fixed::from_integer(1)
                                        } else {
                                            x
                                        }
                                    };
                                    let fract_x = normalize(
                                        (-glyph.x) - Fixed::from_integer(gl_x.get() as _),
                                    );
                                    let off_x = delta32 * off_x as i32 + fract_x;
                                    let fract_y =
                                        normalize(glyph.y - Fixed::from_integer(gl_y.get() as _));
                                    let off_y = delta32 * off_y as i32 + fract_y;
                                    let texture = SceneTexture {
                                        data,
                                        pixel_stride,
                                        format: TexturePixelFormat::SignedDistanceField,
                                        extra: SceneTextureExtra {
                                            colorize: color,
                                            // color already is mixed with global alpha
                                            alpha: color.alpha(),
                                            rotation: self.rotation.orientation,
                                            dx: scale_delta,
                                            dy: scale_delta,
                                            off_x: Fixed::try_from_fixed(off_x).unwrap(),
                                            off_y: Fixed::try_from_fixed(off_y).unwrap(),
                                        },
                                    };
                                    self.processor.process_scene_texture(
                                        geometry.transformed(self.rotation),
                                        texture,
                                    );
                                    continue;
                                };

                                target_pixel_buffer::TextureDataContainer::Static(
                                    target_pixel_buffer::TextureData::new(
                                        data,
                                        TexturePixelFormat::AlphaMap,
                                        glyph.pixel_stride as usize,
                                        euclid::size2(glyph.width.get(), glyph.height.get()).cast(),
                                    ),
                                )
                            }
                            fonts::GlyphAlphaMap::Shared(data) => {
                                let source_rect = euclid::rect(0, 0, glyph.width.0, glyph.height.0);
                                target_pixel_buffer::TextureDataContainer::Shared {
                                    buffer: SharedBufferData::AlphaMap {
                                        data: data.clone(),
                                        width: glyph.pixel_stride,
                                    },
                                    source_rect,
                                }
                            }
                        };
                        let clipped_target =
                            clipped_target.translate(offset).round().transformed(self.rotation);
                        let target_rect =
                            target_rect.translate(offset).round().transformed(self.rotation);
                        let t = target_pixel_buffer::DrawTextureArgs {
                            data,
                            colorize: Some(color),
                            // color already is mixed with global alpha
                            alpha: color.alpha(),
                            dst_x: target_rect.origin.x as _,
                            dst_y: target_rect.origin.y as _,
                            dst_width: target_rect.size.width as _,
                            dst_height: target_rect.size.height as _,
                            rotation: self.rotation.orientation,
                            tiling: None,
                        };

                        self.processor.process_target_texture(&t, clipped_target.cast());
                    }
                    core::ops::ControlFlow::Continue(())
                },
                selection.as_ref().map(|s| s.selection.clone()),
            )
            .ok();
    }

    /// Returns the color, mixed with the current_state's alpha
    fn alpha_color(&self, color: Color) -> Color {
        if self.current_state.alpha < 1.0 {
            Color::from_argb_u8(
                (color.alpha() as f32 * self.current_state.alpha) as u8,
                color.red(),
                color.green(),
                color.blue(),
            )
        } else {
            color
        }
    }
}

fn alpha_color(color: Color, alpha: u8) -> Color {
    if alpha < 255 {
        Color::from_argb_u8(
            ((color.alpha() as u32 * alpha as u32) / 255) as u8,
            color.red(),
            color.green(),
            color.blue(),
        )
    } else {
        color
    }
}

struct SelectionInfo {
    selection_color: Color,
    selection_background: Color,
    selection: core::ops::Range<usize>,
}

#[derive(Clone, Copy, Debug)]
struct RenderState {
    alpha: f32,
    offset: LogicalPoint,
    clip: LogicalRect,
}

impl<T: ProcessScene> i_slint_core::item_rendering::ItemRenderer for SceneBuilder<'_, T> {
    fn global_alpha_transparent(&self) -> bool {
        self.current_state.alpha == 0.0
    }

    fn draw_rectangle(
        &mut self,
        rect: Pin<&dyn RenderRectangle>,
        _: &ItemRc,
        size: LogicalSize,
        _cache: &CachedRenderingData,
    ) {
        let geom = LogicalRect::from(size);
        if self.should_draw(&geom) {
            let geom = (geom.translate(self.current_state.offset.to_vector()).cast()
                * self.scale_factor)
                .transformed(self.rotation);

            let clipped =
                (self.current_state.clip.translate(self.current_state.offset.to_vector()).cast()
                    * self.scale_factor)
                    .round()
                    .cast()
                    .transformed(self.rotation);

            let mut args =
                target_pixel_buffer::DrawRectangleArgs::from_rect(geom, rect.background());
            args.alpha = (self.current_state.alpha * 255.) as u8;
            args.rotation = self.rotation.orientation;
            self.processor.process_rectangle(&args, clipped);
        }
    }

    fn draw_border_rectangle(
        &mut self,
        rect: Pin<&dyn RenderBorderRectangle>,
        _: &ItemRc,
        size: LogicalSize,
        _: &CachedRenderingData,
    ) {
        let geom = LogicalRect::from(size);
        if self.should_draw(&geom) {
            let geom = (geom.translate(self.current_state.offset.to_vector()).cast()
                * self.scale_factor)
                .transformed(self.rotation);

            let clipped =
                (self.current_state.clip.translate(self.current_state.offset.to_vector()).cast()
                    * self.scale_factor)
                    .round()
                    .cast()
                    .transformed(self.rotation);

            let radius = (rect.border_radius().cast() * self.scale_factor)
                .transformed(self.rotation)
                .min(BorderRadius::from_length(geom.width_length() / 2.))
                .min(BorderRadius::from_length(geom.height_length() / 2.));

            let border = rect.border_width().cast() * self.scale_factor;
            let border_color =
                if border.get() > 0.01 { rect.border_color() } else { Default::default() };

            let args = target_pixel_buffer::DrawRectangleArgs {
                x: geom.origin.x,
                y: geom.origin.y,
                width: geom.size.width,
                height: geom.size.height,
                top_left_radius: radius.top_left,
                top_right_radius: radius.top_right,
                bottom_right_radius: radius.bottom_right,
                bottom_left_radius: radius.bottom_left,
                border_width: border.get(),
                background: rect.background(),
                border: border_color,
                alpha: (self.current_state.alpha * 255.) as u8,
                rotation: self.rotation.orientation,
            };

            self.processor.process_rectangle(&args, clipped);
        }
    }

    fn draw_window_background(
        &mut self,
        rect: Pin<&dyn RenderRectangle>,
        _self_rc: &ItemRc,
        _size: LogicalSize,
        _cache: &CachedRenderingData,
    ) {
        // register a dependency for the partial renderer's dirty tracker. The actual rendering is done earlier in the software renderer.
        let _ = rect.background();
    }

    fn draw_image(
        &mut self,
        image: Pin<&dyn RenderImage>,
        _: &ItemRc,
        size: LogicalSize,
        _: &CachedRenderingData,
    ) {
        let geom = LogicalRect::from(size);
        if self.should_draw(&geom) {
            let source = image.source();

            let image_inner: &ImageInner = (&source).into();
            if let ImageInner::NineSlice(nine) = image_inner {
                let colorize = image.colorize().color();
                let source_size = source.size();
                for fit in i_slint_core::graphics::fit9slice(
                    source_size,
                    nine.1,
                    size.cast() * self.scale_factor,
                    self.scale_factor,
                    image.alignment(),
                    image.tiling(),
                ) {
                    self.draw_image_impl(&nine.0, fit, colorize);
                }
                return;
            }

            let source_clip = image.source_clip().map_or_else(
                || euclid::Rect::new(Default::default(), source.size().cast()),
                |clip| {
                    clip.intersection(&euclid::Rect::from_size(source.size().cast()))
                        .unwrap_or_default()
                },
            );

            let phys_size = geom.size_length().cast() * self.scale_factor;
            let fit = i_slint_core::graphics::fit(
                image.image_fit(),
                phys_size,
                source_clip,
                self.scale_factor,
                image.alignment(),
                image.tiling(),
            );
            self.draw_image_impl(image_inner, fit, image.colorize().color());
        }
    }

    fn draw_text(
        &mut self,
        text: Pin<&dyn i_slint_core::item_rendering::RenderText>,
        self_rc: &ItemRc,
        size: LogicalSize,
        _cache: &CachedRenderingData,
    ) {
        let font_request = text.font_request(self_rc);

        #[cfg(feature = "systemfonts")]
        let mut font_ctx = self.window.context().font_context().borrow_mut();
        let font = fonts::match_font(
            &font_request,
            self.scale_factor,
            #[cfg(feature = "systemfonts")]
            &mut font_ctx,
        );

        #[cfg(feature = "systemfonts")]
        if uses_parley(&font) {
            drop(font_ctx);
            sharedparley::draw_text(self, text, Some(self_rc), size, Some(self.text_layout_cache));
            return;
        }

        let content = text.text();
        let string = match &content {
            PlainOrStyledText::Plain(string) => alloc::borrow::Cow::Borrowed(string.as_str()),
            PlainOrStyledText::Styled(styled_text) => {
                i_slint_core::styled_text::get_raw_text(styled_text)
            }
        };

        if string.trim().is_empty() {
            return;
        }

        let geom = LogicalRect::from(size);
        if !self.should_draw(&geom) {
            return;
        }

        let color = self.alpha_color(text.color().color());
        let max_size = (geom.size.cast() * self.scale_factor).cast();

        // Clip glyphs not only against the global clip but also against the Text's geometry to avoid drawing outside
        // of its boundaries (that breaks partial rendering and the cast to usize for the item relative coordinate below).
        // FIXME: we should allow drawing outside of the Text element's boundaries.
        let physical_clip = if let Some(logical_clip) = self.current_state.clip.intersection(&geom)
        {
            logical_clip.cast() * self.scale_factor
        } else {
            return; // This should have been caught earlier already
        };
        let offset = self.current_state.offset.to_vector().cast() * self.scale_factor;

        let (horizontal_alignment, vertical_alignment) = text.alignment();
        let max_lines = text.line_limit();

        with_font!(&font, |font| {
            let layout = fonts::text_layout_for_font(font, &font_request, self.scale_factor);
            let paragraph = TextParagraphLayout {
                string: &string,
                layout,
                max_width: max_size.width_length(),
                max_height: max_size.height_length(),
                horizontal_alignment,
                vertical_alignment,
                wrap: text.wrap(),
                overflow: text.overflow(),
                single_line: false,
                max_lines,
            };

            self.draw_text_paragraph(&paragraph, physical_clip, offset, color, None);
        });
    }

    fn draw_text_input(
        &mut self,
        text_input: Pin<&i_slint_core::items::TextInput>,
        self_rc: &ItemRc,
        size: LogicalSize,
    ) {
        let font_request = text_input.font_request(self_rc);
        #[cfg(feature = "systemfonts")]
        let mut font_ctx = self.window.context().font_context().borrow_mut();
        let font = fonts::match_font(
            &font_request,
            self.scale_factor,
            #[cfg(feature = "systemfonts")]
            &mut font_ctx,
        );

        #[cfg(feature = "systemfonts")]
        if uses_parley(&font) {
            drop(font_ctx);
            sharedparley::draw_text_input(self, text_input, self_rc, size, self.text_layout_cache);
            return;
        }

        let geom = LogicalRect::from(size);
        if !self.should_draw(&geom) {
            return;
        }

        let max_size = (geom.size.cast() * self.scale_factor).cast();

        // Clip glyphs not only against the global clip but also against the Text's geometry to avoid drawing outside
        // of its boundaries (that breaks partial rendering and the cast to usize for the item relative coordinate below).
        // FIXME: we should allow drawing outside of the Text element's boundaries.
        let physical_clip = if let Some(logical_clip) = self.current_state.clip.intersection(&geom)
        {
            logical_clip.cast() * self.scale_factor
        } else {
            return; // This should have been caught earlier already
        };
        let offset = self.current_state.offset.to_vector().cast() * self.scale_factor;

        let text_visual_representation = text_input.visual_representation();
        let color = self.alpha_color(text_visual_representation.text_color.color());

        let selection =
            (!text_visual_representation.selection_range.is_empty()).then_some(SelectionInfo {
                selection_background: self.alpha_color(text_input.selection_background_color()),
                selection_color: self.alpha_color(text_input.selection_foreground_color()),
                selection: text_visual_representation.selection_range.clone(),
            });

        let cursor_pos_and_height = with_font!(&font, |font| {
            let paragraph = TextParagraphLayout {
                string: &text_visual_representation.text,
                layout: fonts::text_layout_for_font(font, &font_request, self.scale_factor),
                max_width: max_size.width_length(),
                max_height: max_size.height_length(),
                horizontal_alignment: text_input.horizontal_alignment(),
                vertical_alignment: text_input.vertical_alignment(),
                wrap: text_input.wrap(),
                overflow: TextOverflow::Clip,
                single_line: text_input.single_line(),
                max_lines: None,
            };

            self.draw_text_paragraph(&paragraph, physical_clip, offset, color, selection);

            text_visual_representation.cursor_position.map(|cursor_offset| {
                let (band_offset, band_height) = paragraph.layout.cursor_band();
                let (cursor_x, cursor_y) = paragraph.cursor_pos_for_byte_offset(cursor_offset);
                ((cursor_x, cursor_y + band_offset), band_height)
            })
        });

        // Nothing below depends on the concrete font, so keep it out of the monomorphized body.
        if let Some(((cursor_x, cursor_y), cursor_height)) = cursor_pos_and_height {
            let cursor_rect = PhysicalRect::new(
                PhysicalPoint::from_lengths(cursor_x, cursor_y),
                PhysicalSize::from_lengths(
                    (text_input.text_cursor_width().cast() * self.scale_factor).cast(),
                    cursor_height,
                ),
            );

            if let Some(clipped_src) = cursor_rect.intersection(&physical_clip.cast()) {
                let geometry = clipped_src.translate(offset.cast()).transformed(self.rotation);
                let args = target_pixel_buffer::DrawRectangleArgs::from_rect(
                    geometry.cast(),
                    self.alpha_color(text_visual_representation.cursor_color).into(),
                );
                self.processor.process_rectangle(&args, geometry);
            }
        }
    }

    #[cfg(all(feature = "std", not(feature = "path")))]
    fn draw_path(
        &mut self,
        _path: Pin<&i_slint_core::items::Path>,
        _self_rc: &ItemRc,
        _size: LogicalSize,
    ) {
        // Path rendering is disabled without the path feature
    }

    #[cfg(feature = "path")]
    fn draw_path(
        &mut self,
        path: Pin<&i_slint_core::items::Path>,
        self_rc: &ItemRc,
        size: LogicalSize,
    ) {
        i_slint_core::frame_profile::time(
            |p| &mut p.path_draw,
            || self.draw_path_inner(path, self_rc, size),
        )
    }

    fn draw_box_shadow(
        &mut self,
        _box_shadow: Pin<&i_slint_core::items::BoxShadow>,
        _: &ItemRc,
        _size: LogicalSize,
    ) {
        // TODO
    }

    fn visit_layer(
        &mut self,
        layer: Pin<&i_slint_core::items::Layer>,
        item_rc: &ItemRc,
        size: LogicalSize,
    ) -> RenderingResult {
        #[cfg(feature = "std")]
        if layer.cache_rendering_hint() {
            // A layer inside a layer's image is drawn into that image.
            if self.nested_in_layer_image {
                if let Some(cache) = self.layer_cache {
                    cache.direct.borrow_mut().remove(&layer_key(item_rc));
                }
                return RenderingResult::ContinueRenderingChildren;
            }
            // Under opacity or rotation the children are drawn directly: a cached layer
            // replays their draw calls unrotated, each at full opacity.
            if self.current_state.alpha < 1.0
                || self.rotation.orientation != RenderingRotation::NoRotation
            {
                return RenderingResult::ContinueRenderingChildren;
            }
            if let Some(cache) = self.layer_cache {
                // An image is rendered only from content that stayed the same for a frame: a
                // layer whose content changed is drawn directly, and its image entry is marked
                // clean while keeping its dependencies, so the next frame tells whether the
                // content changed again. An animation inside a layer renders no image that the
                // next frame throws away; once it ends, the image is rendered once.
                let key = layer_key(item_rc);
                let state = cache.is_dirty(item_rc);
                let changed = state == Some(true);
                let direct = cache.direct.borrow().contains_key(&key);
                let area = LogicalRect::new(self.current_state.offset, size);
                let (invalidated, waiting, blitted) = cache.visits.get();
                if changed {
                    cache.visits.set((invalidated + 1, waiting, blitted));
                    cache.acknowledge_dirty(item_rc);
                    let layer =
                        DirectLayer { item: item_rc.downgrade(), area, changed_this_frame: true };
                    cache.direct.borrow_mut().insert(key, layer);
                    return RenderingResult::ContinueRenderingChildren;
                }
                // Its content stayed the same for a frame, or it has no image yet. A frame
                // renders one image: another such layer is drawn directly once more.
                if direct || state.is_none() {
                    if cache.rendering.get() {
                        cache.visits.set((invalidated, waiting + 1, blitted));
                        let layer =
                            DirectLayer { item: item_rc.downgrade(), area, changed_this_frame: false };
                        cache.direct.borrow_mut().insert(key, layer);
                        return RenderingResult::ContinueRenderingChildren;
                    }
                    if direct {
                        // Its image is of the content before the change.
                        cache.direct.borrow_mut().remove(&key);
                        cache.release(item_rc);
                    }
                }
                if self.draw_cached_layer(cache, item_rc, size) {
                    cache.visits.set((invalidated, waiting, blitted + 1));
                    return RenderingResult::ContinueRenderingWithoutChildren;
                }
                // Moving to a new sub-pixel phase: drawn directly, its image kept.
                return RenderingResult::ContinueRenderingChildren;
            }
        }
        let _ = (layer, item_rc, size);
        RenderingResult::ContinueRenderingChildren
    }

    fn combine_clip(&mut self, other: LogicalRect, _radius: LogicalBorderRadius) -> bool {
        match self.current_state.clip.intersection(&other) {
            Some(r) => {
                self.current_state.clip = r;
                true
            }
            None => {
                self.current_state.clip = LogicalRect::default();
                false
            }
        }
        // TODO: handle radius
    }

    fn get_current_clip(&self) -> LogicalRect {
        self.current_state.clip
    }

    fn translate(&mut self, distance: LogicalVector) {
        self.current_state.offset += distance;
        self.current_state.clip = self.current_state.clip.translate(-distance)
    }

    fn current_transform(&self) -> i_slint_core::lengths::ItemTransform {
        let v = self.current_state.offset.to_vector().cast::<f32>();
        i_slint_core::lengths::ItemTransform::translation(v.x, v.y)
    }

    fn rotate(&mut self, _angle_in_degrees: f32) {
        // TODO (#6068)
    }

    fn scale(&mut self, _x_factor: f32, _y_factor: f32) {
        // TODO
    }

    fn apply_opacity(&mut self, opacity: f32) {
        self.current_state.alpha *= opacity;
    }

    fn save_state(&mut self) {
        self.state_stack.push(self.current_state);
    }

    fn restore_state(&mut self) {
        self.current_state = self.state_stack.pop().unwrap();
    }

    fn scale_factor(&self) -> ScaleFactor {
        self.scale_factor
    }

    fn draw_cached_pixmap(
        &mut self,
        _item: &ItemRc,
        update_fn: &dyn Fn(&mut dyn FnMut(u32, u32, &[u8])),
    ) {
        // FIXME: actually cache the pixmap
        update_fn(&mut |width, height, data| {
            let img = SharedImageBuffer::RGBA8Premultiplied(SharedPixelBuffer::clone_from_slice(
                data, width, height,
            ));

            let physical_clip = (self.current_state.clip.cast() * self.scale_factor).cast();
            let source_rect = euclid::rect(0, 0, width as _, height as _);

            if let Some(clipped_src) = source_rect.intersection(&physical_clip) {
                let offset = self.current_state.offset.cast() * self.scale_factor;
                let geometry = clipped_src.translate(offset.to_vector().cast()).round_in();

                let t = target_pixel_buffer::DrawTextureArgs {
                    data: target_pixel_buffer::TextureDataContainer::Shared {
                        buffer: SharedBufferData::SharedImage(img),
                        source_rect,
                    },
                    colorize: None,
                    alpha: (self.current_state.alpha * 255.) as u8,
                    dst_x: offset.x as _,
                    dst_y: offset.y as _,
                    dst_width: width as _,
                    dst_height: height as _,
                    rotation: self.rotation.orientation,
                    tiling: None,
                };
                self.processor
                    .process_target_texture(&t, geometry.cast().transformed(self.rotation));
            }
        });
    }

    fn draw_string(&mut self, string: &str, color: Color) {
        let font_request = Default::default();
        #[cfg(feature = "systemfonts")]
        let mut font_ctx = self.window.context().font_context().borrow_mut();
        let font = fonts::match_font(
            &font_request,
            self.scale_factor,
            #[cfg(feature = "systemfonts")]
            &mut font_ctx,
        );

        #[cfg(feature = "systemfonts")]
        if uses_parley(&font) {
            drop(font_ctx);
            sharedparley::draw_text(
                self,
                std::pin::pin!((i_slint_core::SharedString::from(string), Brush::from(color))),
                None,
                self.current_state.clip.size.cast(),
                None,
            );
            return;
        }

        let clip = self.current_state.clip.cast() * self.scale_factor;

        with_font!(&font, |font| {
            let layout = fonts::text_layout_for_font(font, &font_request, self.scale_factor);

            let paragraph = TextParagraphLayout {
                string,
                layout,
                max_width: clip.width_length().cast(),
                max_height: clip.height_length().cast(),
                horizontal_alignment: Default::default(),
                vertical_alignment: Default::default(),
                wrap: Default::default(),
                overflow: Default::default(),
                single_line: false,
                max_lines: None,
            };

            self.draw_text_paragraph(&paragraph, clip, Default::default(), color, None);
        });
    }

    fn draw_image_direct(&mut self, image: i_slint_core::graphics::Image) {
        let image_inner: &ImageInner = (&image).into();
        let source_size = image.size();
        if source_size.is_empty() {
            return;
        }
        let target_size = euclid::Size2D::<f32, i_slint_core::lengths::LogicalPx>::from_untyped(
            source_size.cast(),
        ) * self.scale_factor;
        let fit = i_slint_core::graphics::fit(
            i_slint_core::items::ImageFit::Fill,
            target_size,
            i_slint_core::graphics::IntRect::from_size(source_size.cast()),
            self.scale_factor,
            Default::default(),
            Default::default(),
        );
        self.draw_image_impl(image_inner, fit, i_slint_core::Color::default());
    }

    fn window(&self) -> &i_slint_core::window::WindowInner {
        self.window
    }

    fn as_any(&mut self) -> Option<&mut dyn core::any::Any> {
        None
    }
}

impl<T: ProcessScene> i_slint_core::item_rendering::ItemRendererFeatures for SceneBuilder<'_, T> {
    const SUPPORTS_TRANSFORMATIONS: bool = false;
}

#[cfg(feature = "systemfonts")]
use i_slint_core::textlayout::sharedparley::{self, fontique};

#[cfg(feature = "systemfonts")]
impl<T: ProcessScene> sharedparley::GlyphRenderer for SceneBuilder<'_, T> {
    type PlatformBrush = Color;

    fn platform_brush_for_color(&mut self, color: &Color) -> Option<Self::PlatformBrush> {
        Some(*color)
    }

    fn platform_text_fill_brush(
        &mut self,
        brush: Brush,
        _size: LogicalSize,
    ) -> Option<Self::PlatformBrush> {
        Some(brush.color())
    }

    fn platform_text_stroke_brush(
        &mut self,
        brush: Brush,
        _physical_stroke_width: f32,
        _size: LogicalSize,
    ) -> Option<Self::PlatformBrush> {
        Some(brush.color())
    }

    fn fill_rectangle(
        &mut self,
        mut physical_rect: sharedparley::PhysicalRect,
        color: Color,
        radius: sharedparley::PhysicalLength,
        border: Option<sharedparley::RectangleBorder<Color>>,
    ) {
        let has_visible_border =
            border.as_ref().is_some_and(|b| b.width.get() > 0.0 && b.brush.alpha() > 0);
        if color.alpha() == 0 && !has_visible_border {
            return;
        }

        let global_offset =
            (self.current_state.offset.to_vector().cast() * self.scale_factor).cast();

        physical_rect.origin += global_offset;
        // Round the edges instead of truncating them, so that they quantize the way `draw_glyph_run`
        // quantizes the glyph origin and the clip it cuts glyphs with. Truncating here puts a
        // selection highlight edge a whole pixel away from the glyph clip meant to line up with it,
        // as soon as the item's own offset lands on a fractional device pixel.
        let geometry: PhysicalRect = physical_rect.round().cast().transformed(self.rotation);
        // These fills reach the processor directly rather than through `draw_rectangle`, so they
        // have to bring the clip along themselves. Without it a text decoration, a selection
        // highlight or a cursor taller than the item it belongs to paints right over its
        // surroundings, while the glyphs beside it are clipped.
        let clip =
            (self.current_state.clip.translate(self.current_state.offset.to_vector()).cast()
                * self.scale_factor)
                .round()
                .cast()
                .transformed(self.rotation);
        let mut args = target_pixel_buffer::DrawRectangleArgs::from_rect(
            geometry.cast(),
            Brush::SolidColor(color),
        );

        if radius.get() > 0.0 {
            let r = radius.get().min(args.width / 2.0).min(args.height / 2.0);
            args.top_left_radius = r;
            args.top_right_radius = r;
            args.bottom_right_radius = r;
            args.bottom_left_radius = r;
        }

        if let Some(sharedparley::RectangleBorder { brush: border_color, width: border_width }) =
            border
            && border_width.get() > 0.0
            && border_color.alpha() > 0
        {
            args.border_width = border_width.get();
            args.border = Brush::SolidColor(border_color);
        }

        self.processor.process_rectangle(&args, clip);
    }

    fn draw_glyph_run(
        &mut self,
        font: &sharedparley::parley::FontData,
        font_size: sharedparley::PhysicalLength,
        normalized_coords: &[i16],
        _synthesis: &fontique::Synthesis,
        color: Self::PlatformBrush,
        y_offset: sharedparley::PhysicalLength,
        glyphs_it: &mut dyn Iterator<Item = sharedparley::parley::layout::Glyph>,
    ) {
        let slint_context = self.window.context();
        let font = i_slint_core::frame_profile::time(
            |p| &mut p.glyph_run_setup,
            || {
                let (swash_key, swash_offset) =
                    fonts::systemfonts::get_swash_font_info(&font.data, font.index);
                fonts::vectorfont::VectorFont::new_from_blob_and_index_with_coords(
                    font.data.clone(),
                    font.index,
                    swash_key,
                    swash_offset,
                    font_size.cast(),
                    normalized_coords,
                )
            },
        );

        let global_offset: euclid::Vector2D<f32, PhysicalPx> =
            self.current_state.offset.to_vector().cast() * self.scale_factor;

        const SUBPIXEL_BINS: i32 = fonts::vectorfont::SUBPIXEL_BIN_COUNT;

        let color = self.alpha_color(color);
        let physical_clip: euclid::Rect<i32, PhysicalPx> =
            (self.current_state.clip.translate(self.current_state.offset.to_vector()).cast()
                * self.scale_factor)
                .round()
                .cast()
                .transformed(self.rotation);

        for positioned_glyph in glyphs_it {
            let Some(id) = std::num::NonZero::new(positioned_glyph.id as u16) else {
                continue;
            };

            // Absolute, sub-pixel-accurate device position of the pen for this glyph.
            let abs_x = global_offset.x + positioned_glyph.x;
            let abs_y = global_offset.y + positioned_glyph.y + y_offset.get();

            // Quantize the horizontal position to SUBPIXEL_BINS positions per pixel.
            // The integer part is the blit origin; the fractional part is rendered
            // into the glyph coverage (see `render_vector_glyph`) instead of being
            // discarded. This keeps inter-glyph spacing even: snapping the pen to a
            // whole pixel (truncate or round) redistributes the sub-pixel advances
            // unevenly between neighboring glyph pairs.
            let quantized_x = (abs_x * SUBPIXEL_BINS as f32).round() as i32;
            let dst_int_x = quantized_x.div_euclid(SUBPIXEL_BINS);
            let subpixel_bin = quantized_x.rem_euclid(SUBPIXEL_BINS) as u8;

            let Some(glyph) = font.render_vector_glyph(id, subpixel_bin, slint_context) else {
                continue;
            };
            i_slint_core::frame_profile::record(|p| p.glyphs += 1);

            let gl_y = PhysicalLength::new(glyph.y.truncate() as i16);
            // i32 so a glyph past the i16 range survives until it is clipped below.
            let target_rect: euclid::Rect<i32, PhysicalPx> = euclid::Rect::<f32, PhysicalPx>::new(
                euclid::Point2D::new(
                    dst_int_x as f32 + glyph.glyph_origin_x,
                    abs_y.round() + (-gl_y - glyph.height).get() as f32,
                ),
                glyph.size().cast(),
            )
            .cast()
            .transformed(self.rotation);

            let Some(clipped_target) = physical_clip.intersection(&target_rect) else {
                continue;
            };

            let data = {
                let source_rect = euclid::rect(0, 0, glyph.width.0, glyph.height.0);
                target_pixel_buffer::TextureDataContainer::Shared {
                    buffer: SharedBufferData::AlphaMap {
                        data: glyph.alpha_map,
                        width: glyph.pixel_stride,
                    },
                    source_rect,
                }
            };

            let t = target_pixel_buffer::DrawTextureArgs {
                data,
                colorize: Some(color),
                // color already is mixed with global alpha
                alpha: color.alpha(),
                dst_x: target_rect.origin.x as _,
                dst_y: target_rect.origin.y as _,
                dst_width: target_rect.size.width as _,
                dst_height: target_rect.size.height as _,
                rotation: self.rotation.orientation,
                tiling: None,
            };

            self.processor.process_target_texture(&t, clipped_target.cast());
        }
    }
}
