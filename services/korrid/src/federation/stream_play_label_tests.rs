use super::*;
use crate::{
    host::{control::InMemoryLaunchUnitBackend, HostRuntime},
    stream_play::{SessionStreamPlayOutcome, SessionStreamPlayRequest},
    RpcRequest, RpcResponse,
};

#[tokio::test]
async fn stream_play_prefers_a_verified_directory_label_and_otherwise_keeps_the_caller_label() {
    let fixture = Fixture::new();
    let directory = fixture.directory().unwrap();
    fixture.roster(&directory);
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("host.toml");
    fs::write(
        &config,
        "label = \"holder\"\n[[games]]\nid = \"game\"\ntitle = \"Game\"\ncommand = [\"game\"]\n",
    )
    .unwrap();
    let runtime = HostRuntime::from_paths_with_backend(
        &config,
        None,
        root.path().join("private"),
        Arc::new(InMemoryLaunchUnitBackend::default()),
    );
    runtime.claim_save_owner();
    let launch = runtime
        .prepare("game", None)
        .await
        .unwrap()
        .session
        .launch_id;
    let (mut state, _) = crate::app_states(runtime);
    state.federation = Some(directory.clone());
    let caller = AuthorizationContext::Peer(Principal::OwnerDevice {
        device_public_key: fixture.key(),
        owner_statement: fixture.peer.owner_statement_json().unwrap().into(),
        owner_public_key: fixture.owner.public_key().to_hex(),
    });
    for (generation, label) in [(1, None), (2, Some("Living room")), (3, None)] {
        let mut endpoint = fixture.endpoint(generation, 1000);
        endpoint.label = label.map(str::to_owned);
        directory
            .apply_endpoint_event(&directory.begin_work().unwrap(), &fixture.event(&endpoint))
            .unwrap();
        let request = RpcRequest::SessionStreamPlay(SessionStreamPlayRequest {
            launch_id: launch.clone(),
            device_label: "Pocket Mini".into(),
        });
        let response = crate::dispatch(&state, &caller, request).await.unwrap();
        let RpcResponse::SessionStreamPlay(SessionStreamPlayOutcome::Ok(active)) = response else {
            panic!("registration failed")
        };
        assert_eq!(
            active.stream_players[0].label,
            label.unwrap_or("Pocket Mini")
        );
        assert_ne!(active.stream_players[0].label, fixture.key());
    }
}
