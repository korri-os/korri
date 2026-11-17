// SPDX-License-Identifier: GPL-3.0-or-later
//! The X11 software picture through EGL: the Rust port of
//! `picture/video/egl.c` (#391 step F, slice F3).
//!
//! Ported from Moonlight Embedded's `src/video/egl.c` (Copyright (C) 2017
//! Iwan Timmer, GPL-3.0-or-later) with Korri's changes for #476 (any frame
//! size at its place in the window, rows read at their own length, black
//! bars). The Rust adaptation is Korri work dated 2026-11-17.
//!
//! The same config, context, buffers, shaders, textures, uniforms and draw
//! calls, in the same order. What differs: the invalid `glEnable(GL_TEXTURE_2D)`
//! of OpenGL ES 2 is not made; `position` is bound to attribute 0 before the
//! link (egl.c binds it after, where the call has no effect and the linker
//! picks 0 for the only attribute); and a setup failure ends the stream
//! through its teardown (`failed`) where egl.c calls `exit()`.
use super::{
    egl::{Display, Failure, Window},
    ffi,
};
use crate::log;
use glow::{HasContext, PixelUnpackData};
use khronos_egl as egl;
use std::{
    ffi::{c_ulong, c_void},
    sync::{
        Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
};

const VERTEX_SOURCE: &str = "\
attribute vec2 position;\
varying mediump vec2 tex_position;\
\
void main() {\
  gl_Position = vec4(position, 0, 1);\
  tex_position = vec2((position.x + 1.) / 2., (1. - position.y) / 2.);\
}\
";

const FRAGMENT_SOURCE: &str = "\
uniform lowp sampler2D ymap;\
uniform lowp sampler2D umap;\
uniform lowp sampler2D vmap;\
uniform mediump vec3 visible;\
uniform mediump vec3 last;\
varying mediump vec2 tex_position;\
\
void main() {\
  mediump float y = texture2D(ymap, vec2(min(tex_position.x * visible.x, last.x), tex_position.y)).r;\
  mediump float u = texture2D(umap, vec2(min(tex_position.x * visible.y, last.y), tex_position.y)).r - .5;\n\
  mediump float v = texture2D(vmap, vec2(min(tex_position.x * visible.z, last.z), tex_position.y)).r - .5;\n\
  lowp float r = y + 1.28033 * v;\
  lowp float g = y - .21482 * u - .38059 * v;\
  lowp float b = y + 2.12798 * u;\
  gl_FragColor = vec4(r, g, b, 1.0);\
}\
";

const VERTICES: [f32; 8] = [-1., 1., -1., -1., 1., -1., 1., 1.];
const ELEMENTS: [u32; 6] = [0, 1, 2, 2, 3, 0];
const SAMPLERS: [&str; 3] = ["ymap", "umap", "vmap"];
/// OpenGL ES gave no name for a buffer, shader, program or texture.
const NO_OBJECT: &str = "OpenGL ES made no object";

/// egl.c's state between `egl_init` and `egl_destroy`.
struct Software {
    window: Window,
    program: glow::Program,
    textures: [glow::Texture; 3],
    samplers: [Option<glow::UniformLocation>; 3],
    visible: Option<glow::UniformLocation>,
    last: Option<glow::UniformLocation>,
    width: i32,
    height: i32,
    /// Korri (#476): each texture holds whole rows of its plane, padding
    /// included, and the shader reads only the visible part of them. A new
    /// frame size or row length makes the textures again.
    line_sizes: [i32; 3],
    current: bool,
}

static SOFTWARE: Mutex<Option<Software>> = Mutex::new(None);
static FAILED: AtomicBool = AtomicBool::new(false);

fn software() -> MutexGuard<'static, Option<Software>> {
    SOFTWARE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A new stream: no failure yet.
pub(super) fn reset() {
    FAILED.store(false, Ordering::Relaxed);
}

pub(super) fn failed() -> bool {
    FAILED.load(Ordering::Relaxed)
}

/// `egl_init`.
///
/// # Safety
/// `display` is a live Xlib `Display*` and `window` one of its windows.
pub(super) unsafe fn init(display: *mut c_void, window: c_ulong, width: i32, height: i32) {
    // SAFETY: the caller's display and window.
    match unsafe { Software::open(display, window, width, height) } {
        Ok(new) => {
            log::line("picture: the Rust EGL presenter draws the software picture");
            *software() = Some(new);
        }
        Err(why) => {
            log::line(&format!("EGL: {why}; the stream ends"));
            FAILED.store(true, Ordering::Relaxed);
        }
    }
}

/// `egl_draw`.
pub(super) fn draw(frame: &ffi::AVFrame, place: ffi::picture_place) {
    if let Some(software) = software().as_mut() {
        software.draw(frame, place);
    }
}

/// `egl_destroy`.
pub(super) fn destroy() {
    drop(software().take());
}

impl Software {
    /// # Safety
    /// As `init`.
    unsafe fn open(
        display: *mut c_void,
        window: c_ulong,
        width: i32,
        height: i32,
    ) -> Result<Self, &'static str> {
        // SAFETY: the caller's display.
        let display = unsafe { Display::open(display) }.map_err(|failure| match failure {
            Failure::Display => "error get display",
            _ => "error initialising display",
        })?;
        let config = [
            egl::RED_SIZE,
            8,
            egl::GREEN_SIZE,
            8,
            egl::BLUE_SIZE,
            8,
            egl::ALPHA_SIZE,
            8,
            egl::SURFACE_TYPE,
            egl::WINDOW_BIT,
            egl::NONE,
        ];
        let window = display
            .window(window, &config)
            .map_err(|failure| match failure {
                Failure::Config => "Unable to query for available configs, found 0.",
                Failure::Api => "error binding API",
                Failure::Context => "couldn't get a valid context",
                _ => "couldn't make the context current on the window",
            })?;
        let gl = &window.gl;
        // SAFETY: the window's context is current on this thread.
        let (program, textures, samplers, visible, last) = unsafe {
            let vbo = gl.create_buffer().map_err(|_| NO_OBJECT)?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes_of(&VERTICES), glow::STATIC_DRAW);
            let ebo = gl.create_buffer().map_err(|_| NO_OBJECT)?;
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
            gl.buffer_data_u8_slice(
                glow::ELEMENT_ARRAY_BUFFER,
                bytes_of(&ELEMENTS),
                glow::STATIC_DRAW,
            );
            let program = gl.create_program().map_err(|_| NO_OBJECT)?;
            for (kind, source) in [
                (glow::VERTEX_SHADER, VERTEX_SOURCE),
                (glow::FRAGMENT_SHADER, FRAGMENT_SOURCE),
            ] {
                let shader = gl.create_shader(kind).map_err(|_| NO_OBJECT)?;
                gl.shader_source(shader, source);
                gl.compile_shader(shader);
                gl.attach_shader(program, shader);
            }
            gl.bind_attrib_location(program, 0, "position");
            gl.link_program(program);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 2 * 4, 0);
            let mut textures = [None; 3];
            let mut samplers = [None, None, None];
            for (i, name) in SAMPLERS.into_iter().enumerate() {
                let texture = gl.create_texture().map_err(|_| NO_OBJECT)?;
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MIN_FILTER,
                    glow::LINEAR as i32,
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MAG_FILTER,
                    glow::LINEAR as i32,
                );
                // A row's length is seldom a power of two; OpenGL ES 2
                // samples such a texture only when it clamps.
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_WRAP_S,
                    glow::CLAMP_TO_EDGE as i32,
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_WRAP_T,
                    glow::CLAMP_TO_EDGE as i32,
                );
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::LUMINANCE as i32,
                    if i > 0 { width / 2 } else { width },
                    if i > 0 { height / 2 } else { height },
                    0,
                    glow::LUMINANCE,
                    glow::UNSIGNED_BYTE,
                    PixelUnpackData::Slice(None),
                );
                textures[i] = Some(texture);
                samplers[i] = gl.get_uniform_location(program, name);
            }
            (
                program,
                textures.map(|t| t.expect("made above")),
                samplers,
                gl.get_uniform_location(program, "visible"),
                gl.get_uniform_location(program, "last"),
            )
        };
        window.release();
        Ok(Self {
            window,
            program,
            textures,
            samplers,
            visible,
            last,
            width,
            height,
            line_sizes: [0; 3],
            current: false,
        })
    }

    fn draw(&mut self, frame: &ffi::AVFrame, place: ffi::picture_place) {
        if !self.current {
            self.window.make_current();
            self.current = true;
        }
        let gl = &self.window.gl;
        let (width, height) = (frame.width, frame.height);
        let widths = [width, (width + 1) / 2, (width + 1) / 2];
        let heights = [height, (height + 1) / 2, (height + 1) / 2];
        let resized = width != self.width || height != self.height;
        let (mut visible, mut last) = ([0f32; 3], [0f32; 3]);
        // SAFETY: the context is current on this thread; each plane holds
        // `linesize` bytes for each of its rows, as FFmpeg gives it.
        unsafe {
            gl.use_program(Some(self.program));
            gl.enable_vertex_attrib_array(0);
            for i in 0..3 {
                let line = frame.linesize[i];
                if line < widths[i] {
                    return;
                }
                gl.active_texture(glow::TEXTURE0 + i as u32);
                gl.bind_texture(glow::TEXTURE_2D, Some(self.textures[i]));
                let plane = std::slice::from_raw_parts(
                    frame.data[i].cast_const(),
                    line as usize * heights[i] as usize,
                );
                if resized || line != self.line_sizes[i] {
                    gl.tex_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        glow::LUMINANCE as i32,
                        line,
                        heights[i],
                        0,
                        glow::LUMINANCE,
                        glow::UNSIGNED_BYTE,
                        PixelUnpackData::Slice(Some(plane)),
                    );
                    self.line_sizes[i] = line;
                } else {
                    gl.tex_sub_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        0,
                        0,
                        line,
                        heights[i],
                        glow::LUMINANCE,
                        glow::UNSIGNED_BYTE,
                        PixelUnpackData::Slice(Some(plane)),
                    );
                }
                gl.uniform_1_i32(self.samplers[i].as_ref(), i as i32);
                // Stop at the centre of the last visible texel, so that the
                // filter does not blend the row's padding in.
                visible[i] = widths[i] as f32 / line as f32;
                last[i] = (widths[i] as f32 - 0.5) / line as f32;
            }
            self.width = width;
            self.height = height;
            gl.uniform_3_f32(self.visible.as_ref(), visible[0], visible[1], visible[2]);
            gl.uniform_3_f32(self.last.as_ref(), last[0], last[1], last[2]);
            // Black bars around the picture; OpenGL counts rows from the bottom.
            gl.clear_color(0., 0., 0., 1.);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.viewport(
                place.x,
                place.window_height - place.y - place.height,
                place.width,
                place.height,
            );
            gl.draw_elements(glow::TRIANGLES, 6, glow::UNSIGNED_INT, 0);
        }
        if self.window.swap() {
            // SAFETY: place.c's own call, from any thread.
            unsafe { ffi::picture_shown(width, height) };
        }
    }
}

/// The bytes of plain numbers, for a buffer upload.
fn bytes_of<T: Copy>(values: &[T]) -> &[u8] {
    // SAFETY: f32 and u32 have no padding; the borrow stays the same.
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), size_of_val(values)) }
}
