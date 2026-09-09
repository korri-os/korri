use crate::{Error, browser};
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

pub(crate) const MAX_FRAME: usize = 1024 * 1024;

pub(crate) struct Transport {
    input: File,
    output: File,
    buffered: Vec<u8>,
    partial_since: Option<Instant>,
}

impl Transport {
    pub(crate) fn new(input: File, output: File) -> Result<Self, Error> {
        for file in [&input, &output] {
            // Both ends are private descriptors; preserve all existing flags.
            let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL) };
            if flags < 0
                || unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) }
                    < 0
            {
                return Err(Error::Io);
            }
        }
        Ok(Self {
            input,
            output,
            buffered: Vec::new(),
            partial_since: None,
        })
    }

    pub(crate) fn send(&mut self, value: &Value, deadline: Instant) -> Result<(), Error> {
        let mut bytes = serde_json::to_vec(value).map_err(|_| Error::Protocol)?;
        if bytes.len() > MAX_FRAME {
            return Err(Error::Protocol);
        }
        bytes.push(0);
        let output = &mut self.output;
        let mut remaining = bytes.as_slice();
        while !remaining.is_empty() {
            wait(output, libc::POLLOUT, deadline)?;
            match output.write(remaining) {
                Ok(0) => return Err(Error::Closed),
                Ok(n) => remaining = &remaining[n..],
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => return Err(Error::Io),
            }
        }
        Ok(())
    }

    pub(crate) fn receive(&mut self, deadline: Instant) -> Result<Value, Error> {
        loop {
            if browser::interrupted() {
                return Err(Error::Interrupted);
            }
            if Instant::now() >= deadline {
                return Err(Error::Timeout);
            }
            if let Some(end) = self.buffered.iter().position(|b| *b == 0) {
                if end > MAX_FRAME {
                    return Err(Error::Protocol);
                }
                let value =
                    serde_json::from_slice(&self.buffered[..end]).map_err(|_| Error::Protocol)?;
                self.buffered.drain(..=end);
                self.partial_since = if self.buffered.is_empty() {
                    None
                } else {
                    Some(Instant::now())
                };
                return Ok(value);
            }
            if self.buffered.len() > MAX_FRAME {
                return Err(Error::Protocol);
            }
            let frame_deadline = self
                .partial_since
                .map(|since| since + Duration::from_secs(10));
            wait(
                &self.input,
                libc::POLLIN,
                frame_deadline.map_or(deadline, |d| d.min(deadline)),
            )?;
            let mut chunk = [0; 8192];
            match self.input.read(&mut chunk) {
                Ok(0) => {
                    return Err(if self.buffered.is_empty() {
                        Error::Closed
                    } else {
                        Error::Protocol
                    });
                }
                Ok(n) => {
                    self.partial_since.get_or_insert_with(Instant::now);
                    self.buffered.extend_from_slice(&chunk[..n]);
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => return Err(Error::Io),
            }
        }
    }

    pub(crate) fn has_partial_frame(&self) -> bool {
        !self.buffered.is_empty()
    }
}

fn wait(file: &File, events: libc::c_short, deadline: Instant) -> Result<(), Error> {
    loop {
        if browser::interrupted() {
            return Err(Error::Interrupted);
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(Error::Timeout)?;
        let timeout = remaining.as_millis().clamp(1, 100) as i32;
        let mut poll = libc::pollfd {
            fd: file.as_raw_fd(),
            events,
            revents: 0,
        };
        let result = unsafe { libc::poll(&mut poll, 1, timeout) };
        if result < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(Error::Io);
        }
        if result > 0 {
            if poll.revents & libc::POLLNVAL != 0 {
                return Err(Error::Io);
            }
            // Read on HUP to consume buffered bytes before reporting EOF.
            return Ok(());
        }
    }
}
