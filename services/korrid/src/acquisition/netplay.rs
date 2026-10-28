//! A peer copy uses the same space floor, identity, atomic placement and
//! library publication as a store download. There is no second content store.
use super::{game::GameContext, placement, space::Floor, Failure};
use crate::{netplay::failure, RpcFailure};
use std::{
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

pub(crate) fn room(root: &Path, bytes: u64) -> bool {
    let cache = placement::copied_release_cache(root);
    let directory = if cache.is_dir() {
        cache.as_path()
    } else {
        root
    };
    Floor::new(directory, None)
        .and_then(|floor| floor.room())
        .is_ok_and(|room| room >= bytes.saturating_add(8192))
}
fn failed(error: Failure, bytes: u64) -> RpcFailure {
    if error == Failure::NoSpace {
        failure("NotEnoughSpace", &format!("The copy needs {bytes} bytes."))
    } else {
        failure(
            "CopyFailed",
            "The release copy could not be verified or stored.",
        )
    }
}
pub(crate) struct Copy {
    work: placement::WorkFolder,
    file: fs::File,
    floor: Floor,
    expected: u64,
    written: u64,
    cache: PathBuf,
}
impl Copy {
    pub(crate) fn begin(context: &GameContext, name: &str, bytes: u64) -> Result<Self, RpcFailure> {
        Self::begin_with_probe(context, name, bytes, None)
    }
    fn begin_with_probe(
        context: &GameContext,
        name: &str,
        bytes: u64,
        probe: Option<super::space::Probe>,
    ) -> Result<Self, RpcFailure> {
        let cache = placement::create_base(
            &context.root,
            placement::copied_release_cache(&context.root),
        )
        .map_err(|error| failed(error, bytes))?;
        let mut floor = Floor::new(&cache, probe).map_err(|error| failed(error, bytes))?;
        if floor.room().map_err(|error| failed(error, bytes))? < bytes.saturating_add(8192) {
            return Err(failed(Failure::NoSpace, bytes));
        }
        floor
            .reserve_entry()
            .map_err(|error| failed(error, bytes))?;
        let work = placement::WorkFolder::create(&cache).map_err(|error| failed(error, bytes))?;
        if !super::claims::safe_file_name(name)
            || name.starts_with('.')
            || name.chars().any(char::is_control)
        {
            return Err(failed(Failure::UnsafePath, bytes));
        }
        floor
            .reserve_entry()
            .map_err(|error| failed(error, bytes))?;
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .custom_flags(libc::O_NOFOLLOW)
            .open(work.path.join(name))
            .map_err(|_| failed(Failure::Storage, bytes))?;
        self::fs::set_permissions(
            work.path.join(name),
            <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o644),
        )
        .map_err(|_| failed(Failure::Storage, bytes))?;
        Ok(Self {
            work,
            file,
            floor,
            expected: bytes,
            written: 0,
            cache,
        })
    }
    pub(crate) fn write(&mut self, bytes: &[u8]) -> Result<(), RpcFailure> {
        if self.written.saturating_add(bytes.len() as u64) > self.expected {
            return Err(failed(Failure::Storage, self.expected));
        }
        self.floor
            .reserve(bytes.len() as u64)
            .map_err(|error| failed(error, self.expected))?;
        self.file
            .write_all(bytes)
            .map_err(|_| failed(Failure::Storage, self.expected))?;
        self.written += bytes.len() as u64;
        Ok(())
    }
    pub(crate) fn finish(
        self,
        context: &GameContext,
        key: &str,
        title: &str,
    ) -> Result<String, RpcFailure> {
        if self.written != self.expected {
            return Err(failed(Failure::Storage, self.expected));
        }
        self.file
            .sync_all()
            .map_err(|_| failed(Failure::Storage, self.expected))?;
        self.floor
            .check()
            .map_err(|error| failed(error, self.expected))?;
        let (system, identity) = context
            .registry
            .systems()
            .values()
            .find_map(|system| {
                let system = &system.id;
                let rules = context.registry.content_rules(system)?;
                let identity = super::game::identify_work(&self.work.path, rules).ok()?;
                (identity.value() == key).then_some((system.clone(), identity))
            })
            .ok_or_else(|| failed(Failure::NotSystemFile, self.expected))?;
        placement::publish_acquired_game(&self.cache, self.work, key)
            .map_err(|error| failed(error, self.expected))?;
        context
            .discovery
            .record_copied_release(title, &system, &identity)
            .map_err(|error| {
                log::debug!("netplay library record: {error}");
                failed(Failure::Storage, self.expected)
            })
    }
}

/// Same queue and sleep inhibitor as provider downloads.
pub(crate) struct CopyTurn {
    _turn: super::Turn,
}
pub(crate) async fn turn(
    coordinator: &super::AcquisitionCoordinator,
    root: &Path,
    key: &str,
) -> CopyTurn {
    let turn = coordinator
        .queue_turn(
            placement::copied_release_cache(root)
                .join(placement::ready_name(key).expect("validated release key")),
        )
        .turn()
        .await;
    CopyTurn { _turn: turn }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        discovery::{DiscoveryLifecycleCoordinator, FolderSelectionGrantStore},
        plugin::{load_plugin_source, PluginRegistry},
        plugin_policy::RegistrySource,
    };
    use sha2::{Digest, Sha256};
    use std::sync::{Arc, Mutex};
    const CONTENT: &[u8] = b"a real file release";
    fn key() -> String {
        format!("sha256:{}", hex::encode(Sha256::digest(CONTENT)))
    }
    fn fixture() -> (tempfile::TempDir, GameContext) {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("catalog")).unwrap();
        fs::write(root.path().join("device.yaml"), "{}\n").unwrap();
        fs::write(root.path().join("catalog/library.yaml"), "{}\n").unwrap();
        let plugin = load_plugin_source("@test", "export const name='core'; export const systems={snes:{title:'Super Nintendo',content:{extensions:['sfc']}}};").unwrap();
        let registry = PluginRegistry::new(vec![plugin], ["@test:core".into()]).unwrap();
        let private_root = root.path().join("private");
        let discovery = DiscoveryLifecycleCoordinator::new_with_registry_source(
            root.path(),
            &private_root,
            Arc::new(Mutex::new(())),
            FolderSelectionGrantStore::default(),
            RegistrySource::Selected(Arc::new(registry.clone())),
        );
        let context = GameContext {
            root: root.path().into(),
            private_root,
            registry,
            discovery,
        };
        (root, context)
    }
    #[test]
    fn netplay_copy_checks_hash_publishes_atomically_and_records_the_library_release() {
        let (_root, context) = fixture();
        let mut copy = Copy::begin(&context, "Turtles.sfc", CONTENT.len() as u64).unwrap();
        copy.write(&CONTENT[..4]).unwrap();
        copy.write(&CONTENT[4..]).unwrap();
        let game = copy.finish(&context, &key(), "Turtles").unwrap();
        let ready = context
            .root
            .join("cache")
            .join(placement::ready_name(&key()).unwrap());
        assert_eq!(fs::read(ready.join("Turtles.sfc")).unwrap(), CONTENT);
        let snapshot = context.discovery.published().unwrap();
        assert_eq!(
            snapshot.library[&game].releases[0]
                .identity
                .as_ref()
                .unwrap()
                .value(),
            key()
        );
        assert!(
            snapshot.provider_links.is_empty(),
            "a peer must not invent a provider"
        );
        assert_eq!(
            super::super::game::acquired_game_dirs(
                &context.root,
                &context.registry,
                &snapshot,
                &game
            )[0]
            .1,
            ready
        );
    }
    #[test]
    fn netplay_copy_cancel_or_hash_failure_leaves_no_partial_file_or_library_record() {
        let (_root, context) = fixture();
        let mut copy = Copy::begin(&context, "Turtles.sfc", CONTENT.len() as u64).unwrap();
        copy.write(&CONTENT[..4]).unwrap();
        let work = copy.work.path.clone();
        drop(copy);
        assert!(!work.exists());
        let mut copy = Copy::begin(&context, "Turtles.sfc", CONTENT.len() as u64).unwrap();
        copy.write(CONTENT).unwrap();
        let work = copy.work.path.clone();
        assert_eq!(
            copy.finish(&context, &format!("sha256:{}", "0".repeat(64)), "Turtles")
                .unwrap_err()
                .code,
            "CopyFailed"
        );
        assert!(!work.exists());
        assert!(context.discovery.published().unwrap().library.is_empty());
    }
    #[test]
    fn netplay_copy_checks_the_existing_space_floor_before_any_file_write() {
        let (_root, context) = fixture();
        let result = Copy::begin_with_probe(
            &context,
            "Turtles.sfc",
            12345,
            Some(super::super::space::fixed(12345)),
        );
        let failure = result.err().unwrap();
        assert_eq!(failure.code, "NotEnoughSpace");
        assert!(failure.message.contains("12345"));
        assert!(fs::read_dir(context.root.join("cache"))
            .unwrap()
            .next()
            .is_none());
    }
}
