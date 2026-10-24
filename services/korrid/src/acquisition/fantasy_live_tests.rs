//! Opt-in native fantasy-console acceptance using immutable publisher packages.
//! Network requests use the production public-address and exact-host policies.
//! No installation, trust-key change or physical-device claim is made here.
use super::{
    game::GameContext, provider_live_tests::package, sleep, AcquireArtifactRequest,
    AcquireJobState, AcquisitionCoordinator, DetailsRequest, SearchRequest,
};
use crate::plugin::PluginRegistry;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf, sync::Arc, time::Duration};

fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name}"))
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "live source; coordinator supplies pinned packages, reviewed content and prebuilt native probe"]
async fn public_fantasy_source_search_acquire_route_and_native_launch() {
    const PERSON: &str = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";
    let provider_id = required("KORRI_LIVE_FANTASY_PROVIDER_ID");
    let runner_plugin = required("KORRI_LIVE_FANTASY_RUNNER_PLUGIN_ID");
    let runner_id = required("KORRI_LIVE_FANTASY_RUNNER_ID");
    let system = required("KORRI_LIVE_FANTASY_SYSTEM");
    let query = required("KORRI_LIVE_FANTASY_QUERY");
    let claim_id = required("KORRI_LIVE_FANTASY_CLAIM_ID");
    let probe = PathBuf::from(required("KORRI_LIVE_FANTASY_NATIVE_PROBE"));
    let korrid = PathBuf::from(required("KORRI_LIVE_KORRID"));
    assert!(probe.is_file() && korrid.is_file());
    let provider = package("KORRI_LIVE_FANTASY_PROVIDER_PACKAGE", &provider_id);
    let runner = package("KORRI_LIVE_FANTASY_RUNNER_PACKAGE", &runner_plugin);
    let runner_package = runner.package.clone();
    let runner_source = crate::script::source::SourceSnapshot::package_plugin(
        &runner.package,
        &runner.entry,
        &runner.sources,
    )
    .unwrap();
    let registry = PluginRegistry::from_installed(vec![provider, runner]).unwrap();
    let root = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    let config = root.path().join("host.toml");
    fs::write(&config, "label = \"live-fantasy-test\"\ngames = []\n").unwrap();
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
    let coordinator = AcquisitionCoordinator {
        sleep: sleep::SleepBlock::InMemory(Arc::new(sleep::InMemoryBlocks::default())),
        ..Default::default()
    };
    let claims = coordinator
        .games
        .search(
            &context.registry,
            &SearchRequest {
                query,
                provider_ids: vec![provider_id.clone()],
            },
        )
        .await
        .expect("real source search must succeed without retries");
    assert!(claims.claims.iter().any(|claim| claim.id == claim_id));
    let details = coordinator
        .games
        .details(
            &context.registry,
            &DetailsRequest {
                provider_id: provider_id.clone(),
                id: claim_id.clone(),
            },
        )
        .await
        .expect("real source details must succeed");
    assert_eq!(
        details.artifact.unwrap().system.as_deref(),
        Some(system.as_str())
    );
    let job = coordinator
        .acquire(
            &context,
            &AcquireArtifactRequest {
                provider_id,
                id: claim_id,
                file_name: None,
            },
        )
        .await
        .unwrap();
    let game_id = tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            match coordinator.games.status(&job.job_id).unwrap().state {
                AcquireJobState::Imported { game_id } => break game_id,
                AcquireJobState::Queued
                | AcquireJobState::Resolving
                | AcquireJobState::Downloading { .. }
                | AcquireJobState::Unpacking
                | AcquireJobState::Placing => tokio::time::sleep(Duration::from_millis(20)).await,
                other => panic!("real acquisition did not import: {other:?}"),
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
    assert_eq!(route.runner_id, runner_id);
    let content = route
        .cache_target
        .as_ref()
        .expect("host-owned acquired content");
    let bytes = fs::read(content).unwrap();
    // This is an execution allowlist for this opt-in test, not a product trust
    // policy. The complete Space Rescue source was reviewed as inert ZIP data.
    // Never execute arbitrary downloaded Python in an automated acceptance run.
    if system == "pyxel" {
        assert_eq!(
            hex::encode(Sha256::digest(&bytes)),
            "250ababacc8f8b50a0e12c30e966b81e6aea1d8559e065a9a734baf59ba82be9",
            "only the reviewed Space Rescue cartridge may execute in this test"
        );
    }
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
    crate::launcher::plugin_launch::evaluate_snapshot(&runner_source, &input).unwrap();
    // The prebuilt publisher probe owns its graphics environment and invokes
    // real plugin-run and its native runtime, not an in-memory backend.
    let output = std::process::Command::new(probe)
        .env(
            "KORRI_NATIVE_PROBE_INPUT",
            serde_json::to_string(&input).unwrap(),
        )
        .arg(runner_package)
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
        "native probe failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(content).unwrap(), bytes, "source media changed");
    eprintln!("Real source search/details/acquire/import/cache route and native startup passed for {system}");
}
