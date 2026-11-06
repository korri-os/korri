//! The shell's notice after it unloads a surface (`src/notice.slint`, contract
//! amendment A5), drawn headless at the five sizes every surface must work at:
//! the column fits at its preferred size, so no text or action is squeezed or
//! clipped, also with the longest surface id (64 characters, one word).
//!
//! `NOTICE_PREVIEW_OUT=DIR` writes every frame as a PNG there.

use korri_shell::{
    backend::headless::Headless,
    notice::{self, Notice, Reason},
};
use slint::ComponentHandle;
use slint_interpreter::Value;

const SIZES: [(u32, u32); 5] = [
    (320, 240),
    (640, 480),
    (480, 800),
    (1280, 720),
    (1920, 1080),
];

fn save(pixels: &[u16], width: u32, height: u32, name: &str) {
    let Some(dir) = std::env::var_os("NOTICE_PREVIEW_OUT") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut rgb = Vec::with_capacity(pixels.len() * 3);
    for p in pixels {
        let (r, g, b) = ((p >> 11) & 31, (p >> 5) & 63, p & 31);
        rgb.extend([
            (r << 3 | r >> 2) as u8,
            (g << 2 | g >> 4) as u8,
            (b << 3 | b >> 2) as u8,
        ]);
    }
    image::save_buffer(
        dir.join(format!("{name}.{width}x{height}.png")),
        &rgb,
        width,
        height,
        image::ColorType::Rgb8,
    )
    .unwrap();
}

#[test]
fn the_notice_fits_unclipped_at_every_size() {
    Headless::install();
    let definition = notice::compile().unwrap();
    let long = "a".repeat(64);
    let cases = [
        ("short", Notice::new("pico", Reason::Stuck, Some("zone"))),
        ("one", Notice::new("zone", Reason::Memory, None)),
        ("long", Notice::new(&long, Reason::NotStarted, Some(&long))),
    ];
    let mut wrong = Vec::new();
    for (name, notice) in &cases {
        for (width, height) in SIZES {
            let component = definition.create().unwrap();
            notice::bind(&component, notice).unwrap();
            component
                .window()
                .set_size(slint::PhysicalSize::new(width, height));
            component.show().unwrap();
            let pixels = Headless::render().expect("drawn");
            save(&pixels, width, height, name);
            assert!(
                pixels.iter().any(|&p| p != pixels[0]),
                "{name} {width}x{height} draws nothing"
            );
            if component.get_property("fits").unwrap() != Value::Bool(true) {
                wrong.push(format!("{name} at {width}x{height}"));
            }
            component.hide().unwrap();
        }
    }
    assert!(wrong.is_empty(), "squeezed or clipped: {wrong:?}");
}
