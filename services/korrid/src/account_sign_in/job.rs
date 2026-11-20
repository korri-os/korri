//! One account-host job: a connection to the plugin's account socket.
//!
//! The plugin host generates, for a plugin that declares `accounts`, a socket
//! unit `korri-account-<sha256(id)>.socket` (`Accept=yes`, korrid's user
//! only) and its service template, which runs the declared program as the
//! plugin's own service user with the plugin's own hardening and the
//! connection as its input and output. korrid connects: systemd starts one
//! program for the connection. korrid writes command lines and reads event
//! lines on it, and closes it to end the job; the program ends at the end of
//! its input, and its unit's `RuntimeMaxSec` bounds one that does not.
use super::protocol::{self, Command, Event};
use std::{
    io::{self, Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    time::Duration,
};

/// Starts account-host jobs. Production connects to the plugin's socket;
/// tests run a fake program on a socket pair, as systemd's `Accept=yes`
/// does.
pub trait AccountHosts: Send + Sync {
    fn connect(&self, plugin: &str) -> io::Result<UnixStream>;
}

/// The plugin host's generated socket for `plugin`.
pub struct SocketHosts;

impl SocketHosts {
    pub fn path(plugin: &str) -> PathBuf {
        crate::plugin_references::account_socket(plugin)
    }
}

impl AccountHosts for SocketHosts {
    fn connect(&self, plugin: &str) -> io::Result<UnixStream> {
        UnixStream::connect(Self::path(plugin))
    }
}

/// One program's event line is a bounded JSON event. A longer line ends the
/// job: the program is not speaking the protocol.
const MAX_LINE: usize = 64 * 1024;
/// One `poll` reads at most this much; the rest waits for the next call.
const MAX_POLL_BYTES: usize = 256 * 1024;
/// How long one `poll` waits for output.
const READ_WAIT: Duration = Duration::from_millis(50);

pub struct Job {
    stream: UnixStream,
    partial: Vec<u8>,
    ended: bool,
    overflowed: bool,
}

impl Job {
    pub fn start(hosts: &dyn AccountHosts, plugin: &str) -> io::Result<Self> {
        let stream = hosts.connect(plugin)?;
        stream.set_read_timeout(Some(READ_WAIT))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        Ok(Self {
            stream,
            partial: Vec::new(),
            ended: false,
            overflowed: false,
        })
    }

    pub fn send(&mut self, command: Command) -> io::Result<()> {
        self.stream.write_all(command.line().as_bytes())
    }

    /// The events written since the last call; waits at most `READ_WAIT`
    /// for the first byte and reads at most `MAX_POLL_BYTES`. A line longer
    /// than `MAX_LINE` ends the job (`overflowed`).
    pub fn poll(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        let mut buffer = [0_u8; 8192];
        let mut read = 0;
        while !self.ended && read < MAX_POLL_BYTES {
            match self.stream.read(&mut buffer) {
                Ok(0) => self.ended = true,
                Ok(count) => {
                    read += count;
                    self.partial.extend_from_slice(&buffer[..count]);
                    self.take_lines(&mut events);
                    if self.partial.len() > MAX_LINE {
                        self.partial.clear();
                        self.overflowed = true;
                        self.ended = true;
                        let _ = self.stream.shutdown(std::net::Shutdown::Both);
                        break;
                    }
                    if count < buffer.len() {
                        break;
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    break
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => self.ended = true,
            }
        }
        events
    }

    /// Move each complete line out of `partial`, parsed.
    fn take_lines(&mut self, events: &mut Vec<Event>) {
        let mut start = 0;
        while let Some(end) = self.partial[start..].iter().position(|byte| *byte == b'\n') {
            let line = &self.partial[start..start + end + 1];
            if let Some(event) = std::str::from_utf8(line).ok().and_then(protocol::parse) {
                events.push(event);
            }
            start += end + 1;
        }
        self.partial.drain(..start);
    }

    /// False once the program closed its output, or the job ended.
    pub fn running(&self) -> bool {
        !self.ended
    }

    /// The program wrote a line longer than `MAX_LINE`; the job ended.
    pub fn overflowed(&self) -> bool {
        self.overflowed
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // The end of the program's input: it ends the job.
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

#[cfg(test)]
pub mod process {
    //! The account host as a child of the test, with one end of a socket
    //! pair as its input and output, as systemd's `Accept=yes` gives it.
    use super::*;
    use std::{
        os::fd::OwnedFd,
        path::{Path, PathBuf},
        process::Stdio,
    };

    pub struct ProcessHosts {
        pub program: PathBuf,
    }

    impl AccountHosts for ProcessHosts {
        fn connect(&self, _: &str) -> io::Result<UnixStream> {
            let (ours, theirs) = UnixStream::pair()?;
            let input: OwnedFd = theirs.try_clone()?.into();
            let output: OwnedFd = theirs.into();
            let mut child = std::process::Command::new(&self.program)
                .stdin(Stdio::from(input))
                .stdout(Stdio::from(output))
                .spawn()?;
            std::thread::spawn(move || child.wait());
            Ok(ours)
        }
    }

    /// A shell script account host with `body`. Commands arrive on stdin;
    /// events go to stdout.
    pub fn script(dir: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(format!("account-host-{:x}", rand::random::<u64>()));
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
}

#[cfg(test)]
mod tests {
    use super::{process::*, *};

    /// A fake program that answers `status` and `signin-qr`, and ends at the
    /// end of its input.
    const FAKE: &str = r#"while read -r line; do
  case "$line" in
    status) echo '{"event":"signed_out","reason":"expired"}' ;;
    signin-qr) echo '{"event":"qr_code","challenge_url":"https://login.example/a/1"}'
               echo '{"event":"something_else","x":1}'
               echo '{"event":"signed_in","account_name":"p@example.com"}' ;;
    signout) echo '{"event":"signed_out"}' ;;
  esac
done
echo '{"event":"signed_out","reason":"input_ended"}' >&2"#;

    fn until(job: &mut Job, count: usize) -> Vec<Event> {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut events = Vec::new();
        while events.len() < count {
            assert!(std::time::Instant::now() < deadline, "{events:?}");
            events.extend(job.poll());
        }
        events
    }

    #[test]
    fn a_job_writes_command_lines_and_reads_event_lines() {
        let temp = tempfile::tempdir().unwrap();
        let hosts = ProcessHosts {
            program: script(temp.path(), FAKE),
        };
        let mut job = Job::start(&hosts, "@korri:fake").unwrap();
        job.send(Command::Status).unwrap();
        assert_eq!(
            until(&mut job, 1),
            [Event::SignedOut {
                reason: Some("expired".into())
            }]
        );
        job.send(Command::SignInQr).unwrap();
        assert_eq!(
            until(&mut job, 3),
            [
                Event::QrCode {
                    challenge_url: "https://login.example/a/1".into()
                },
                Event::Other,
                Event::SignedIn {
                    account_name: "p@example.com".into()
                }
            ]
        );
        assert!(job.running());
    }

    #[test]
    fn a_program_that_exits_ends_the_job() {
        let temp = tempfile::tempdir().unwrap();
        let hosts = ProcessHosts {
            program: script(temp.path(), r#"echo '{"event":"signed_out"}'"#),
        };
        let mut job = Job::start(&hosts, "@korri:fake").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut events = Vec::new();
        while job.running() {
            assert!(std::time::Instant::now() < deadline);
            events.extend(job.poll());
        }
        assert_eq!(events, [Event::SignedOut { reason: None }]);
    }

    /// Closing the connection is the end of the program's input.
    #[test]
    fn dropping_the_job_ends_the_programs_input() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("ended");
        let hosts = ProcessHosts {
            program: script(
                temp.path(),
                &format!(
                    "while read -r line; do :; done\ntouch '{}'",
                    marker.display()
                ),
            ),
        };
        drop(Job::start(&hosts, "@korri:fake").unwrap());
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !marker.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "the program did not end"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// A program that floods its output with no line end: each poll reads a
    /// bounded amount, and the job ends as soon as the line passes the limit.
    #[test]
    fn a_program_that_floods_its_output_ends_the_job() {
        let temp = tempfile::tempdir().unwrap();
        let hosts = ProcessHosts {
            program: script(
                temp.path(),
                r#"echo '{"event":"signed_out"}'
exec tr -d '\n' < /dev/zero"#,
            ),
        };
        let mut job = Job::start(&hosts, "@korri:fake").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut events = Vec::new();
        while job.running() {
            assert!(std::time::Instant::now() < deadline, "the flood went on");
            events.extend(job.poll());
            assert!(job.partial.len() <= MAX_LINE);
        }
        assert!(job.overflowed());
        assert_eq!(events, [Event::SignedOut { reason: None }]);
        assert!(job.poll().is_empty());
    }

    /// Many short lines in one write are each read; a poll stops at its cap.
    #[test]
    fn one_poll_reads_a_bounded_amount() {
        let temp = tempfile::tempdir().unwrap();
        let hosts = ProcessHosts {
            program: script(temp.path(), r#"yes '{"event":"other"}' | head -n 100000"#),
        };
        let mut job = Job::start(&hosts, "@korri:fake").unwrap();
        std::thread::sleep(Duration::from_millis(500));
        let first = job.poll();
        // 18 bytes a line: one poll reads at most MAX_POLL_BYTES and one
        // more buffer, far less than the 1.8 MB the program wrote.
        assert!(!first.is_empty());
        assert!(
            first.len() <= (MAX_POLL_BYTES + 8192) / 18,
            "{}",
            first.len()
        );
        assert!(!job.overflowed());
    }

    #[test]
    fn the_socket_is_the_plugin_hosts_generated_one() {
        assert_eq!(
            SocketHosts::path("@korri:tailscale"),
            PathBuf::from(format!(
                "/run/korri-account-{}.sock",
                &hex::encode(<sha2::Sha256 as sha2::Digest>::digest(b"@korri:tailscale"))[..32]
            ))
        );
    }
}
