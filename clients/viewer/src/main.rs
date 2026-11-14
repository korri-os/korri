// SPDX-License-Identifier: GPL-3.0-or-later
use anyhow::Result;
use korri_viewer::{launch, lifecycle, options::Options};
fn run() -> Result<()> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    if matches!(
        args.first().map(String::as_str),
        Some("--help" | "-help" | "--version")
    ) {
        println!(
            "korri-viewer 0.1.0 (GPL-3.0-or-later)\nUsage: korri-viewer stream|pair [Moonlight arguments] HOST\nPC picture, sound and input. -viewonly disables input device discovery; the window's own keyboard and mouse still forward, as in Moonlight."
        );
        #[cfg(v4l2request)]
        println!("Handheld picture: -platform v4l2request, with -input <device> up to 6 times.");
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
