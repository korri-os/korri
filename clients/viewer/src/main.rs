// SPDX-License-Identifier: GPL-3.0-or-later
use anyhow::Result;
use korri_viewer::{launch, lifecycle, options::Options};
// The binary's licence (ADR 0059). Its own code is GPL-3.0-or-later. With
// nightjar-core, which is GPL-3.0-only (vendor/README.md), the binary is
// GPL-3.0-only: the x86_64 package builds it (#391). The aarch64 package
// does not, and its check reads this first line as "(GPL-3.0-or-later)".
#[cfg(feature = "nightjar")]
const LICENCE: &str =
    "GPL-3.0-only: GPL-3.0-or-later code with nightjar-core, which is GPL-3.0-only";
#[cfg(not(feature = "nightjar"))]
const LICENCE: &str = "GPL-3.0-or-later";
fn run() -> Result<()> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    if matches!(
        args.first().map(String::as_str),
        Some("--help" | "-help" | "--version")
    ) {
        println!(
            "korri-viewer 0.1.0 ({LICENCE})\nUsage: korri-viewer stream|pair [Moonlight arguments] HOST\nPC picture, sound and input. -viewonly disables input device discovery; the window's own keyboard and mouse still forward, as in Moonlight."
        );
        #[cfg(v4l2_handheld)]
        println!(
            "Handheld picture: -platform v4l2request or -platform v4l2m2m, with -input <device> up to 6 times."
        );
        return Ok(());
    }
    let lifecycle = lifecycle::from_args(&mut args)?;
    let options = Options::parse(args)?;
    let pin = std::env::var_os("KORRI_MOONLIGHT_SERVER_CERTIFICATE").map(Into::into);
    launch::run(options, lifecycle.as_ref(), pin)
}
fn main() {
    if let Err(error) = run() {
        eprintln!("korri-viewer: {error:#}");
        std::process::exit(1);
    }
}
