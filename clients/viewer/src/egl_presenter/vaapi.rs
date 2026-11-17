// SPDX-License-Identifier: GPL-3.0-or-later
//! A VA-API decoded surface through EGL: the Rust port of
//! `picture/video/vaapi_egl.c` (#477, #476; #391 step F, slice F3).
//!
//! `video/ffmpeg_vaapi.c` exports the decoder's surface as dmabufs and calls
//! `vaapi_egl_present` when `vaPutSurface` cannot show it (Intel under
//! Xwayland) or when the picture has bars. The same config, context,
//! extension checks, shaders, image import, uniforms and draw calls as the C,
//! in the same order, and the same log lines. What differs: the corners go
//! in a buffer object (glow takes no client-side array; the pixels are the
//! same), and a setup that fails releases its display at once, where the C
//! keeps it until the cleanup.
use super::{
    egl::{Display, Step, Window, error_code},
    ffi,
};
use crate::log;
use glow::HasContext;
use khronos_egl as egl;
use std::{
    ffi::{c_ulong, c_void},
    sync::{Mutex, MutexGuard, PoisonError},
};

/// `DRM_FORMAT_MOD_INVALID`, without libdrm, as the C defines it.
const MODIFIER_INVALID: u64 = 0x00ff_ffff_ffff_ffff;
const MAX_PLANES: usize = 3;

const VERTEX_SOURCE: &str = "attribute vec2 position;\
uniform vec2 visible;\
varying mediump vec2 at;\
void main() {\
  gl_Position = vec4(position, 0, 1);\
  at = vec2((position.x + 1.) / 2., (1. - position.y) / 2.) * visible;\
}";
// The driver converts to RGB, by the colour hints of the image.
const FRAGMENT_SOURCE: &str = "#extension GL_OES_EGL_image_external : require\n\
uniform samplerExternalOES picture;\
uniform mediump vec2 last;\
varying mediump vec2 at;\
void main() { gl_FragColor = texture2D(picture, min(at, last)); }";
const CORNERS: [f32; 8] = [-1., 1., -1., -1., 1., 1., 1., -1.];

type CreateImage =
    unsafe extern "C" fn(*mut c_void, *mut c_void, u32, *mut c_void, *const i32) -> *mut c_void;
type DestroyImage = unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32;
type ImageTargetTexture = unsafe extern "C" fn(u32, *mut c_void);

/// The presenter after its setup on the first frame.
struct Vaapi {
    window: Window,
    create_image: CreateImage,
    destroy_image: DestroyImage,
    image_target_texture: ImageTargetTexture,
    modifiers: bool,
    visible: Option<glow::UniformLocation>,
    last: Option<glow::UniformLocation>,
}

enum State {
    Unset,
    /// The setup failed once and said why; later frames are not shown.
    Failed,
    // glow's table of OpenGL ES functions is large.
    Ready(Box<Vaapi>),
}

static STATE: Mutex<State> = Mutex::new(State::Unset);

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The log line of a failure, with EGL's error code as the C prints it.
fn fail(what: &str, code: i32) -> bool {
    log::line(&format!("VAAPI EGL presentation: {what} (EGL 0x{code:x})"));
    false
}

/// A failure after a call whose error is still pending, as the C reads it.
fn pending(what: &'static str) -> (&'static str, i32) {
    (what, error_code())
}

/// `vaapi_egl_present`: draws the exported surface at `place` in the
/// window, black around it, and returns after the GPU has read it. The
/// first call binds EGL to this display and window; later calls name the
/// same pair from the same thread. False: nothing was shown, and the log
/// says why.
///
/// # Safety
/// `x11` is a live Xlib `Display*` and `window` one of its windows; the
/// descriptor's fds are open for this call.
pub(super) unsafe fn present(
    x11: *mut c_void,
    window: c_ulong,
    place: ffi::picture_place,
    picture: &ffi::VADRMPRIMESurfaceDescriptor,
    frame: &ffi::AVFrame,
) -> bool {
    let mut state = state();
    if let State::Unset = *state {
        // SAFETY: the caller's display and window.
        *state = match unsafe { Vaapi::setup(x11, window) } {
            Ok(ready) => {
                log::line("picture: the Rust EGL presenter shows VA-API surfaces");
                State::Ready(Box::new(ready))
            }
            Err((what, code)) => {
                fail(what, code);
                State::Failed
            }
        };
    }
    match &*state {
        State::Ready(vaapi) => vaapi.present(place, picture, frame),
        _ => false,
    }
}

/// The setup's refusal for this display and window, if any: the C's words
/// and EGL's error code.
///
/// # Safety
/// As `present`.
#[cfg(test)]
pub(super) unsafe fn refusal(x11: *mut c_void, window: c_ulong) -> Option<(&'static str, i32)> {
    // SAFETY: the caller's display and window.
    unsafe { Vaapi::setup(x11, window) }.err()
}

/// `vaapi_egl_cleanup`.
pub(super) fn cleanup() {
    *state() = State::Unset;
}

impl Vaapi {
    /// # Safety
    /// As `present`.
    /// Why it failed: the C's words and EGL's error code.
    unsafe fn setup(x11: *mut c_void, window: c_ulong) -> Result<Self, (&'static str, i32)> {
        // SAFETY: the caller's display.
        let display =
            unsafe { Display::open(x11) }.map_err(|failure| ("no EGL display", failure.code))?;
        let extensions = display
            .extensions()
            .map_err(|code| ("EGL cannot import a dmabuf", code))?;
        if !extensions.contains("EGL_EXT_image_dma_buf_import") {
            return Err(pending("EGL cannot import a dmabuf"));
        }
        let modifiers = extensions.contains("EGL_EXT_image_dma_buf_import_modifiers");
        let config = [
            egl::RED_SIZE,
            8,
            egl::GREEN_SIZE,
            8,
            egl::BLUE_SIZE,
            8,
            egl::SURFACE_TYPE,
            egl::WINDOW_BIT,
            egl::RENDERABLE_TYPE,
            egl::OPENGL_ES2_BIT,
            egl::NONE,
        ];
        let window = display.window(window, &config).map_err(|failure| {
            let what = match failure.step {
                Step::Config => "no window config",
                Step::Api => "no OpenGL ES",
                _ => "no context on the window",
            };
            (what, failure.code)
        })?;
        // The newest frame goes out at once. The compositor picks the frame
        // that each refresh shows, so a wait for the display here only adds
        // delay.
        let _ = egl::API.swap_interval(window.display, 0);
        let external = window
            .gl
            .supported_extensions()
            .contains("GL_OES_EGL_image_external");
        let procs = (
            window.proc_address("eglCreateImageKHR"),
            window.proc_address("eglDestroyImageKHR"),
            window.proc_address("glEGLImageTargetTexture2DOES"),
        );
        let (Some(create_image), Some(destroy_image), Some(image_target_texture), true) =
            (procs.0, procs.1, procs.2, external)
        else {
            return Err(pending("OpenGL ES cannot sample an external image"));
        };
        let gl = &window.gl;
        // SAFETY: the window's context is current on this thread.
        let (visible, last) = unsafe {
            let program = gl.create_program().map_err(|_| pending(NO_OBJECT))?;
            for (kind, source) in [
                (glow::VERTEX_SHADER, VERTEX_SOURCE),
                (glow::FRAGMENT_SHADER, FRAGMENT_SOURCE),
            ] {
                let shader = gl.create_shader(kind).map_err(|_| pending(NO_OBJECT))?;
                gl.shader_source(shader, source);
                gl.compile_shader(shader);
                gl.attach_shader(program, shader);
            }
            gl.bind_attrib_location(program, 0, "position");
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                return Err(pending("the shader did not link"));
            }
            gl.use_program(Some(program));
            let visible = gl.get_uniform_location(program, "visible");
            let last = gl.get_uniform_location(program, "last");
            let corners = gl.create_buffer().map_err(|_| pending(NO_OBJECT))?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(corners));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes_of(&CORNERS), glow::STATIC_DRAW);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 0, 0);
            gl.enable_vertex_attrib_array(0);
            let texture = gl.create_texture().map_err(|_| pending(NO_OBJECT))?;
            let external = ffi::GL_TEXTURE_EXTERNAL_OES;
            gl.bind_texture(external, Some(texture));
            gl.tex_parameter_i32(external, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(external, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(external, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(external, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            (visible, last)
        };
        Ok(Self {
            window,
            // SAFETY: the functions of these names, by EGL_KHR_image_base and
            // GL_OES_EGL_image, whose presence was checked above.
            create_image: unsafe {
                std::mem::transmute::<extern "system" fn(), CreateImage>(create_image)
            },
            destroy_image: unsafe {
                std::mem::transmute::<extern "system" fn(), DestroyImage>(destroy_image)
            },
            image_target_texture: unsafe {
                std::mem::transmute::<extern "system" fn(), ImageTargetTexture>(
                    image_target_texture,
                )
            },
            modifiers,
            visible,
            last,
        })
    }

    fn present(
        &self,
        place: ffi::picture_place,
        picture: &ffi::VADRMPRIMESurfaceDescriptor,
        frame: &ffi::AVFrame,
    ) -> bool {
        let Some(attributes) = import_attributes(picture, colour_hints(frame), self.modifiers)
        else {
            return fail("EGL refused the decoded picture", error_code());
        };
        let display = self.window.display.as_ptr();
        // SAFETY: a valid attribute list that ends with EGL_NONE; the fds
        // stay open for this call.
        let image = unsafe {
            (self.create_image)(
                display,
                std::ptr::null_mut(),
                ffi::EGL_LINUX_DMA_BUF_EXT,
                std::ptr::null_mut(),
                attributes.as_ptr(),
            )
        };
        if image.is_null() {
            return fail("EGL refused the decoded picture", error_code());
        }
        let gl = &self.window.gl;
        let (coded_width, coded_height) = (picture.width as f32, picture.height as f32);
        // SAFETY: the context is current on this thread; the image lives
        // until the destroy below.
        unsafe {
            (self.image_target_texture)(ffi::GL_TEXTURE_EXTERNAL_OES, image);
            // The surface has the coded size. Map the visible part onto its
            // place in the window, and stop at the centre of its last texel,
            // so that the filter does not blend the padding in.
            gl.uniform_2_f32(
                self.visible.as_ref(),
                frame.width as f32 / coded_width,
                frame.height as f32 / coded_height,
            );
            gl.uniform_2_f32(
                self.last.as_ref(),
                (frame.width as f32 - 0.5) / coded_width,
                (frame.height as f32 - 0.5) / coded_height,
            );
            // Black bars around the picture (#476). A clear covers the whole
            // window, whatever the viewport; OpenGL counts the viewport's
            // rows from the bottom.
            gl.clear_color(0., 0., 0., 1.);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.viewport(
                place.x,
                place.window_height - place.y - place.height,
                place.width,
                place.height,
            );
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            // The decoder reuses the surface when its frame is released.
            gl.finish();
        }
        let shown = self.window.swap();
        // SAFETY: the image made above, no longer sampled.
        unsafe { (self.destroy_image)(display, image) };
        // The swap's own error code: the C reads eglGetError after the
        // destroy, which has cleared it.
        shown.map_or_else(
            |code| fail("the window did not take the frame", code),
            |()| true,
        )
    }
}

/// OpenGL ES gave no name for a shader, program, buffer or texture.
const NO_OBJECT: &str = "OpenGL ES made no object";

/// The frame's colour description as EGL's hints: matrix, then range.
pub(super) fn colour_hints(frame: &ffi::AVFrame) -> (i32, i32) {
    let matrix = match frame.colorspace {
        ffi::AVColorSpace_AVCOL_SPC_BT709 => ffi::EGL_ITU_REC709_EXT,
        ffi::AVColorSpace_AVCOL_SPC_BT2020_NCL => ffi::EGL_ITU_REC2020_EXT,
        _ => ffi::EGL_ITU_REC601_EXT,
    };
    let range = if frame.color_range == ffi::AVColorRange_AVCOL_RANGE_JPEG {
        ffi::EGL_YUV_FULL_RANGE_EXT
    } else {
        ffi::EGL_YUV_NARROW_RANGE_EXT
    };
    (matrix as i32, range as i32)
}

/// The attributes of `eglCreateImageKHR` for a composed export: one layer
/// that holds every plane of the picture. None: an export that EGL cannot
/// take this way.
pub(super) fn import_attributes(
    picture: &ffi::VADRMPRIMESurfaceDescriptor,
    (matrix, range): (i32, i32),
    modifiers: bool,
) -> Option<Vec<i32>> {
    const FD: [u32; MAX_PLANES] = [
        ffi::EGL_DMA_BUF_PLANE0_FD_EXT,
        ffi::EGL_DMA_BUF_PLANE1_FD_EXT,
        ffi::EGL_DMA_BUF_PLANE2_FD_EXT,
    ];
    const OFFSET: [u32; MAX_PLANES] = [
        ffi::EGL_DMA_BUF_PLANE0_OFFSET_EXT,
        ffi::EGL_DMA_BUF_PLANE1_OFFSET_EXT,
        ffi::EGL_DMA_BUF_PLANE2_OFFSET_EXT,
    ];
    const PITCH: [u32; MAX_PLANES] = [
        ffi::EGL_DMA_BUF_PLANE0_PITCH_EXT,
        ffi::EGL_DMA_BUF_PLANE1_PITCH_EXT,
        ffi::EGL_DMA_BUF_PLANE2_PITCH_EXT,
    ];
    const MODIFIER_LOW: [u32; MAX_PLANES] = [
        ffi::EGL_DMA_BUF_PLANE0_MODIFIER_LO_EXT,
        ffi::EGL_DMA_BUF_PLANE1_MODIFIER_LO_EXT,
        ffi::EGL_DMA_BUF_PLANE2_MODIFIER_LO_EXT,
    ];
    const MODIFIER_HIGH: [u32; MAX_PLANES] = [
        ffi::EGL_DMA_BUF_PLANE0_MODIFIER_HI_EXT,
        ffi::EGL_DMA_BUF_PLANE1_MODIFIER_HI_EXT,
        ffi::EGL_DMA_BUF_PLANE2_MODIFIER_HI_EXT,
    ];
    let layer = &picture.layers[0];
    let planes = layer.num_planes as usize;
    if picture.num_layers != 1 || planes == 0 || planes > MAX_PLANES {
        return None;
    }
    let mut attributes = vec![
        egl::WIDTH,
        picture.width as i32,
        egl::HEIGHT,
        picture.height as i32,
        ffi::EGL_LINUX_DRM_FOURCC_EXT as i32,
        layer.drm_format as i32,
        ffi::EGL_YUV_COLOR_SPACE_HINT_EXT as i32,
        matrix,
        ffi::EGL_SAMPLE_RANGE_HINT_EXT as i32,
        range,
    ];
    for plane in 0..planes {
        let object = layer.object_index[plane];
        if object >= picture.num_objects {
            return None;
        }
        let object = &picture.objects[object as usize];
        attributes.extend([
            FD[plane] as i32,
            object.fd,
            OFFSET[plane] as i32,
            layer.offset[plane] as i32,
            PITCH[plane] as i32,
            layer.pitch[plane] as i32,
        ]);
        let modifier = object.drm_format_modifier;
        if modifiers && modifier != MODIFIER_INVALID {
            attributes.extend([
                MODIFIER_LOW[plane] as i32,
                (modifier & 0xffff_ffff) as i32,
                MODIFIER_HIGH[plane] as i32,
                (modifier >> 32) as i32,
            ]);
        }
    }
    attributes.push(egl::NONE);
    Some(attributes)
}

/// The bytes of plain numbers, for a buffer upload.
fn bytes_of(values: &[f32]) -> &[u8] {
    // SAFETY: f32 has no padding; the borrow stays the same.
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), size_of_val(values)) }
}
