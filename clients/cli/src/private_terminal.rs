//! Native hidden input with cancellation delivered through an async-signal-safe
//! self-pipe. Only the reading thread changes termios, including on cancellation.
use crate::{error::CliError, output};
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    mem::MaybeUninit,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::fs::OpenOptionsExt,
    },
    sync::{
        atomic::{AtomicBool, AtomicI32, Ordering},
        Arc, Mutex, MutexGuard, OnceLock,
    },
};

static SIGNALS: OnceLock<Option<TerminalSignals>> = OnceLock::new();
static INPUT_LOCK: Mutex<()> = Mutex::new(());

fn unavailable() -> CliError {
    CliError::usage(
        "Could not read or restore the private terminal input. Use explicit private stdin instead.",
    )
}
fn cancelled() -> CliError {
    CliError::failure(
        "Cancelled",
        "Private input was cancelled.",
        "Nothing was submitted. Run the command again when ready.",
    )
}

pub(crate) fn read_line(label: &str, limit: u64) -> Result<Vec<u8>, CliError> {
    let mut terminal = HiddenTerminal::new().map_err(|_| unavailable())?;
    eprint!("{}: ", output::text(label));
    io::stderr().flush().map_err(|_| unavailable())?;
    let result = terminal.read_line(limit);
    // Discard a partially entered secret before echo is restored on errors.
    terminal.discard_input = result.is_err();
    terminal.restore().map_err(|_| unavailable())?;
    eprintln!();
    if terminal.signals.state.cancelled.load(Ordering::SeqCst) != 0 {
        return Err(cancelled());
    }
    result.map_err(|_| unavailable())
}

struct SignalState {
    active: AtomicBool,
    cancelled: AtomicI32,
    writer: OwnedFd,
}
struct TerminalSignals {
    reader: OwnedFd,
    state: Arc<SignalState>,
}
impl TerminalSignals {
    fn new() -> io::Result<Self> {
        let mut pipe = [-1; 2];
        // SAFETY: pipe points to two writable descriptors. Successful pipe2
        // returns new owned FDs; CLOEXEC prevents passing them to subprocesses.
        if unsafe { libc::pipe2(pipe.as_mut_ptr(), libc::O_NONBLOCK | libc::O_CLOEXEC) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: Each fresh pipe FD is adopted exactly once.
        let reader = unsafe { OwnedFd::from_raw_fd(pipe[0]) };
        let writer = unsafe { OwnedFd::from_raw_fd(pipe[1]) };
        let state = Arc::new(SignalState {
            active: AtomicBool::new(false),
            cancelled: AtomicI32::new(0),
            writer,
        });
        for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGQUIT] {
            let state = Arc::clone(&state);
            // SAFETY: The callback only accesses lock-free atomics and invokes
            // async-signal-safe write/_exit. It never allocates, locks, formats,
            // touches terminal state, or runs Rust destructors. Captured state
            // and its descriptor remain owned by the registered callback.
            unsafe {
                signal_hook_registry::register(signal, move || {
                    if !state.active.load(Ordering::SeqCst) {
                        // Keep ordinary termination after the private read. The
                        // handlers stay installed: unregistering the final hook
                        // would silently leave these signals ignored.
                        libc::_exit(128 + signal);
                    }
                    state.cancelled.store(signal, Ordering::SeqCst);
                    let wake = [1u8];
                    // Nonblocking: a full pipe already guarantees a wakeup.
                    libc::write(state.writer.as_raw_fd(), wake.as_ptr().cast(), 1);
                })?;
            }
        }
        Ok(Self { reader, state })
    }
    fn start(&self) {
        let mut byte = 0u8;
        // SAFETY: Valid owned nonblocking pipe FD and one-byte output buffer.
        while unsafe { libc::read(self.reader.as_raw_fd(), (&mut byte as *mut u8).cast(), 1) } > 0 {
        }
        self.state.cancelled.store(0, Ordering::SeqCst);
        self.state.active.store(true, Ordering::SeqCst);
    }
}

struct HiddenTerminal {
    input: File,
    saved: libc::termios,
    signals: &'static TerminalSignals,
    restored: bool,
    discard_input: bool,
    _lock: MutexGuard<'static, ()>,
}
impl HiddenTerminal {
    fn new() -> io::Result<Self> {
        let lock = INPUT_LOCK
            .lock()
            .map_err(|_| io::Error::other("private terminal input unavailable"))?;
        // Linux: reopen the exact stdin terminal, not a possibly different
        // controlling /dev/tty. This creates a separate file description, so
        // nonblocking reads do not change the caller's stdin flags. Nonblocking
        // also closes the poll/read race when Ctrl-C flushes a ready input line.
        let input = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC | libc::O_NOCTTY)
            .open("/proc/self/fd/0")?;
        let mut saved = MaybeUninit::uninit();
        // SAFETY: tcgetattr initializes native termios on success. The owned FD
        // refers to stdin, checked as a terminal before this module is called.
        if unsafe { libc::tcgetattr(input.as_raw_fd(), saved.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: Successful tcgetattr initialized saved above.
        let saved = unsafe { saved.assume_init() };
        let signals = SIGNALS
            .get_or_init(|| TerminalSignals::new().ok())
            .as_ref()
            .ok_or_else(|| io::Error::other("private terminal signals unavailable"))?;
        // Handlers must be ready before disabling echo. Signals only wake this
        // reader; they cannot race a second termios writer on another thread.
        signals.start();
        let guard = Self {
            input,
            saved,
            signals,
            restored: false,
            discard_input: true,
            _lock: lock,
        };
        let mut hidden = saved;
        hidden.c_lflag &= !(libc::ECHO | libc::ECHONL);
        set_attributes(guard.input.as_raw_fd(), &hidden)?;
        Ok(guard)
    }
    fn read_line(&self, limit: u64) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        loop {
            if self.signals.state.cancelled.load(Ordering::SeqCst) != 0 {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let mut descriptors = [
                libc::pollfd {
                    fd: self.input.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: self.signals.reader.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            // SAFETY: descriptors is a writable native pollfd array of length 2.
            if unsafe { libc::poll(descriptors.as_mut_ptr(), 2, -1) } < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if self.signals.state.cancelled.load(Ordering::SeqCst) != 0 {
                continue;
            }
            if descriptors[0].revents & libc::POLLIN == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            let mut byte = 0u8;
            // SAFETY: The owned FD is an open nonblocking terminal and byte is
            // writable. Reading one byte never consumes the next prompt's line.
            let count =
                unsafe { libc::read(self.input.as_raw_fd(), (&mut byte as *mut u8).cast(), 1) };
            if count < 0 {
                let error = io::Error::last_os_error();
                if matches!(
                    error.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                ) {
                    continue;
                }
                return Err(error);
            }
            if count == 0 {
                return Ok(bytes);
            }
            bytes.push(byte);
            if bytes.len() as u64 > limit {
                return Err(io::ErrorKind::InvalidData.into());
            }
            if byte == b'\n' {
                return Ok(bytes);
            }
        }
    }
    fn restore(&mut self) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }
        if self.discard_input || self.signals.state.cancelled.load(Ordering::SeqCst) != 0 {
            // SAFETY: Discard pending private input on the same terminal FD.
            if unsafe { libc::tcflush(self.input.as_raw_fd(), libc::TCIFLUSH) } != 0 {
                // Still restore echo even if the terminal cannot flush input.
                let error = io::Error::last_os_error();
                set_attributes(self.input.as_raw_fd(), &self.saved)?;
                return Err(error);
            }
        }
        set_attributes(self.input.as_raw_fd(), &self.saved)?;
        self.restored = true;
        self.signals.state.active.store(false, Ordering::SeqCst);
        Ok(())
    }
}
impl Drop for HiddenTerminal {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
fn set_attributes(fd: libc::c_int, attributes: &libc::termios) -> io::Result<()> {
    loop {
        // SAFETY: attributes came from this terminal's successful tcgetattr.
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, attributes) } == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}
