// SPDX-License-Identifier: GPL-3.0-or-later
use anyhow::{Context, Result};
use korri_viewer::{
    options::Options,
    pair,
    session::{Identity, Transport},
    stream,
};
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if matches!(
        args.first().map(String::as_str),
        Some("--help" | "-help" | "--version")
    ) {
        println!(
            "korri-viewer 0.1.0 (GPL-3.0-or-later)\nUsage: korri-viewer stream|pair [Moonlight arguments] HOST\nPC picture paths only. Input and audio output are not included."
        );
        return Ok(());
    }
    let options = Options::parse(args)?;
    let pin = std::env::var_os("KORRI_MOONLIGHT_SERVER_CERTIFICATE")
        .context("KORRI_MOONLIGHT_SERVER_CERTIFICATE is required")?;
    let identity = if options.action == "pair" {
        Identity::load_or_generate(&options.keydir)?
    } else {
        Identity::load(&options.keydir)?
    };
    let mut transport = Transport::new(
        &options.host,
        options.port,
        &options.keydir,
        std::path::Path::new(&pin),
    )?;
    transport.discover_port()?;
    if options.action == "pair" {
        pair::pair(&mut transport, &identity)
    } else {
        stream::run(&mut transport, &options)
    }
}
fn main() {
    if let Err(error) = run() {
        eprintln!("korri-viewer: {error:#}");
        std::process::exit(1);
    }
}
