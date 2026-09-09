use super::*;
use crate::pipe::Transport;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

const BOOTSTRAP: &str = "http://127.0.0.1:8099/kiosk-blank.html";

fn session(headless: bool) -> (Session, Transport) {
    let (input, output) = UnixStream::pair().unwrap();
    let (peer_input, peer_output) = UnixStream::pair().unwrap();
    let pipe = Transport::new(
        OwnedFd::from(input).into(),
        OwnedFd::from(peer_output).into(),
    )
    .unwrap();
    let peer = Transport::new(
        OwnedFd::from(peer_input).into(),
        OwnedFd::from(output).into(),
    )
    .unwrap();
    (
        Session {
            client: cdp::Client::new(pipe),
            id: String::new(),
            trust: None,
            startup_deadline: Instant::now() + Duration::from_secs(10),
            options: Options {
                chromium: "/unused/chromium".into(),
                profile_parent: "/unused/profiles".into(),
                brain_file: "/absent/brain.json".into(),
                surface_id: None,
                headless,
            },
        },
        peer,
    )
}

fn targets(url: &str) -> Value {
    json!({"targetInfos":[{"type":"page","url":url,"targetId":"page"}]})
}
fn frame(url: &str) -> Value {
    json!({"frameTree":{"frame":{"id":"top","url":url,"securityOrigin":PORTAL_ORIGIN}}})
}
fn command(peer: &mut Transport) -> Value {
    peer.receive(Instant::now() + Duration::from_secs(1))
        .unwrap()
}

#[test]
fn native_waits_for_bootstrap_before_attach_and_interception() {
    let (mut session, mut peer) = session(false);
    session
        .response(Action::Targets, targets("about:blank"))
        .unwrap();
    assert_eq!(command(&mut peer)["method"], "Target.getTargets");
    assert!(session.trust.is_none());
    session
        .response(Action::Targets, targets(BOOTSTRAP))
        .unwrap();
    assert_eq!(command(&mut peer)["method"], "Target.attachToTarget");
    session
        .response(Action::Attach, json!({"sessionId":"session"}))
        .unwrap();
    assert_eq!(command(&mut peer)["method"], "Page.enable");
    session.response(Action::PageEnabled, json!({})).unwrap();
    assert_eq!(command(&mut peer)["method"], "Page.getFrameTree");
    session
        .response(Action::InitialFrame, frame(BOOTSTRAP))
        .unwrap();
    let fetch = command(&mut peer);
    assert_eq!(fetch["method"], "Fetch.enable");
    assert_eq!(fetch["params"]["patterns"][0]["urlPattern"], RUNTIME_URL);
    session.response(Action::FetchEnabled, json!({})).unwrap();
    let navigate = command(&mut peer);
    assert_eq!(navigate["method"], "Page.navigate");
    assert_eq!(navigate["params"]["url"], PORTAL_URL);
    session
        .event(
            json!({"sessionId":"session","method":"Fetch.requestPaused","params":{
                "requestId":"bootstrap-fetch","frameId":"top","resourceType":"Fetch",
                "request":{"url":RUNTIME_URL,"method":"GET"}
            }}),
        )
        .unwrap();
    assert_eq!(command(&mut peer)["method"], "Fetch.failRequest");
}

#[test]
fn startup_rejects_unrelated_targets_and_ambiguous_pages() {
    for headless in [false, true] {
        for url in [
            PORTAL_URL,
            "http://attacker.invalid/",
            "chrome://newtab/",
            "",
            "http://127.0.0.1:8099/kiosk-blank.html?x",
            "http://127.0.0.1:8099/kiosk-blank.html#x",
            "http://localhost:8099/kiosk-blank.html",
        ] {
            let (mut session, _peer) = session(headless);
            assert_eq!(
                session.response(Action::Targets, targets(url)),
                Err(Error::Protocol)
            );
        }
        for pages in [
            json!([]),
            json!([{"type":"page","url":"about:blank"},{"type":"page","url":"about:blank"}]),
        ] {
            let (mut session, _peer) = session(headless);
            assert_eq!(
                session.response(Action::Targets, json!({"targetInfos":pages})),
                Err(Error::Protocol)
            );
        }
    }
}

#[test]
fn native_rechecks_committed_frame_before_interception() {
    for url in [PORTAL_URL, "http://attacker.invalid/"] {
        let (mut session, _peer) = session(false);
        assert_eq!(
            session.response(Action::InitialFrame, frame(url)),
            Err(Error::Protocol)
        );
        assert!(session.trust.is_none());
    }
    for key in ["parentId", "securityOrigin"] {
        let (mut session, _peer) = session(false);
        let mut tree = frame(BOOTSTRAP);
        tree["frameTree"]["frame"][key] = json!("untrusted");
        assert_eq!(
            session.response(Action::InitialFrame, tree),
            Err(Error::Protocol)
        );
    }
}

#[test]
fn native_bootstrap_wait_has_a_fixed_deadline() {
    for url in ["about:blank", BOOTSTRAP] {
        let (mut session, mut peer) = session(false);
        let deadline = session.startup_deadline;
        session
            .response(Action::Targets, targets("about:blank"))
            .unwrap();
        assert_eq!(command(&mut peer)["method"], "Target.getTargets");
        assert_eq!(
            session.startup_deadline, deadline,
            "polling must not reset the deadline"
        );
        session.startup_deadline = Instant::now() - Duration::from_secs(1);
        assert_eq!(
            session.response(Action::Targets, targets(url)),
            Err(Error::Timeout)
        );
        assert!(session.id.is_empty() && session.trust.is_none());
    }
}

#[test]
fn native_waits_for_the_actual_committed_bootstrap_frame() {
    // Chromium 143 advertises the requested app URL in Target.getTargets
    // before Page.getFrameTree has committed it. Captured native Wayland
    // response: url=":" and securityOrigin="://".
    for url in [":", "about:blank"] {
        let (mut session, mut peer) = session(false);
        let deadline = session.startup_deadline;
        let mut pending = frame(url);
        pending["frameTree"]["frame"]["securityOrigin"] = json!("://");
        session
            .response(Action::InitialFrame, pending.clone())
            .unwrap();
        assert_eq!(command(&mut peer)["method"], "Page.getFrameTree");
        assert!(session.trust.is_none());
        assert_eq!(session.startup_deadline, deadline);
        session
            .response(Action::InitialFrame, frame(BOOTSTRAP))
            .unwrap();
        assert_eq!(command(&mut peer)["method"], "Fetch.enable");
        session.startup_deadline = Instant::now() - Duration::from_secs(1);
        assert_eq!(
            session.response(Action::InitialFrame, pending),
            Err(Error::Timeout)
        );
        assert_eq!(
            session.response(Action::InitialFrame, frame(BOOTSTRAP)),
            Err(Error::Timeout)
        );
    }
}

#[test]
fn browser_command_uses_an_http_app_only_for_native_startup() {
    for (headless, expected) in [
        (false, vec![format!("--app={BOOTSTRAP}")]),
        (true, vec!["--headless=new".into(), "about:blank".into()]),
    ] {
        let mut command = std::process::Command::new("/unused/chromium");
        browser::configure_startup(&mut command, headless);
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            expected
                .iter()
                .map(std::ffi::OsStr::new)
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn headless_attaches_only_to_about_blank() {
    let (mut session, mut peer) = session(true);
    assert_eq!(
        session.response(Action::Targets, targets(BOOTSTRAP)),
        Err(Error::Protocol)
    );
    session
        .response(Action::Targets, targets("about:blank"))
        .unwrap();
    assert_eq!(command(&mut peer)["method"], "Target.attachToTarget");
    session
        .response(Action::InitialFrame, frame("about:blank"))
        .unwrap();
    assert_eq!(command(&mut peer)["method"], "Fetch.enable");
}
