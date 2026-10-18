use std::{
    ffi::OsStr,
    io::{ErrorKind, Read},
    os::{fd::AsRawFd, unix::process::CommandExt},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub struct Output {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Helpers print at most this much unless their output grows with a package
/// closure. Plugins have no size limit, so closure-sized output has no cap.
const MAX_OUTPUT_BYTES: usize = 1024 * 1024;

pub fn run<I, S>(program: &Path, args: I, timeout: Duration) -> Result<Output, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_until(
        program,
        args,
        Some(Instant::now() + timeout),
        Some(MAX_OUTPUT_BYTES),
    )
}

/// Package transfers, imports and verification take as long as the package
/// needs: plugins have no size or time limit. An operator can still interrupt.
pub fn run_unbounded<I, S>(program: &Path, args: I) -> Result<Output, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_until(program, args, None, Some(MAX_OUTPUT_BYTES))
}

fn run_until<I, S>(
    program: &Path,
    args: I,
    deadline: Option<Instant>,
    max_output: Option<usize>,
) -> Result<Output, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command
        .spawn()
        .map_err(|e| format!("could not start {}: {e}", program.display()))?;
    let mut out = child.stdout.take().ok_or("missing stdout pipe")?;
    let mut err = child.stderr.take().ok_or("missing stderr pipe")?;
    for fd in [out.as_raw_fd(), err.as_raw_fd()] {
        if unsafe { libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK) } < 0 {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.wait();
            return Err("could not make helper output nonblocking".into());
        }
    }
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    loop {
        if let Err(error) = drain(&mut out, &mut stdout, max_output)
            .and_then(|_| drain(&mut err, &mut stderr, max_output))
        {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.wait();
            return Err(error);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                // Helpers do not own persistent children. Actual services are
                // systemd-owned and are outside this command's process group.
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                drain(&mut out, &mut stdout, max_output)?;
                drain(&mut err, &mut stderr, max_output)?;
                return Ok(Output {
                    success: status.success(),
                    stdout: String::from_utf8_lossy(&stdout).into(),
                    stderr: String::from_utf8_lossy(&stderr).into(),
                });
            }
            Ok(None) => {}
            Err(error) => {
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.wait();
                return Err(format!("helper status failed: {error}"));
            }
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.wait();
            return Err(format!("{} exceeded its deadline", program.display()));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn drain(
    reader: &mut impl Read,
    bytes: &mut Vec<u8>,
    max_output: Option<usize>,
) -> Result<(), String> {
    let mut buffer = [0; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(n) => {
                if max_output.is_some_and(|max| bytes.len() + n > max) {
                    return Err("helper output exceeded 1 MiB".into());
                }
                bytes.extend_from_slice(&buffer[..n]);
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(()),
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(format!("helper output failed: {error}")),
        }
    }
}

pub fn checked<I, S>(program: &Path, args: I, timeout: Duration) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    succeeded(program, run(program, args, timeout)?)
}

pub fn checked_unbounded<I, S>(program: &Path, args: I) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    succeeded(program, run_unbounded(program, args)?)
}

/// For Nix commands whose output grows with a package closure: recursive
/// path-info listings and per-path progress lines from build, copy, verify
/// and content-address conversion. Their output has no cap. `None` also
/// removes the deadline, for work whose run time grows with the closure too.
pub fn checked_closure<I, S>(
    program: &Path,
    args: I,
    timeout: Option<Duration>,
) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let deadline = timeout.map(|timeout| Instant::now() + timeout);
    succeeded(program, run_until(program, args, deadline, None)?)
}

fn succeeded(program: &Path, output: Output) -> Result<String, String> {
    if !output.success {
        return Err(format!(
            "{} failed: {}{}",
            program.display(),
            output.stdout,
            output.stderr
        ));
    }
    Ok(output.stdout)
}
