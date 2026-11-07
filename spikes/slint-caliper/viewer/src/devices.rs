//! The devices a state is drawn for. Caliper's own form (src/client/device-frame.js,
//! `Device`: id, name, widthMm, heightMm, cssWidth, cssHeight, viewportNote), kept in
//! code: Korri has no device list file, so where this table lives is unresolved.
//!
//! Every device here runs its compositor at scale 1, so its CSS size is its panel's
//! pixel size: the shell draws `css_width x css_height` device pixels.

pub struct Device {
    pub id: &'static str,
    pub name: &'static str,
    pub width_mm: f32,
    pub height_mm: f32,
    pub css_width: u32,
    pub css_height: u32,
    /// Where each number comes from.
    pub note: &'static str,
    /// The millimetres have no maker's figure behind them.
    pub mm_unresolved: bool,
}

pub const DEVICES: &[Device] = &[
    Device {
        id: "rg35xxsp",
        name: "RG35XX SP",
        // 3.5 in diagonal, 4:3: 88.9 mm x 4/5 and x 3/5.
        width_mm: 71.12,
        height_mm: 53.34,
        css_width: 640,
        css_height: 480,
        note: "640 x 480 at scale 1: nix/devices/h700/board.nix (DSI-1 640x480@60Hz, no scale). \
               \"3.5-inch IPS ... 640*480\": anbernic.com/products/rg35xxsp (read 2026-11-07), also nix/devices/h700/README.md. \
               Millimetres from the diagonal and square pixels.",
        mm_unresolved: false,
    },
    Device {
        id: "rg35xxpro",
        name: "RG35XX Pro",
        width_mm: 71.12,
        height_mm: 53.34,
        css_width: 640,
        css_height: 480,
        note: "640 x 480 at scale 1: nix/devices/h700/board.nix (DSI-1 640x480@60Hz, no scale). \
               \"3.5-inch IPS ... 640*480\": anbernic.com/products/rg35xxpro (read 2026-11-07), also nix/devices/h700/README.md. \
               Millimetres from the diagonal and square pixels.",
        mm_unresolved: false,
    },
    Device {
        id: "rpminiv2",
        name: "RP Mini V2",
        width_mm: 75.0,
        height_mm: 65.0,
        css_width: 1240,
        css_height: 1080,
        note: "1240 x 1080 at scale 1: nix/devices/rpminiv2/hardware.nix (DSI-1 1080x1240, transform 90, scale 1). \
               75 x 65 mm: Pico's Caliper device (surfaces/pico/vite.config.ts at 28e385db3^), \
               which says inferred from a 3.92 in diagonal. Retroid's own page could not be read \
               (goretroid.com answered 429, 2026-11-07): the millimetres are unresolved.",
        mm_unresolved: true,
    },
    Device {
        id: "rg353m",
        name: "RG353M",
        width_mm: 72.0,
        height_mm: 52.0,
        css_width: 640,
        css_height: 480,
        note: "Caliper's rg353m (src/client/device-frame.js): 72 x 52 mm, 640 x 480 at scale 1. \
               Korri: nix/devices/rg353m/hardware.nix (DSI-1 640x480@60Hz).",
        mm_unresolved: false,
    },
    Device {
        id: "odin2portal",
        name: "Odin 2 Portal",
        width_mm: 156.0,
        height_mm: 85.0,
        css_width: 1920,
        css_height: 1080,
        note: "Caliper's odin2portal (src/client/device-frame.js): 156 x 85 mm, 1920 x 1080 at scale 1. \
               Korri: nix/devices/odin2portal/hardware.nix (DSI-1 1080x1920@120Hz, transform 270, no scale).",
        mm_unresolved: false,
    },
];

pub fn index_of(id: &str) -> Option<usize> {
    DEVICES.iter().position(|d| d.id == id)
}
