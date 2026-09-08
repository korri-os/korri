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

pub fn run<I, S>(program: &Path, args: I, timeout: Duration) -> Result<Output, String>
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
    let deadline = Instant::now() + timeout;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    loop {
        if let Err(error) = drain(&mut out, &mut stdout).and_then(|_| drain(&mut err, &mut stderr))
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
                drain(&mut out, &mut stdout)?;
                drain(&mut err, &mut stderr)?;
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
        if Instant::now() >= deadline {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.wait();
            return Err(format!("{} exceeded its deadline", program.display()));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn drain(reader: &mut impl Read, bytes: &mut Vec<u8>) -> Result<(), String> {
    let mut buffer = [0; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(n) => {
                if bytes.len() + n > 1024 * 1024 {
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
    let output = run(program, args, timeout)?;
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
