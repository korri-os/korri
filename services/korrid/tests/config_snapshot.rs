#[path = "fixtures/readable.rs"]
mod readable;
use std::fs;
use std::sync::Arc;
use std::thread;

use korrid::config::snapshot::{
    ConfigSnapshotCoordinator, SnapshotAuthorization, SnapshotDiagnosticCode, DEVICE_FILE_NAME,
    EMPTY_DOCUMENT_BYTES, GAMES_FILE_NAME, RELEASES_FILE_NAME,
};
use proseql_engine::errors::{EngineError, StorageError, StorageOperation};
use proseql_storage::host::{StorageEvent, StorageHost, WatchHandle};

const CHECKPOINT_CONFIG: &str =
    include_str!("../../../docs/research/android-app-plugin-schema-checkpoint/device.yaml");
const CHECKPOINT_LIBRARY: &str =
    include_str!("../../../docs/research/android-app-plugin-schema-checkpoint/catalog/games.yaml");

#[test]
fn empty_storage_root_creates_only_canonical_fixed_documents_and_loads_empty_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let coordinator = ConfigSnapshotCoordinator::new(root.path());

    let state = coordinator.reload();

    assert_eq!(state.authorization, SnapshotAuthorization::Authorized);
    assert_eq!(state.generation, 1);
    assert!(state.diagnostic.is_none());
    assert!(state.snapshot.host.is_none());
    assert!(state.snapshot.games.is_empty());
    assert_eq!(
        fs::read(root.path().join(DEVICE_FILE_NAME)).unwrap(),
        EMPTY_DOCUMENT_BYTES
    );
    assert_eq!(
        fs::read(root.path().join(GAMES_FILE_NAME)).unwrap(),
        EMPTY_DOCUMENT_BYTES
    );

    let mut names: Vec<String> = fs::read_dir(root.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, vec!["catalog", DEVICE_FILE_NAME]);
}

#[test]
fn existing_file_is_preserved_and_only_the_missing_fixed_file_is_created() {
    let root = tempfile::tempdir().unwrap();
    let existing_config = "host:\n  title: preserved\n";
    readable::write(root.path().join(DEVICE_FILE_NAME), existing_config).unwrap();

    let state = ConfigSnapshotCoordinator::new(root.path()).reload();

    assert_eq!(state.authorization, SnapshotAuthorization::Authorized);
    assert_eq!(
        state.snapshot.host.as_ref().unwrap().title.as_deref(),
        Some("preserved")
    );
    assert_eq!(
        fs::read_to_string(root.path().join(DEVICE_FILE_NAME)).unwrap(),
        existing_config
    );
    assert_eq!(
        fs::read(root.path().join(GAMES_FILE_NAME)).unwrap(),
        EMPTY_DOCUMENT_BYTES
    );
}

#[test]
fn additional_files_do_not_contribute_to_the_fixed_snapshot() {
    let root = tempfile::tempdir().unwrap();
    readable::write(root.path().join(DEVICE_FILE_NAME), "{}\n").unwrap();
    readable::write(root.path().join(GAMES_FILE_NAME), "{}\n").unwrap();
    readable::write(root.path().join(RELEASES_FILE_NAME), "{}\n").unwrap();
    readable::write(
        root.path().join("extra.yaml"),
        "library:\n  should-not-load:\n    releases:\n      - id: android\n        system: android\n",
    )
    .unwrap();

    let state = ConfigSnapshotCoordinator::new(root.path()).reload();

    assert_eq!(state.authorization, SnapshotAuthorization::Authorized);
    assert!(state.diagnostic.is_none());
    assert!(state.snapshot.games.is_empty());
}

#[test]
fn exact_checkpoint_pair_loads_as_one_authorized_snapshot() {
    let root = tempfile::tempdir().unwrap();
    readable::write(root.path().join(DEVICE_FILE_NAME), CHECKPOINT_CONFIG).unwrap();
    readable::write(root.path().join(GAMES_FILE_NAME), CHECKPOINT_LIBRARY).unwrap();
    readable::write(
        root.path().join(RELEASES_FILE_NAME),
        readable::ANDROID_RELEASES,
    )
    .unwrap();

    let state = ConfigSnapshotCoordinator::new(root.path()).reload();

    assert_eq!(state.authorization, SnapshotAuthorization::Authorized);
    assert_eq!(state.generation, 1);
    assert!(state.diagnostic.is_none());
    assert_eq!(
        state.snapshot.host.as_ref().unwrap().title.as_deref(),
        Some("usu")
    );
    assert!(state
        .snapshot
        .games
        .contains_key("01K4J6K8Y00000000000000001"));
}

#[test]
fn storage_records_load_without_marking_the_snapshot_unsupported() {
    let root = tempfile::tempdir().unwrap();
    readable::write(
        root.path().join(DEVICE_FILE_NAME),
        format!(
            "host:\n  title: usu\nstorage:\n  selected-gba:\n    root: {}\n",
            root.path().display()
        ),
    )
    .unwrap();
    readable::write(root.path().join(GAMES_FILE_NAME), "{}\n").unwrap();
    readable::write(root.path().join(RELEASES_FILE_NAME), "{}\n").unwrap();

    let state = ConfigSnapshotCoordinator::new(root.path()).reload();

    assert_eq!(state.authorization, SnapshotAuthorization::Authorized);
    assert!(state.diagnostic.is_none());
    assert!(state.snapshot.storage.contains_key("selected-gba"));
}

#[test]
fn malformed_or_unsupported_reload_retains_last_known_good_and_diagnostic_until_valid_reload() {
    let root = tempfile::tempdir().unwrap();
    readable::write(root.path().join(DEVICE_FILE_NAME), CHECKPOINT_CONFIG).unwrap();
    readable::write(root.path().join(GAMES_FILE_NAME), CHECKPOINT_LIBRARY).unwrap();
    readable::write(
        root.path().join(RELEASES_FILE_NAME),
        readable::ANDROID_RELEASES,
    )
    .unwrap();
    let coordinator = ConfigSnapshotCoordinator::new(root.path());
    let good = coordinator.reload();

    readable::write(
        root.path().join(GAMES_FILE_NAME),
        "library:\n  bad id:\n    releases: []\n",
    )
    .unwrap();
    let stale = coordinator.reload();

    assert_eq!(stale.authorization, SnapshotAuthorization::Authorized);
    assert_eq!(stale.generation, good.generation);
    assert!(stale
        .snapshot
        .games
        .contains_key("01K4J6K8Y00000000000000001"));
    let diagnostic = stale.diagnostic.as_ref().expect("reload diagnostic");
    assert_eq!(
        diagnostic.code,
        SnapshotDiagnosticCode::LocalConfigReloadFailed
    );
    assert!(diagnostic.message.contains(GAMES_FILE_NAME));
    assert!(!diagnostic.message.contains(root.path().to_str().unwrap()));

    readable::write(root.path().join(GAMES_FILE_NAME), "{}\n").unwrap();
    readable::write(root.path().join(RELEASES_FILE_NAME), "{}\n").unwrap();
    let recovered = coordinator.reload();

    assert_eq!(recovered.authorization, SnapshotAuthorization::Authorized);
    assert!(recovered.generation > stale.generation);
    assert!(recovered.diagnostic.is_none());
    assert!(recovered.snapshot.games.is_empty());
}

#[test]
fn unsupported_populated_behavior_retains_last_known_good_with_unsupported_diagnostic() {
    let root = tempfile::tempdir().unwrap();
    readable::write(root.path().join(DEVICE_FILE_NAME), CHECKPOINT_CONFIG).unwrap();
    readable::write(root.path().join(GAMES_FILE_NAME), CHECKPOINT_LIBRARY).unwrap();
    readable::write(
        root.path().join(RELEASES_FILE_NAME),
        readable::ANDROID_RELEASES,
    )
    .unwrap();
    let coordinator = ConfigSnapshotCoordinator::new(root.path());
    let good = coordinator.reload();

    readable::write(
        root.path().join(DEVICE_FILE_NAME),
        "host:\n  title: usu\n  moonlight:\n    platform:\n      name: v4l2m2m\n",
    )
    .unwrap();
    let stale = coordinator.reload();

    assert_eq!(stale.authorization, SnapshotAuthorization::Authorized);
    assert_eq!(stale.generation, good.generation);
    assert!(stale
        .snapshot
        .games
        .contains_key("01K4J6K8Y00000000000000001"));
    let diagnostic = stale.diagnostic.as_ref().expect("unsupported diagnostic");
    assert_eq!(
        diagnostic.code,
        SnapshotDiagnosticCode::LocalConfigUnsupported
    );
    assert!(diagnostic.message.contains("host.moonlight"));
}

#[test]
fn coordinator_publishes_the_once_captured_documents_through_graph_conversion() {
    let root = tempfile::tempdir().unwrap();
    readable::write(
        root.path().join(DEVICE_FILE_NAME),
        "host:\n  title: captured\n",
    )
    .unwrap();
    readable::write(root.path().join(GAMES_FILE_NAME), "{}\n").unwrap();
    readable::write(root.path().join(RELEASES_FILE_NAME), "{}\n").unwrap();
    let storage = Arc::new(
        SwitchableStorageHost::new(root.path().to_owned())
            .with_config_second_read("host:\n  title: second-read\n"),
    );
    let coordinator = ConfigSnapshotCoordinator::with_storage(root.path(), storage.clone());

    let state = coordinator.reload();

    assert_eq!(state.authorization, SnapshotAuthorization::Authorized);
    assert_eq!(state.generation, 1);
    assert!(state.diagnostic.is_none());
    assert_eq!(
        state.snapshot.host.as_ref().unwrap().title.as_deref(),
        Some("captured")
    );
    assert_ne!(
        state.snapshot.host.as_ref().unwrap().title.as_deref(),
        Some("second-read")
    );
    assert_eq!(storage.read_count(DEVICE_FILE_NAME), 1);
    assert_eq!(storage.read_count("games.yaml"), 1);
}

#[test]
fn storage_failures_withhold_the_retained_snapshot_until_a_valid_reload_reauthorizes_it() {
    let root = tempfile::tempdir().unwrap();
    readable::write(root.path().join(DEVICE_FILE_NAME), CHECKPOINT_CONFIG).unwrap();
    readable::write(root.path().join(GAMES_FILE_NAME), CHECKPOINT_LIBRARY).unwrap();
    readable::write(
        root.path().join(RELEASES_FILE_NAME),
        readable::ANDROID_RELEASES,
    )
    .unwrap();
    let storage = Arc::new(SwitchableStorageHost::new(root.path().to_owned()));
    let coordinator = ConfigSnapshotCoordinator::with_storage(root.path(), storage.clone());

    let good = coordinator.reload();
    storage.deny();
    let denied = coordinator.reload();

    assert_eq!(denied.authorization, SnapshotAuthorization::Unauthorized);
    assert_eq!(denied.generation, good.generation);
    assert!(denied
        .snapshot
        .games
        .contains_key("01K4J6K8Y00000000000000001"));
    assert_eq!(
        denied.diagnostic.as_ref().unwrap().code,
        SnapshotDiagnosticCode::LocalConfigUnauthorized
    );

    storage.allow();
    let reauthorized = coordinator.reload();
    assert_eq!(
        reauthorized.authorization,
        SnapshotAuthorization::Authorized
    );
    assert!(reauthorized.generation > denied.generation);
    assert!(reauthorized.diagnostic.is_none());
}

#[test]
fn partial_initialization_is_idempotent_and_does_not_publish_a_candidate() {
    let root = tempfile::tempdir().unwrap();
    let storage =
        Arc::new(SwitchableStorageHost::new(root.path().to_owned()).fail_write_once("games.yaml"));
    let coordinator = ConfigSnapshotCoordinator::with_storage(root.path(), storage.clone());

    let failed = coordinator.reload();

    assert_eq!(failed.authorization, SnapshotAuthorization::Unauthorized);
    assert_eq!(failed.generation, 0);
    assert!(failed.snapshot.games.is_empty());
    assert_eq!(
        fs::read(root.path().join(DEVICE_FILE_NAME)).unwrap(),
        EMPTY_DOCUMENT_BYTES
    );
    assert!(!root.path().join(GAMES_FILE_NAME).exists());

    let converged = coordinator.reload();
    assert_eq!(converged.authorization, SnapshotAuthorization::Authorized);
    assert_eq!(converged.generation, 1);
    assert_eq!(
        fs::read(root.path().join(DEVICE_FILE_NAME)).unwrap(),
        EMPTY_DOCUMENT_BYTES
    );
    assert_eq!(
        fs::read(root.path().join(GAMES_FILE_NAME)).unwrap(),
        EMPTY_DOCUMENT_BYTES
    );
}

#[test]
fn serialized_reloads_prevent_out_of_order_publication_and_clones_share_state() {
    let root = tempfile::tempdir().unwrap();
    readable::write(root.path().join(DEVICE_FILE_NAME), CHECKPOINT_CONFIG).unwrap();
    readable::write(root.path().join(GAMES_FILE_NAME), CHECKPOINT_LIBRARY).unwrap();
    readable::write(
        root.path().join(RELEASES_FILE_NAME),
        readable::ANDROID_RELEASES,
    )
    .unwrap();
    let storage = Arc::new(SwitchableStorageHost::new(root.path().to_owned()));
    storage.block_reads();
    let coordinator = ConfigSnapshotCoordinator::with_storage(root.path(), storage.clone());
    let clone = coordinator.clone();

    let first = thread::spawn(move || clone.reload());
    storage.wait_until_read_is_blocked();
    readable::write(root.path().join(GAMES_FILE_NAME), "{}\n").unwrap();
    readable::write(root.path().join(RELEASES_FILE_NAME), "{}\n").unwrap();
    storage.unblock_reads();
    let first_state = first.join().unwrap();
    let second_state = coordinator.reload();

    assert_eq!(first_state.generation, 1);
    assert_eq!(second_state.generation, 2);
    assert!(second_state.snapshot.games.is_empty());
    assert_eq!(coordinator.current().generation, 2);
}

#[derive(Debug)]
struct SwitchableStorageHost {
    denied: std::sync::atomic::AtomicBool,
    fail_write_once: std::sync::Mutex<Option<String>>,
    read_gate: (std::sync::Mutex<ReadGate>, std::sync::Condvar),
    config_second_read: std::sync::Mutex<Option<String>>,
    read_counts: std::sync::Mutex<std::collections::BTreeMap<String, usize>>,
}

#[derive(Debug)]
struct ReadGate {
    block: bool,
    blocked: bool,
}

impl SwitchableStorageHost {
    fn new(_root: std::path::PathBuf) -> Self {
        Self {
            denied: std::sync::atomic::AtomicBool::new(false),
            fail_write_once: std::sync::Mutex::new(None),
            read_gate: (
                std::sync::Mutex::new(ReadGate {
                    block: false,
                    blocked: false,
                }),
                std::sync::Condvar::new(),
            ),
            config_second_read: std::sync::Mutex::new(None),
            read_counts: std::sync::Mutex::new(std::collections::BTreeMap::new()),
        }
    }

    fn fail_write_once(self, filename: &str) -> Self {
        *self.fail_write_once.lock().unwrap() = Some(filename.to_owned());
        self
    }

    fn with_config_second_read(self, yaml: &str) -> Self {
        *self.config_second_read.lock().unwrap() = Some(yaml.to_owned());
        self
    }

    fn read_count(&self, filename: &str) -> usize {
        self.read_counts
            .lock()
            .unwrap()
            .get(filename)
            .copied()
            .unwrap_or(0)
    }

    fn deny(&self) {
        self.denied.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn allow(&self) {
        self.denied
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    fn block_reads(&self) {
        self.read_gate.0.lock().unwrap().block = true;
    }

    fn wait_until_read_is_blocked(&self) {
        let (lock, cvar) = &self.read_gate;
        let mut gate = lock.lock().unwrap();
        while !gate.blocked {
            gate = cvar.wait(gate).unwrap();
        }
    }

    fn unblock_reads(&self) {
        let (lock, cvar) = &self.read_gate;
        let mut gate = lock.lock().unwrap();
        gate.block = false;
        cvar.notify_all();
    }

    fn check_allowed(&self, path: &str, operation: StorageOperation) -> Result<(), EngineError> {
        if self.denied.load(std::sync::atomic::Ordering::SeqCst) {
            Err(test_storage_error(path, operation, "storage access denied"))
        } else {
            Ok(())
        }
    }

    fn maybe_block_read(&self) {
        let (lock, cvar) = &self.read_gate;
        let mut gate = lock.lock().unwrap();
        if gate.block {
            gate.blocked = true;
            cvar.notify_all();
            while gate.block {
                gate = cvar.wait(gate).unwrap();
            }
        }
    }
}

impl StorageHost for SwitchableStorageHost {
    fn read(&self, path: &str) -> Result<String, EngineError> {
        self.check_allowed(path, StorageOperation::Read)?;
        self.maybe_block_read();
        let filename = std::path::Path::new(path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let read_index = {
            let mut read_counts = self.read_counts.lock().unwrap();
            let count = read_counts.entry(filename.clone()).or_insert(0);
            let read_index = *count;
            *count += 1;
            read_index
        };
        if filename == DEVICE_FILE_NAME && read_index > 0 {
            if let Some(candidate) = self.config_second_read.lock().unwrap().clone() {
                return Ok(candidate);
            }
        }
        fs::read_to_string(path)
            .map_err(|error| test_storage_error(path, StorageOperation::Read, error.to_string()))
    }

    fn write(&self, path: &str, data: &str) -> Result<(), EngineError> {
        self.check_allowed(path, StorageOperation::Write)?;
        let filename = std::path::Path::new(path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut fail_once = self.fail_write_once.lock().unwrap();
        if fail_once.as_deref() == Some(filename.as_str()) {
            fail_once.take();
            return Err(test_storage_error(
                path,
                StorageOperation::Write,
                "scripted write failure",
            ));
        }
        drop(fail_once);
        readable::write(path, data)
            .map_err(|error| test_storage_error(path, StorageOperation::Write, error.to_string()))
    }

    fn append(&self, path: &str, data: &str) -> Result<(), EngineError> {
        self.check_allowed(path, StorageOperation::Write)?;
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| {
                test_storage_error(path, StorageOperation::Write, error.to_string())
            })?;
        file.write_all(data.as_bytes())
            .map_err(|error| test_storage_error(path, StorageOperation::Write, error.to_string()))
    }

    fn exists(&self, path: &str) -> Result<bool, EngineError> {
        self.check_allowed(path, StorageOperation::Read)?;
        Ok(std::path::Path::new(path).exists())
    }

    fn remove(&self, path: &str) -> Result<(), EngineError> {
        self.check_allowed(path, StorageOperation::Delete)?;
        fs::remove_file(path)
            .map_err(|error| test_storage_error(path, StorageOperation::Delete, error.to_string()))
    }

    fn ensure_dir(&self, path: &str) -> Result<(), EngineError> {
        self.check_allowed(path, StorageOperation::Write)?;
        fs::create_dir_all(path)
            .map_err(|error| test_storage_error(path, StorageOperation::Write, error.to_string()))
    }

    fn list_directory(&self, dir_path: &str) -> Result<Vec<String>, EngineError> {
        self.check_allowed(dir_path, StorageOperation::List)?;
        let mut entries = fs::read_dir(dir_path)
            .map_err(|error| {
                test_storage_error(dir_path, StorageOperation::List, error.to_string())
            })?
            .map(|entry| entry.unwrap().path().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        entries.sort();
        Ok(entries)
    }

    fn list_recursive(&self, root_path: &str) -> Result<Vec<String>, EngineError> {
        let mut files = self.list_directory(root_path)?;
        let dirs: Vec<_> = files
            .iter()
            .filter(|path| std::path::Path::new(path).is_dir())
            .cloned()
            .collect();
        for dir in dirs {
            files.extend(self.list_recursive(&dir)?);
        }
        Ok(files)
    }

    fn watch(
        &self,
        _path: &str,
        _on_change: Box<dyn Fn(StorageEvent) + Send + Sync>,
    ) -> Result<Box<dyn WatchHandle>, EngineError> {
        panic!("snapshot loading must not create watchers")
    }

    fn watch_dir(
        &self,
        _path: &str,
        _on_change: Box<dyn Fn(StorageEvent) + Send + Sync>,
    ) -> Result<Box<dyn WatchHandle>, EngineError> {
        panic!("snapshot loading must not create watchers")
    }
}

fn test_storage_error(
    path: &str,
    operation: StorageOperation,
    message: impl Into<String>,
) -> EngineError {
    EngineError::Storage(Box::new(StorageError {
        path: path.to_owned(),
        operation,
        message: message.into(),
        cause: None,
    }))
}
