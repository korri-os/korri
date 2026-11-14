// SPDX-License-Identifier: GPL-3.0-or-later
//! One run of the viewer: a pairing, or a stream under an optional lifecycle.
//! The Linux command and the Android app run the same steps.
use crate::{
    lifecycle::{self, Lifecycle},
    options::Options,
    pair,
    player_io::Platform,
    session::{Identity, Transport},
    stream,
};
use anyhow::{Context, Result};
use std::path::PathBuf;

/// With a lifecycle: wait for Start, connect, stream until a stop, then
/// report Stopped or the failure to the controller. `pin` is the authorized
/// server certificate (`KORRI_MOONLIGHT_SERVER_CERTIFICATE`); its absence is
/// a failure after Start, so the controller hears of it.
#[allow(
    clippy::default_constructed_unit_structs,
    reason = "Platform is a unit on Linux only; Android's is an Option"
)]
pub fn run(options: Options, lifecycle: Option<&Lifecycle>, pin: Option<PathBuf>) -> Result<()> {
    run_on(options, lifecycle, pin, Platform::default())
}

/// The same run on Android, with the picture that the app gives.
#[cfg(target_os = "android")]
pub fn run_with_picture(
    options: Options,
    lifecycle: Option<&Lifecycle>,
    pin: Option<PathBuf>,
    picture: crate::picture::Picture,
) -> Result<()> {
    run_on(options, lifecycle, pin, Some(picture))
}

fn run_on(
    mut options: Options,
    lifecycle: Option<&Lifecycle>,
    pin: Option<PathBuf>,
    platform: Platform,
) -> Result<()> {
    let result = (|| {
        if let Some(lifecycle) = lifecycle {
            anyhow::ensure!(
                options.action == "stream",
                "lifecycle socket requires stream"
            );
            let Some(start) = lifecycle.wait_start()? else {
                // Stopped before Start: nothing to connect or tear down.
                return Ok(());
            };
            start.configure(&mut options)?;
        }
        let pin = pin.context("KORRI_MOONLIGHT_SERVER_CERTIFICATE is required")?;
        let identity = if options.action == "pair" {
            Identity::load_or_generate(&options.keydir)?
        } else {
            Identity::load(&options.keydir)?
        };
        let mut transport = Transport::new(&options.host, options.port, &options.keydir, &pin)?;
        if let Some(lifecycle) = lifecycle {
            lifecycle.ensure_running()?;
            let lifecycle = lifecycle.clone();
            transport.interrupt_when(move || lifecycle.ensure_running());
        }
        transport.discover_port()?;
        if options.action == "pair" {
            pair::pair(&mut transport, &identity)
        } else {
            stream::run_with_lifecycle(&mut transport, &options, lifecycle, platform)
        }
    })();
    // A stop between Start and the stream ends the launch as requested.
    let result = result.or_else(|error| {
        if error.is::<lifecycle::Stopping>() {
            Ok(())
        } else {
            Err(error)
        }
    });
    if let Some(lifecycle) = lifecycle {
        match &result {
            Ok(()) => lifecycle.stopped(),
            Err(error) => lifecycle.failed(format!("{error:#}")),
        }
        lifecycle.drain();
    }
    result
}
