//! Runner progress through the real reserve/start/status RPC boundary.
use super::*;

#[tokio::test]
async fn runner_steps_are_snapshotted_for_the_exact_launch_until_initial_handoff() {
    launch_progress(true).await;
}

#[tokio::test]
async fn runner_steps_end_when_the_session_ends_before_handoff() {
    launch_progress(false).await;
}

async fn launch_progress(observe_handoff: bool) {
    let root = tempfile::tempdir().unwrap();
    crate::config::test_fixtures::gba(root.path());
    std::fs::create_dir(root.path().join("roms")).unwrap();
    std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
    crate::plugin_test_fixtures::installed(root.path());
    let package = root.path().join("plugin-mgba");
    let source = std::fs::read_to_string(package.join("plugin.ts")).unwrap();
    let source = source.replace("systems: [\"gba\"],", "systems: [\"gba\"], steps: [{key: 'start', label: 'Starting runner'}, {key: 'game', label: 'Starting the game'}],");
    std::fs::write(package.join("plugin.ts"), source).unwrap();
    let registry = crate::plugin::PluginRegistry::from_installed(vec![
        crate::plugin_installation::EnabledPackage {
            id: "@korri:mgba".into(),
            package: package.clone(),
            files: std::collections::BTreeMap::from([
                ("retroarch".into(), root.path().join("retroarch")),
                ("autoconfig".into(), root.path().join("autoconfig")),
                ("mgba".into(), root.path().join("mgba.so")),
            ]),
            entry: "plugin.ts".into(),
            sources: vec![
                "plugin.ts".into(),
                "retroarch.ts".into(),
                "settings.ts".into(),
            ],
            requires: vec![],
            hosts: vec![],
        },
    ])
    .unwrap();
    let config = root.path().join("host.toml");
    std::fs::write(&config, "label = \"steps\"\ngames = []\n").unwrap();
    let backend = Arc::new(host::control::InMemoryLaunchUnitBackend::default());
    let compositor = Arc::new(Compositor {
        tree: Mutex::new(json!({"id":1,"nodes":[{"id":3,"pid":9100,"focused":false}]})),
        gate: Mutex::new(None),
    });
    let runtime = host::HostRuntime::from_paths_with_backend(
        &config,
        Some(root.path().into()),
        root.path().join("private"),
        backend.clone(),
    )
    .with_route_registry(root.path().into(), registry)
    .with_compositor(compositor.clone());
    runtime.claim_save_owner();
    let app = router(
        runtime,
        root.path(),
        TOKEN,
        SurfaceHostPermission::LocalSessions,
    );
    let game = crate::config::test_fixtures::GBA_ID;
    let reservation = rpc(&app, "app.session.reserve", json!({"gameId":game})).await;
    let id = reservation["outcome"]["payload"]["launchId"]
        .as_str()
        .unwrap();
    let started = rpc(
        &app,
        "app.session.start",
        json!({"gameId":game,"expectedLaunchId":id}),
    )
    .await;
    assert_eq!(started["outcome"]["_tag"], "Ok", "{started}");
    backend.set_window_pids(id, [9100].into());
    // Mutating the package cannot change the declaration retained at launch.
    std::fs::remove_file(package.join("plugin.ts")).unwrap();
    let status = rpc(&app, "app.session.status", json!({})).await;
    assert_eq!(
        status["outcome"]["payload"]["active"]["steps"],
        json!({"labels":["Starting runner","Starting the game"],"current":0})
    );
    backend.set_status_text(id, "korri-step: game");
    let status = rpc(&app, "app.session.status", json!({})).await;
    assert_eq!(
        status["outcome"]["payload"]["active"]["steps"]["current"],
        1
    );
    // A failed observation is not handoff or completion. The next successful
    // observation must retain this exact launch's declaration and position.
    backend.set_state_unavailable(true);
    let unavailable = rpc(&app, "app.session.status", json!({})).await;
    assert_eq!(unavailable["outcome"]["_tag"], "Err");
    backend.set_state_unavailable(false);
    let recovered = rpc(&app, "app.session.status", json!({})).await;
    assert_eq!(
        recovered["outcome"]["payload"]["active"]["steps"]["current"], 1,
        "{recovered}"
    );
    backend.set_status_text(id, "korri-step: start");
    let status = rpc(&app, "app.session.status", json!({})).await;
    assert_eq!(
        status["outcome"]["payload"]["active"]["steps"]["current"],
        1
    );
    if observe_handoff {
        compositor.tree.lock().unwrap()["nodes"][0]["focused"] = json!(true);
        let status = rpc(&app, "app.session.status", json!({})).await;
        assert_eq!(
            status["outcome"]["payload"]["active"]["initialHandoff"],
            "observed"
        );
        assert!(
            status["outcome"]["payload"]["active"]
                .get("steps")
                .is_none(),
            "{status}"
        );
        compositor.tree.lock().unwrap()["nodes"][0]["focused"] = json!(false);
        let status = rpc(&app, "app.session.status", json!({})).await;
        assert!(status["outcome"]["payload"]["active"]
            .get("steps")
            .is_none());
    }
    backend.complete_live();
    let status = rpc(&app, "app.session.status", json!({})).await;
    assert!(status["outcome"]["payload"].get("active").is_none());
}
