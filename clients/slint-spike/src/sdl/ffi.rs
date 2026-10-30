//! Minimal run-time bindings to an existing `libSDL2-2.0.so.0`.
//!
//! The binary links no SDL. It loads the library that the system already has
//! (the LG B7 ships `/usr/lib/libSDL2-2.0.so.0.4.0`) with `dlopen`, so the
//! cross build needs no SDL headers or stubs.
//!
//! Every function, constant and struct offset here exists in SDL 2.0.4. They
//! were read from the SDL `release-2.0.4` tag: `include/SDL_events.h`,
//! `SDL_video.h`, `SDL_render.h`, `SDL_pixels.h`, `SDL_scancode.h`,
//! `SDL_keycode.h`, `SDL_keyboard.h`, `SDL_hints.h`, and the export list
//! `src/dynapi/SDL_dynapi_procs.h`. `sdl-abi-check.sh` compares the names in
//! `sdl_functions!` below with that export list.

#![allow(non_snake_case)]

use std::ffi::{c_char, c_int, c_void, CStr, CString};

/// The SONAME every SDL 2.x build carries.
pub const LIBRARY: &str = "libSDL2-2.0.so.0";

// SDL_Init flags (SDL.h).
pub const SDL_INIT_VIDEO: u32 = 0x0000_0020;

// SDL_WindowFlags (SDL_video.h).
pub const SDL_WINDOW_SHOWN: u32 = 0x0000_0004;
pub const SDL_WINDOW_RESIZABLE: u32 = 0x0000_0020;
pub const SDL_WINDOW_FULLSCREEN_DESKTOP: u32 = 0x0000_1001;
pub const SDL_WINDOWPOS_UNDEFINED: c_int = 0x1FFF_0000;

// SDL_RendererFlags and SDL_TextureAccess (SDL_render.h).
pub const SDL_RENDERER_ACCELERATED: u32 = 0x0000_0002;
pub const SDL_TEXTUREACCESS_STREAMING: c_int = 1;
// SDL_BlendMode (SDL_blendmode.h).
pub const SDL_BLENDMODE_NONE: c_int = 0;

/// `SDL_PIXELFORMAT_ABGR8888` (SDL_pixels.h): a native-endian u32 with A in
/// the high byte. On little-endian ARM and x86 the bytes are R, G, B, A,
/// which is Slint's `PremultipliedRgbaColor` layout. The GLES2 renderer
/// takes it without conversion.
pub const SDL_PIXELFORMAT_ABGR8888: u32 = 0x1676_2004;

// SDL_EventType (SDL_events.h).
pub const SDL_QUIT: u32 = 0x100;
pub const SDL_APP_TERMINATING: u32 = 0x101;
pub const SDL_APP_LOWMEMORY: u32 = 0x102;
pub const SDL_APP_WILLENTERBACKGROUND: u32 = 0x103;
pub const SDL_APP_DIDENTERBACKGROUND: u32 = 0x104;
pub const SDL_APP_WILLENTERFOREGROUND: u32 = 0x105;
pub const SDL_APP_DIDENTERFOREGROUND: u32 = 0x106;
pub const SDL_WINDOWEVENT: u32 = 0x200;
pub const SDL_KEYDOWN: u32 = 0x300;
pub const SDL_KEYUP: u32 = 0x301;
pub const SDL_MOUSEMOTION: u32 = 0x400;
pub const SDL_MOUSEBUTTONDOWN: u32 = 0x401;
pub const SDL_MOUSEBUTTONUP: u32 = 0x402;
pub const SDL_MOUSEWHEEL: u32 = 0x403;
pub const SDL_RENDER_TARGETS_RESET: u32 = 0x2000;
pub const SDL_RENDER_DEVICE_RESET: u32 = 0x2001;

// SDL_WindowEventID (SDL_video.h).
pub const SDL_WINDOWEVENT_SHOWN: u8 = 1;
pub const SDL_WINDOWEVENT_HIDDEN: u8 = 2;
pub const SDL_WINDOWEVENT_EXPOSED: u8 = 3;
pub const SDL_WINDOWEVENT_SIZE_CHANGED: u8 = 6;
pub const SDL_WINDOWEVENT_MINIMIZED: u8 = 7;
pub const SDL_WINDOWEVENT_RESTORED: u8 = 9;
pub const SDL_WINDOWEVENT_LEAVE: u8 = 11;
pub const SDL_WINDOWEVENT_FOCUS_GAINED: u8 = 12;
pub const SDL_WINDOWEVENT_FOCUS_LOST: u8 = 13;
pub const SDL_WINDOWEVENT_CLOSE: u8 = 14;

// SDL_BUTTON_* (SDL_mouse.h).
pub const SDL_BUTTON_LEFT: u8 = 1;
pub const SDL_BUTTON_MIDDLE: u8 = 2;
pub const SDL_BUTTON_RIGHT: u8 = 3;

// Hint names (SDL_hints.h, all present in 2.0.4).
/// SDL 2.0.4 `SDL_VideoInit` disables the screen saver unless this is "1"
/// (src/video/SDL_video.c). Korri must never hold off the OLED protections.
pub const SDL_HINT_VIDEO_ALLOW_SCREENSAVER: &str = "SDL_VIDEO_ALLOW_SCREENSAVER";
/// The process blocks SIGINT/SIGTERM itself and waits for them on one thread.
pub const SDL_HINT_NO_SIGNAL_HANDLERS: &str = "SDL_NO_SIGNAL_HANDLERS";
/// LG `libsdl2-webos` only (string present in the B7's library; webosbrew
/// SDL-webOS sets the shell property `_WEBOS_ACCESS_POLICY_KEYS_BACK` from
/// it). Upstream SDL ignores an unknown hint.
pub const SDL_HINT_WEBOS_ACCESS_POLICY_KEYS_BACK: &str = "SDL_WEBOS_ACCESS_POLICY_KEYS_BACK";

/// `SDL_Event` is a 56-byte union in SDL 2.0.4 (`Uint8 padding[56]`).
/// Fields are read at the offsets of the 2.0.4 structs.
#[repr(C, align(8))]
pub struct Event(pub [u8; 56]);

impl Event {
    pub fn zeroed() -> Self {
        Self([0; 56])
    }
    fn u32_at(&self, offset: usize) -> u32 {
        u32::from_ne_bytes(self.0[offset..offset + 4].try_into().unwrap())
    }
    fn i32_at(&self, offset: usize) -> i32 {
        i32::from_ne_bytes(self.0[offset..offset + 4].try_into().unwrap())
    }
    fn u16_at(&self, offset: usize) -> u16 {
        u16::from_ne_bytes(self.0[offset..offset + 2].try_into().unwrap())
    }
    /// `type` is the first `Uint32` of every event struct.
    pub fn kind(&self) -> u32 {
        self.u32_at(0)
    }
    pub fn set_kind(&mut self, kind: u32) {
        self.0[0..4].copy_from_slice(&kind.to_ne_bytes());
    }
    /// SDL_WindowEvent: event @12, data1 @16, data2 @20.
    pub fn window(&self) -> (u8, i32, i32) {
        (self.0[12], self.i32_at(16), self.i32_at(20))
    }
    /// SDL_KeyboardEvent: repeat @13, keysym @16 = scancode, sym @20, mod @24.
    pub fn key(&self) -> Key {
        Key {
            repeat: self.0[13] != 0,
            scancode: self.i32_at(16),
            keycode: self.i32_at(20),
            modifiers: self.u16_at(24),
        }
    }
    /// SDL_MouseMotionEvent: x @20, y @24.
    pub fn motion(&self) -> (i32, i32) {
        (self.i32_at(20), self.i32_at(24))
    }
    /// SDL_MouseButtonEvent: button @16, x @20, y @24.
    pub fn button(&self) -> (u8, i32, i32) {
        (self.0[16], self.i32_at(20), self.i32_at(24))
    }
    /// SDL_MouseWheelEvent: x @16, y @20, direction @24 (1 = flipped).
    pub fn wheel(&self) -> (i32, i32) {
        let (x, y) = (self.i32_at(16), self.i32_at(20));
        if self.u32_at(24) == 1 {
            (-x, -y)
        } else {
            (x, y)
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Key {
    pub repeat: bool,
    pub scancode: i32,
    pub keycode: i32,
    pub modifiers: u16,
}

/// `SDL_version` (SDL_version.h).
#[repr(C)]
#[derive(Default)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}

/// `SDL_DisplayMode` (SDL_video.h).
#[repr(C)]
pub struct DisplayMode {
    pub format: u32,
    pub w: c_int,
    pub h: c_int,
    pub refresh_rate: c_int,
    pub driverdata: *mut c_void,
}

/// `SDL_RendererInfo` (SDL_render.h).
#[repr(C)]
pub struct RendererInfo {
    pub name: *const c_char,
    pub flags: u32,
    pub num_texture_formats: u32,
    pub texture_formats: [u32; 16],
    pub max_texture_width: c_int,
    pub max_texture_height: c_int,
}

/// `SDL_Rect` (SDL_rect.h).
#[repr(C)]
pub struct Rect {
    pub x: c_int,
    pub y: c_int,
    pub w: c_int,
    pub h: c_int,
}

pub type Window = c_void;
pub type Renderer = c_void;
pub type Texture = c_void;

macro_rules! sdl_functions {
    ( $( $name:ident ( $( $arg:ty ),* ) $( -> $ret:ty )? ; )* ) => {
        /// Function pointers resolved from the loaded library. Plain `fn`
        /// pointers, so the table is `Send + Sync`.
        pub struct Sdl {
            $( pub $name: unsafe extern "C" fn( $( $arg ),* ) $( -> $ret )?, )*
        }

        impl Sdl {
            /// Load `LIBRARY` and resolve every function, or say which one is
            /// missing. The library stays loaded for the life of the process.
            pub fn load() -> Result<Self, String> {
                let name = CString::new(LIBRARY).unwrap();
                // SAFETY: dlopen with a valid C string; the handle is never closed.
                let handle = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
                if handle.is_null() {
                    return Err(format!("{LIBRARY} could not be loaded: {}", dl_error()));
                }
                Ok(Self {
                    $( $name: {
                        let symbol = concat!(stringify!($name), "\0");
                        // SAFETY: dlsym with a NUL-terminated name on a live handle.
                        let pointer = unsafe { libc::dlsym(handle, symbol.as_ptr().cast()) };
                        if pointer.is_null() {
                            return Err(format!("{LIBRARY} has no {}", stringify!($name)));
                        }
                        // SAFETY: the signature matches the SDL 2.0.4 declaration.
                        unsafe { std::mem::transmute::<*mut c_void, unsafe extern "C" fn( $( $arg ),* ) $( -> $ret )?>(pointer) }
                    }, )*
                })
            }
        }
    };
}

sdl_functions! {
    SDL_GetVersion(*mut Version);
    SDL_SetHint(*const c_char, *const c_char) -> c_int;
    SDL_Init(u32) -> c_int;
    SDL_Quit();
    SDL_GetError() -> *const c_char;
    SDL_GetCurrentVideoDriver() -> *const c_char;
    SDL_EnableScreenSaver();
    SDL_IsScreenSaverEnabled() -> c_int;
    SDL_StopTextInput();
    SDL_GetDesktopDisplayMode(c_int, *mut DisplayMode) -> c_int;
    SDL_CreateWindow(*const c_char, c_int, c_int, c_int, c_int, u32) -> *mut Window;
    SDL_DestroyWindow(*mut Window);
    SDL_GetWindowSize(*mut Window, *mut c_int, *mut c_int);
    SDL_CreateRenderer(*mut Window, c_int, u32) -> *mut Renderer;
    SDL_DestroyRenderer(*mut Renderer);
    SDL_GetRendererInfo(*mut Renderer, *mut RendererInfo) -> c_int;
    SDL_GetRendererOutputSize(*mut Renderer, *mut c_int, *mut c_int) -> c_int;
    SDL_CreateTexture(*mut Renderer, u32, c_int, c_int, c_int) -> *mut Texture;
    SDL_DestroyTexture(*mut Texture);
    SDL_SetTextureBlendMode(*mut Texture, c_int) -> c_int;
    SDL_UpdateTexture(*mut Texture, *const Rect, *const c_void, c_int) -> c_int;
    SDL_RenderCopy(*mut Renderer, *mut Texture, *const Rect, *const Rect) -> c_int;
    SDL_RenderPresent(*mut Renderer);
    SDL_RegisterEvents(c_int) -> u32;
    SDL_PushEvent(*mut Event) -> c_int;
    SDL_PollEvent(*mut Event) -> c_int;
    SDL_WaitEventTimeout(*mut Event, c_int) -> c_int;
    SDL_GetKeyboardFocus() -> *mut Window;
}

impl Sdl {
    pub fn error(&self) -> String {
        // SAFETY: SDL_GetError returns a valid, possibly empty, C string.
        unsafe { text((self.SDL_GetError)()) }
    }

    pub fn set_hint(&self, name: &str, value: &str) -> bool {
        let (name, value) = (CString::new(name).unwrap(), CString::new(value).unwrap());
        // SAFETY: two valid C strings; SDL copies them.
        unsafe { (self.SDL_SetHint)(name.as_ptr(), value.as_ptr()) != 0 }
    }
}

/// Copy a C string SDL owns; null gives "".
///
/// # Safety
/// `pointer` is null or a valid NUL-terminated string.
pub unsafe fn text(pointer: *const c_char) -> String {
    if pointer.is_null() {
        String::new()
    } else {
        CStr::from_ptr(pointer).to_string_lossy().into_owned()
    }
}

fn dl_error() -> String {
    // SAFETY: dlerror returns null or a valid C string.
    unsafe { text(libc::dlerror()) }
}
