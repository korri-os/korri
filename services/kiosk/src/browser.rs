use crate::{BOOTSTRAP_URL, Error, kiosk::Options, pipe::Transport};
use std::fs::{DirBuilder, File, OpenOptions};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
extern "C" fn interrupt(_: libc::c_int) {
    INTERRUPTED.store(true, Ordering::Relaxed);
}
pub(crate) fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::Relaxed)
}

pub(crate) fn harden() -> Result<(), Error> {
    // Private IPC does not protect a dump of the launcher's memory.
    unsafe {
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if libc::setrlimit(libc::RLIMIT_CORE, &limit) != 0
            || libc::prctl(libc::PR_SET_DUMPABLE, 0) != 0
        {
            return Err(Error::Io);
        }
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = interrupt as *const () as usize;
        libc::sigemptyset(&mut action.sa_mask);
        for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            if libc::sigaction(signal, &action, std::ptr::null_mut()) != 0 {
                return Err(Error::Io);
            }
        }
    }
    Ok(())
}

struct Profile {
    path: PathBuf,
    parent_lock: File,
}
impl Profile {
    fn create(parent: &std::path::Path) -> Result<Self, Error> {
        if !parent.is_absolute() {
            return Err(Error::Configuration);
        }
        let lock = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_CLOEXEC)
            .open(parent)?;
        let metadata = lock.metadata()?;
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err(Error::Configuration);
        }
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(Error::Configuration);
        }
        // Pin all filesystem effects to the locked directory, not a path that
        // could be replaced. Never remove the permanently provisioned parent.
        let pinned_parent = PathBuf::from(format!("/proc/self/fd/{}", lock.as_raw_fd()));
        // The service cgroup must have stopped old Chromium processes first.
        // Standalone callers must provide that process cleanup before relaunch.
        for entry in std::fs::read_dir(&pinned_parent)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(pid) = name.as_encoded_bytes().strip_prefix(b"chromium-") else {
                continue;
            };
            if pid.is_empty() || !pid.iter().all(u8::is_ascii_digit) {
                continue;
            }
            // file_type does not follow symlinks. On Linux remove_dir_all uses
            // fd-relative, no-follow traversal, including during rename races.
            if !entry.file_type()?.is_dir() {
                return Err(Error::Configuration);
            }
            std::fs::remove_dir_all(entry.path())?;
        }
        let name = format!("chromium-{}", std::process::id());
        DirBuilder::new()
            .mode(0o700)
            .create(pinned_parent.join(&name))?;
        Ok(Self {
            path: parent.join(name),
            parent_lock: lock,
        })
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        let parent = PathBuf::from(format!("/proc/self/fd/{}", self.parent_lock.as_raw_fd()));
        if let Some(name) = self.path.file_name() {
            let _ = std::fs::remove_dir_all(parent.join(name));
        }
    }
}

pub(crate) struct Browser {
    child: Child,
    _profile: Profile,
}
impl Browser {
    pub(crate) fn spawn(options: &Options) -> Result<(Self, Transport), Error> {
        let profile = Profile::create(&options.profile_parent)?;
        let (child_input, parent_output) = pipe()?;
        let (parent_input, child_output) = pipe()?;
        // Keep child sources above 4 so dup2 cannot overwrite the other source.
        let child_input = duplicate(&child_input)?;
        let child_output = duplicate(&child_output)?;
        let mut command = Command::new(&options.chromium);
        command.args([
            "--remote-debugging-pipe",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-sync",
            "--disable-extensions",
            "--disable-background-networking",
            "--disable-component-update",
            "--disable-default-apps",
            "--disable-breakpad",
            "--disable-crash-reporter",
            "--noerrdialogs",
        ]);
        command.arg(format!(
            "--user-data-dir={}",
            profile.path.to_str().ok_or(Error::Configuration)?
        ));
        configure_startup(&mut command, options.headless);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let parent_pid = unsafe { libc::getpid() };
        unsafe {
            command.pre_exec(move || {
                if libc::setpgid(0, 0) != 0
                    || libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0
                    || libc::getppid() != parent_pid
                    || libc::dup2(child_input.as_raw_fd(), 3) < 0
                    || libc::dup2(child_output.as_raw_fd(), 4) < 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().map_err(|_| Error::Browser)?;
        // Drop Command too: its pre_exec closure owns copies of both child ends.
        drop(command);
        let browser = Self {
            child,
            _profile: profile,
        };
        let transport = Transport::new(parent_input, parent_output)?;
        Ok((browser, transport))
    }
}
impl Drop for Browser {
    fn drop(&mut self) {
        // Keep the leader unreaped until after the final group signal, so its
        // PID cannot be reused. The systemd cgroup is the final descendant guard.
        let group = -(self.child.id() as i32);
        unsafe {
            libc::kill(group, libc::SIGTERM);
        }
        std::thread::sleep(Duration::from_millis(200));
        unsafe {
            libc::kill(group, libc::SIGKILL);
        }
        let _ = self.child.wait();
    }
}

pub(crate) fn configure_startup(command: &mut Command, headless: bool) {
    if headless {
        // Chromium 143 ignores --app in headless mode and opens chrome://newtab/.
        command.args(["--headless=new", "about:blank"]);
    } else {
        // Chromium 143 needs an HTTP app URL for a nonfullscreen, tabless Wayland
        // window. about:blank opens a normal tabbed window instead.
        command.arg(format!("--app={BOOTSTRAP_URL}"));
    }
}

fn pipe() -> Result<(File, File), Error> {
    let mut fds = [-1; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(Error::Io);
    }
    // pipe2 has transferred two distinct, owned file descriptors to this scope.
    Ok(unsafe { (File::from_raw_fd(fds[0]), File::from_raw_fd(fds[1])) })
}
fn duplicate(file: &File) -> Result<File, Error> {
    let fd = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 10) };
    if fd < 0 {
        return Err(Error::Io);
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
