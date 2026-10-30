//! LG's Magic Remote cursor, through the compositor's own pointer timer.
//!
//! The LG menu hides the cursor on a D-pad press and shows it again when the
//! remote is shaken; LG's SDL reports the changes as scancodes 484 (shown)
//! and 485 (hidden). Moonlight TV hides it with `SDL_webOSCursorVisibility`,
//! which sends `wl_webos_input_manager.set_cursor_visibility`. The B7
//! (webOS 3.9) has no such request: its `libwayland-webos-client.so.1`
//! interface carries only `get_webos_seat` and the `cursor_visibility` event,
//! and the TV probe confirmed it (`cursor:no_method`).
//!
//! What webOS 3.9 does have is `wl_starfish_pointer.set_mrcu_standby_timer`,
//! the idle time after which the compositor hides the cursor. LG's SDL sets it
//! once from `SDL_WEBOS_CURSOR_SLEEP_TIME`; webosbrew SDL-webOS does the same
//! (`SDL_waylandvideo.c`, default 300000 ms). This module sets it on SDL's own
//! connection: almost zero on a D-pad press, so the cursor goes as soon as
//! the remote is still, and back to the default when LG shows the cursor
//! again, so a pointing hand keeps it.
//!
//! Opcodes are looked up by name in the interface LG's library exports, as
//! webosbrew SDL-webOS does (`SDL_waylandwebos_abifix.c`), so a different
//! method order cannot send the wrong request. Anything missing leaves the
//! cursor to LG.

use std::ffi::{c_char, c_int, c_void, CStr};

type Proxy = c_void;

#[repr(C)]
struct Message {
    name: *const c_char,
    signature: *const c_char,
    types: *const *const Interface,
}

#[repr(C)]
struct Interface {
    name: *const c_char,
    version: c_int,
    method_count: c_int,
    methods: *const Message,
    event_count: c_int,
    events: *const Message,
}

#[repr(C)]
struct RegistryListener {
    global: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32, *const c_char, u32),
    global_remove: unsafe extern "C" fn(*mut c_void, *mut Proxy, u32),
}

type Constructor = unsafe extern "C" fn(*mut Proxy, u32, *const Interface, ...) -> *mut Proxy;
type Marshal = unsafe extern "C" fn(*mut Proxy, u32, ...);

const WL_DISPLAY_GET_REGISTRY: u32 = 1;
const WL_REGISTRY_BIND: u32 = 0;
const STARFISH_POINTER: &CStr = c"wl_starfish_pointer";
/// webosbrew SDL-webOS's default `webos_cursor_sleep_time`, in ms.
const DEFAULT_STANDBY_MS: u32 = 300_000;
/// Hide as soon as the remote is still. Not zero, which may mean "never".
const DPAD_STANDBY_MS: u32 = 1;

/// Why the request is not available, as a fixed label for the probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unavailable {
    /// A Wayland library or symbol is missing.
    Library,
    /// The interface has no `set_mrcu_standby_timer(uint)`.
    Method,
    /// SDL gave no Wayland display, or the roundtrip failed.
    Display,
    /// The compositor advertises no `wl_starfish_pointer`.
    Global,
}

pub struct WebosCursor {
    display: *mut Proxy,
    pointer: *mut Proxy,
    opcode: u32,
    marshal: Marshal,
    flush: unsafe extern "C" fn(*mut Proxy) -> c_int,
}

/// Filled by the registry listener during the one roundtrip.
struct Found {
    name: u32,
}

unsafe extern "C" fn global(
    data: *mut c_void,
    _registry: *mut Proxy,
    name: u32,
    interface: *const c_char,
    _version: u32,
) {
    if !interface.is_null() && CStr::from_ptr(interface) == STARFISH_POINTER {
        *(data as *mut Option<Found>) = Some(Found { name });
    }
}

unsafe extern "C" fn global_remove(_: *mut c_void, _: *mut Proxy, _: u32) {}

static LISTENER: RegistryListener = RegistryListener {
    global,
    global_remove,
};

unsafe fn symbol(handle: *mut c_void, name: &CStr) -> Result<*mut c_void, Unavailable> {
    let pointer = libc::dlsym(handle, name.as_ptr());
    if pointer.is_null() {
        Err(Unavailable::Library)
    } else {
        Ok(pointer)
    }
}

impl WebosCursor {
    /// Bind `wl_starfish_pointer` on `display`, SDL's `wl_display`.
    pub fn open(display: *mut c_void) -> Result<Self, Unavailable> {
        use Unavailable::*;
        if display.is_null() {
            return Err(Display);
        }
        // SAFETY: dlopen/dlsym with NUL-terminated names; handles are never
        // closed; every pointer is checked before use; the transmuted
        // signatures match wayland-client-core.h and the generated
        // wl_starfish_pointer client header. `display` is SDL's live
        // wl_display, used from the thread that runs SDL's event loop.
        unsafe {
            let client = libc::dlopen(
                c"libwayland-client.so.0".as_ptr(),
                libc::RTLD_NOW | libc::RTLD_LOCAL,
            );
            let starfish = libc::dlopen(
                c"libwayland-starfish-client.so.1".as_ptr(),
                libc::RTLD_NOW | libc::RTLD_LOCAL,
            );
            if client.is_null() || starfish.is_null() {
                return Err(Library);
            }
            let roundtrip: unsafe extern "C" fn(*mut Proxy) -> c_int =
                std::mem::transmute(symbol(client, c"wl_display_roundtrip")?);
            let flush: unsafe extern "C" fn(*mut Proxy) -> c_int =
                std::mem::transmute(symbol(client, c"wl_display_flush")?);
            let add_listener: unsafe extern "C" fn(
                *mut Proxy,
                *const RegistryListener,
                *mut c_void,
            ) -> c_int = std::mem::transmute(symbol(client, c"wl_proxy_add_listener")?);
            let destroy: unsafe extern "C" fn(*mut Proxy) =
                std::mem::transmute(symbol(client, c"wl_proxy_destroy")?);
            let construct: Constructor =
                std::mem::transmute(symbol(client, c"wl_proxy_marshal_constructor")?);
            let marshal: Marshal = std::mem::transmute(symbol(client, c"wl_proxy_marshal")?);
            let registry_interface = symbol(client, c"wl_registry_interface")? as *const Interface;
            let pointer_interface =
                symbol(starfish, c"wl_starfish_pointer_interface")? as *const Interface;

            let interface = &*pointer_interface;
            let count = usize::try_from(interface.method_count).map_err(|_| Method)?;
            let methods = std::slice::from_raw_parts(interface.methods, count);
            let opcode = methods
                .iter()
                .position(|method| {
                    CStr::from_ptr(method.name) == c"set_mrcu_standby_timer"
                        && CStr::from_ptr(method.signature).to_bytes().ends_with(b"u")
                })
                .ok_or(Method)?;

            let registry = construct(
                display,
                WL_DISPLAY_GET_REGISTRY,
                registry_interface,
                std::ptr::null_mut::<c_void>(),
            );
            if registry.is_null() {
                return Err(Display);
            }
            let mut found: Option<Found> = None;
            add_listener(
                registry,
                &LISTENER,
                (&mut found as *mut Option<Found>).cast(),
            );
            // Also dispatches SDL's pending events; its listeners only queue
            // SDL events, which the next SDL_PollEvent delivers.
            let ok = roundtrip(display) >= 0;
            let Some(found) = found.filter(|_| ok) else {
                destroy(registry);
                return Err(if ok { Global } else { Display });
            };
            // Version 1, as webosbrew SDL-webOS binds it.
            let pointer = construct(
                registry,
                WL_REGISTRY_BIND,
                pointer_interface,
                found.name,
                interface.name,
                1u32,
                std::ptr::null_mut::<c_void>(),
            );
            // No further registry events can reach the listener's `found`.
            destroy(registry);
            if pointer.is_null() {
                return Err(Global);
            }
            Ok(Self {
                display,
                pointer,
                opcode: opcode as u32,
                marshal,
                flush,
            })
        }
    }

    /// D-pad: let the cursor sleep as soon as the remote is still.
    /// Pointer: restore the normal idle time, so a pointing hand keeps it.
    pub fn set_visible(&self, visible: bool) {
        let standby = if visible {
            DEFAULT_STANDBY_MS
        } else {
            DPAD_STANDBY_MS
        };
        // SAFETY: live proxies from `open`; the request takes one uint.
        unsafe {
            (self.marshal)(self.pointer, self.opcode, standby);
            (self.flush)(self.display);
        }
    }
}
