//! Opt-in public-provider acceptance. Never part of the offline suite.
//! Loads real immutable publisher packages and uses the production public-DNS
//! DownloadEngine, not the loopback fixture transport or an unrestricted fetch.
//! Administrative signature/approval lifecycle remains a separate publisher gate.
use super::{
    game::GameContext, sleep, AcquireArtifactRequest, AcquireJobState, AcquisitionCoordinator,
    DetailsRequest, SearchRequest,
};
use crate::{plugin::PluginRegistry, plugin_installation::EnabledPackage};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::PathBuf, sync::Arc, time::Duration};

// Only existing producer fields, like tests/published_game_launch.rs.
#[derive(Deserialize)]
struct Manifest {
    publisher: Publisher,
    entry: String,
    sources: Vec<String>,
    files: BTreeMap<String, PathBuf>,
    #[serde(default)]
    hosts: Vec<String>,
}
#[derive(Deserialize)]
struct Publisher {
    namespace: String,
}

fn package(variable: &str, id: &str) -> EnabledPackage {
    let path =
        PathBuf::from(std::env::var_os(variable).unwrap_or_else(|| panic!("set {variable}")));
    assert_eq!(path.parent().unwrap(), std::path::Path::new("/nix/store"));
    assert_eq!(fs::canonicalize(&path).unwrap(), path);
    let manifest: Manifest =
        serde_json::from_slice(&fs::read(path.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest.publisher.namespace, "@korri");
    EnabledPackage {
        id: id.into(),
        package: path,
        files: manifest.files,
        entry: manifest.entry,
        sources: manifest.sources,
        requires: vec![],
        hosts: manifest.hosts,
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "live itch.io; needs packaged itchio/bitsybox, korrid, Python, Xvfb DISPLAY and xdotool; coordinator runs once"]
async fn public_bitsy_attachment_search_acquire_route_and_native_launch() {
    const PROVIDER: &str = "@korri:itchio";
    const ID: &str = "0neforgetfulbug/average-day-in-america";
    const PAGE: &str = "https://0neforgetfulbug.itch.io/average-day-in-america";
    const PERSON: &str = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";
    // Require all prerequisites before any live request.
    let runner_test = std::env::var_os("KORRI_LIVE_BITSY_RUNTIME_TEST")
        .expect("set the publisher's plugins/bitsybox/runtime.test.py path");
    let korrid =
        std::env::var_os("KORRI_LIVE_KORRID").expect("set the current build's korrid binary");
    assert!(std::env::var_os("DISPLAY").is_some(), "run under Xvfb");
    let provider = package("KORRI_LIVE_ITCHIO_PACKAGE", PROVIDER);
    let runner = package("KORRI_LIVE_BITSYBOX_PACKAGE", "@korri:bitsybox");
    assert_eq!(
        provider.hosts,
        [
            "itch.io",
            "0neforgetfulbug.itch.io",
            "itchio-mirror.cb031a832f44726753d6267436f3b414.r2.cloudflarestorage.com",
        ]
    );
    let runner_source = crate::script::source::SourceSnapshot::package_plugin(
        &runner.package,
        &runner.entry,
        &runner.sources,
    )
    .unwrap();
    let runner_package = runner.package.clone();
    let registry = PluginRegistry::from_installed(vec![provider, runner]).unwrap();
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let config = root.path().join("host.toml");
    fs::write(&config, "label = \"live-provider-test\"\ngames = []\n").unwrap();
    let runtime = crate::host::HostRuntime::from_paths_with_backend(
        &config,
        Some(root.path().to_owned()),
        private.path().to_owned(),
        Arc::new(crate::host::control::InMemoryLaunchUnitBackend::default()),
    )
    .with_route_registry(root.path().to_owned(), registry.clone())
    .with_library_discovery();
    let discovery = runtime.library_discovery().unwrap().clone();
    discovery.wait_until_idle(Duration::from_secs(10));
    let context = GameContext {
        root: root.path().to_owned(),
        private_root: private.path().to_owned(),
        registry,
        discovery,
    };
    // Both network engines keep their default AddressPolicy::Public. Only the
    // sleep inhibitor is in-memory: no systemd, root install or trust changes.
    let coordinator = AcquisitionCoordinator {
        sleep: sleep::SleepBlock::InMemory(Arc::new(sleep::InMemoryBlocks::default())),
        ..Default::default()
    };
    let claims = coordinator
        .games
        .search(
            &context.registry,
            &SearchRequest {
                query: "average day in america".into(),
                provider_ids: vec![PROVIDER.into()],
            },
        )
        .await
        .expect("public candidate search must succeed without retries");
    assert_eq!(claims.claims.len(), 1);
    assert_eq!(claims.claims[0].id, ID);
    assert_eq!(claims.claims[0].url, PAGE);
    assert!(
        claims.claims[0].artifact.is_none(),
        "a listing must not infer Bitsy data"
    );
    let details = coordinator
        .games
        .details(
            &context.registry,
            &DetailsRequest {
                provider_id: PROVIDER.into(),
                id: ID.into(),
            },
        )
        .await
        .expect("actual page must establish free raw Bitsy attachment metadata");
    let artifact = details.artifact.unwrap();
    assert_eq!(artifact.system.as_deref(), Some("bitsy"));
    assert_eq!(artifact.file.as_ref().unwrap().name, "adia.bitsy");
    let job = coordinator
        .acquire(
            &context,
            &AcquireArtifactRequest {
                provider_id: PROVIDER.into(),
                id: ID.into(),
                file_name: Some("adia.bitsy".into()),
            },
        )
        .await
        .unwrap();
    let game_id = tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            let status = coordinator.games.status(&job.job_id).unwrap();
            match status.state {
                AcquireJobState::Imported { game_id } => break game_id,
                AcquireJobState::Queued
                | AcquireJobState::Resolving
                | AcquireJobState::Downloading { .. }
                | AcquireJobState::Unpacking
                | AcquireJobState::Placing => tokio::time::sleep(Duration::from_millis(20)).await,
                other => panic!("live acquire did not import: {other:?}"),
            }
        }
    })
    .await
    .expect("live acquisition timed out; no retry is performed");
    let snapshot = context.discovery.published().unwrap();
    let route = crate::config::resolver::resolve_linux_route(
        root.path(),
        &snapshot,
        &context.registry,
        &game_id,
        None,
    )
    .unwrap();
    let content = route
        .cache_target
        .as_ref()
        .expect("must route to the host-owned cache");
    let bytes = fs::read(content).unwrap();
    assert_eq!(bytes.len(), 30267);
    assert_eq!(
        hex::encode(Sha256::digest(&bytes)),
        "dafd29ad1638425c743efda2af273a29376989904502193d216164235e8f3ace"
    );
    assert_eq!(route.runner_id, "@korri:bitsybox/bitsybox");
    let launch = crate::launcher::linux_plugin::launch_route(
        root.path(),
        &snapshot,
        &context.registry,
        &route,
        None,
        None,
        PERSON,
    )
    .unwrap();
    let input: crate::launcher::plugin_launch::PluginLaunchInput =
        serde_json::from_str(&launch.command[3]).unwrap();
    assert_eq!(input.content_path.as_deref(), content.to_str());
    let plan = crate::launcher::plugin_launch::evaluate_snapshot(&runner_source, &input).unwrap();
    assert_eq!(plan.args, [content.display().to_string()]);
    assert_eq!(plan.command, input.program);
    // The producer's existing native runtime test invokes actual plugin-run,
    // checks a live world and exits it, then proves no media writes. No game
    // bytes or signed URL are copied into a repository or persistent library.
    let output = std::process::Command::new("python3")
        .arg(runner_test)
        .arg(&runner_package)
        .arg(if cfg!(target_arch = "aarch64") {
            "aarch64-linux"
        } else {
            "x86_64-linux"
        })
        .arg(korrid)
        .arg(content)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "native runtime: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(content).unwrap(), bytes);
    eprintln!("Live packaged provider: search/details/acquire/import/cache route, 30267-byte identity and native runtime passed");
}
