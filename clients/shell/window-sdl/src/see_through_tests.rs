//! Workstation readback of the real upload/composition path. This proves
//! texture alpha, not the TV's EGL config or compositor's video-plane order.
use super::*;
use std::ffi::c_void;

fn rgba(pixel: &PremultipliedRgbaColor) -> [u8; 4] {
    [pixel.red, pixel.green, pixel.blue, pixel.alpha]
}

#[test]
#[ignore = "requires workstation SDL and Xvfb; never run on the TV"]
fn transparent_half_preserves_rgba_and_partial_uploads() {
    let sdl = Sdl::load().unwrap();
    sdl.set_hint(ffi::SDL_HINT_VIDEO_ALLOW_SCREENSAVER, "1");
    // SAFETY: this test runs alone in its own process on the UI thread.
    assert_eq!(unsafe { (sdl.SDL_Init)(ffi::SDL_INIT_VIDEO) }, 0);
    assert!(see_through::enabled(
        std::env::var_os("KORRI_SHELL_SEE_THROUGH").as_deref()
    ));
    // These test-only functions all exist in SDL 2.0.4. Do not add them to
    // the shipped adapter's required symbols just for readback assertions.
    let (set_target, read_pixels) = unsafe {
        let handle = libc::dlopen(
            c"libSDL2-2.0.so.0".as_ptr(),
            libc::RTLD_NOW | libc::RTLD_LOCAL,
        );
        assert!(!handle.is_null());
        let set_target = libc::dlsym(handle, c"SDL_SetRenderTarget".as_ptr());
        let read_pixels = libc::dlsym(handle, c"SDL_RenderReadPixels".as_ptr());
        assert!(!set_target.is_null() && !read_pixels.is_null());
        (
            std::mem::transmute::<
                *mut c_void,
                unsafe extern "C" fn(*mut ffi::Renderer, *mut ffi::Texture) -> c_int,
            >(set_target),
            std::mem::transmute::<
                *mut c_void,
                unsafe extern "C" fn(
                    *mut ffi::Renderer,
                    *const ffi::Rect,
                    u32,
                    *mut c_void,
                    c_int,
                ) -> c_int,
            >(read_pixels),
        )
    };
    for width in [4, 5, 1] {
        let height = 3;
        let mut screen = open_screen(
            &sdl,
            "See-through readback",
            Placement::Windowed { width, height },
        )
        .unwrap();
        let pixel = PremultipliedRgbaColor {
            red: 80,
            green: 40,
            blue: 20,
            alpha: 128,
        };
        screen.buffer.fill(pixel);
        screen.buffer[0] = PremultipliedRgbaColor::default();
        upload(&sdl, &screen, (0, 0, width, height)).unwrap();
        // ARGB render target makes alpha measurable even under Xvfb, whose
        // desktop window itself need not be composited or alpha-capable.
        // SAFETY: live renderer and valid output dimensions.
        let target = unsafe {
            (sdl.SDL_CreateTexture)(
                screen.renderer,
                ffi::SDL_PIXELFORMAT_ABGR8888,
                2,
                width as c_int,
                height as c_int,
            )
        };
        assert!(!target.is_null(), "{}", sdl.error());
        assert_eq!(unsafe { set_target(screen.renderer, target) }, 0);
        let mut actual = vec![[0u8; 4]; screen.buffer.len()];
        for partial in [false, true] {
            if partial {
                screen.buffer[width as usize - 1].red = 100;
                upload(&sdl, &screen, (width - 1, 0, 1, 1)).unwrap();
            }
            // The normal copy preserves all four channels, without blending.
            assert_eq!(
                unsafe {
                    (sdl.SDL_RenderCopy)(
                        screen.renderer,
                        screen.texture,
                        std::ptr::null(),
                        std::ptr::null(),
                    )
                },
                0
            );
            assert_eq!(
                unsafe {
                    read_pixels(
                        screen.renderer,
                        std::ptr::null(),
                        ffi::SDL_PIXELFORMAT_ABGR8888,
                        actual.as_mut_ptr().cast(),
                        width as c_int * 4,
                    )
                },
                0
            );
            let source: Vec<_> = screen.buffer.iter().map(rgba).collect();
            assert_eq!(actual, source);
            compose(&sdl, &screen).unwrap();
            assert_eq!(
                unsafe {
                    read_pixels(
                        screen.renderer,
                        std::ptr::null(),
                        ffi::SDL_PIXELFORMAT_ABGR8888,
                        actual.as_mut_ptr().cast(),
                        width as c_int * 4,
                    )
                },
                0
            );
            for (i, actual) in actual.iter().enumerate() {
                let expected = if i % (width as usize) < width as usize / 2 {
                    [0; 4]
                } else {
                    rgba(&screen.buffer[i])
                };
                assert_eq!(
                    *actual, expected,
                    "width={width}, pixel={i}, partial={partial}"
                );
            }
            screen.diagnostic = false;
            for (presenting, overlay) in [
                (false, false),
                (true, false),
                (true, true),
                (true, false),
                (false, false),
                (true, true),
                (false, true),
            ] {
                screen.presentation = Presentation::for_viewer(presenting, overlay);
                compose(&sdl, &screen).unwrap();
                assert_eq!(
                    unsafe {
                        read_pixels(
                            screen.renderer,
                            std::ptr::null(),
                            ffi::SDL_PIXELFORMAT_ABGR8888,
                            actual.as_mut_ptr().cast(),
                            width as c_int * 4,
                        )
                    },
                    0
                );
                let expected = if presenting && !overlay {
                    vec![[0; 4]; screen.buffer.len()]
                } else {
                    source.clone()
                };
                assert_eq!(
                    actual, expected,
                    "presenting={presenting}, overlay={overlay}, width={width}"
                );
            }
            screen.presentation = Presentation::Surface;
            screen.diagnostic = true;
            assert_eq!(
                screen.buffer.iter().map(rgba).collect::<Vec<_>>(),
                source,
                "composition must not damage Slint's reused buffer"
            );
        }
        // SAFETY: live objects owned only by this test, no further rendering.
        unsafe {
            assert_eq!(set_target(screen.renderer, std::ptr::null_mut()), 0);
            (sdl.SDL_DestroyTexture)(target);
            (sdl.SDL_DestroyTexture)(screen.texture);
            (sdl.SDL_DestroyRenderer)(screen.renderer);
            (sdl.SDL_DestroyWindow)(screen.window);
        }
    }
    // SAFETY: every SDL object has been destroyed.
    unsafe { (sdl.SDL_Quit)() };
}

slint::slint! {
    export component PresentationProbe inherits Window {
        in property <bool> changed;
        background: changed ? #00ff00 : #ff0000;
    }
}

struct TestPlatform(Rc<SdlPlatform>);
impl slint::platform::Platform for TestPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        self.0.create_window_adapter()
    }
}

#[test]
#[ignore = "requires workstation SDL and Xvfb; never run on the TV"]
fn presenting_skips_surface_render_and_resumes_with_the_latest_view() {
    use slint::ComponentHandle;
    let platform = Rc::new(
        SdlPlatform::open(
            "Presentation test",
            Placement::Windowed {
                width: 64,
                height: 64,
            },
        )
        .unwrap(),
    );
    slint::platform::set_platform(Box::new(TestPlatform(platform.clone()))).unwrap();
    let view = PresentationProbe::new().unwrap();
    view.show().unwrap();
    platform.render().unwrap();
    let before = platform.screen.borrow().buffer.clone();
    let frames = platform.stats.borrow().frames;
    set_presentation(Presentation::for_viewer(true, false));
    platform.render().unwrap();
    view.set_changed(true);
    platform.render().unwrap();
    assert_eq!(
        platform.stats.borrow().frames,
        frames,
        "no surface renders during stream"
    );
    assert!(platform
        .screen
        .borrow()
        .buffer
        .iter()
        .zip(&before)
        .all(|(a, b)| rgba(a) == rgba(b)));
    set_presentation(Presentation::for_viewer(true, true));
    platform.render().unwrap();
    let screen = platform.screen.borrow();
    let pixel = &screen.buffer[screen.buffer.len() / 2];
    assert_eq!(
        rgba(pixel),
        [0, 255, 0, 255],
        "overlay renders the latest view"
    );
    drop(screen);
    set_presentation(Presentation::Stream);
    platform.render().unwrap();
    view.set_changed(false);
    set_presentation(Presentation::Surface);
    platform.render().unwrap();
    let screen = platform.screen.borrow();
    assert_eq!(
        rgba(&screen.buffer[screen.buffer.len() / 2]),
        [255, 0, 0, 255]
    );
    drop(screen);
    platform.close();
}
