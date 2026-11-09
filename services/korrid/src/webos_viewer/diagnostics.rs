//! Child-owned literal diagnostics only. Never relay arbitrary dependency text.
use super::{error, LinuxViewerError};
use std::{
    io::Read,
    process::ChildStderr,
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

pub(super) struct Diagnostics {
    failure: Arc<Mutex<Option<String>>>,
    thread: Option<JoinHandle<()>>,
}
impl Diagnostics {
    pub(super) fn start(mut stderr: ChildStderr) -> Result<Self, LinuxViewerError> {
        let failure = Arc::new(Mutex::new(None));
        let observed = failure.clone();
        let thread = std::thread::Builder::new()
            .name("webos-viewer-log".into())
            .spawn(move || {
                let mut buffer = [0; 512];
                let mut line = Vec::new();
                let mut oversized = false;
                loop {
                    let n = match stderr.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    for &byte in &buffer[..n] {
                        if byte == b'\n' {
                            if !oversized {
                                if let Ok(text) = std::str::from_utf8(&line) {
                                    // mode.c emits only stages, numbers and kv_error's
                                    // literal messages. common-c/ss4s remain muted.
                                    if text.starts_with("korri-webos viewer ")
                                        && !text.chars().any(char::is_control)
                                    {
                                        eprintln!("{text}");
                                        if text.contains(" outcome=failed cause=") {
                                            let (before, cause) = text
                                                .split_once(" outcome=failed cause=")
                                                .expect("checked failure");
                                            let stage =
                                                before.split(" stage=").nth(1).unwrap_or("startup");
                                            *observed.lock().expect("diagnostics mutex poisoned") =
                                                Some(format!("viewer {stage}: {cause}"));
                                        }
                                    }
                                }
                            }
                            line.clear();
                            oversized = false;
                        } else if line.len() < 512 {
                            line.push(byte);
                        } else {
                            oversized = true;
                        }
                    }
                }
            })
            .map_err(error)?;
        Ok(Self {
            failure,
            thread: Some(thread),
        })
    }
    pub(super) fn failure(&self) -> Option<String> {
        self.failure
            .lock()
            .expect("diagnostics mutex poisoned")
            .clone()
    }
    /// Only after the owned child has exited/reaped, so EOF is guaranteed.
    pub(super) fn finish(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Drop for Diagnostics {
    fn drop(&mut self) {
        self.finish();
    }
}
