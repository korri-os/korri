// SPDX-License-Identifier: GPL-3.0-or-later
//! The Rust VA-API EGL presenter beside the carried C, on a real Xvfb window
//! with Mesa's software EGL (llvmpipe): the same window pixels, read back
//! from the X server, for the same surfaces and places. No VA driver runs:
//! Mesa imports a memfd as a dmabuf, so a decoded NV12 surface is written by
//! hand, with padding beyond its visible size.
use super::{Kind, chosen, ffi, select, vaapi};
use std::{
    ffi::{c_ulong, c_void},
    fs::File,
    io::Write,
    os::fd::{AsRawFd, FromRawFd},
    sync::{Mutex, MutexGuard, PoisonError},
};

/// One presenter at a time: each keeps its state for the process, and EGL
/// contexts are current for a thread.
static PICTURE: Mutex<()> = Mutex::new(());

fn exclusive() -> MutexGuard<'static, ()> {
    PICTURE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The test run's one X11 connection, never closed: an X server resets
/// when its last client leaves, and refuses connections while it does.
fn connection() -> *mut ffi::Display {
    struct Connection(*mut ffi::Display);
    // SAFETY: the tests that use it take turns (`PICTURE`).
    unsafe impl Send for Connection {}
    unsafe impl Sync for Connection {}
    static CONNECTION: std::sync::OnceLock<Connection> = std::sync::OnceLock::new();
    CONNECTION
        .get_or_init(|| {
            // Another test program's last client may have just left.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                // SAFETY: Xlib's own call.
                let display = unsafe { ffi::XOpenDisplay(std::ptr::null()) };
                if !display.is_null() {
                    return Connection(display);
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "an X server (xvfb-run)"
                );
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        })
        .0
}

/// One mapped window on the test run's connection.
struct X11 {
    display: *mut ffi::Display,
    window: c_ulong,
}

impl X11 {
    fn open(width: u32, height: u32) -> Self {
        let display = connection();
        // SAFETY: Xlib's own calls on the live connection.
        unsafe {
            let window = ffi::XCreateSimpleWindow(
                display,
                ffi::XDefaultRootWindow(display),
                0,
                0,
                width,
                height,
                0,
                0,
                0,
            );
            ffi::XMapWindow(display, window);
            ffi::XSync(display, 0);
            Self { display, window }
        }
    }

    fn native(&self) -> *mut c_void {
        self.display.cast()
    }

    /// The window's pixels as the X server holds them, RGB, row by row.
    fn pixels(&self, width: u32, height: u32) -> Vec<u32> {
        // SAFETY: Xlib's own calls on this window; the image is freed here.
        unsafe {
            ffi::XSync(self.display, 0);
            let image = ffi::XGetImage(
                self.display,
                self.window,
                0,
                0,
                width,
                height,
                !0,
                ffi::ZPixmap as i32,
            );
            assert!(!image.is_null());
            let pixels = (0..height as i32)
                .flat_map(|y| (0..width as i32).map(move |x| (x, y)))
                .map(|(x, y)| (ffi::XGetPixel(image, x, y) & 0xff_ffff) as u32)
                .collect();
            ffi::XDestroyImage(image);
            pixels
        }
    }
}

impl Drop for X11 {
    fn drop(&mut self) {
        // SAFETY: ours, destroyed once; the connection stays.
        unsafe {
            ffi::XDestroyWindow(self.display, self.window);
            ffi::XSync(self.display, 0);
        }
    }
}

fn red(rgb: u32) -> i32 {
    ((rgb >> 16) & 0xff) as i32
}

/// The check of one picture: black outside `place`; inside, the left half
/// bright and the right half dark, two pixels from the edges and the middle,
/// where the filter blends.
fn assert_halves(pixels: &[u32], window: (i32, i32), place: ffi::picture_place, what: &str) {
    for y in 0..window.1 {
        for x in 0..window.0 {
            let value = pixels[(y * window.0 + x) as usize];
            let inside = x >= place.x
                && x < place.x + place.width
                && y >= place.y
                && y < place.y + place.height;
            if !inside {
                assert_eq!(
                    value, 0,
                    "{what}: ({x}, {y}) outside the picture is {value:06x}"
                );
                continue;
            }
            let (across, down) = (x - place.x, y - place.y);
            if across < 2
                || across >= place.width - 2
                || (2 * across - place.width).abs() < 4
                || down < 2
                || down >= place.height - 2
            {
                continue;
            }
            let left = 2 * across < place.width;
            let bright = red(value);
            assert!(
                if left { bright >= 200 } else { bright <= 80 },
                "{what}: ({x}, {y}) in the picture's {} half has brightness {bright}",
                if left { "left" } else { "right" }
            );
        }
    }
}

/// The decoder's frame of a surface: its visible size and colour.
struct Frame(*mut ffi::AVFrame);

impl Frame {
    fn size(&self) -> (i32, i32) {
        // SAFETY: a live frame.
        unsafe { ((*self.0).width, (*self.0).height) }
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        // SAFETY: ours, freed once.
        unsafe { ffi::av_frame_free(&mut self.0) };
    }
}

fn fit(picture: (i32, i32), window: (i32, i32)) -> ffi::picture_place {
    // SAFETY: place.c's pure function.
    unsafe { ffi::picture_fit(picture.0, picture.1, window.0, window.1) }
}

/// `DRM_FORMAT_NV12`.
const DRM_NV12: u32 = 0x3231_564e;
/// `DRM_FORMAT_MOD_INVALID` and `DRM_FORMAT_MOD_LINEAR`.
const MODIFIER_INVALID: u64 = 0x00ff_ffff_ffff_ffff;
const MODIFIER_LINEAR: u64 = 0;

/// A decoded NV12 surface of `coded` size in a memfd, its visible part's
/// left half bright and right half dark in every row, its padding white luma
/// and pink chroma, and the composed export that `vaExportSurfaceHandle`
/// would give. Padding drawn would show.
struct Surface {
    file: File,
    descriptor: ffi::VADRMPRIMESurfaceDescriptor,
}

impl Surface {
    fn nv12(coded: (u32, u32), visible: (u32, u32), modifier: u64) -> Self {
        let (pitch, rows) = (coded.0 as usize, coded.1 as usize);
        let mut bytes = vec![0u8; pitch * rows * 3 / 2];
        let (luma, chroma) = bytes.split_at_mut(pitch * rows);
        for (at, value) in luma.iter_mut().enumerate() {
            let (x, y) = ((at % pitch) as u32, (at / pitch) as u32);
            *value = if x >= visible.0 || y >= visible.1 {
                255
            } else if x < visible.0 / 2 {
                235
            } else {
                40
            };
        }
        for (at, value) in chroma.iter_mut().enumerate() {
            let (x, y) = (((at % pitch) / 2) as u32, (at / pitch) as u32);
            let padding = x >= visible.0.div_ceil(2) || y >= visible.1.div_ceil(2);
            *value = if padding { 240 } else { 128 };
        }
        // SAFETY: memfd_create's own call; the descriptor becomes the File's.
        let file = unsafe {
            let fd = libc::memfd_create(c"korri-nv12".as_ptr(), 0);
            assert!(fd >= 0, "memfd_create");
            File::from_raw_fd(fd)
        };
        (&file).write_all(&bytes).unwrap();
        let mut descriptor = ffi::VADRMPRIMESurfaceDescriptor {
            fourcc: ffi::VA_FOURCC_NV12,
            width: coded.0,
            height: coded.1,
            num_objects: 1,
            num_layers: 1,
            ..Default::default()
        };
        descriptor.objects[0].fd = file.as_raw_fd();
        descriptor.objects[0].size = bytes.len() as u32;
        descriptor.objects[0].drm_format_modifier = modifier;
        let layer = &mut descriptor.layers[0];
        layer.drm_format = DRM_NV12;
        layer.num_planes = 2;
        layer.object_index = [0, 0, 0, 0];
        layer.offset = [0, (pitch * rows) as u32, 0, 0];
        layer.pitch = [pitch as u32, pitch as u32, 0, 0];
        Self { file, descriptor }
    }
}

/// The decoder's frame for a surface.
fn colour_frame(visible: (u32, u32), space: u32, range: u32) -> Frame {
    // SAFETY: libavutil's own call; only the fields below are read.
    unsafe {
        let frame = ffi::av_frame_alloc();
        assert!(!frame.is_null());
        (*frame).width = visible.0 as i32;
        (*frame).height = visible.1 as i32;
        (*frame).colorspace = space;
        (*frame).color_range = range;
        Frame(frame)
    }
}

/// One VA-API case: a surface, its frame, and the window it shows in.
struct Case {
    surface: Surface,
    frame: Frame,
    window: (i32, i32),
}

impl Case {
    fn new(coded: (u32, u32), visible: (u32, u32), colour: (u32, u32), modifier: u64) -> Self {
        Self {
            surface: Surface::nv12(coded, visible, modifier),
            frame: colour_frame(visible, colour.0, colour.1),
            window: (160, 90),
        }
    }

    fn place(&self) -> ffi::picture_place {
        fit(self.frame.size(), self.window)
    }
}

/// Each case through `vaapi_egl_present` and then `vaapi_egl_cleanup`, as
/// ffmpeg_vaapi.c calls them: whether each was shown, and the window's
/// pixels after it.
fn vaapi_pixels(kind: Kind, x11: &X11, cases: &[Case]) -> Vec<(bool, Vec<u32>)> {
    select(kind);
    let shown = cases
        .iter()
        .map(|case| {
            // SAFETY: the call ffmpeg_vaapi.c makes, with a live window, an
            // open descriptor and a live frame.
            let shown = unsafe {
                chosen::vaapi_egl_present(
                    x11.native(),
                    x11.window,
                    case.place(),
                    &case.surface.descriptor,
                    case.frame.0,
                )
            };
            (
                shown,
                x11.pixels(case.window.0 as u32, case.window.1 as u32),
            )
        })
        .collect();
    // SAFETY: as ffmpeg_vaapi.c's cleanup calls it.
    unsafe { chosen::vaapi_egl_cleanup() };
    shown
}

#[test]
fn the_rust_vaapi_picture_draws_the_carried_cs_pixels() {
    let _picture = exclusive();
    let x11 = X11::open(160, 90);
    let (bt601, bt709, narrow, full) = (
        ffi::AVColorSpace_AVCOL_SPC_UNSPECIFIED,
        ffi::AVColorSpace_AVCOL_SPC_BT709,
        ffi::AVColorRange_AVCOL_RANGE_MPEG,
        ffi::AVColorRange_AVCOL_RANGE_JPEG,
    );
    let cases = [
        // The coded size is larger than the visible one, as a decoder's is.
        Case::new((144, 80), (128, 72), (bt601, narrow), MODIFIER_INVALID),
        // A portrait stream in the landscape window: bars at the sides.
        Case::new((80, 144), (72, 128), (bt709, narrow), MODIFIER_INVALID),
        // A smaller stream, full range, with a modifier that EGL is told.
        Case::new((112, 64), (96, 54), (bt709, full), MODIFIER_LINEAR),
    ];
    let carried = vaapi_pixels(Kind::Carried, &x11, &cases);
    let rust = vaapi_pixels(Kind::Rust, &x11, &cases);
    for ((case, carried), rust) in cases.iter().zip(&carried).zip(&rust) {
        let what = format!("{:?}", case.frame.size());
        assert!(carried.0, "{what}: the C showed it");
        assert!(rust.0, "{what}: the Rust showed it");
        assert_halves(&carried.1, case.window, case.place(), &format!("C {what}"));
        assert!(
            rust.1 == carried.1,
            "{what}: the Rust picture's pixels differ from the C's"
        );
    }
}

#[test]
fn an_export_that_egl_cannot_take_is_not_shown_by_either() {
    let _picture = exclusive();
    let x11 = X11::open(160, 90);
    let narrow = (
        ffi::AVColorSpace_AVCOL_SPC_UNSPECIFIED,
        ffi::AVColorRange_AVCOL_RANGE_MPEG,
    );
    let mut layers = Case::new((144, 80), (128, 72), narrow, MODIFIER_INVALID);
    layers.surface.descriptor.num_layers = 2;
    let mut object = Case::new((144, 80), (128, 72), narrow, MODIFIER_INVALID);
    object.surface.descriptor.layers[0].object_index[1] = 1;
    let cases = [layers, object];
    for kind in [Kind::Carried, Kind::Rust] {
        let shown: Vec<bool> = vaapi_pixels(kind, &x11, &cases)
            .into_iter()
            .map(|(shown, _)| shown)
            .collect();
        assert_eq!(shown, [false, false], "{kind:?}");
    }
}

#[test]
fn a_setup_that_fails_reports_egls_own_error_code() {
    let _picture = exclusive();
    let display = connection();
    // A window that is gone: EGL can make no surface on it.
    // SAFETY: Xlib's own calls on the live connection.
    let gone = unsafe {
        let window = ffi::XCreateSimpleWindow(
            display,
            ffi::XDefaultRootWindow(display),
            0,
            0,
            16,
            16,
            0,
            0,
            0,
        );
        ffi::XDestroyWindow(display, window);
        ffi::XSync(display, 0);
        window
    };
    // SAFETY: a live display; the window's XID names nothing.
    let refused = unsafe { vaapi::refusal(display.cast(), gone) };
    // khronos-egl reads and so clears eglGetError: a later read would give
    // EGL_SUCCESS (0x3000), not the surface's error.
    assert_eq!(
        refused,
        Some(("no context on the window", khronos_egl::BAD_NATIVE_WINDOW))
    );
}

#[test]
fn the_import_names_each_plane_and_the_colour_as_the_c_does() {
    let narrow = ffi::EGL_YUV_NARROW_RANGE_EXT as i32;
    let rec709 = ffi::EGL_ITU_REC709_EXT as i32;
    let surface = Surface::nv12((144, 80), (128, 72), MODIFIER_LINEAR);
    let fd = surface.file.as_raw_fd();
    let head = [
        khronos_egl::WIDTH,
        144,
        khronos_egl::HEIGHT,
        80,
        ffi::EGL_LINUX_DRM_FOURCC_EXT as i32,
        DRM_NV12 as i32,
        ffi::EGL_YUV_COLOR_SPACE_HINT_EXT as i32,
        rec709,
        ffi::EGL_SAMPLE_RANGE_HINT_EXT as i32,
        narrow,
    ];
    let planes = [
        ffi::EGL_DMA_BUF_PLANE0_FD_EXT as i32,
        fd,
        ffi::EGL_DMA_BUF_PLANE0_OFFSET_EXT as i32,
        0,
        ffi::EGL_DMA_BUF_PLANE0_PITCH_EXT as i32,
        144,
        ffi::EGL_DMA_BUF_PLANE1_FD_EXT as i32,
        fd,
        ffi::EGL_DMA_BUF_PLANE1_OFFSET_EXT as i32,
        144 * 80,
        ffi::EGL_DMA_BUF_PLANE1_PITCH_EXT as i32,
        144,
    ];
    let attributes =
        |modifiers| vaapi::import_attributes(&surface.descriptor, (rec709, narrow), modifiers);
    // Without the modifiers extension, no modifier is named.
    let plain: Vec<i32> = [&head[..], &planes[..], &[khronos_egl::NONE]].concat();
    assert_eq!(attributes(false), Some(plain));
    // With it, each plane names its object's modifier, low half first.
    let with = attributes(true).unwrap();
    let modifier = [
        ffi::EGL_DMA_BUF_PLANE0_MODIFIER_LO_EXT as i32,
        0,
        ffi::EGL_DMA_BUF_PLANE0_MODIFIER_HI_EXT as i32,
        0,
    ];
    assert_eq!(&with[10..16], &planes[..6]);
    assert_eq!(&with[16..20], &modifier);
    assert_eq!(with.len(), 10 + 2 * (6 + 4) + 1);
    // An invalid modifier is never named.
    let mut invalid = Surface::nv12((144, 80), (128, 72), MODIFIER_INVALID);
    let unnamed = vaapi::import_attributes(&invalid.descriptor, (rec709, narrow), true).unwrap();
    assert_eq!(unnamed.len(), 10 + 2 * 6 + 1);
    // Four planes, no plane, or two layers: not a composed export of at most
    // three planes.
    for planes in [0, 4] {
        invalid.descriptor.layers[0].num_planes = planes;
        assert_eq!(
            vaapi::import_attributes(&invalid.descriptor, (rec709, narrow), true),
            None
        );
    }
}

#[test]
fn the_colour_hints_follow_the_frames_matrix_and_range() {
    let hints = |space, range| {
        let frame = colour_frame((16, 16), space, range);
        vaapi::colour_hints(frame.as_ref_frame())
    };
    let (narrow, full) = (
        ffi::EGL_YUV_NARROW_RANGE_EXT as i32,
        ffi::EGL_YUV_FULL_RANGE_EXT as i32,
    );
    assert_eq!(
        hints(
            ffi::AVColorSpace_AVCOL_SPC_BT709,
            ffi::AVColorRange_AVCOL_RANGE_MPEG
        ),
        (ffi::EGL_ITU_REC709_EXT as i32, narrow)
    );
    assert_eq!(
        hints(
            ffi::AVColorSpace_AVCOL_SPC_BT2020_NCL,
            ffi::AVColorRange_AVCOL_RANGE_JPEG
        ),
        (ffi::EGL_ITU_REC2020_EXT as i32, full)
    );
    // Anything else is BT.601, as the core's default stream is.
    for space in [
        ffi::AVColorSpace_AVCOL_SPC_UNSPECIFIED,
        ffi::AVColorSpace_AVCOL_SPC_SMPTE170M,
        ffi::AVColorSpace_AVCOL_SPC_BT2020_CL,
    ] {
        assert_eq!(
            hints(space, ffi::AVColorRange_AVCOL_RANGE_UNSPECIFIED),
            (ffi::EGL_ITU_REC601_EXT as i32, narrow)
        );
    }
}

impl Frame {
    fn as_ref_frame(&self) -> &ffi::AVFrame {
        // SAFETY: a live frame, borrowed as long as `self`.
        unsafe { &*self.0 }
    }
}
