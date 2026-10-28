use super::*;
use crate::saves::{peer::SavePeerService, store::NativeSaveStore};

#[tokio::test]
async fn reach_saves_use_the_iroh_route_for_a_roster_only_peer() {
    let devices = Devices::new();
    let (a, b, outgoing, incoming) = pair().await;
    let service_a = SavePeerService::new(
        Arc::new(NativeSaveStore::new(devices._a.path())),
        devices.ca.clone(),
        devices.da.clone(),
        devices._a.path().to_owned(),
        Arc::new(|_, _| Ok(None)),
    )
    .unwrap();
    let service_b = Arc::new(
        SavePeerService::new(
            Arc::new(NativeSaveStore::new(devices.b.path())),
            devices.cb.clone(),
            devices._db.clone(),
            devices.b.path().to_owned(),
            Arc::new(|_, _| Ok(None)),
        )
        .unwrap(),
    );
    let config = devices.b.path().join("save-host.toml");
    std::fs::write(&config, "label = \"save peer\"\n").unwrap();
    let runtime = crate::host::HostRuntime::from_paths_with_backend(
        &config,
        None,
        devices.b.path().to_owned(),
        Arc::new(crate::host::control::InMemoryLaunchUnitBackend::default()),
    );
    let (mut state, _) = crate::app_states(runtime);
    state.saves = Some(service_b);
    let router = PeerRpcServer::with_shared_authority(
        state,
        devices.b.path(),
        devices.cb.clone(),
        devices._db.authorization.clone(),
    )
    .unwrap()
    .iroh_router();
    let task = tokio::spawn(tunnel::serve(incoming, router));
    let key = devices.cb.public_key().unwrap();
    devices.da.reach.live.write().unwrap().insert(
        key.clone(),
        LiveRoute {
            connection: outgoing,
            signalled: Vec::new(),
        },
    );
    assert!(devices.da.snapshot().unwrap()[0]
        .remembered_endpoint
        .is_none());
    tokio::time::timeout(Duration::from_secs(3), service_a.on_contact(&key))
        .await
        .unwrap()
        .unwrap();
    task.abort();
    a.close().await;
    b.close().await;
}
