//! Real CLI processes and a credential-free Unix server for the approved wire.
use axum::{extract::State, routing::post, Json, Router};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    fs,
    process::{Command, Output},
    sync::{Arc, Mutex},
};
#[path = "support/plugin_tools.rs"]
mod plugin_tools;
use plugin_tools::AuthorTools;
#[path = "support/plugin_staging.rs"]
mod plugin_staging;
use plugin_staging::Staging;
const KEY: &str = "author:AbCdEfGhIjKlMnOpAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
const FP: &str = "author AbCd EfGh IjKl MnOp";
const OLD_FP: &str = "author AAAA BBBB CCCC DDDD";
const DISCLOSURE: &str = "This plugin is not signed. Korri cannot confirm who made it, or that updates come from the same person. Install it only if you trust the address you got it from.";
const SOURCE: &str = "https://example.com/catalog.json";
const PEER: &str = "2222222222222222222222222222222222222222222222222222222222222222";
fn request(tag: &str, payload: Value) -> Value {
    json!({"_tag":tag,"payload":payload})
}
fn ok(tag: &str, payload: Value) -> Value {
    json!({"_tag":tag,"outcome":{"_tag":"Ok","payload":payload}})
}
fn refused(tag: &str, code: &str) -> Value {
    json!({"_tag":tag,"outcome":{"_tag":"Err","payload":{"code":code,"message":format!("Refused: {code}")}}})
}
fn source() -> Value {
    json!({"url":SOURCE,"pluginId":"@author:hello","releaseVersion":"v1"})
}
fn publisher(state: &str) -> Value {
    let mut value = json!({"namespace":"@author","state":state});
    if state != "Unsigned" {
        value["fingerprint"] = json!(FP);
    }
    if state == "Unsigned" {
        value["disclosure"] = json!(DISCLOSURE);
    }
    if state == "KeyChanged" {
        value["previousFingerprint"] = json!(OLD_FP);
    }
    if state == "New" || state == "KeyChanged" {
        value["publicKey"] = json!(KEY);
    }
    value
}
fn inspection(state: &str) -> Value {
    json!({"id":"@author:hello","title":"Hello","approval":"exact-digest","warning":"Privileged warning","ports":{"allowedTCPPorts":[80],"allowedUDPPorts":[90]},"nativeUnits":["plugin.service"],"publisher":publisher(state),"brings":[{"id":"@author:dependency","title":"Dependency","warning":"Dependency warning","alreadyApproved":false,"publisher":publisher(state)}]})
}
#[derive(Clone)]
struct Exchange {
    device: bool,
    request: Value,
    response: Value,
}
fn exchange(device: bool, tag: &str, payload: Value, response: Value) -> Exchange {
    Exchange {
        device,
        request: request(tag, payload),
        response,
    }
}
fn target() -> Vec<Exchange> {
    vec![
        exchange(
            false,
            "system.identity.status",
            json!({}),
            ok(
                "system.identity.status",
                json!({"canSwitchIdentity":true,"identity":{"_tag":"Unowned","devicePublicKey":"11".repeat(32)},"localBackupAvailable":false,"retiredPublicKeys":[]}),
            ),
        ),
        exchange(
            false,
            "system.settings.snapshot",
            json!({}),
            ok(
                "system.settings.snapshot",
                json!({"revision":"v1","deviceName":"Here","playerCount":1,"editableSettingIds":[],"plugins":[],"steamGridDbCredential":"NotConfigured"}),
            ),
        ),
        exchange(
            false,
            "app.peer.list",
            json!({}),
            ok(
                "app.peer.list",
                json!({"peers":[{"devicePublicKey":PEER,"label":"There","state":"ready","updatedAt":1}]}),
            ),
        ),
    ]
}
#[derive(Clone)]
struct Service {
    exchanges: Arc<Mutex<VecDeque<Exchange>>>,
    staging: Option<Arc<Mutex<Staging>>>,
}
struct Server {
    root: tempfile::TempDir,
    state: Service,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
fn respond(state: Service, device: bool, request: Value) -> Value {
    if let Some(staging) = state.staging {
        return staging.lock().unwrap().respond(device, request);
    }
    let next = state
        .exchanges
        .lock()
        .unwrap()
        .pop_front()
        .expect("unexpected extra RPC");
    assert_eq!(next.device, device);
    assert_eq!(next.request, request);
    next.response
}
impl Server {
    fn new(exchanges: Vec<Exchange>) -> Self {
        Self::configured(exchanges, None)
    }
    fn staging(trusted: bool) -> Self {
        Self::configured(vec![], Some(Arc::new(Mutex::new(Staging::new(trusted)))))
    }
    fn configured(exchanges: Vec<Exchange>, staging: Option<Arc<Mutex<Staging>>>) -> Self {
        let root = tempfile::tempdir().unwrap();
        let listener =
            std::os::unix::net::UnixListener::bind(root.path().join("cli.sock")).unwrap();
        listener.set_nonblocking(true).unwrap();
        let state = Service {
            exchanges: Arc::new(Mutex::new(exchanges.into())),
            staging,
        };
        let serving = state.clone();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let thread = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async move {
                let app = Router::new()
                    .route("/rpc",post(|State(state):State<Service>,Json(request):Json<Value>| async move { Json(respond(state,false,request)) }))
                    .route("/forward",post(|State(state):State<Service>,Json(body):Json<Value>| async move {
                        assert_eq!(body["devicePublicKey"],PEER);
                        Json(respond(state,true,body["request"].clone()))
                    })).with_state(serving);
                axum::serve(tokio::net::UnixListener::from_std(listener).unwrap(),app).with_graceful_shutdown(async { let _ = stopped.await; }).await.unwrap();
            });
        });
        Self {
            root,
            state,
            stop: Some(stop),
            thread: Some(thread),
        }
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_korri"));
        command
            .arg("--socket")
            .arg(self.root.path().join("cli.sock"));
        command
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }
    fn terminal(&self, args: &[&str], answers: Value) -> Command {
        let mut command = Command::new("python3");
        command
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/plugin-terminal.py"
            ))
            .arg(env!("CARGO_BIN_EXE_korri"))
            .arg("--socket")
            .arg(self.root.path().join("cli.sock"))
            .args(args)
            .env("KORRI_PLUGIN_ANSWERS", answers.to_string());
        command
    }
    fn done(&self) {
        assert!(
            self.state.exchanges.lock().unwrap().is_empty(),
            "unconsumed RPC expectations"
        );
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.stop.take().unwrap().send(());
        self.thread.take().unwrap().join().unwrap();
    }
}
fn assert_json(output: Output, expected: &Value, exit: i32) {
    assert_eq!(
        output.status.code(),
        Some(exit),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        *expected
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn assert_staged_json(output: Output, response: &Value, exit: i32) {
    assert_json(
        output,
        &json!([
            ok(
                "system.plugins.archive.begin",
                json!({"uploadId":"upload-1"})
            ),
            response
        ]),
        exit,
    );
}

#[test]
fn plugin_review_shows_all_publisher_states_and_each_dependency_locally_and_on_device() {
    for remote in [false, true] {
        for state in ["Bound", "Trusted", "New", "KeyChanged", "Unsigned"] {
            for json in [false, true] {
                let response = ok("system.plugins.inspect", inspection(state));
                let mut exchanges = if remote { target() } else { vec![] };
                exchanges.push(exchange(
                    remote,
                    "system.plugins.inspect",
                    source(),
                    response.clone(),
                ));
                let server = Server::new(exchanges);
                let mut args = vec!["plugins", "review", SOURCE, "@author:hello", "v1", "--yes"];
                if remote {
                    args.extend(["--device", "There"]);
                }
                if json {
                    args.push("--json");
                }
                let output = server.run(&args);
                if json {
                    assert_json(output, &response, 0);
                } else {
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    let text = String::from_utf8(output.stdout).unwrap();
                    for expected in [
                        "@author:hello",
                        "Privileged warning",
                        "plugin.service",
                        "80",
                        "90",
                        "@author:dependency",
                        "Dependency warning",
                        "Publisher: @author",
                        "Approval: exact-digest",
                    ] {
                        assert!(text.contains(expected), "{expected}: {text}");
                    }
                    assert_eq!(text.matches("Publisher: @author").count(), 2);
                    assert!(text.contains(if state == "KeyChanged" {
                        "Key changed"
                    } else {
                        state
                    }));
                    if state == "Unsigned" {
                        assert!(text.contains(DISCLOSURE));
                    } else {
                        assert!(text.contains(FP));
                    }
                    if state == "KeyChanged" {
                        assert!(text.contains(&format!("The publisher key for @author changed. Before: {OLD_FP}. Now: {FP}. This can mean the author replaced their key, or that someone else is publishing as them. Trust the new key only if the author told you about the change.")));
                    }
                }
                server.done();
            }
        }
    }
}
#[test]
fn plugin_publisher_owner_actions_keep_wire_responses_and_target_device() {
    for remote in [false, true] {
        for (args, tag, payload, response) in [
            (
                vec!["publishers"],
                "system.plugins.publishers",
                json!({}),
                ok(
                    "system.plugins.publishers",
                    json!({"entries":[{"namespace":"@author","fingerprint":FP,"state":"Trusted","installed":["@author:hello"]}]}),
                ),
            ),
            (
                vec!["trust", "@author", "--key", KEY],
                "system.plugins.publisher.trust",
                json!({"namespace":"@author","publicKey":KEY}),
                ok("system.plugins.publisher.trust", json!({})),
            ),
            (
                vec!["revoke", "@author", "--yes"],
                "system.plugins.publisher.revoke",
                json!({"namespace":"@author"}),
                ok("system.plugins.publisher.revoke", json!({})),
            ),
        ] {
            let mut exchanges = if remote { target() } else { vec![] };
            exchanges.push(exchange(remote, tag, payload, response.clone()));
            let server = Server::new(exchanges);
            let mut command = server.command();
            command.arg("plugins").args(args).arg("--json");
            if remote {
                command.args(["--device", "There"]);
            }
            assert_json(command.output().unwrap(), &response, 0);
            server.done();
        }
    }
    let server = Server::new(vec![]);
    assert_eq!(
        server
            .run(&["plugins", "revoke", "@author", "--json"])
            .status
            .code(),
        Some(2)
    );
    let output = server.run(&["plugins", "trust", "@author", "--key", FP]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("full public key"));
    let output = server.run(&["plugins", "trust", "@author", "--key", FP, "--json"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stderr).unwrap()["code"],
        "Usage"
    );
    server.done();
}
#[test]
fn plugin_publisher_refusals_remain_the_wire_failure_and_exit_one() {
    for remote in [false, true] {
        for code in [
            "PluginPublisherNotTrusted",
            "PluginPublisherKeyChanged",
            "PluginNamespaceTaken",
            "PluginRequirementConflict",
        ] {
            let response = refused("system.plugins.publisher.trust", code);
            let mut exchanges = if remote { target() } else { vec![] };
            exchanges.push(exchange(
                remote,
                "system.plugins.publisher.trust",
                json!({"namespace":"@author","publicKey":KEY}),
                response.clone(),
            ));
            let server = Server::new(exchanges);
            let mut args = vec!["plugins", "trust", "@author", "--key", KEY, "--json"];
            if remote {
                args.extend(["--device", "There"]);
            }
            assert_json(server.run(&args), &response, 1);
            server.done();
        }
    }
}
#[test]
fn plugin_review_trusts_new_or_changed_keys_only_after_the_terminal_choice_then_reinspects() {
    for state in ["New", "KeyChanged"] {
        for answer in ["yes", "no"] {
            let mut exchanges = vec![exchange(
                false,
                "system.plugins.inspect",
                source(),
                ok("system.plugins.inspect", inspection(state)),
            )];
            if answer == "yes" {
                exchanges.extend([
                    exchange(
                        false,
                        "system.plugins.publisher.trust",
                        json!({"namespace":"@author","publicKey":KEY}),
                        ok("system.plugins.publisher.trust", json!({})),
                    ),
                    exchange(
                        false,
                        "system.plugins.inspect",
                        source(),
                        ok("system.plugins.inspect", inspection("Trusted")),
                    ),
                ]);
            }
            let server = Server::new(exchanges);
            let output = server
                .terminal(
                    &["plugins", "review", SOURCE, "@author:hello", "v1", "--yes"],
                    json!([["Trust @author with key", answer]]),
                )
                .output()
                .unwrap();
            assert_eq!(
                output.status.code(),
                Some(if answer == "yes" { 0 } else { 1 }),
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            server.done();
        }
    }
}
fn archive_bytes() -> Vec<u8> {
    (0..(1024 * 1024 + 73)).map(|n| (n % 251) as u8).collect()
}
fn upload_exchanges(bytes: &[u8], fail_chunk: bool) -> Vec<Exchange> {
    let mut exchanges = target();
    exchanges.push(exchange(
        true,
        "system.plugins.archive.begin",
        json!({"sha256":format!("{:x}",Sha256::digest(bytes)),"size":bytes.len()}),
        ok(
            "system.plugins.archive.begin",
            json!({"uploadId":"upload-1"}),
        ),
    ));
    let mut offset = 0;
    for data in bytes.chunks(1024 * 1024) {
        let response = if fail_chunk {
            refused("system.plugins.archive.chunk", "PluginUploadFailed")
        } else {
            ok("system.plugins.archive.chunk", json!({}))
        };
        exchanges.push(exchange(
            true,
            "system.plugins.archive.chunk",
            json!({"uploadId":"upload-1","offset":offset,"data":STANDARD.encode(data)}),
            response,
        ));
        if fail_chunk {
            return exchanges;
        }
        offset += data.len();
    }
    exchanges.push(exchange(
        true,
        "system.plugins.archive.finish",
        json!({"uploadId":"upload-1"}),
        ok("system.plugins.archive.finish", json!({})),
    ));
    exchanges
}
fn sideload_command(tools: &AuthorTools, server: &Server, bytes: &[u8]) -> Command {
    fs::write(tools.root.path().join("archive.bytes"), bytes).unwrap();
    let mut command = tools.cli();
    command
        .arg("--socket")
        .arg(server.root.path().join("cli.sock"))
        .env("ARCHIVE_SHA256", format!("{:x}", Sha256::digest(bytes)))
        .args([
            "plugins",
            "sideload",
            "--device",
            "There",
            "--platform",
            "aarch64-linux",
            "--json",
            "--yes",
        ]);
    command
}
#[test]
fn plugin_sideload_builds_locally_uploads_sequential_chunks_and_requires_exact_approval() {
    for approval in [None, Some("stale"), Some("exact-digest")] {
        let tools = AuthorTools::new();
        let bytes = archive_bytes();
        let mut exchanges = upload_exchanges(&bytes, false);
        exchanges.push(exchange(
            true,
            "system.plugins.archive.inspect",
            json!({"uploadId":"upload-1"}),
            ok("system.plugins.archive.inspect", inspection("Trusted")),
        ));
        let response = ok("system.plugins.archive.install", json!({}));
        if approval == Some("exact-digest") {
            exchanges.push(exchange(
                true,
                "system.plugins.archive.install",
                json!({"uploadId":"upload-1","approval":"exact-digest"}),
                response.clone(),
            ));
        }
        let server = Server::new(exchanges);
        let mut command = sideload_command(&tools, &server, &bytes);
        if let Some(approval) = approval {
            command.args(["--approval", approval]);
        }
        let output = command.output().unwrap();
        match approval {
            None => assert_staged_json(
                output,
                &ok("system.plugins.archive.inspect", inspection("Trusted")),
                0,
            ),
            Some("stale") => {
                assert_eq!(output.status.code(), Some(1));
                assert!(output.stdout.is_empty());
                assert_eq!(
                    serde_json::from_slice::<Value>(&output.stderr).unwrap()["code"],
                    "PluginNeedsReview"
                );
            }
            _ => assert_staged_json(output, &response, 0),
        }
        let calls = fs::read_to_string(tools.root.path().join("nix.calls")).unwrap();
        assert!(calls.contains(".#packages.aarch64-linux.default"));
        assert!(!calls.contains(".#packages.x86_64-linux.default"));
        server.done();
    }
}
#[test]
fn plugin_sideload_stops_on_upload_failure_without_finish_inspect_or_install() {
    let tools = AuthorTools::new();
    let bytes = archive_bytes();
    let server = Server::new(upload_exchanges(&bytes, true));
    assert_staged_json(
        sideload_command(&tools, &server, &bytes).output().unwrap(),
        &refused("system.plugins.archive.chunk", "PluginUploadFailed"),
        1,
    );
    server.done();
}
#[test]
fn plugin_sideload_yes_does_not_trust_the_author() {
    let tools = AuthorTools::new();
    let bytes = archive_bytes();
    let mut exchanges = upload_exchanges(&bytes, false);
    let response = refused(
        "system.plugins.archive.inspect",
        "PluginPublisherNotTrusted",
    );
    exchanges.push(exchange(
        true,
        "system.plugins.archive.inspect",
        json!({"uploadId":"upload-1"}),
        response.clone(),
    ));
    let server = Server::new(exchanges);
    assert_staged_json(
        sideload_command(&tools, &server, &bytes)
            .args(["--approval", "exact-digest"])
            .output()
            .unwrap(),
        &response,
        1,
    );
    server.done();
}

#[test]
fn plugin_sideload_terminal_trust_and_approval_are_separate_even_with_yes() {
    for expires in [false, true] {
        let tools = AuthorTools::new();
        let bytes = archive_bytes();
        fs::write(tools.root.path().join("archive.bytes"), &bytes).unwrap();
        let mut exchanges = upload_exchanges(&bytes, false);
        exchanges.extend([
            exchange(
                true,
                "system.plugins.archive.inspect",
                json!({"uploadId":"upload-1"}),
                refused(
                    "system.plugins.archive.inspect",
                    "PluginPublisherNotTrusted",
                ),
            ),
            exchange(
                true,
                "system.plugins.publisher.trust",
                json!({"namespace":"@author","publicKey":KEY}),
                ok("system.plugins.publisher.trust", json!({})),
            ),
        ]);
        let id = if expires { "upload-2" } else { "upload-1" };
        if expires {
            exchanges.push(exchange(
                true,
                "system.plugins.archive.inspect",
                json!({"uploadId":"upload-1"}),
                refused("system.plugins.archive.inspect", "PluginUploadNotFound"),
            ));
            for mut next in upload_exchanges(&bytes, false).into_iter().skip(3) {
                if next.request["payload"].get("uploadId").is_some() {
                    next.request["payload"]["uploadId"] = json!(id);
                }
                if next.response["outcome"]["payload"]
                    .get("uploadId")
                    .is_some()
                {
                    next.response["outcome"]["payload"]["uploadId"] = json!(id);
                }
                exchanges.push(next);
            }
        }
        exchanges.extend([
            exchange(
                true,
                "system.plugins.archive.inspect",
                json!({"uploadId":id}),
                ok("system.plugins.archive.inspect", inspection("Trusted")),
            ),
            exchange(
                true,
                "system.plugins.archive.install",
                json!({"uploadId":id,"approval":"exact-digest"}),
                ok("system.plugins.archive.install", json!({})),
            ),
        ]);
        let server = Server::new(exchanges);
        let mut command = server.terminal(
            &[
                "plugins",
                "sideload",
                "--device",
                "There",
                "--platform",
                "aarch64-linux",
                "--yes",
            ],
            json!([
                ["Trust this author's key", "yes"],
                ["then type the exact approval digest", "exact-digest"]
            ]),
        );
        tools.configure(&mut command);
        let output = command
            .env("ARCHIVE_SHA256", format!("{:x}", Sha256::digest(&bytes)))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains(FP));
        assert!(text.contains("Uploaded 1048649 of 1048649 bytes"));
        assert!(text.contains("Privileged warning"));
        assert!(text.contains("Approval: exact-digest"));
        assert!(text.contains("Plugin installed, off"));
        if expires {
            assert!(text.contains("staged archive expired"));
        }
        server.done();
    }
}

#[test]
fn plugin_sideload_platform_is_explicit_unless_the_publisher_proves_any_architecture() {
    for platform in ["any", "aarch64-linux"] {
        let tools = AuthorTools::new();
        let bytes = archive_bytes();
        fs::write(tools.root.path().join("archive.bytes"), &bytes).unwrap();
        let mut exchanges = if platform == "any" {
            upload_exchanges(&bytes, false)
        } else {
            target()
        };
        if platform == "any" {
            exchanges.push(exchange(
                true,
                "system.plugins.archive.inspect",
                json!({"uploadId":"upload-1"}),
                ok("system.plugins.archive.inspect", inspection("Trusted")),
            ));
        }
        let server = Server::new(exchanges);
        let output = tools
            .cli()
            .arg("--socket")
            .arg(server.root.path().join("cli.sock"))
            .args(["plugins", "sideload", "--device", "There", "--json"])
            .env("ARCHIVE_PLATFORM", platform)
            .env("ARCHIVE_SHA256", format!("{:x}", Sha256::digest(&bytes)))
            .output()
            .unwrap();
        if platform == "any" {
            assert_staged_json(
                output,
                &ok("system.plugins.archive.inspect", inspection("Trusted")),
                0,
            );
        } else {
            assert_eq!(output.status.code(), Some(2));
            assert!(output.stdout.is_empty());
            assert!(String::from_utf8_lossy(&output.stderr).contains("--platform"));
        }
        let publication = fs::read_to_string(tools.root.path().join("publisher.calls")).unwrap();
        assert!(
            !publication.lines().any(|argument| argument == "--platform"),
            "{publication}"
        );
        server.done();
    }
}

#[test]
fn plugin_sideload_publisher_failure_never_uploads_any_bytes() {
    let tools = AuthorTools::new();
    let bytes = archive_bytes();
    let server = Server::new(target());
    let output = sideload_command(&tools, &server, &bytes)
        .env("PUBLISH_EXIT", "7")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stderr).unwrap()["code"],
        "PluginPublishFailed"
    );
    server.done();
}

#[test]
fn plugin_publishers_human_output_names_bound_and_pinned_publishers_and_their_installs() {
    for remote in [false, true] {
        let mut exchanges = if remote { target() } else { vec![] };
        exchanges.push(exchange(remote, "system.plugins.publishers", json!({}), ok("system.plugins.publishers", json!({"entries":[
            {"namespace":"@korri","fingerprint":"korri-plugins-1 AbCd EfGh IjKl MnOp","state":"Bound","installed":["@korri:runner"]},
            {"namespace":"@author","fingerprint":FP,"state":"Trusted","installed":["@author:hello"]}
        ]}))));
        let server = Server::new(exchanges);
        let mut args = vec!["plugins", "publishers"];
        if remote {
            args.extend(["--device", "There"]);
        }
        let output = server.run(&args);
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        for expected in [
            "@korri",
            "bound",
            "@korri:runner",
            "@author",
            FP,
            "pinned",
            "@author:hello",
        ] {
            assert!(text.contains(expected), "{expected}: {text}");
        }
        server.done();
    }
}

#[test]
fn plugin_sideload_accepts_host_platform_names_and_any_architecture() {
    let longest_platform = format!("{}-x", "a".repeat(62));
    for (platform, published_platform) in [
        ("x86_64-linux", "x86_64-linux"),
        ("aarch64-linux", "aarch64-linux"),
        ("any", "any"),
        ("x86_64-linux", "any"),
        (longest_platform.as_str(), longest_platform.as_str()),
    ] {
        let tools = AuthorTools::new();
        let bytes = b"archive";
        fs::write(tools.root.path().join("archive.bytes"), bytes).unwrap();
        let mut exchanges = upload_exchanges(bytes, false);
        let response = ok("system.plugins.archive.inspect", inspection("Trusted"));
        exchanges.push(exchange(
            true,
            "system.plugins.archive.inspect",
            json!({"uploadId":"upload-1"}),
            response.clone(),
        ));
        let server = Server::new(exchanges);
        let output = tools
            .cli()
            .arg("--socket")
            .arg(server.root.path().join("cli.sock"))
            .args([
                "plugins",
                "sideload",
                "--device",
                "There",
                "--platform",
                platform,
                "--json",
            ])
            .env("ARCHIVE_PLATFORM", published_platform)
            .env("ARCHIVE_SHA256", format!("{:x}", Sha256::digest(bytes)))
            .output()
            .unwrap();
        assert_staged_json(output, &response, 0);
        let publication = fs::read_to_string(tools.root.path().join("publisher.calls")).unwrap();
        let arguments = publication.lines().collect::<Vec<_>>();
        let hint = arguments
            .iter()
            .position(|argument| *argument == "--platform");
        if platform == "any" {
            assert!(hint.is_none(), "{publication}");
        } else {
            assert_eq!(
                arguments[hint.expect("archive publisher receives the concrete platform") + 1],
                platform
            );
        }
        let calls = fs::read_to_string(tools.root.path().join("nix.calls")).unwrap();
        assert!(
            calls
                .find("eval --json .#packages --apply builtins.attrNames")
                .unwrap()
                < calls.find("build --json").unwrap()
        );
        if platform == "any" {
            assert!(!calls.contains(".#packages.any.default"));
            assert!(calls.contains(".#packages.x86_64-linux.default"));
        } else {
            assert!(calls.contains(&format!(".#packages.{platform}.default")));
        }
        server.done();
    }
}

#[test]
fn plugin_sideload_rejects_invalid_platforms_before_running_author_tools() {
    let too_long = format!("{}-x", "a".repeat(63));
    for platform in [
        "",
        "x86_64",
        "x86_64/linux",
        "x86_64.linux",
        "x86_64-linux\n",
        "é-linux",
        too_long.as_str(),
    ] {
        let tools = AuthorTools::new();
        let server = Server::new(target());
        let output = tools
            .cli()
            .arg("--socket")
            .arg(server.root.path().join("cli.sock"))
            .args([
                "plugins",
                "sideload",
                "--device",
                "There",
                "--platform",
                platform,
                "--json",
            ])
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(2),
            "{platform:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stderr).unwrap()["code"],
            "Usage"
        );
        assert!(!tools.root.path().join("nix.calls").exists());
        assert!(!tools.root.path().join("publisher.calls").exists());
        server.done();
    }
}

#[test]
fn plugin_sideload_any_refuses_native_archives_before_upload() {
    let tools = AuthorTools::new();
    let bytes = b"archive";
    fs::write(tools.root.path().join("archive.bytes"), bytes).unwrap();
    let server = Server::new(target());
    let output = tools
        .cli()
        .arg("--socket")
        .arg(server.root.path().join("cli.sock"))
        .args([
            "plugins",
            "sideload",
            "--device",
            "There",
            "--platform",
            "any",
            "--json",
        ])
        .env("ARCHIVE_PLATFORM", "aarch64-linux")
        .env("ARCHIVE_SHA256", format!("{:x}", Sha256::digest(bytes)))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    server.done();
}

#[test]
fn plugin_sideload_review_then_resume_installs_the_same_staged_upload() {
    let tools = AuthorTools::new();
    let server = Server::staging(true);
    let first = sideload_command(&tools, &server, b"archive")
        .output()
        .unwrap();
    assert_eq!(
        first.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let responses: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(
        responses
            .as_array()
            .expect("sideload returns observed RPC responses")
            .len(),
        2
    );
    let upload = responses[0]["outcome"]["payload"]["uploadId"]
        .as_str()
        .expect("begin response exposes the resumable upload ID");
    let approval = responses[1]["outcome"]["payload"]["approval"]
        .as_str()
        .unwrap();
    let builds = fs::read(tools.root.path().join("nix.calls")).unwrap();
    let publications = fs::read(tools.root.path().join("publisher.calls")).unwrap();
    let output = tools
        .cli()
        .arg("--socket")
        .arg(server.root.path().join("cli.sock"))
        .args([
            "plugins",
            "sideload",
            "--device",
            "There",
            "--upload",
            upload,
            "--approval",
            approval,
            "--json",
        ])
        .env("NIX_EXIT", "7")
        .env("PUBLISH_EXIT", "7")
        .output()
        .unwrap();
    assert_json(
        output,
        &json!([ok("system.plugins.archive.install", json!({}))]),
        0,
    );
    assert_eq!(
        fs::read(tools.root.path().join("nix.calls")).unwrap(),
        builds
    );
    assert_eq!(
        fs::read(tools.root.path().join("publisher.calls")).unwrap(),
        publications
    );
    let staged = server.state.staging.as_ref().unwrap().lock().unwrap();
    assert_eq!(staged.begins, 1);
    assert!(staged.installed);
}

#[test]
fn plugin_sideload_trust_then_resume_keeps_the_original_upload() {
    let tools = AuthorTools::new();
    let server = Server::staging(false);
    let first = sideload_command(&tools, &server, b"archive")
        .output()
        .unwrap();
    assert_eq!(first.status.code(), Some(1));
    let responses: Value = serde_json::from_slice(&first.stdout).unwrap();
    let upload = responses[0]["outcome"]["payload"]["uploadId"]
        .as_str()
        .expect("a trust refusal still exposes its upload ID");
    assert_json(
        server.run(&[
            "plugins", "trust", "@author", "--key", KEY, "--device", "There", "--json",
        ]),
        &ok("system.plugins.publisher.trust", json!({})),
        0,
    );
    let reviewed = tools
        .cli()
        .arg("--socket")
        .arg(server.root.path().join("cli.sock"))
        .args([
            "plugins", "sideload", "--device", "There", "--upload", upload, "--json",
        ])
        .env("NIX_EXIT", "7")
        .env("PUBLISH_EXIT", "7")
        .output()
        .unwrap();
    assert!(
        reviewed.status.success(),
        "{}",
        String::from_utf8_lossy(&reviewed.stderr)
    );
    let reviewed: Value = serde_json::from_slice(&reviewed.stdout).unwrap();
    assert_eq!(
        reviewed,
        json!([ok("system.plugins.archive.inspect", inspection("Trusted"))])
    );
    let approval = reviewed[0]["outcome"]["payload"]["approval"]
        .as_str()
        .unwrap();
    let output = tools
        .cli()
        .arg("--socket")
        .arg(server.root.path().join("cli.sock"))
        .args([
            "plugins",
            "sideload",
            "--device",
            "There",
            "--upload",
            upload,
            "--approval",
            approval,
            "--json",
        ])
        .env("NIX_EXIT", "7")
        .env("PUBLISH_EXIT", "7")
        .output()
        .unwrap();
    assert_json(
        output,
        &json!([ok("system.plugins.archive.install", json!({}))]),
        0,
    );
    let staged = server.state.staging.as_ref().unwrap().lock().unwrap();
    assert_eq!(staged.begins, 1);
    assert!(staged.installed);
}

#[test]
fn plugin_sideload_missing_upload_returns_failure_and_requires_a_fresh_run() {
    let unused = Server::new(vec![]);
    assert_eq!(
        unused
            .run(&["plugins", "sideload", "--device", "There", "--upload", ""])
            .status
            .code(),
        Some(2)
    );
    unused.done();
    let server = Server::staging(true);
    let output = server.run(&[
        "plugins", "sideload", "--device", "There", "--upload", "expired",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains("Next: run sideload again without --upload"),
        "{text}"
    );
    assert_json(
        server.run(&[
            "plugins", "sideload", "--device", "There", "--upload", "expired", "--json",
        ]),
        &json!([refused(
            "system.plugins.archive.inspect",
            "PluginUploadNotFound"
        )]),
        1,
    );
    let staged = server.state.staging.as_ref().unwrap().lock().unwrap();
    assert_eq!(staged.begins, 0);
    assert!(!staged.installed);
    assert_eq!(
        staged
            .calls
            .iter()
            .filter(|request| request["_tag"] == "system.plugins.archive.inspect")
            .count(),
        2
    );
}

#[test]
fn plugin_sideload_resumed_terminal_trust_uses_the_project_and_existing_key() {
    let tools = AuthorTools::new();
    let server = Server::staging(false);
    let first = sideload_command(&tools, &server, b"archive")
        .output()
        .unwrap();
    assert_eq!(first.status.code(), Some(1));
    let responses: Value = serde_json::from_slice(&first.stdout).unwrap();
    let upload = responses[0]["outcome"]["payload"]["uploadId"]
        .as_str()
        .unwrap();
    let publications = fs::read(tools.root.path().join("publisher.calls")).unwrap();
    let mut command = server.terminal(
        &[
            "plugins", "sideload", "--device", "There", "--upload", upload, "--yes",
        ],
        json!([
            ["Trust this author's key", "yes"],
            ["then type the exact approval digest", "exact-digest"]
        ]),
    );
    tools.configure(&mut command);
    let output = command.env("PUBLISH_EXIT", "7").output().unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains(FP));
    assert!(text.contains("Plugin installed, off"));
    let calls = fs::read_to_string(tools.root.path().join("nix.calls")).unwrap();
    assert_eq!(
        calls
            .lines()
            .filter(|line| line.starts_with("build "))
            .count(),
        1
    );
    assert_eq!(calls.matches("key generate-secret").count(), 1);
    assert_eq!(calls.matches("key convert-secret-to-public").count(), 2);
    assert_eq!(
        fs::read(tools.root.path().join("publisher.calls")).unwrap(),
        publications
    );
    let staged = server.state.staging.as_ref().unwrap().lock().unwrap();
    assert_eq!(staged.begins, 1);
    assert!(staged.installed);
}

#[test]
fn plugin_sideload_resumed_terminal_without_project_or_key_never_prompts_or_creates_a_key() {
    for missing in ["project", "key"] {
        let tools = AuthorTools::new();
        let server = Server::staging(false);
        let first = sideload_command(&tools, &server, b"archive")
            .output()
            .unwrap();
        assert_eq!(first.status.code(), Some(1));
        let responses: Value = serde_json::from_slice(&first.stdout).unwrap();
        let upload = responses[0]["outcome"]["payload"]["uploadId"]
            .as_str()
            .unwrap();
        let key = tools
            .root
            .path()
            .join("config/korri/publishers/@author.secret");
        if missing == "project" {
            fs::remove_file(tools.root.path().join("flake.nix")).unwrap();
        } else {
            fs::remove_file(&key).unwrap();
        }
        let mut command = server.terminal(
            &[
                "plugins", "sideload", "--device", "There", "--upload", upload, "--yes",
            ],
            json!([]),
        );
        tools.configure(&mut command);
        let output = command.output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(1),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(
            text.contains("no available plugin publisher and existing author key"),
            "{text}"
        );
        assert!(
            text.contains("korri plugins trust <namespace> --key <full public key> --device"),
            "{text}"
        );
        assert!(text.contains(&format!("--upload '{upload}'")), "{text}");
        assert!(!text.contains("Trust this author's key on the selected device?"));
        if missing == "key" {
            assert!(!key.exists());
        }
        let calls = fs::read_to_string(tools.root.path().join("nix.calls")).unwrap();
        assert_eq!(calls.matches("key generate-secret").count(), 1);
        let staged = server.state.staging.as_ref().unwrap().lock().unwrap();
        assert_eq!(staged.begins, 1);
        assert!(!staged.installed);
    }
}

#[test]
fn plugin_sideload_human_review_prints_an_executable_resume_command() {
    let tools = AuthorTools::new();
    let server = Server::staging(true);
    fs::create_dir(tools.root.path().join("plugin project")).unwrap();
    fs::write(tools.root.path().join("archive.bytes"), b"archive").unwrap();
    let first = tools
        .cli()
        .arg("--socket")
        .arg(server.root.path().join("cli.sock"))
        .args([
            "plugins",
            "sideload",
            "plugin project",
            "--device",
            "There",
            "--platform",
            "aarch64-linux",
        ])
        .env(
            "ARCHIVE_SHA256",
            format!("{:x}", Sha256::digest(b"archive")),
        )
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let text = String::from_utf8(first.stdout).unwrap();
    assert!(text.contains("Upload: upload-1"), "{text}");
    let resume = text
        .lines()
        .find_map(|line| line.strip_prefix("Resume: "))
        .expect("exact resume command");
    assert!(resume.contains("'plugin project'"));
    assert!(resume.contains("--upload 'upload-1' --approval 'exact-digest'"));
    std::os::unix::fs::symlink(
        env!("CARGO_BIN_EXE_korri"),
        tools.root.path().join("bin/korri"),
    )
    .unwrap();
    let mut command = Command::new("/bin/sh");
    tools.configure(&mut command);
    let output = command
        .args(["-c", resume])
        .env("KORRI_CLI_SOCKET", server.root.path().join("cli.sock"))
        .env("NIX_EXIT", "7")
        .env("PUBLISH_EXIT", "7")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Plugin installed, off"));
    let staged = server.state.staging.as_ref().unwrap().lock().unwrap();
    assert_eq!(staged.begins, 1);
    assert!(staged.installed);
}
