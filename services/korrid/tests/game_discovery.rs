use std::{
    fs,
    sync::{Arc, Mutex},
    time::Duration,
};

use korrid::{
    discovery::{
        attention::{AttentionDecisions, AttentionFinding, AttentionKind},
        coordinator::DiscoveryRequestError,
        sources::PairingError,
        DiscoveryCoordinator, DiscoveryError, DiscoveryLifecycleCoordinator, DiscoveryOptions,
        DiscoveryPhase, FolderSelectionGrantError, FolderSelectionGrantStore,
    },
    plugin::PluginRegistry,
    plugin_policy::RegistrySource,
};
use sha2::{Digest, Sha256};

const PAIRING: &str = "gameboy\n/games";
fn registry() -> RegistrySource {
    let plugin = korrid::plugin::load_plugin_source(
        "@korri",
        r#"
export const name = "discovery-test";
export const systems = {
  gb: { id: "gb", title: "Game Boy", content: { extensions: ["gb"] } },
  gbc: { id: "gbc", title: "Game Boy Color", content: { extensions: ["gb"] } },
};
export const groups = { gameboy: { id: "gameboy", title: "Game Boy", systems: ["gb", "gbc"] } };
"#,
    )
    .unwrap();
    RegistrySource::Selected(Arc::new(
        PluginRegistry::new(vec![plugin], vec!["@korri:discovery-test".into()]).unwrap(),
    ))
}

fn options() -> DiscoveryOptions {
    DiscoveryOptions {
        first_seen_at: "2026-10-16T00:00:00Z".into(),
        ..DiscoveryOptions::default()
    }
}

fn lifecycle(
    readable: &tempfile::TempDir,
    private: &tempfile::TempDir,
    grants: FolderSelectionGrantStore,
) -> DiscoveryLifecycleCoordinator {
    DiscoveryLifecycleCoordinator::new_with_registry_source(
        readable.path(),
        private.path(),
        Arc::new(Mutex::new(())),
        grants,
        registry(),
    )
    .with_options(options())
}

fn idle(discovery: &DiscoveryLifecycleCoordinator) -> korrid::discovery::DiscoverySnapshot {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let snapshot = discovery.snapshot();
        if !matches!(
            snapshot.state,
            DiscoveryPhase::Scanning | DiscoveryPhase::Enriching
        ) {
            assert_eq!(
                snapshot.state,
                DiscoveryPhase::Idle,
                "{:?}",
                snapshot.diagnostics
            );
            return snapshot;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "discovery did not settle"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn folder_selection_receipts_are_one_use_short_lived_and_pathless() {
    let folder = tempfile::tempdir().unwrap();
    let grants = FolderSelectionGrantStore::new(Duration::from_millis(20));
    let receipt = grants.issue_approved_path(folder.path()).unwrap().token;
    assert!(!receipt.contains(folder.path().to_string_lossy().as_ref()));
    assert_eq!(
        grants.consume(&receipt).unwrap(),
        folder.path().canonicalize().unwrap()
    );
    assert_eq!(
        grants.consume(&receipt).unwrap_err(),
        FolderSelectionGrantError::Unknown
    );
    let expired = grants.issue_approved_path(folder.path()).unwrap().token;
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(
        grants.consume(&expired).unwrap_err(),
        FolderSelectionGrantError::Expired
    );
}

#[test]
fn attention_persists_until_actual_bytes_change_and_drops_removed_sources_at_save() {
    let private = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("x.gb");
    fs::write(&file, b"first bytes").unwrap();
    let identity = || {
        format!(
            "sha256:{}",
            hex::encode(Sha256::digest(fs::read(&file).unwrap()))
        )
    };
    let mut finding = AttentionFinding {
        kind: AttentionKind::Clash,
        source_id: "gameboy".into(),
        path: Some("x.gb".into()),
        identity: Some(identity()),
        candidates: vec!["gb".into(), "gbc".into()],
        message: "choose a system".into(),
    };
    let mut decisions = AttentionDecisions::default();
    decisions.assign(&finding, "gbc", PAIRING).unwrap();
    decisions.ignore(&finding, PAIRING);
    korrid::discovery::attention::save(private.path(), &decisions).unwrap();
    let mut loaded = korrid::discovery::attention::load(private.path()).unwrap();
    assert!(loaded.is_ignored(&finding, PAIRING));
    assert_eq!(
        loaded.assigned_system(
            "gameboy",
            "x.gb",
            finding.identity.as_deref().unwrap(),
            PAIRING
        ),
        Some("gbc")
    );
    fs::write(&file, b"second bytes").unwrap();
    finding.identity = Some(identity());
    assert!(!loaded.is_ignored(&finding, PAIRING));
    assert_eq!(
        loaded.assigned_system(
            "gameboy",
            "x.gb",
            finding.identity.as_deref().unwrap(),
            PAIRING
        ),
        None
    );
    loaded.retain_pairings(&Default::default());
    korrid::discovery::attention::save(private.path(), &loaded).unwrap();
    assert_eq!(
        korrid::discovery::attention::load(private.path()).unwrap(),
        AttentionDecisions::default()
    );
}

#[test]
fn startup_pair_change_unpair_and_rescan_publish_sources_without_removing_original_files() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    fs::write(folder.path().join("game.gb"), b"game bytes").unwrap();
    let grants = FolderSelectionGrantStore::default();
    let discovery = lifecycle(&readable, &private, grants.clone());
    let startup = idle(&discovery);
    let receipt = grants.issue_approved_path(folder.path()).unwrap().token;
    discovery.pair(&receipt, "gb").unwrap();
    assert!(matches!(
        discovery.pair(&receipt, "gb"),
        Err(DiscoveryRequestError::Receipt(
            FolderSelectionGrantError::Unknown
        ))
    ));
    let paired = idle(&discovery);
    assert_ne!(paired.generation, startup.generation);
    assert_eq!(paired.sources.len(), 1);
    let source = &paired.sources[0].id;
    discovery.change(source, "gbc").unwrap();
    let changed = idle(&discovery);
    assert_eq!(changed.sources[0].system, "gbc");
    let source = &changed.sources[0].id;
    let documents = ["device.yaml", "catalog/library.yaml"];
    let before = documents.map(|path| fs::read(readable.path().join(path)).unwrap());
    discovery.rescan();
    idle(&discovery);
    assert_eq!(
        documents.map(|path| fs::read(readable.path().join(path)).unwrap()),
        before
    );
    discovery.unpair(source).unwrap();
    assert!(idle(&discovery).sources.is_empty());
    assert_eq!(
        fs::read(folder.path().join("game.gb")).unwrap(),
        b"game bytes"
    );
}

#[test]
fn paired_group_attention_ignore_expires_and_assign_resolves_the_clash() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("ambiguous.gb");
    fs::write(&file, b"first bytes").unwrap();
    let grants = FolderSelectionGrantStore::default();
    let discovery = lifecycle(&readable, &private, grants.clone());
    idle(&discovery);
    discovery
        .pair(
            &grants.issue_approved_path(folder.path()).unwrap().token,
            "gameboy",
        )
        .unwrap();
    idle(&discovery);
    let finding = discovery.attention().unwrap().pop().unwrap();
    assert_eq!(finding.kind, AttentionKind::Clash);
    assert!(matches!(
        discovery.assign(&finding.item_id(), "gba"),
        Err(DiscoveryRequestError::NotACandidate)
    ));
    assert!(discovery.ignore(&finding.item_id()).unwrap().is_empty());
    discovery.rescan();
    idle(&discovery);
    assert!(discovery.attention().unwrap().is_empty());
    fs::write(file, b"new bytes").unwrap();
    discovery.rescan();
    idle(&discovery);
    let changed = discovery.attention().unwrap().pop().unwrap();
    assert_eq!(changed.item_id(), finding.item_id());
    discovery.assign(&changed.item_id(), "gbc").unwrap();
    idle(&discovery);
    assert!(discovery.attention().unwrap().is_empty());
    let decisions = korrid::discovery::attention::load(private.path()).unwrap();
    assert_eq!(
        decisions.assigned_system(
            &changed.source_id,
            "ambiguous.gb",
            changed.identity.as_deref().unwrap(),
            &korrid::discovery::sources::pairing_key("gameboy", folder.path())
        ),
        Some("gbc")
    );
}

#[test]
fn pairing_refuses_overlap_plugin_data_and_unknown_system() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let child = folder.path().join("child");
    fs::create_dir(&child).unwrap();
    let coordinator =
        DiscoveryCoordinator::new(readable.path(), private.path()).with_registry_source(registry());
    coordinator.pair(child.clone(), "gb", &options()).unwrap();
    assert!(matches!(
        coordinator.pair(folder.path().into(), "gb", &options()),
        Err(DiscoveryError::Pairing(PairingError::SourceOverlap(_)))
    ));
    let account = readable
        .path()
        .join("users/f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9");
    fs::create_dir_all(&account).unwrap();
    assert!(matches!(
        coordinator.pair(account, "gb", &options()),
        Err(DiscoveryError::Pairing(PairingError::PluginData))
    ));
    let other = tempfile::tempdir().unwrap();
    assert!(matches!(
        coordinator.pair(other.path().into(), "unknown", &options()),
        Err(DiscoveryError::Pairing(PairingError::UnknownSystem(_)))
    ));
}

#[test]
fn hand_edited_pairings_expire_ignored_and_assigned_decisions() {
    let readable = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    for root in [first.path(), second.path()] {
        fs::write(root.join("same.gb"), b"same bytes").unwrap();
    }
    let grants = FolderSelectionGrantStore::default();
    let discovery = lifecycle(&readable, &private, grants.clone());
    idle(&discovery);
    discovery
        .pair(
            &grants.issue_approved_path(first.path()).unwrap().token,
            "gameboy",
        )
        .unwrap();
    idle(&discovery);
    let finding = discovery.attention().unwrap().pop().unwrap();
    discovery.ignore(&finding.item_id()).unwrap();
    let device_path = readable.path().join("device.yaml");
    let mut device: serde_yaml::Value =
        serde_yaml::from_slice(&fs::read(&device_path).unwrap()).unwrap();
    device["storage"]["gameboy"]["root"] = second.path().display().to_string().into();
    fs::write(&device_path, serde_yaml::to_string(&device).unwrap()).unwrap();
    discovery.rescan();
    idle(&discovery);
    let finding = discovery
        .attention()
        .unwrap()
        .pop()
        .expect("a root edit must expire an ignore even with identical bytes");
    assert!(korrid::discovery::attention::load(private.path())
        .unwrap()
        .ignored
        .is_empty());
    discovery.assign(&finding.item_id(), "gbc").unwrap();
    idle(&discovery);
    assert_eq!(
        korrid::discovery::attention::load(private.path())
            .unwrap()
            .assigned[0]
            .pairing,
        korrid::discovery::sources::pairing_key("gameboy", second.path())
    );
    device["sources"]["gameboy"]["system"] = "temporarily-unavailable".into();
    fs::write(&device_path, serde_yaml::to_string(&device).unwrap()).unwrap();
    discovery.rescan();
    idle(&discovery);
    assert!(korrid::discovery::attention::load(private.path())
        .unwrap()
        .assigned
        .is_empty());
    let unavailable = discovery.attention().unwrap().pop().unwrap();
    discovery.ignore(&unavailable.item_id()).unwrap();
    device["sources"]["gameboy"]["system"] = "another-unavailable-system".into();
    fs::write(&device_path, serde_yaml::to_string(&device).unwrap()).unwrap();
    discovery.rescan();
    idle(&discovery);
    assert_eq!(discovery.attention().unwrap().len(), 1);
    assert!(korrid::discovery::attention::load(private.path())
        .unwrap()
        .ignored
        .is_empty());
}
