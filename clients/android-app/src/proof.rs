// SPDX-License-Identifier: GPL-3.0-or-later
//! The emulator proof's trigger for the stream's controls (#434, slice
//! D11). Only the fixture APK carries it: its library is built with the
//! cargo feature `proof`, and the product library's build checks that none
//! of this is in it (`default.nix`).
//!
//! korrid reads the viewer's snapshot itself while a stream runs (its
//! live-shape poll), but only its gameplay overlay calls Apply (the shell's
//! `InvokeGameplayControl`), and in this app that overlay comes with slice
//! D5b. Until then the proof writes one command at a time into
//! `<cache>/proof-command` with `run-as`, and this thread runs it against
//! korrid's own viewer adapter, the object that korrid holds
//! (`viewer::korrids_adapter`), and logs the answer as the treaty's JSON:
//!
//! | Command | Call |
//! | --- | --- |
//! | `snapshot LAUNCH` | `LinuxStreamViewer::snapshot` |
//! | `apply LAUNCH CONTROL toggle true\|false` | `LinuxStreamViewer::apply` with a toggle |
//! | `apply LAUNCH CONTROL choice VALUE` | with a choice |
//! | `apply LAUNCH CONTROL range NUMBER` | with a range value |
//! | `apply LAUNCH CONTROL none` | with no value (a command) |
//! | `resize LAUNCH WIDTH HEIGHT` | korrid's resize request to the holder (#476 slice 3, `android_app::proof_resize_stream`) |
//!
//! The resize answer is the holder's: `Applied`, `Refused` or `Unavailable`.
//! It stands in for korrid's Portrait control, which asks the same request
//! only once the viewer reports that it can present a new size.
use crate::{logcat, viewer};
use korri_viewer_treaty::SurfaceGameplayControlValue as Value;
use korrid::linux_viewer::LinuxStreamViewer;
use std::{path::PathBuf, sync::Once, thread, time::Duration};

const COMMAND: &str = "proof-command";
const LOG: &str = "korri-app: proof:";

/// Watch for the proof's commands, once per process.
pub fn watch(cache: PathBuf) {
    static STARTED: Once = Once::new();
    STARTED.call_once(|| {
        let path = cache.join(COMMAND);
        let spawned = thread::Builder::new()
            .name("korri-proof".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(cause) => {
                        logcat::error(&format!("{LOG} no runtime: {cause}"));
                        return;
                    }
                };
                logcat::info(&format!("{LOG} watching {}", path.display()));
                loop {
                    thread::sleep(Duration::from_millis(250));
                    let Ok(text) = std::fs::read_to_string(&path) else {
                        continue;
                    };
                    let _ = std::fs::remove_file(&path);
                    run(&runtime, text.trim());
                }
            });
        if let Err(cause) = spawned {
            logcat::error(&format!("{LOG} the watcher did not start: {cause}"));
        }
    });
}

fn run(runtime: &tokio::runtime::Runtime, command: &str) {
    if let ["resize", launch, width, height] =
        command.split_whitespace().collect::<Vec<_>>().as_slice()
    {
        let (Ok(width), Ok(height)) = (width.parse(), height.parse()) else {
            return logcat::error(&format!("{LOG} not a size: {command}"));
        };
        let display = korrid::launcher::plugin_launch::PluginDisplay { width, height };
        return match korrid::android_app::proof_resize_stream(launch, display) {
            Ok(answer) => logcat::info(&format!(
                "{LOG} resize {launch} {width}x{height} {answer:?}"
            )),
            Err(cause) => logcat::error(&format!(
                "{LOG} resize {launch} {width}x{height} failed: {cause}"
            )),
        };
    }
    let Some(adapter) = viewer::korrids_adapter() else {
        return logcat::error(&format!("{LOG} korrid holds no viewer adapter yet"));
    };
    let words: Vec<&str> = command.split_whitespace().collect();
    match words.as_slice() {
        ["snapshot", launch] => match runtime.block_on(adapter.snapshot(launch)) {
            Ok(snapshot) => logcat::info(&format!(
                "{LOG} snapshot {launch} {}",
                serde_json::to_string(&snapshot).unwrap_or_default()
            )),
            Err(cause) => logcat::error(&format!("{LOG} snapshot {launch} failed: {cause}")),
        },
        ["apply", launch, control, value @ ..] => {
            let value = match value {
                ["toggle", "true"] => Some(Value::Toggle { value: true }),
                ["toggle", "false"] => Some(Value::Toggle { value: false }),
                ["choice", choice] => Some(Value::Choice {
                    value: (*choice).to_owned(),
                }),
                ["range", number] => match number.parse() {
                    Ok(value) => Some(Value::Range { value }),
                    Err(_) => return logcat::error(&format!("{LOG} not a number: {command}")),
                },
                ["none"] => None,
                _ => return logcat::error(&format!("{LOG} no such value: {command}")),
            };
            let asked = serde_json::to_string(&value).unwrap_or_default();
            match runtime.block_on(adapter.apply(launch, control, value)) {
                Ok(()) => logcat::info(&format!("{LOG} apply {launch} {control} {asked} ok")),
                Err(cause) => logcat::error(&format!(
                    "{LOG} apply {launch} {control} {asked} failed: {cause}"
                )),
            }
        }
        _ => logcat::error(&format!("{LOG} no such command: {command}")),
    }
}
