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
}

pub struct JobHost {
    units: Arc<dyn HostUnits>,
    mode: Mode,
    channel: Channel,
    reader: EventReader,
}

impl JobRunner {
    pub fn start(&self, plan: &HostPlan) -> Result<JobHost, String> {
        let mode = Mode::Job {
            id: format!("{:032x}", rand::random::<u128>()),
        };
        let channel = Channel::create(&mode.channel(&self.launches_root), self.runtime_uid)
            .map_err(|error| format!("cannot prepare the Steam engine channel: {error}"))?;
        if let Err(error) = self.units.start(plan, &mode) {
            let _ = channel.remove();
            return Err(format!("cannot start the Steam engine host: {error}"));
        }
        Ok(JobHost {
            units: Arc::clone(&self.units),
            reader: EventReader::new(&channel.dir),
            mode,
            channel,
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
        self.reader.read().unwrap_or_default()
    }

    /// False once the host has exited by itself.
    pub fn running(&self) -> bool {
        self.units.active(&self.mode).unwrap_or(false)
    }

    pub fn channel(&self) -> &Path {
        &self.channel.dir
    }

    /// Stop the host now and remove its channel.
    pub fn stop(self) {
        drop(self)
    }
}

impl Drop for JobHost {
    fn drop(&mut self) {
        let _ = self.units.stop(&self.mode);
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
  esac
done"#,
        );
        let runner = JobRunner {
            units: Arc::new(ProcessHostUnits::new(&launches)),
            launches_root: launches.clone(),
            runtime_uid: unsafe { libc::geteuid() },
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
        job.stop();
        assert!(!channel.exists());
        assert_eq!(std::fs::read_dir(&launches).unwrap().count(), 0);
    }
}
