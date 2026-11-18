//! The engine host in job mode: one host for one job outside a session
//! (sign-in, sign-out, list, install, uninstall), stopped when the job ends
//! (ADR 0083: the engine runs only while korrid needs it).
//!
//! In job mode the host signs in from the engine's own saved login at start
//! and reports `signed_in {account_name}`, or `signin_failed {reason:
//! "no_cached_login"}` when there is none; korrid learns the sign-in state
//! only this way.
use super::{
    host::{Channel, EventReader, HostPlan, HostUnits, Mode},
    protocol::{Command, Event},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

/// What korrid needs to run job hosts on this device.
#[derive(Clone)]
pub struct JobRunner {
    pub units: Arc<dyn HostUnits>,
    pub launches_root: PathBuf,
    pub runtime_uid: u32,
    /// Session hosts running now: a job waits for them, because the plugin's
    /// start wrapper allows one engine per person's folder.
    pub live: Arc<std::sync::atomic::AtomicUsize>,
    /// Every job host's engine states are reported here.
    pub health: Arc<super::availability::EngineHealth>,
}

/// A job waits for a session at most this long.
const SESSION_WAIT: Duration = Duration::from_secs(6 * 3600);

pub struct JobHost {
    units: Arc<dyn HostUnits>,
    health: Arc<super::availability::EngineHealth>,
    plan: HostPlan,
    mode: Mode,
    channel: Channel,
    reader: EventReader,
    ended: Option<JobEnd>,
    /// The host signed in, or saved a login, in this job: its `shutdown` may
    /// wait for the engine to write the login (`deferred: login_save`).
    saving_login: bool,
}

impl JobRunner {
    /// Start a job host, after any running session. Blocking.
    pub fn start(&self, plan: &HostPlan) -> Result<JobHost, String> {
        let deadline = std::time::Instant::now() + SESSION_WAIT;
        while self.live.load(std::sync::atomic::Ordering::SeqCst) > 0 {
            if std::time::Instant::now() >= deadline {
                return Err("a Steam game session is still running".into());
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        let mode = Mode::Job {
            id: format!("{:032x}", rand::random::<u128>()),
        };
        let dir = mode.channel(&self.launches_root);
        let channel = Channel::create(&dir, self.runtime_uid).map_err(|error| {
            format!(
                "cannot prepare the Steam engine channel {}: {error}",
                dir.display()
            )
        })?;
        if let Err(error) = self.units.start(plan, &mode) {
            let _ = channel.remove();
            return Err(format!(
                "cannot start the Steam engine host {}: {error}",
                mode.unit()
            ));
        }
        Ok(JobHost {
            units: Arc::clone(&self.units),
            health: Arc::clone(&self.health),
            plan: plan.clone(),
            reader: EventReader::new(&channel.dir),
            mode,
            channel,
            ended: None,
            saving_login: false,
        })
    }
}

impl JobHost {
    pub fn send(&self, command: Command) -> Result<(), String> {
        self.channel
            .send_within(command, Duration::from_secs(30))
            .map_err(|error| error.to_string())
    }

    /// Events since the last call.
    pub fn poll(&mut self) -> Vec<Event> {
        let events = self.reader.read().unwrap_or_default();
        for event in &events {
            self.health.observe(&self.plan, event);
            self.saving_login |= saves_login(event);
        }
        events
    }

    /// False once the host has exited by itself.
    pub fn running(&self) -> bool {
        self.units.active(&self.mode).unwrap_or(false)
    }

    pub fn channel(&self) -> &Path {
        &self.channel.dir
    }

    /// The host's unit name, for the journal.
    pub fn unit(&self) -> String {
        self.mode.unit()
    }

    /// End the host and remove its channel: ask it to `shutdown` and wait
    /// for its own exit, so the engine logs off and writes its stores (the
    /// saved login of a sign-in among them), then stop the unit, whose
    /// KillSignal is SIGKILL, for whatever is left. Says how it ended.
    pub fn stop(mut self) -> JobEnd {
        let end = self.end(self.grace());
        // Drop finds the host gone and only removes the channel.
        end
    }

    fn grace(&self) -> Duration {
        if self.saving_login {
            LOGIN_SAVE_GRACE
        } else {
            SHUTDOWN_GRACE
        }
    }

    fn end(&mut self, grace: Duration) -> JobEnd {
        if self.ended.is_some() || !self.running() {
            return *self.ended.get_or_insert(JobEnd::Exited);
        }
        let started = std::time::Instant::now();
        let mut deadline = started + grace;
        let asked = self
            .channel
            .send_within(Command::Shutdown, Duration::from_secs(2))
            .is_ok();
        while asked && std::time::Instant::now() < deadline {
            if !self.running() {
                return *self.ended.insert(JobEnd::ShutDown);
            }
            // The host may say only now that it waits for the login save.
            if self.poll().iter().any(saves_login) {
                deadline = deadline.max(started + LOGIN_SAVE_GRACE);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.units.stop(&self.mode);
        *self.ended.insert(JobEnd::Killed { asked })
    }
}

/// A job host gets this long to end after `shutdown` before it is killed.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
/// After a sign-in the host defers `shutdown` until the engine wrote the
/// login (`config.vdf`), up to 30 s plus 1 s; korrid allows a margin.
const LOGIN_SAVE_GRACE: Duration = Duration::from_secs(40);

/// An event after which the host's `shutdown` may wait for the login save.
fn saves_login(event: &Event) -> bool {
    match event {
        Event::SignedIn { .. } | Event::LoginSaved { .. } => true,
        Event::Command { command, deferred } => {
            command == "shutdown" && deferred.as_deref() == Some("login_save")
        }
        _ => false,
    }
}

/// How a job host ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobEnd {
    /// It had exited by itself before korrid stopped it.
    Exited,
    /// It exited by itself after `shutdown`.
    ShutDown,
    /// korrid killed it: it did not end within the grace, or (`asked`
    /// false) it took no `shutdown` command.
    Killed { asked: bool },
}

impl std::fmt::Display for JobEnd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Exited => "it exited by itself",
            Self::ShutDown => "it shut down when asked",
            Self::Killed { asked: true } => "killed: it did not end after shutdown",
            Self::Killed { asked: false } => "killed: it took no shutdown command",
        })
    }
}

impl Drop for JobHost {
    fn drop(&mut self) {
        self.end(self.grace());
        let _ = std::fs::remove_dir_all(&self.channel.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steam_engine::host::{process_units::ProcessHostUnits, tests::script_host};

    #[test]
    fn a_job_host_signs_in_runs_its_command_and_stops() {
        let temp = tempfile::tempdir().unwrap();
        let launches = temp.path().join("launches");
        let host = script_host(
            temp.path(),
            r#"echo '{"event":"signed_in","account_name":"korri_player"}'
exec 3<>"$fifo"
while read -r line <&3; do
  case "$line" in
    list-apps) echo '{"event":"app","appid":70,"name":"Half-Life","source":"owned"}'; echo '{"event":"apps_done"}' ;;
    shutdown) echo '{"event":"phase","phase":"stopped"}'; exit 0 ;;
  esac
done"#,
        );
        let runner = JobRunner {
            units: Arc::new(ProcessHostUnits::new(&launches)),
            launches_root: launches.clone(),
            runtime_uid: unsafe { libc::geteuid() },
            live: Arc::default(),
            health: Arc::default(),
        };
        let mut job = runner
            .start(&HostPlan {
                host_program: host,
                engine: temp.path().join("steamclient.so"),
                proton: None,
                account_root: temp.path().into(),
            })
            .unwrap();
        assert!(job.running());
        job.send(Command::ListApps).unwrap();
        let mut list = crate::steam_engine::provider::AppList::default();
        let mut signed_in = None;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !list.done() {
            assert!(std::time::Instant::now() < deadline);
            for event in job.poll() {
                if let Event::SignedIn { account_name } = &event {
                    signed_in = Some(account_name.clone());
                }
                list.apply(&event);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(signed_in.as_deref(), Some("korri_player"));
        assert_eq!(list.search("half")[0].id, "70");
        let channel = job.channel().to_path_buf();
        // The host is asked to shut down and ends by itself.
        assert_eq!(job.stop(), JobEnd::ShutDown);
        assert!(!channel.exists());
        assert_eq!(std::fs::read_dir(&launches).unwrap().count(), 0);
    }
}

#[cfg(test)]
mod end_tests {
    use super::*;
    use crate::steam_engine::host::{process_units::ProcessHostUnits, tests::script_host};

    fn job(temp: &Path, body: &str) -> JobHost {
        let launches = temp.join("launches");
        JobRunner {
            units: Arc::new(ProcessHostUnits::new(&launches)),
            launches_root: launches,
            runtime_uid: unsafe { libc::geteuid() },
            live: Arc::default(),
            health: Arc::default(),
        }
        .start(&HostPlan {
            host_program: script_host(temp, body),
            engine: temp.join("steamclient.so"),
            proton: None,
            account_root: temp.into(),
        })
        .unwrap()
    }

    /// A host that ignores `shutdown` is killed once the grace runs out.
    #[test]
    fn a_host_that_does_not_end_after_shutdown_is_killed() {
        let temp = tempfile::tempdir().unwrap();
        let mut host = job(
            temp.path(),
            r#"exec 3<>"$fifo"
while read -r line <&3; do :; done"#,
        );
        assert!(host.running());
        let started = std::time::Instant::now();
        assert_eq!(
            host.end(Duration::from_millis(300)),
            JobEnd::Killed { asked: true }
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(!host.running());
        let channel = host.channel().to_path_buf();
        drop(host);
        assert!(!channel.exists());
    }

    /// After a sign-in the host waits for the login save before it ends;
    /// korrid does not kill it inside that window.
    #[test]
    fn a_host_saving_a_login_gets_the_longer_grace() {
        let temp = tempfile::tempdir().unwrap();
        let mut host = job(
            temp.path(),
            r#"echo '{"event":"login_saved","confirmed":true}'
echo '{"event":"signed_in","account_name":"p"}'
exec 3<>"$fifo"
while read -r line <&3; do
  case "$line" in
    shutdown) echo '{"event":"command","command":"shutdown","deferred":"login_save"}'; sleep 1; exit 0 ;;
  esac
done"#,
        );
        assert_eq!(host.grace(), SHUTDOWN_GRACE);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !host
            .poll()
            .iter()
            .any(|e| matches!(e, Event::SignedIn { .. }))
        {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(host.grace(), LOGIN_SAVE_GRACE);
        assert_eq!(host.stop(), JobEnd::ShutDown);
    }

    /// A host that defers shutdown, though korrid saw no sign-in, gets the
    /// longer grace from its answer: a 300 ms grace does not kill it at 1 s.
    #[test]
    fn a_deferred_shutdown_extends_the_grace() {
        let temp = tempfile::tempdir().unwrap();
        let mut host = job(
            temp.path(),
            r#"exec 3<>"$fifo"
while read -r line <&3; do
  case "$line" in
    shutdown) echo '{"event":"command","command":"shutdown","deferred":"login_save"}'; sleep 1; exit 0 ;;
  esac
done"#,
        );
        assert_eq!(host.end(Duration::from_millis(300)), JobEnd::ShutDown);
    }

    /// A host that already ended is not asked again.
    #[test]
    fn a_host_that_exited_by_itself_says_so() {
        let temp = tempfile::tempdir().unwrap();
        let host = job(temp.path(), "exit 0");
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while host.running() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(host.stop(), JobEnd::Exited);
    }
}

#[cfg(test)]
mod wait_tests {
    use super::*;
    use crate::steam_engine::host::{process_units::ProcessHostUnits, tests::script_host};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// One engine per person's folder: a job waits while a session runs.
    #[test]
    fn a_job_waits_for_a_running_session() {
        let temp = tempfile::tempdir().unwrap();
        let launches = temp.path().join("launches");
        let live = Arc::new(AtomicUsize::new(1));
        let runner = JobRunner {
            units: Arc::new(ProcessHostUnits::new(&launches)),
            launches_root: launches.clone(),
            runtime_uid: unsafe { libc::geteuid() },
            live: Arc::clone(&live),
            health: Arc::default(),
        };
        let plan = HostPlan {
            host_program: script_host(temp.path(), "sleep 5"),
            engine: temp.path().join("steamclient.so"),
            proton: None,
            account_root: temp.path().into(),
        };
        let started = std::thread::spawn(move || runner.start(&plan).map(JobHost::stop));
        std::thread::sleep(Duration::from_millis(700));
        assert!(!launches.exists(), "no job host while the session runs");
        live.store(0, Ordering::SeqCst);
        started.join().unwrap().unwrap();
    }
}
