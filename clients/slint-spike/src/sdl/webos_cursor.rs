//! LG's pointer cursor, through the compositor's own request.
//!
//! The LG menu and Moonlight TV hide the Magic Remote cursor on a D-pad
//! press; the compositor shows it again when the remote is shaken and LG's
//! SDL reports that as scancode 484. Moonlight TV does it with
//! `SDL_webOSCursorVisibility`, which sends
//! `wl_webos_input_manager.set_cursor_visibility`. The B7's SDL 2.0.4 exports
//! no such call, and its `SDL_ShowCursor(0)` blanks the cursor so that the
//! shake no longer brings it back. So this module sends the same request on
//! its own connection to the compositor.
//!
//! The opcode is looked up by name in the interface LG's
//! `libwayland-webos-client.so.1` exports, as webosbrew SDL-webOS does
//! (`SDL_waylandwebos_abifix.c`), so a different method order cannot send the
//! wrong request. Anything missing leaves the cursor to LG.

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
const INPUT_MANAGER: &CStr = c"wl_webos_input_manager";

pub struct WebosCursor {
    display: *mut Proxy,
    manager: *mut Proxy,
    opcode: u32,
    marshal: Marshal,
    flush: unsafe extern "C" fn(*mut Proxy) -> c_int,
}

/// Filled by the registry listener during the one roundtrip.
struct Found {
    name: u32,
    version: u32,
}

unsafe extern "C" fn global(
    data: *mut c_void,
    _registry: *mut Proxy,
    name: u32,
    interface: *const c_char,
    version: u32,
) {
    if !interface.is_null() && CStr::from_ptr(interface) == INPUT_MANAGER {
        *(data as *mut Option<Found>) = Some(Found { name, version });
    }
}

unsafe extern "C" fn global_remove(_: *mut c_void, _: *mut Proxy, _: u32) {}

static LISTENER: RegistryListener = RegistryListener {
    global,
    global_remove,
};

unsafe fn symbol(handle: *mut c_void, name: &CStr) -> Option<*mut c_void> {
    let pointer = libc::dlsym(handle, name.as_ptr());
    (!pointer.is_null()).then_some(pointer)
}

impl WebosCursor {
    /// Connect and bind the input manager, or `None` when this is not
    /// webOS or anything is missing.
    pub fn open() -> Option<Self> {
        // SAFETY: dlopen/dlsym with NUL-terminated names; handles are never
        // closed; every pointer is checked before use; the transmuted
        // signatures match wayland-client-core.h and the generated
        // wl_webos_input_manager client header.
        unsafe {
            let client = libc::dlopen(
                c"libwayland-client.so.0".as_ptr(),
                libc::RTLD_NOW | libc::RTLD_LOCAL,
            );
            let webos = libc::dlopen(
                c"libwayland-webos-client.so.1".as_ptr(),
                libc::RTLD_NOW | libc::RTLD_LOCAL,
            );
            if client.is_null() || webos.is_null() {
                return None;
            }
            let connect: unsafe extern "C" fn(*const c_char) -> *mut Proxy =
                std::mem::transmute(symbol(client, c"wl_display_connect")?);
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
            let manager_interface =
                symbol(webos, c"wl_webos_input_manager_interface")? as *const Interface;

            let interface = &*manager_interface;
            let methods = std::slice::from_raw_parts(
                interface.methods,
                usize::try_from(interface.method_count).ok()?,
            );
            let opcode = methods.iter().position(|method| {
                CStr::from_ptr(method.name) == c"set_cursor_visibility"
                    && CStr::from_ptr(method.signature).to_bytes().ends_with(b"u")
            })?;

            let display = connect(std::ptr::null());
            if display.is_null() {
                return None;
            }
            let registry = construct(
                display,
                WL_DISPLAY_GET_REGISTRY,
                registry_interface,
                std::ptr::null_mut::<c_void>(),
            );
            if registry.is_null() {
                return None;
            }
            let mut found: Option<Found> = None;
            add_listener(
                registry,
                &LISTENER,
                (&mut found as *mut Option<Found>).cast(),
            );
            if roundtrip(display) < 0 {
                return None;
            }
            let found = found?;
            let version = found.version.min(u32::try_from(interface.version).ok()?);
            let manager = construct(
                registry,
                WL_REGISTRY_BIND,
                manager_interface,
                found.name,
                interface.name,
                version,
                std::ptr::null_mut::<c_void>(),
            );
            // The listener's `found` dies here; no further registry events.
            destroy(registry);
            if manager.is_null() {
                return None;
            }
            Some(Self {
                display,
                manager,
                opcode: u32::try_from(opcode).ok()?,
                marshal,
                flush,
            })
        }
    }

    /// Ask the compositor to show or hide its cursor. A shake shows it again
    /// whatever was asked last; LG's SDL then reports scancode 484.
    pub fn set_visible(&self, visible: bool) {
        // SAFETY: live proxies from `open`; the request takes one uint.
        unsafe {
            (self.marshal)(self.manager, self.opcode, u32::from(visible));
            (self.flush)(self.display);
        }
    }
}
