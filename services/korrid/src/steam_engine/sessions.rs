//! The engine host beside one game unit: started before it, watched while
//! it runs, stopped after the exit sync (ADR 0083, contract "Lifetime").
//!
//! While the session runs korrid reads the host's events: each
//! `save_export` replaces the contents of the game's `steam-cloud` save
//! folder (the session capture snapshots it), a `sync_conflict` writes the
//! cloud side of a split, and the engine states become the runner's
//! availability. The shim in the game unit sends the pre-game commands.
use super::{
    availability::Unavailable,
    host::{Channel, EventReader, HostPlan, HostUnits, Mode},
    protocol::{Command, Event},
    saves::Conflicts,
};
use crate::saves::{store::NativeSaveStore, types::SnapshotRecord};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// How long korrid waits for the exit sync after the game unit is gone.
/// The host's own stop grace is 45 s (FINDINGS section 8); an upload of a
/// large save can take longer, so korrid allows a margin over it.
pub const EXIT_SYNC_BOUND: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(200);

/// Who and what the session is, for its save snapshots.
#[derive(Clone, Debug)]
pub struct SessionContext {
    pub person: String,
    pub game_id: String,
    pub runner_id: String,
    pub release_id: Option<String>,
    pub device: String,
    /// `<saveRoot>/steam-cloud`.
    pub save_folder: PathBuf,
    pub private_root: PathBuf,
}

struct Running {
    mode: Mode,
    channel: PathBuf,
    stop: Arc<AtomicBool>,
    exit_synced: Arc<AtomicBool>,
    watcher: Option<JoinHandle<()>>,
}

pub struct Sessions {
    units: Arc<dyn HostUnits>,
    launches_root: PathBuf,
    runtime_uid: u32,
    running: Mutex<BTreeMap<String, Running>>,
    pub conflicts: Arc<Conflicts>,
    /// The last engine state a host reported, for the sign-in model.
    pub unavailable: Arc<Mutex<Option<Unavailable>>>,
    /// Session hosts running now. The plugin's start wrapper allows one
    /// engine per person's folder, so job hosts wait while this is not 0.
    pub live: Arc<std::sync::atomic::AtomicUsize>,
}

impl Sessions {
    pub fn new(units: Arc<dyn HostUnits>, launches_root: &Path, runtime_uid: u32) -> Self {
        Self::with_state(
            units,
            launches_root,
            runtime_uid,
            Arc::default(),
            Arc::default(),
            Arc::default(),
        )
    }

    /// The device's sessions share the conflicts and availability that the
    /// save pick and the sign-in model read.
    pub fn with_state(
        units: Arc<dyn HostUnits>,
        launches_root: &Path,
        runtime_uid: u32,
        conflicts: Arc<Conflicts>,
        unavailable: Arc<Mutex<Option<Unavailable>>>,
        live: Arc<std::sync::atomic::AtomicUsize>,
    ) -> Self {
        Self {
            units,
            launches_root: launches_root.to_path_buf(),
            runtime_uid,
            running: Mutex::new(BTreeMap::new()),
            conflicts,
            unavailable,
            live,
        }
    }

    /// The channel the game unit binds, made before the host starts.
    pub fn channel(&self, launch_id: &str) -> PathBuf {
        Mode::Session {
            launch_id: launch_id.into(),
        }
        .channel(&self.launches_root)
    }

    /// Start the host before the game unit. A resolve the person chose for
    /// this game's conflict is sent before anything else.
    pub fn start(
        &self,
        launch_id: &str,
        plan: &HostPlan,
        context: Option<SessionContext>,
    ) -> Result<(), String> {
        if !super::host::valid_id(launch_id) {
            return Err("invalid launch identity".into());
        }
        let mode = Mode::Session {
            launch_id: launch_id.into(),
        };
        let dir = mode.channel(&self.launches_root);
        // A channel left by a crashed korrid belongs to no running host.
        let _ = self.units.stop(&mode);
        let _ = Channel::open(&dir).remove();
        let channel = Channel::create(&dir, self.runtime_uid)
            .map_err(|error| format!("cannot prepare the Steam engine channel: {error}"))?;
        if let Err(error) = self.units.start(plan, &mode) {
            let _ = channel.remove();
            return Err(format!("cannot start the Steam engine host: {error}"));
        }
        if let Some(context) = &context {
            if let Some((appid, side)) = self
                .conflicts
                .take_resolve(&context.person, &context.game_id)
            {
                if let Err(error) =
                    channel.send_within(Command::Resolve(appid, side), Duration::from_secs(30))
                {
                    // Keep the pick for the next attempt.
                    self.conflicts
                        .keep_resolve(&context.person, &context.game_id, appid, side);
                    let _ = self.units.stop(&mode);
                    let _ = channel.remove();
                    return Err(format!("cannot apply the Steam Cloud choice: {error}"));
                }
            }
        }
        let stop = Arc::new(AtomicBool::new(false));
        let exit_synced = Arc::new(AtomicBool::new(false));
        let watcher = {
            let watch = Watch {
                launch_id: launch_id.into(),
                reader: EventReader::new(&dir),
                channel: dir.clone(),
                host_channel: mode.host_channel(),
                context,
                conflicts: Arc::clone(&self.conflicts),
                unavailable: Arc::clone(&self.unavailable),
                exit_synced: Arc::clone(&exit_synced),
            };
            let stop = Arc::clone(&stop);
            std::thread::Builder::new()
                .name("steam-engine-watch".into())
                .spawn(move || watch.run(&stop))
                .map_err(|error| error.to_string())?
        };
        self.live.fetch_add(1, Ordering::SeqCst);
        self.running.lock().expect("sessions poisoned").insert(
            launch_id.into(),
            Running {
                mode,
                channel: dir,
                stop,
                exit_synced,
                watcher: Some(watcher),
            },
        );
        Ok(())
    }

    /// After the game unit is gone: wait for the exit sync (or the host's
    /// own end), read the last events, then stop the host and remove the
    /// channel. Repeated calls for the same launch do nothing.
    pub fn finish(&self, launch_id: &str, bound: Duration) -> Result<(), String> {
        let Some(mut running) = self
            .running
            .lock()
            .expect("sessions poisoned")
            .remove(launch_id)
        else {
            return Ok(());
        };
        let _live = Live(&self.live);
        let deadline = Instant::now() + bound;
        while !running.exit_synced.load(Ordering::SeqCst)
            && self.units.active(&running.mode).unwrap_or(false)
            && Instant::now() < deadline
        {
            std::thread::sleep(POLL);
        }
        running.stop.store(true, Ordering::SeqCst);
        if let Some(watcher) = running.watcher.take() {
            let _ = watcher.join();
        }
        self.units
            .stop(&running.mode)
            .map_err(|error| format!("cannot stop the Steam engine host: {error}"))?;
        Channel::open(&running.channel)
            .remove()
            .map_err(|error| format!("cannot remove the Steam engine channel: {error}"))
    }
}

/// Counts a session down when its finish ends, however it ends.
struct Live<'a>(&'a std::sync::atomic::AtomicUsize);
impl Drop for Live<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

struct Watch {
    launch_id: String,
    reader: EventReader,
    channel: PathBuf,
    host_channel: PathBuf,
    context: Option<SessionContext>,
    conflicts: Arc<Conflicts>,
    unavailable: Arc<Mutex<Option<Unavailable>>>,
    exit_synced: Arc<AtomicBool>,
}

impl Watch {
    fn run(mut self, stop: &AtomicBool) {
        loop {
            // The stop flag is read before the last read, so events written
            // before finish() returned are never lost.
            let last = stop.load(Ordering::SeqCst);
            match self.reader.read() {
                Ok(events) => {
                    for event in events {
                        self.handle(&event);
                    }
                }
                Err(error) => eprintln!("korrid steam engine: cannot read host events: {error}"),
            }
            if last {
                return;
            }
            std::thread::sleep(POLL);
        }
    }

    fn handle(&mut self, event: &Event) {
        match event {
            Event::EngineMapMissing {} => self.set(Unavailable::MapMissing),
            Event::EngineMismatch {} => self.set(Unavailable::EngineMismatch),
            Event::EngineRefused { eresult } => {
                self.set(Unavailable::EngineRefused { eresult: *eresult })
            }
            Event::LogonState {
                logged_on: true, ..
            } => {
                // The engine signed in: a refusal recorded earlier is past.
                let mut current = self.unavailable.lock().expect("availability poisoned");
                if matches!(*current, Some(Unavailable::EngineRefused { .. })) {
                    *current = None;
                }
            }
            Event::SyncFinished { kind, .. } if kind == "exit" => {
                self.exit_synced.store(true, Ordering::SeqCst)
            }
            Event::SaveExportEmpty { .. } => {
                if let Some(context) = &self.context {
                    if let Err(error) = replace_folder(&context.save_folder, |_| Ok(())) {
                        eprintln!("korrid steam engine: empty save export not applied: {error}");
                    }
                }
            }
            Event::SaveExport { archive, .. } => {
                if let Err(error) = self.import(archive) {
                    eprintln!("korrid steam engine: save export not imported: {error}");
                }
            }
            Event::SyncConflict {
                appid, remote_time, ..
            } => {
                if let Err(error) = self.conflict(*appid, *remote_time) {
                    eprintln!("korrid steam engine: conflict not recorded: {error}");
                }
            }
            _ => {}
        }
    }

    fn set(&self, state: Unavailable) {
        *self.unavailable.lock().expect("availability poisoned") = Some(state);
    }

    /// The archive path as the host names it, inside the channel only.
    fn archive(&self, reported: &str) -> Result<PathBuf, String> {
        let reported = Path::new(reported);
        let relative = reported
            .strip_prefix(&self.host_channel)
            .or_else(|_| reported.strip_prefix(&self.channel))
            .map_err(|_| "the export is outside the channel".to_string())?;
        // The host writes `saves/<appid>-<launch|exit>-<n>.tar`.
        use std::path::Component::Normal;
        let parts: Vec<_> = relative.components().collect();
        match parts.as_slice() {
            [Normal(saves), Normal(name)] if saves.to_str() == Some("saves") => {
                Ok(self.channel.join("saves").join(name))
            }
            _ => Err("the export is not a file of the channel's saves".into()),
        }
    }

    fn import(&self, reported: &str) -> Result<(), String> {
        let Some(context) = &self.context else {
            return Ok(());
        };
        let archive = self.archive(reported)?;
        replace_folder(&context.save_folder, |work| {
            super::saves::extract(&archive, work)
        })?;
        let _ = fs::remove_file(&archive);
        Ok(())
    }

    /// The launch sync reported a conflict: the files the host exported
    /// before it (already in the save folder) and the cloud side become a
    /// split now, so the shim's refusal of this same attempt can go straight
    /// to the split choice. This session's completion adds no snapshot.
    fn conflict(&self, appid: u32, remote_time: u64) -> Result<(), String> {
        let Some(context) = &self.context else {
            return Ok(());
        };
        let store = NativeSaveStore::new(&context.private_root);
        let local = super::saves::capture(
            &store,
            &context.person,
            &context.save_folder,
            SnapshotRecord {
                parents: vec![],
                game_id: context.game_id.clone(),
                release_id: context.release_id.clone(),
                runner_id: context.runner_id.clone(),
                device: context.device.clone(),
                ended_at: rfc3339_now(),
                duration_seconds: 0,
                interrupted: false,
                locations: vec![],
            },
        )
        .map_err(|error| error.to_string())?;
        let conflict = super::saves::split(&store, &context.person, appid, local, remote_time)
            .map_err(|error| error.to_string())?;
        self.conflicts.open(
            &context.person,
            &context.game_id,
            Some(&self.launch_id),
            conflict,
        );
        Ok(())
    }
}

fn rfc3339_now() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// Fill a fresh folder beside `folder`, then put it in `folder`'s place.
fn replace_folder(
    folder: &Path,
    fill: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let parent = folder.parent().ok_or("save folder has no parent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let suffix = format!("{:016x}", rand::random::<u64>());
    let work = parent.join(format!(".steam-cloud-new-{suffix}"));
    let old = parent.join(format!(".steam-cloud-old-{suffix}"));
    fs::create_dir(&work).map_err(|e| e.to_string())?;
    if let Err(error) = fill(&work) {
        let _ = fs::remove_dir_all(&work);
        return Err(error);
    }
    let had = match fs::rename(folder, &old) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            let _ = fs::remove_dir_all(&work);
            return Err(error.to_string());
        }
    };
    fs::rename(&work, folder).map_err(|e| e.to_string())?;
    if had {
        let _ = fs::remove_dir_all(&old);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steam_engine::host::{process_units::ProcessHostUnits, tests::script_host};

    const PERSON: &str = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";
    const LAUNCH: &str = "0123456789abcdef0123456789abcdef";

    /// The host's export: a plain tar.
    fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, data) in files {
            let mut header = tar::Header::new_ustar();
            header.set_size(data.len() as u64);
            header.set_mode(0o600);
            header.set_entry_type(tar::EntryType::Regular);
            builder.append_data(&mut header, path, *data).unwrap();
        }
        builder.into_inner().unwrap()
    }

    /// A host that, like the product host, exports before the launch sync
    /// and after the exit sync, and reports a conflict when told to.
    fn session_host(dir: &Path, export: &Path, conflict: bool) -> PathBuf {
        let conflict = if conflict {
            r#"echo '{"event":"sync_conflict","appid":70,"found":true,"local_time":1,"remote_time":1763390000}'"#
        } else {
            ""
        };
        script_host(
            dir,
            &format!(
                r#"channel="$(dirname "$fifo")"
exec 3<>"$fifo"
while read -r line <&3; do
  case "$line" in
    "resolve "*) echo "{{\"event\":\"resolved\",\"line\":\"$line\"}}"; echo "$line" > "$channel/resolved" ;;
    "launched "*) mkdir -p "$channel/saves"; cp '{export}' "$channel/saves/70-launch-1.tar"; echo "{{\"event\":\"save_export\",\"appid\":70,\"archive\":\"$channel/saves/70-launch-1.tar\"}}"; {conflict} ;;
    "exited "*) mkdir -p "$channel/saves"; cp '{export}' "$channel/saves/70-exit-2.tar"; echo "{{\"event\":\"save_export\",\"appid\":70,\"archive\":\"$channel/saves/70-exit-2.tar\"}}"; sleep 0.3; echo '{{"event":"sync_finished","appid":70,"kind":"exit","result":"synchronized"}}' ;;
  esac
done"#,
                export = export.display()
            ),
        )
    }

    fn context(root: &Path) -> SessionContext {
        SessionContext {
            person: PERSON.into(),
            game_id: "half-life".into(),
            runner_id: "@korri:steam/engine".into(),
            release_id: None,
            device: "a".repeat(64),
            save_folder: root.join("saves/half-life/steam-cloud"),
            private_root: root.join("private"),
        }
    }

    fn plan(host: PathBuf, root: &Path) -> HostPlan {
        HostPlan {
            host_program: host,
            engine: root.join("steamclient.so"),
            proton: None,
            account_root: root.to_path_buf(),
        }
    }

    #[test]
    fn the_host_runs_before_and_after_the_game_and_its_exports_fill_the_save_folder() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("private")).unwrap();
        let export = temp.path().join("export.zip");
        fs::write(&export, zip(&[("remote/SaveData.bin", b"exit save")])).unwrap();
        let host = session_host(temp.path(), &export, false);
        let launches = temp.path().join("launches");
        let units = Arc::new(ProcessHostUnits::new(&launches));
        let sessions = Sessions::new(units.clone(), &launches, unsafe { libc::geteuid() });
        let context = context(temp.path());
        sessions
            .start(LAUNCH, &plan(host, temp.path()), Some(context.clone()))
            .unwrap();
        let mode = Mode::Session {
            launch_id: LAUNCH.into(),
        };
        assert!(
            units.active(&mode).unwrap(),
            "the host runs before the game"
        );
        // The shim in the game unit: launched, then exited.
        let channel = Channel::open(&sessions.channel(LAUNCH));
        channel
            .send_within(Command::Launched(70), Duration::from_secs(5))
            .unwrap();
        channel
            .send_within(Command::Exited(70), Duration::from_secs(5))
            .unwrap();
        let started = Instant::now();
        sessions.finish(LAUNCH, Duration::from_secs(10)).unwrap();
        assert!(
            started.elapsed() < Duration::from_secs(9),
            "it waits for the exit sync, not the bound"
        );
        assert!(
            !units.active(&mode).unwrap(),
            "the host is stopped after the exit sync"
        );
        assert!(!sessions.channel(LAUNCH).exists());
        assert_eq!(
            fs::read(context.save_folder.join("remote/SaveData.bin")).unwrap(),
            b"exit save"
        );
        // A second finish is a no-op.
        sessions.finish(LAUNCH, Duration::from_secs(1)).unwrap();
    }

    #[test]
    fn a_conflict_writes_the_cloud_side_and_the_pick_is_resolved_before_the_next_launch() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("private")).unwrap();
        let export = temp.path().join("export.zip");
        fs::write(&export, zip(&[("remote/s", b"local")])).unwrap();
        let launches = temp.path().join("launches");
        let units = Arc::new(ProcessHostUnits::new(&launches));
        let sessions = Sessions::new(units.clone(), &launches, unsafe { libc::geteuid() });
        let context = context(temp.path());
        let host = session_host(temp.path(), &export, true);
        sessions
            .start(
                LAUNCH,
                &plan(host.clone(), temp.path()),
                Some(context.clone()),
            )
            .unwrap();
        Channel::open(&sessions.channel(LAUNCH))
            .send_within(Command::Launched(70), Duration::from_secs(5))
            .unwrap();
        // The shim refuses the game; the session ends with no exit sync.
        std::thread::sleep(Duration::from_millis(800));
        sessions.finish(LAUNCH, Duration::from_millis(500)).unwrap();
        let store = NativeSaveStore::new(&context.private_root);
        use crate::saves::SaveRepository;
        let records = store.records(PERSON, "half-life").unwrap();
        assert_eq!(
            records.len(),
            2,
            "both sides, before the refused session ends"
        );
        let cloud = records
            .iter()
            .find(|record| crate::steam_engine::saves::in_steam_cloud(record))
            .map(|record| crate::saves::record::snapshot_id(record).unwrap())
            .unwrap();
        assert!(
            sessions.conflicts.take_launch(LAUNCH),
            "the refused session takes no snapshot of its own"
        );
        assert_eq!(
            sessions.conflicts.pick(PERSON, "half-life", &cloud),
            Some(crate::steam_engine::protocol::Side::Cloud)
        );
        let next = "fedcba9876543210fedcba9876543210";
        sessions
            .start(next, &plan(host, temp.path()), Some(context))
            .unwrap();
        let resolved = sessions.channel(next).join("resolved");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !resolved.exists() {
            assert!(Instant::now() < deadline, "no resolve reached the host");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(fs::read_to_string(&resolved).unwrap(), "resolve 70 cloud\n");
        sessions.finish(next, Duration::from_millis(100)).unwrap();
    }

    #[test]
    fn an_export_outside_the_channel_is_refused() {
        let temp = tempfile::tempdir().unwrap();
        let watch = Watch {
            launch_id: LAUNCH.into(),
            reader: EventReader::new(temp.path()),
            channel: temp.path().join("c"),
            host_channel: "/run/korri-steam-host-x/channel".into(),
            context: None,
            conflicts: Arc::default(),
            unavailable: Arc::default(),
            exit_synced: Arc::default(),
        };
        assert_eq!(
            watch
                .archive("/run/korri-steam-host-x/channel/saves/70-exit-2.tar")
                .unwrap(),
            temp.path().join("c/saves/70-exit-2.tar")
        );
        for bad in [
            "/home/korri/steam-engine/steam/userdata/x.tar",
            "/run/korri-steam-host-x/channel/saves/../../etc/passwd",
            "/run/korri-steam-host-x/channel/a/b.tar",
            "/run/korri-steam-host-x/channel/events.jsonl",
        ] {
            assert!(watch.archive(bad).is_err(), "{bad}");
        }
    }
}
