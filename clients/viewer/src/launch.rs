// SPDX-License-Identifier: GPL-3.0-or-later
//! One run of the viewer: a pairing, or a stream under an optional lifecycle.
//! The Linux command and the Android app run the same steps.
use crate::{
    lifecycle::{self, Lifecycle},
    options::Options,
    pair,
    session::{Identity, Transport},
    stream,
};
use anyhow::{Context, Result};
use std::path::PathBuf;

/// With a lifecycle: wait for Start, connect, stream until a stop, then
/// report Stopped or the failure to the controller. `pin` is the authorized
/// server certificate (`KORRI_MOONLIGHT_SERVER_CERTIFICATE`); its absence is
/// a failure after Start, so the controller hears of it.
pub fn run(
    mut options: Options,
    lifecycle: Option<&Lifecycle>,
    pin: Option<PathBuf>,
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
            stream::run_with_lifecycle(&mut transport, &options, lifecycle)
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
