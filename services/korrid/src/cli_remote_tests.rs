//! Two independent korrid runtimes in OS processes, through real person sockets
//! and signed native peer RPC. Only launch-unit/native-artifact effects are
//! configured test implementations; no HTTP or signing transport is replaced.
use crate::*;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::Arc,
    time::{Duration, Instant},
};

const OWNER: &str = "0000000000000000000000000000000000000000000000000000000000000003";
const GAME: &str = plugin_test_fixtures::GBA_ID;
const RUNNER: &str = "@korri:mgba/mgba";

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn wait(path: &Path, process: &mut Process) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !path.exists() {
        assert!(
            process.0.try_wait().unwrap().is_none(),
            "helper exited before {}",
            path.display()
        );
        assert!(
            Instant::now() < deadline,
            "helper did not publish {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn resources(root: &Path, secret: &str, owner: &str) -> peer_rpc::PeerCredentials {
    peer_rpc::test_owned_credentials(root, secret, owner);
    federation::coordinator::FederationResources::open(root)
        .unwrap()
        .credentials
}
fn launch_process(
    root: &Path,
    peers: &[(&peer_rpc::PeerCredentials, &str)],
    admin: Option<&Path>,
) -> Process {
    let private = root.join("private");
    let resources = federation::coordinator::FederationResources::open(&private).unwrap();
    let now = peer_rpc::unix_time();
    let recipient = resources.credentials.public_key().unwrap();
    for (credentials, origin) in peers {
        let identity = credentials.identity_snapshot().unwrap();
        resources
            .directory
            .apply_membership(
                &resources.directory.begin_work().unwrap(),
                &[identity.owner_statement_json().unwrap()],
            )
            .unwrap();
        let record = relay::EndpointRecord {
            device_public_key: credentials.public_key().unwrap(),
            owner_public_key: nostr::key::Keys::parse(OWNER)
                .unwrap()
                .public_key()
                .to_hex(),
            generation: 1,
            candidates: vec![(*origin).into()],
            issued_at: now,
            expires_at: now + 600,
            label: Some("Peer".into()),
            moonlight_address: None,
        };
        let event = identity
            .encrypt_tagged_event(
                &recipient,
                relay::ENDPOINT_EVENT_KIND,
                vec![
                    vec!["d".into(), format!("org.korri.endpoint:{recipient}")],
                    vec!["p".into(), recipient.clone()],
                    vec!["expiration".into(), record.expires_at.to_string()],
                ],
                &serde_json::to_string(&record).unwrap(),
                now,
            )
            .unwrap();
        resources
            .directory
            .apply_endpoint_event(&resources.directory.begin_work().unwrap(), &event.json)
            .unwrap();
    }
    drop(resources);
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "cli::remote_tests::remote_runtime_process",
            "--ignored",
            "--nocapture",
        ])
        .env("KORRI_REMOTE_RUNTIME_ROOT", root);
    if let Some(admin) = admin {
        command.env(plugin_repositories::SOCKET_ENV, admin);
    }
    let mut process = Process(command.spawn().unwrap());
    wait(&root.join("ready"), &mut process);
    process
}

#[test]
#[ignore = "subprocess helper; launched by the remote process tests"]
fn remote_runtime_process() {
    let root = PathBuf::from(std::env::var_os("KORRI_REMOTE_RUNTIME_ROOT").expect("test root"));
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let library = root.join("library");
            fs::create_dir_all(&library).unwrap();
            plugin_test_fixtures::write_gba_library(&library);
            fs::create_dir_all(library.join("roms")).unwrap();
            fs::write(library.join("roms/wl4.gba"), b"rom").unwrap();
            let registry = plugin_test_fixtures::installed(&library);
            if root.join("save-bearing").exists() {
                use std::io::Write;
                // Existing SaveLocation producer: host/saves.rs MigratedDevice.
                writeln!(fs::OpenOptions::new().append(true).open(library.join("plugin-mgba/retroarch.ts")).unwrap(),
                    "handlers[\"launch.prepare\"] = input => ({{command: input.program, args: [], saveLocations: [{{key: 'saves', kind: 'save', path: input.saveRoot + '/saves'}}]}});").unwrap();
            }
            if root.join("before-hook").exists() {
                let path = library.join("catalog/library.yaml");
                let mut catalog: serde_yaml::Value = serde_yaml::from_slice(&fs::read(&path).unwrap()).unwrap();
                catalog["library"][GAME]["releases"][0]["hooks"] = serde_yaml::from_str("before: [{run: 'record-before-hook'}]").unwrap();
                fs::write(path, serde_yaml::to_string(&catalog).unwrap()).unwrap();
            }
            let config = root.join("host.toml");
            fs::write(&config, "label = \"Remote process\"\n").unwrap();
            let private = root.join("private");
            let resources = federation::coordinator::FederationResources::open(&private).unwrap();
            let backend = Arc::new(host::control::InMemoryLaunchUnitBackend::default());
            let audit = root.join("launches");
            backend.set_launch_probe(Arc::new(move || {
                use std::io::Write;
                writeln!(
                    fs::OpenOptions::new()
                        .append(true)
                        .create(true)
                        .open(&audit)
                        .unwrap(),
                    "launch"
                )
                .unwrap();
            }));
            let hook_audit = root.join("hooks");
            backend.set_hook_probe(Arc::new(move || {
                use std::io::Write;
                writeln!(fs::OpenOptions::new().append(true).create(true).open(&hook_audit).unwrap(), "before").unwrap();
            }));
            // IdentityStatus uses the production local-signer socket too. The
            // fixture has no person secret: status reports a remote-only signer.
            let signer_socket = root.join("signer.sock");
            let listener = tokio::net::UnixListener::bind(&signer_socket).unwrap();
            std::env::set_var("KORRID_LOCAL_SIGNER_SOCKET", &signer_socket);
            let signer_root = root.join("signer");
            let signer_public = root.join("person.pub");
            let device = Arc::new(std::sync::Mutex::new(resources.credentials.public_key().unwrap()));
            tokio::spawn(async move {
                loop {
                    let (stream, _) = listener.accept().await.unwrap();
                    let (root, public, device) = (signer_root.clone(), signer_public.clone(), device.clone());
                    tokio::spawn(async move { local_signer::serve_connection(stream, root, device, public).await.unwrap(); });
                }
            });
            let runtime =
                host::HostRuntime::from_paths_with_backend(&config, None, private.clone(), backend)
                    .with_route_registry(library, registry);
            let (peer, _, cli) =
                secure_host_routers_with_federation(runtime, &private, resources, None, None);
            let peer = if root.join("slow-peer").exists() {
                peer.layer(axum::middleware::from_fn(|request: axum::extract::Request, next: axum::middleware::Next| async move {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    next.run(request).await
                }))
            } else { peer };
            let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let origin = format!("http://{}", tcp.local_addr().unwrap());
            let unix = tokio::net::UnixListener::bind(root.join("cli.sock")).unwrap();
            tokio::spawn(async move {
                axum::serve(tcp, peer).await.unwrap();
            });
            fs::write(root.join("ready"), origin).unwrap();
            axum::serve(unix, cli).await.unwrap();
        });
}
fn call(root: &Path, device: Option<&str>, tag: &str, payload: Value) -> (u16, Value) {
    let client = reqwest::blocking::Client::builder()
        .unix_socket(root.join("cli.sock"))
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();
    let request: RpcRequest =
        serde_json::from_value(json!({"_tag":tag,"payload":payload})).unwrap();
    let (path, body) = match device {
        Some(key) => (
            "forward",
            serde_json::to_value(cli::ForwardRequest {
                device_public_key: key.into(),
                request,
            })
            .unwrap(),
        ),
        None => ("rpc", serde_json::to_value(request).unwrap()),
    };
    let response = client
        .post(format!("http://localhost/{path}"))
        .json(&body)
        .send()
        .unwrap();
    let status = response.status().as_u16();
    let bytes = response.bytes().unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
fn ok(root: &Path, device: Option<&str>, tag: &str, payload: Value) -> Value {
    let (status, reply) = call(root, device, tag, payload);
    assert_eq!(status, 200, "{tag}: {reply}");
    assert_eq!(reply["outcome"]["_tag"], "Ok", "{tag}: {reply}");
    reply["outcome"]["payload"].clone()
}
fn refused(root: &Path, device: &str, tag: &str, payload: Value, code: &str) {
    let (status, reply) = call(root, Some(device), tag, payload);
    assert_eq!(status, 200, "{reply}");
    assert_eq!(reply["outcome"]["_tag"], "Err", "{reply}");
    assert_eq!(reply["outcome"]["payload"]["code"], code, "{reply}");
}

#[test]
fn remote_process_launch_status_and_exact_starter_controls() {
    let target = tempfile::tempdir().unwrap();
    let target_resources = resources(&target.path().join("private"), &"61".repeat(32), OWNER);
    let target_key = target_resources.public_key().unwrap();
    let _target_process = launch_process(target.path(), &[], None);
    let origin = fs::read_to_string(target.path().join("ready")).unwrap();
    let caller = tempfile::tempdir().unwrap();
    resources(&caller.path().join("private"), &"62".repeat(32), OWNER);
    let _caller_process = launch_process(caller.path(), &[(&target_resources, &origin)], None);
    ok(
        caller.path(),
        Some(&target_key),
        "app.local-games.list",
        json!({}),
    );
    ok(
        caller.path(),
        Some(&target_key),
        "app.local-games.routes",
        json!({"gameId":GAME}),
    );
    ok(
        caller.path(),
        Some(&target_key),
        "app.launch-settings.snapshot",
        json!({"gameId":GAME}),
    );
    assert_eq!(
        call(
            caller.path(),
            Some(&target_key),
            "app.saves.check",
            json!({"gameId":GAME,"runnerId":RUNNER})
        )
        .0,
        403
    );
    let launch = ok(
        caller.path(),
        Some(&target_key),
        "app.local-games.launch.selected",
        json!({"gameId":GAME,"runnerId":RUNNER}),
    );
    let id = launch["session"]["launchId"].as_str().unwrap();
    let status = ok(
        caller.path(),
        Some(&target_key),
        "app.session.status",
        json!({}),
    );
    assert_eq!(status["active"]["launchId"], id);
    refused(
        caller.path(),
        &target_key,
        "app.session.leave",
        json!({"expectedLaunchId":"wrong-launch"}),
        "StaleLaunchIdentity",
    );
    ok(
        caller.path(),
        Some(&target_key),
        "app.session.leave",
        json!({"expectedLaunchId":id}),
    );
    // No compositor exists in this host fixture. Resume must fail honestly,
    // rather than reporting that a game returned to the front.
    refused(
        caller.path(),
        &target_key,
        "app.session.resume",
        json!({"expectedLaunchId":id}),
        "HostFocusFailed",
    );
    // A second verified owner device is not the immutable starter.
    let other = tempfile::tempdir().unwrap();
    resources(&other.path().join("private"), &"63".repeat(32), OWNER);
    let _other_process = launch_process(other.path(), &[(&target_resources, &origin)], None);
    refused(
        other.path(),
        &target_key,
        "app.session.end",
        json!({"expectedLaunchId":id}),
        "NotSessionStarter",
    );
    ok(
        caller.path(),
        Some(&target_key),
        "app.session.end",
        json!({"expectedLaunchId":id}),
    );
    // A session started locally on the selected device is never adopted by a peer.
    let local = ok(
        target.path(),
        None,
        "app.local-games.launch.selected",
        json!({"gameId":GAME,"runnerId":RUNNER}),
    );
    refused(
        caller.path(),
        &target_key,
        "app.session.end",
        json!({"expectedLaunchId":local["session"]["launchId"]}),
        "NotSessionStarter",
    );
    ok(
        target.path(),
        None,
        "app.session.end",
        json!({"expectedLaunchId":local["session"]["launchId"]}),
    );
}

fn split_saves(private: &Path) {
    use saves::{types::*, SaveRepository};
    let person = nostr::key::Keys::parse(OWNER)
        .unwrap()
        .public_key()
        .to_hex();
    let store = saves::store::NativeSaveStore::new(private);
    for content in [b"a", b"b"] {
        let hash = saves::record::hash(content);
        let record = SnapshotRecord {
            parents: vec![],
            game_id: GAME.into(),
            release_id: None,
            runner_id: RUNNER.into(),
            device: "11".repeat(32),
            ended_at: "2026-10-21T00:00:00.000Z".into(),
            duration_seconds: 1,
            interrupted: false,
            locations: vec![SnapshotLocation {
                key: "saves".into(),
                kind: SaveKind::Save,
                files: vec![SaveFile {
                    path: "game.srm".into(),
                    sha256: hash.clone(),
                    bytes: 1,
                }],
            }],
        };
        store.put_record(&person, &record).unwrap();
        store
            .put_object(&person, &hash, &mut content.as_slice())
            .unwrap();
    }
    assert_eq!(store.reconcile(&person, GAME).unwrap(), SaveMark::Split);
}
#[test]
fn remote_process_skipped_preflight_cannot_launch_stored_split() {
    let target = tempfile::tempdir().unwrap();
    let target_resources = resources(&target.path().join("private"), &"64".repeat(32), OWNER);
    split_saves(&target.path().join("private"));
    fs::write(target.path().join("save-bearing"), "").unwrap();
    fs::write(target.path().join("before-hook"), "").unwrap();
    let target_key = target_resources.public_key().unwrap();
    let _target_process = launch_process(target.path(), &[], None);
    let origin = fs::read_to_string(target.path().join("ready")).unwrap();
    let caller = tempfile::tempdir().unwrap();
    resources(&caller.path().join("private"), &"65".repeat(32), OWNER);
    let _caller_process = launch_process(caller.path(), &[(&target_resources, &origin)], None);
    assert_eq!(
        call(
            caller.path(),
            Some(&target_key),
            "app.saves.check",
            json!({"gameId":GAME,"runnerId":RUNNER})
        )
        .0,
        403
    );
    refused(
        caller.path(),
        &target_key,
        "app.local-games.launch.selected",
        json!({"gameId":GAME,"runnerId":RUNNER}),
        "SavesSplit",
    );
    assert!(
        !target.path().join("launches").exists(),
        "a stored split must refuse before launch-unit effects"
    );
    assert!(!caller.path().join("launches").exists());
    assert!(
        !target.path().join("hooks").exists(),
        "known split must refuse before hooks"
    );
}
fn revoke(device: &str) -> String {
    use nostr::{
        event::{EventBuilder, FinalizeEvent, Kind, Tag},
        types::Timestamp,
    };
    EventBuilder::new(Kind::Custom(identity::OWNER_EVENT_KIND), "")
        .tags([
            Tag::parse(["d", &format!("org.korri.device-owner:{device}")]).unwrap(),
            Tag::parse(["device", device]).unwrap(),
            Tag::parse(["status", "revoked"]).unwrap(),
        ])
        .custom_created_at(Timestamp::from(peer_rpc::unix_time()))
        .finalize(&nostr::key::Keys::parse(OWNER).unwrap())
        .unwrap()
        .as_json()
}
#[test]
fn remote_process_owner_revocation_and_unavailable_refusals_have_no_launch_effects() {
    let target = tempfile::tempdir().unwrap();
    let target_resources = resources(&target.path().join("private"), &"66".repeat(32), OWNER);
    let target_key = target_resources.public_key().unwrap();
    let _target_process = launch_process(target.path(), &[], None);
    let origin = fs::read_to_string(target.path().join("ready")).unwrap();
    for case in ["other-owner", "revoked-target", "unavailable"] {
        let caller = tempfile::tempdir().unwrap();
        let _caller_resources = resources(
            &caller.path().join("private"),
            &"67".repeat(32),
            if case == "other-owner" {
                "0000000000000000000000000000000000000000000000000000000000000004"
            } else {
                OWNER
            },
        );
        if case == "revoked-target" {
            let directory =
                federation::coordinator::FederationResources::open(&caller.path().join("private"))
                    .unwrap()
                    .directory;
            directory
                .apply_membership(
                    &directory.begin_work().unwrap(),
                    &[
                        target_resources
                            .identity_snapshot()
                            .unwrap()
                            .owner_statement_json()
                            .unwrap(),
                        revoke(&target_key),
                    ],
                )
                .unwrap();
        }
        let peers = if case == "unavailable" {
            vec![(&target_resources, "http://127.0.0.1:9")]
        } else {
            vec![]
        };
        let _caller_process = launch_process(caller.path(), &peers, None);
        let expected = if case == "unavailable" {
            "DeviceUnavailable"
        } else {
            "OwnerDeviceRequired"
        };
        for (tag, payload) in [
            ("app.local-games.list", json!({})),
            (
                "app.local-games.launch.selected",
                json!({"gameId":GAME,"runnerId":RUNNER}),
            ),
            (
                "system.plugins.setEnabled",
                json!({"id":"@test:clock","enabled":true}),
            ),
        ] {
            refused(caller.path(), &target_key, tag, payload, expected);
        }
        assert!(!caller.path().join("launches").exists());
        assert!(!target.path().join("launches").exists());
    }
    // The destination independently rejects revoked caller evidence, even
    // when the sender's directory still contains valid target membership.
    let caller = tempfile::tempdir().unwrap();
    let caller_resources = resources(&caller.path().join("private"), &"68".repeat(32), OWNER);
    let revoked_target = tempfile::tempdir().unwrap();
    let revoked_resources = resources(
        &revoked_target.path().join("private"),
        &"69".repeat(32),
        OWNER,
    );
    let authority =
        authorization::Authorization::load(&revoked_target.path().join("private")).unwrap();
    let attempt = authority
        .attempt(
            revoked_resources.identity_snapshot().unwrap().state(),
            &caller_resources.public_key().unwrap(),
            caller_resources
                .identity_snapshot()
                .unwrap()
                .owner_statement_json()
                .as_deref(),
            None,
            &[revoke(&caller_resources.public_key().unwrap())],
            peer_rpc::unix_time(),
        )
        .unwrap();
    authority.commit_revocations(&attempt).unwrap();
    let _revoked_target_process = launch_process(revoked_target.path(), &[], None);
    let revoked_origin = fs::read_to_string(revoked_target.path().join("ready")).unwrap();
    let _caller_process = launch_process(
        caller.path(),
        &[(&revoked_resources, &revoked_origin)],
        None,
    );
    let key = revoked_resources.public_key().unwrap();
    refused(
        caller.path(),
        &key,
        "app.local-games.list",
        json!({}),
        "PeerAuthorizationDenied",
    );
    refused(
        caller.path(),
        &key,
        "app.local-games.launch.selected",
        json!({"gameId":GAME,"runnerId":RUNNER}),
        "PeerAuthorizationDenied",
    );
    refused(
        caller.path(),
        &key,
        "system.plugins.setEnabled",
        json!({"id":"@test:clock","enabled":true}),
        "PeerAuthorizationDenied",
    );
    assert!(!revoked_target.path().join("launches").exists());
    assert!(!caller.path().join("launches").exists());
    assert!(!origin.is_empty());
}

#[test]
#[ignore = "requires KORRI_REMOTE_PLUGIN_HOST_TEST_BINARY built from plugin-host; see README"]
fn remote_process_plugin_approval_and_admin_boundary() {
    let binary = std::env::var_os("KORRI_REMOTE_PLUGIN_HOST_TEST_BINARY")
        .expect("plugin-host libtest binary");
    let plugin = tempfile::tempdir().unwrap();
    let mut admin = Process(
        Command::new(binary)
            .args([
                "--exact",
                "host::tests::remote_admin_process",
                "--ignored",
                "--nocapture",
            ])
            .env("KORRI_REMOTE_PLUGIN_ROOT", plugin.path())
            .spawn()
            .unwrap(),
    );
    wait(&plugin.path().join("admin.sock"), &mut admin);
    let target = tempfile::tempdir().unwrap();
    let target_resources = resources(&target.path().join("private"), &"71".repeat(32), OWNER);
    let target_key = target_resources.public_key().unwrap();
    let _target_process =
        launch_process(target.path(), &[], Some(&plugin.path().join("admin.sock")));
    let origin = fs::read_to_string(target.path().join("ready")).unwrap();
    let caller = tempfile::tempdir().unwrap();
    resources(&caller.path().join("private"), &"72".repeat(32), OWNER);
    let _caller_process = launch_process(caller.path(), &[(&target_resources, &origin)], None);
    if let Some(binary) = std::env::var_os("KORRI_REMOTE_CLI_BINARY") {
        let before = fs::read(plugin.path().join("requests.jsonl")).unwrap_or_default();
        assert_ambiguous_cli_has_no_effect(&binary, caller.path());
        assert_eq!(
            fs::read(plugin.path().join("requests.jsonl")).unwrap_or_default(),
            before
        );
    }
    let source =
        json!({"url":"https://catalog.example/", "pluginId":"@test:clock", "releaseVersion":"v1"});
    let review = ok(
        caller.path(),
        Some(&target_key),
        "system.plugins.inspect",
        source.clone(),
    );
    assert_eq!(review["approval"], "clock-v1-approval");
    let mut install = source.clone();
    install["approval"] = "stale-approval".into();
    refused(
        caller.path(),
        &target_key,
        "system.plugins.install",
        install.clone(),
        "PluginHostRefused",
    );
    let repos = ok(
        caller.path(),
        Some(&target_key),
        "system.plugins.repositories",
        json!({}),
    );
    assert!(repos["repositoryInstalls"].as_array().unwrap().is_empty());
    install["approval"] = review["approval"].clone();
    if let Some(binary) = std::env::var_os("KORRI_REMOTE_CLI_BINARY") {
        let result = Command::new(binary)
            .arg("--socket")
            .arg(caller.path().join("cli.sock"))
            .args([
                "--device",
                &target_key,
                "--json",
                "plugins",
                "install",
                "https://catalog.example/",
                "@test:clock",
                "v1",
                "--approval",
                "clock-v1-approval",
                "--yes",
            ])
            .output()
            .unwrap();
        assert!(result.status.success(), "{result:?}");
        let result: RpcResponse = serde_json::from_slice(&result.stdout).unwrap();
        assert!(matches!(
            result,
            RpcResponse::PluginInstall(PluginChangeOutcome::Ok(_))
        ));
    } else {
        ok(
            caller.path(),
            Some(&target_key),
            "system.plugins.install",
            install,
        );
    }
    let repos = ok(
        caller.path(),
        Some(&target_key),
        "system.plugins.repositories",
        json!({}),
    );
    assert_eq!(repos["repositoryInstalls"][0]["enabled"], false);
    ok(
        caller.path(),
        Some(&target_key),
        "system.plugins.setEnabled",
        json!({"id":"@test:clock", "enabled":true}),
    );
    ok(
        caller.path(),
        Some(&target_key),
        "system.plugins.setEnabled",
        json!({"id":"@test:clock", "enabled":false}),
    );
    fs::write(plugin.path().join("stale-policy"), "changed").unwrap();
    refused(
        caller.path(),
        &target_key,
        "system.plugins.setEnabled",
        json!({"id":"@test:clock", "enabled":true}),
        "PluginNeedsReview",
    );
    let repos = ok(
        caller.path(),
        Some(&target_key),
        "system.plugins.repositories",
        json!({}),
    );
    assert_eq!(repos["repositoryInstalls"][0]["enabled"], false);
    // Catalog configuration does not approve anything or affect installations.
    // Adding the configured official source is the host's real no-op success.
    ok(
        caller.path(),
        Some(&target_key),
        "system.plugins.repository.add",
        json!({"url":"https://catalog.example/"}),
    );
    // A new source needs a fetched, valid catalog. This host fixture has no
    // downloader: preserve that refusal, without storing an unchecked source.
    refused(
        caller.path(),
        &target_key,
        "system.plugins.repository.add",
        json!({"url":"https://other.example/"}),
        "PluginHostRefused",
    );
    let sources = ok(
        caller.path(),
        Some(&target_key),
        "system.plugins.repositories",
        json!({}),
    );
    assert!(sources["userAdded"].as_array().unwrap().is_empty());
    ok(
        caller.path(),
        Some(&target_key),
        "system.plugins.repository.remove",
        json!({"url":"https://other.example/"}),
    );
    refused(
        caller.path(),
        &target_key,
        "system.plugins.catalog",
        json!({"url":"http://plain.example/"}),
        "PluginRepositoryUrlInvalid",
    );
    let audit = fs::read(plugin.path().join("requests.jsonl")).unwrap();
    for case in ["other-owner", "revoked-target", "unavailable"] {
        let denied = tempfile::tempdir().unwrap();
        resources(
            &denied.path().join("private"),
            &"78".repeat(32),
            if case == "other-owner" {
                "0000000000000000000000000000000000000000000000000000000000000004"
            } else {
                OWNER
            },
        );
        if case == "revoked-target" {
            let directory =
                federation::coordinator::FederationResources::open(&denied.path().join("private"))
                    .unwrap()
                    .directory;
            directory
                .apply_membership(
                    &directory.begin_work().unwrap(),
                    &[
                        target_resources
                            .identity_snapshot()
                            .unwrap()
                            .owner_statement_json()
                            .unwrap(),
                        revoke(&target_key),
                    ],
                )
                .unwrap();
        }
        let peers = if case == "unavailable" {
            vec![(&target_resources, "http://127.0.0.1:9")]
        } else {
            vec![]
        };
        let _process = launch_process(denied.path(), &peers, None);
        refused(
            denied.path(),
            &target_key,
            "system.plugins.setEnabled",
            json!({"id":"@test:clock","enabled":true}),
            if case == "unavailable" {
                "DeviceUnavailable"
            } else {
                "OwnerDeviceRequired"
            },
        );
        assert_eq!(
            fs::read(plugin.path().join("requests.jsonl")).unwrap(),
            audit,
            "unauthorized and unavailable peers must not reach admin"
        );
        assert!(!denied.path().join("launches").exists());
    }
    // No request ran against the caller's unconfigured plugin host.
    assert!(!caller.path().join("launches").exists());
    assert!(!caller.path().join("library/users").exists());
    for (tag, payload) in [
        ("system.identity.status", json!({})),
        ("system.wifi.scan", json!({})),
        ("system.onboarding.status", json!({})),
        ("app.saves.check", json!({"gameId":GAME})),
        (
            "app.saves.choose",
            json!({"gameId":GAME,"snapshotId":"snapshot"}),
        ),
        (
            "app.saves.restore",
            json!({"gameId":GAME,"snapshotId":"snapshot"}),
        ),
    ] {
        assert_eq!(call(caller.path(), Some(&target_key), tag, payload).0, 403);
    }
    assert!(fs::read_to_string(plugin.path().join("requests.jsonl"))
        .unwrap()
        .contains("Install"));
    // A different verified owner device carries a signed revocation while
    // the caller still remembers valid target membership. The receiving
    // korrid must revalidate evidence before touching the plugin host.
    let caller_key = identity::DeviceIdentity::load_or_create(&caller.path().join("private"))
        .unwrap()
        .device_public_key()
        .unwrap()
        .to_owned();
    let carrier = tempfile::tempdir().unwrap();
    let carrier = peer_rpc::test_owned_credentials(carrier.path(), &"79".repeat(32), OWNER)
        .with_revocations(vec![revoke(&caller_key)]);
    let envelope = carrier
        .encode_request(
            &target_key,
            RpcRequest::Health(HealthRequest {}),
            peer_rpc::unix_time(),
        )
        .unwrap();
    let response = reqwest::blocking::Client::new()
        .post(format!("{origin}/peer-rpc"))
        .header("content-type", "application/json")
        .body(envelope.event_json)
        .send()
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let audit = fs::read(plugin.path().join("requests.jsonl")).unwrap();
    refused(
        caller.path(),
        &target_key,
        "system.plugins.setEnabled",
        json!({"id":"@test:clock","enabled":true}),
        "PeerAuthorizationDenied",
    );
    let mut install = source;
    install["approval"] = "changed-policy-approval".into();
    refused(
        caller.path(),
        &target_key,
        "system.plugins.install",
        install,
        "PeerAuthorizationDenied",
    );
    assert_eq!(
        fs::read(plugin.path().join("requests.jsonl")).unwrap(),
        audit
    );
}

#[test]
fn remote_process_selected_launch_nonce_survives_restart() {
    let target = tempfile::tempdir().unwrap();
    let target_resources = resources(&target.path().join("private"), &"73".repeat(32), OWNER);
    let target_key = target_resources.public_key().unwrap();
    let caller = tempfile::tempdir().unwrap();
    let credentials = peer_rpc::test_owned_credentials(caller.path(), &"74".repeat(32), OWNER);
    let request = RpcRequest::SelectedGameLaunch(game_routes::SelectedGameLaunchRequest {
        game_id: GAME.into(),
        runner_id: RUNNER.into(),
        launch: None,
    });
    assert!(authorization::is_security_mutation(&request));
    let encoded = credentials
        .encode_request(&target_key, request, peer_rpc::unix_time())
        .unwrap();
    let process = launch_process(target.path(), &[], None);
    let origin = fs::read_to_string(target.path().join("ready")).unwrap();
    let client = reqwest::blocking::Client::new();
    let first = client
        .post(format!("{origin}/peer-rpc"))
        .header("content-type", "application/json")
        .body(encoded.event_json.clone())
        .send()
        .unwrap();
    assert_eq!(first.status().as_u16(), 200);
    let status = ok(target.path(), None, "app.session.status", json!({}));
    ok(
        target.path(),
        None,
        "app.session.end",
        json!({"expectedLaunchId":status["active"]["launchId"]}),
    );
    drop(process);
    fs::remove_file(target.path().join("ready")).unwrap();
    fs::remove_file(target.path().join("cli.sock")).unwrap();
    fs::remove_file(target.path().join("signer.sock")).unwrap();
    let _restarted = launch_process(target.path(), &[], None);
    let origin = fs::read_to_string(target.path().join("ready")).unwrap();
    let replay = client
        .post(format!("{origin}/peer-rpc"))
        .header("content-type", "application/json")
        .body(encoded.event_json)
        .send()
        .unwrap();
    assert_eq!(replay.status().as_u16(), 409);
    assert_eq!(
        fs::read_to_string(target.path().join("launches"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

fn assert_ambiguous_cli_has_no_effect(binary: &std::ffi::OsStr, caller: &Path) {
    // The signed peer endpoint is labelled Peer. Give this device the same
    // name through the existing revision-checked settings producer.
    let snapshot = ok(caller, None, "system.settings.snapshot", json!({}));
    ok(
        caller,
        None,
        "system.settings.update",
        json!({"expectedRevision":snapshot["revision"],"settingId":"device-name","value":"Peer"}),
    );
    let device_path = caller.join("library/device.yaml");
    let before = fs::read(&device_path).unwrap();
    for command in [vec!["play", GAME], vec!["plugins", "on", "@test:clock"]] {
        let result = Command::new(binary)
            .arg("--socket")
            .arg(caller.join("cli.sock"))
            .args(["--device", "Peer", "--json"])
            .args(command)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1), "{result:?}");
        assert!(result.stdout.is_empty(), "{result:?}");
        let error: RpcFailure = serde_json::from_slice(&result.stderr).unwrap();
        assert_eq!(error.code, "DeviceAmbiguous");
    }
    assert_eq!(fs::read(device_path).unwrap(), before);
    assert!(!caller.join("launches").exists());
}

#[test]
#[ignore = "requires KORRI_REMOTE_CLI_BINARY; run after building clients/cli"]
fn remote_process_cli_play_keeps_save_refusal_and_resolves_target_library() {
    let binary = std::env::var_os("KORRI_REMOTE_CLI_BINARY").expect("korri binary");
    for saves in [false, true] {
        let target = tempfile::tempdir().unwrap();
        let credentials = resources(&target.path().join("private"), &"75".repeat(32), OWNER);
        let key = credentials.public_key().unwrap();
        split_saves(&target.path().join("private"));
        if saves {
            fs::write(target.path().join("save-bearing"), "").unwrap();
            fs::write(target.path().join("before-hook"), "").unwrap();
        }
        let _target_process = launch_process(target.path(), &[], None);
        let origin = fs::read_to_string(target.path().join("ready")).unwrap();
        let caller = tempfile::tempdir().unwrap();
        resources(&caller.path().join("private"), &"76".repeat(32), OWNER);
        let _caller_process = launch_process(caller.path(), &[(&credentials, &origin)], None);
        assert_ambiguous_cli_has_no_effect(&binary, caller.path());
        assert!(!target.path().join("launches").exists());
        let library = Command::new(&binary)
            .args(["--socket"])
            .arg(caller.path().join("cli.sock"))
            .args(["--device", &key, "--json", "library", "games", "list"])
            .output()
            .unwrap();
        assert!(library.status.success(), "{library:?}");
        let library: Value = serde_json::from_slice(&library.stdout).unwrap();
        assert_eq!(library["outcome"]["payload"]["games"][0]["id"], GAME);
        let play = Command::new(&binary)
            .arg("--socket")
            .arg(caller.path().join("cli.sock"))
            .args(["--device", &key, "--json", "play", GAME])
            .output()
            .unwrap();
        assert_eq!(play.status.code(), Some(1), "{play:?}");
        assert!(play.stderr.is_empty(), "{play:?}");
        let replies: Vec<RpcResponse> = serde_json::from_slice(&play.stdout).unwrap();
        assert_eq!(replies.len(), 2);
        assert!(matches!(
            &replies[0],
            RpcResponse::SavesCheck(saves::types::SaveCheckOutcome::Err(_))
        ));
        if saves {
            assert!(
                matches!(&replies[1], RpcResponse::SelectedGameLaunch(game_routes::SelectedGameLaunchOutcome::Err(error)) if error.code == "SavesSplit")
            );
            assert!(!target.path().join("launches").exists());
            assert!(!target.path().join("hooks").exists());
        } else {
            assert!(matches!(
                &replies[1],
                RpcResponse::SelectedGameLaunch(game_routes::SelectedGameLaunchOutcome::Ok(_))
            ));
            assert_eq!(
                fs::read_to_string(target.path().join("launches"))
                    .unwrap()
                    .lines()
                    .count(),
                1
            );
        }
        assert!(!caller.path().join("launches").exists());
    }
}

#[test]
fn remote_process_local_save_bearing_launch_without_split_keeps_hooks() {
    let target = tempfile::tempdir().unwrap();
    resources(&target.path().join("private"), &"77".repeat(32), OWNER);
    fs::write(target.path().join("save-bearing"), "").unwrap();
    fs::write(target.path().join("before-hook"), "").unwrap();
    let _target_process = launch_process(target.path(), &[], None);
    let launch = ok(
        target.path(),
        None,
        "app.local-games.launch.selected",
        json!({"gameId":GAME,"runnerId":RUNNER}),
    );
    assert_eq!(
        fs::read_to_string(target.path().join("hooks"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert_eq!(
        fs::read_to_string(target.path().join("launches"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    ok(
        target.path(),
        None,
        "app.session.end",
        json!({"expectedLaunchId":launch["session"]["launchId"]}),
    );
}

#[test]
#[ignore = "requires KORRI_REMOTE_PLUGIN_HOST_TEST_BINARY; tests scaled native wait"]
fn remote_process_slow_plugin_peer_outlasts_only_the_plugin_wait() {
    let binary =
        std::env::var_os("KORRI_REMOTE_PLUGIN_HOST_TEST_BINARY").expect("plugin-host test binary");
    let plugin = tempfile::tempdir().unwrap();
    let mut admin = Process(
        Command::new(binary)
            .args([
                "--exact",
                "host::tests::remote_admin_process",
                "--ignored",
                "--nocapture",
            ])
            .env("KORRI_REMOTE_PLUGIN_ROOT", plugin.path())
            .spawn()
            .unwrap(),
    );
    wait(&plugin.path().join("admin.sock"), &mut admin);
    let target = tempfile::tempdir().unwrap();
    let credentials = resources(&target.path().join("private"), &"7a".repeat(32), OWNER);
    let key = credentials.public_key().unwrap();
    fs::write(target.path().join("slow-peer"), "").unwrap();
    let _target = launch_process(target.path(), &[], Some(&plugin.path().join("admin.sock")));
    let origin = fs::read_to_string(target.path().join("ready")).unwrap();
    let caller = tempfile::tempdir().unwrap();
    let credentials = peer_rpc::test_owned_credentials(caller.path(), &"7b".repeat(32), OWNER);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let client = upstream_native::NativeClient::new_secure(origin, key, credentials)
                .with_timeout(Duration::from_millis(50));
            let reply = client
                .call(RpcRequest::PluginRepositories(
                    plugin_admin::PluginRepositoriesRequest {},
                ))
                .await
                .unwrap();
            assert!(matches!(
                reply,
                RpcResponse::PluginRepositories(PluginRepositoriesOutcome::Ok(_))
            ));
            let reply = client.call(RpcRequest::Health(HealthRequest {})).await;
            assert!(
                matches!(reply, Err(upstreams::UpstreamError::Unreachable(_))),
                "{reply:?}"
            );
        });
}
