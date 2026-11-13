// SPDX-License-Identifier: GPL-3.0-or-later
use anyhow::{Context, Result};
use korri_viewer::{
    lifecycle,
    options::Options,
    pair,
    session::{Identity, Transport},
    stream,
};
fn run() -> Result<()> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    if matches!(
        args.first().map(String::as_str),
        Some("--help" | "-help" | "--version")
    ) {
        println!(
            "korri-viewer 0.1.0 (GPL-3.0-or-later)\nUsage: korri-viewer stream|pair [Moonlight arguments] HOST\nPC picture paths only. Input and audio output are not included."
        );
        return Ok(());
    }
    let lifecycle = lifecycle::from_args(&mut args)?;
    let mut options = Options::parse(args)?;
    let result = (|| {
        if let Some(lifecycle) = &lifecycle {
            anyhow::ensure!(
                options.action == "stream",
                "lifecycle socket requires stream"
            );
            lifecycle.wait_start()?.configure(&mut options)?;
        }
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
            stream::run_with_lifecycle(&mut transport, &options, lifecycle.as_ref())
        }
    })();
    if let Some(lifecycle) = &lifecycle {
        match &result {
            Ok(()) => lifecycle.stopped(),
            Err(error) => lifecycle.failed(format!("{error:#}")),
        }
        lifecycle.drain();
    }
    result
}
fn main() {
    if let Err(error) = run() {
        eprintln!("korri-viewer: {error:#}");
        std::process::exit(1);
    }
}
