use crate::{
    BOOTSTRAP_URL, BRAIN_FILE, Error, PORTAL_ORIGIN, PORTAL_URL, RUNTIME_URL, browser, cdp, runtime,
};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Launch-only process wiring. No file is written with these options.
pub struct Options {
    pub chromium: PathBuf,
    pub profile_parent: PathBuf,
    pub brain_file: PathBuf,
    pub surface_id: Option<String>,
    pub headless: bool,
}
impl Options {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, Error> {
        let mut args = args.into_iter();
        let mut chromium = None;
        let mut profile_parent = None;
        let mut brain_file = PathBuf::from(BRAIN_FILE);
        let mut surface_id = None;
        let mut headless = false;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--chromium" if chromium.is_none() => {
                    chromium = Some(PathBuf::from(args.next().ok_or(Error::Configuration)?))
                }
                "--profile-parent" if profile_parent.is_none() => {
                    profile_parent = Some(PathBuf::from(args.next().ok_or(Error::Configuration)?))
                }
                "--brain-file" => {
                    brain_file = PathBuf::from(args.next().ok_or(Error::Configuration)?)
                }
                "--surface-id" if surface_id.is_none() => {
                    surface_id = Some(
                        args.next()
                            .filter(|s| !s.is_empty() && s.len() <= runtime::MAX_CONFIG)
                            .ok_or(Error::Configuration)?,
                    )
                }
                "--headless" if !headless => headless = true,
                _ => return Err(Error::Configuration),
            }
        }
        let chromium = chromium.ok_or(Error::Configuration)?;
        let profile_parent = profile_parent.ok_or(Error::Configuration)?;
        if !chromium.is_absolute() || !profile_parent.is_absolute() || !brain_file.is_absolute() {
            return Err(Error::Configuration);
        }
        Ok(Self {
            chromium,
            profile_parent,
            brain_file,
            surface_id,
            headless,
        })
    }
}

pub fn run(options: Options) -> Result<(), Error> {
    browser::harden()?;
    let (_browser, pipe) = browser::Browser::spawn(&options)?;
    let mut session = Session {
        client: cdp::Client::new(pipe),
        id: String::new(),
        trust: None,
        startup_deadline: Instant::now() + Duration::from_secs(10),
        options,
    };
    session.send("Target.getTargets", json!({}), Action::Targets)?;
    loop {
        match session.client.next()? {
            cdp::Message::Response { action, result } => session.response(action, result)?,
            cdp::Message::Event(event) => session.event(event)?,
        }
    }
}

enum Action {
    Targets,
    Attach,
    PageEnabled,
    InitialFrame,
    FetchEnabled,
    Navigate,
    CheckFrame { request: Value, epoch: u64 },
    Ack,
}
struct Session {
    client: cdp::Client<Action>,
    id: String,
    trust: Option<Trust>,
    startup_deadline: Instant,
    options: Options,
}
impl Session {
    fn send(&mut self, method: &str, params: Value, action: Action) -> Result<(), Error> {
        self.client.send(&self.id, method, params, action)
    }
    fn initial_url(&self) -> &'static str {
        if self.options.headless {
            "about:blank"
        } else {
            BOOTSTRAP_URL
        }
    }
    fn response(&mut self, action: Action, result: Value) -> Result<(), Error> {
        match action {
            Action::Targets => {
                let pages: Vec<_> = result["targetInfos"]
                    .as_array()
                    .ok_or(Error::Protocol)?
                    .iter()
                    .filter(|t| t["type"] == "page")
                    .collect();
                if pages.len() != 1 {
                    return Err(Error::Protocol);
                }
                if pages[0]["url"] != self.initial_url() {
                    if !self.options.headless && pages[0]["url"] == "about:blank" {
                        if Instant::now() >= self.startup_deadline {
                            return Err(Error::Timeout);
                        }
                        // Native Chromium may expose its page before the app URL commits.
                        // Do not attach or enable credential interception on that page yet.
                        std::thread::sleep(Duration::from_millis(25));
                        return self.send("Target.getTargets", json!({}), Action::Targets);
                    }
                    return Err(Error::Protocol);
                }
                if Instant::now() >= self.startup_deadline {
                    return Err(Error::Timeout);
                }
                let target = pages[0]["targetId"].as_str().ok_or(Error::Protocol)?;
                self.send(
                    "Target.attachToTarget",
                    json!({"targetId":target,"flatten":true}),
                    Action::Attach,
                )
            }
            Action::Attach => {
                self.id = result["sessionId"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or(Error::Protocol)?
                    .into();
                self.send("Page.enable", json!({}), Action::PageEnabled)
            }
            Action::PageEnabled => self.send("Page.getFrameTree", json!({}), Action::InitialFrame),
            Action::InitialFrame => {
                if Instant::now() >= self.startup_deadline {
                    return Err(Error::Timeout);
                }
                let frame = &result["frameTree"]["frame"];
                // Target metadata can contain the requested app URL before its
                // first document commits. No interception or trust exists yet.
                if !self.options.headless
                    && frame.get("parentId").is_none()
                    && matches!(frame["url"].as_str(), Some(":" | "about:blank"))
                {
                    std::thread::sleep(Duration::from_millis(25));
                    return self.send("Page.getFrameTree", json!({}), Action::InitialFrame);
                }
                if frame["url"] != self.initial_url()
                    || frame.get("parentId").is_some()
                    || (!self.options.headless && frame["securityOrigin"] != PORTAL_ORIGIN)
                {
                    return Err(Error::Protocol);
                }
                self.trust = Some(Trust::new(
                    frame["id"].as_str().ok_or(Error::Protocol)?.into(),
                ));
                self.send(
                    "Fetch.enable",
                    json!({"patterns":[{"urlPattern":RUNTIME_URL,"requestStage":"Request"}]}),
                    Action::FetchEnabled,
                )
            }
            Action::FetchEnabled => {
                self.send("Page.navigate", json!({"url":PORTAL_URL}), Action::Navigate)
            }
            Action::Navigate => {
                if result.get("errorText").is_some()
                    || result["frameId"] != self.trust.as_ref().ok_or(Error::Protocol)?.top
                {
                    return Err(Error::Protocol);
                }
                Ok(())
            }
            Action::CheckFrame { request, epoch } => {
                let trust = self.trust.as_ref().ok_or(Error::Protocol)?;
                let request_id = request["requestId"].as_str().ok_or(Error::Protocol)?;
                if epoch != trust.epoch || !trust.authorizes(&request, &result) {
                    return self.deny(request_id);
                }
                // Reopen after every authorized request, including reloads after
                // korrid replaces brain.json. Never hold an old capability cache.
                let config =
                    runtime::load(&self.options.brain_file, self.options.surface_id.as_deref())?;
                let body = serde_json::to_vec(&config).map_err(|_| Error::Configuration)?;
                self.send(
                    "Fetch.fulfillRequest",
                    json!({
                        "requestId":request_id, "responseCode":200,
                        "responseHeaders":[
                            {"name":"Content-Type","value":"application/json"},
                            {"name":"Cache-Control","value":"no-store"},
                            {"name":"Pragma","value":"no-cache"},
                            {"name":"X-Content-Type-Options","value":"nosniff"}
                        ],
                        "body":runtime::base64(&body)
                    }),
                    Action::Ack,
                )
            }
            Action::Ack => Ok(()),
        }
    }
    fn deny(&mut self, request_id: &str) -> Result<(), Error> {
        self.send(
            "Fetch.failRequest",
            json!({"requestId":request_id,"errorReason":"BlockedByClient"}),
            Action::Ack,
        )
    }
    fn event(&mut self, event: Value) -> Result<(), Error> {
        let method = event["method"].as_str().ok_or(Error::Protocol)?;
        let params = &event["params"];
        if method == "Target.detachedFromTarget" && params["sessionId"] == self.id {
            return Err(Error::Closed);
        }
        if event["sessionId"] != self.id {
            return Ok(());
        }
        match method {
            "Inspector.detached" | "Inspector.targetCrashed" => Err(Error::Closed),
            "Fetch.requestPaused" => {
                let request_id = params["requestId"].as_str().ok_or(Error::Protocol)?;
                let trust = self.trust.as_ref().ok_or(Error::Protocol)?;
                if !trust.eligible(params) {
                    return self.deny(request_id);
                }
                let epoch = trust.epoch;
                self.send(
                    "Page.getFrameTree",
                    json!({}),
                    Action::CheckFrame {
                        request: params.clone(),
                        epoch,
                    },
                )
            }
            "Page.frameNavigated" => {
                if let Some(trust) = &mut self.trust {
                    trust.navigated(&params["frame"]);
                }
                Ok(())
            }
            "Page.frameRequestedNavigation"
            | "Page.frameStartedNavigating"
            | "Page.navigatedWithinDocument" => {
                if let Some(trust) = &mut self.trust
                    && params["frameId"] == trust.top
                {
                    trust.epoch += 1;
                    if params["url"] != PORTAL_URL {
                        trust.revoke();
                    }
                }
                Ok(())
            }
            "Page.frameDetached" => {
                if let Some(trust) = &mut self.trust
                    && params["frameId"] == trust.top
                {
                    trust.revoke();
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
#[path = "startup_tests.rs"]
mod startup_tests;

#[derive(PartialEq, Eq)]
enum FrameTrust {
    Waiting,
    Trusted,
    Revoked,
}

pub(crate) struct Trust {
    top: String,
    state: FrameTrust,
    epoch: u64,
}
impl Trust {
    pub(crate) fn new(top: String) -> Self {
        Self {
            top,
            state: FrameTrust::Waiting,
            epoch: 0,
        }
    }
    fn revoke(&mut self) {
        self.state = FrameTrust::Revoked;
        self.epoch += 1;
    }
    pub(crate) fn navigated(&mut self, frame: &Value) {
        if frame.get("parentId").is_some() {
            return;
        }
        self.epoch += 1;
        if !self.frame_is_trusted(frame) {
            self.revoke();
        } else if self.state != FrameTrust::Revoked {
            self.state = FrameTrust::Trusted;
        }
    }
    fn frame_is_trusted(&self, frame: &Value) -> bool {
        frame["id"] == self.top
            && frame["url"] == PORTAL_URL
            && frame["securityOrigin"] == PORTAL_ORIGIN
            && frame.get("parentId").is_none()
    }
    fn eligible(&self, request: &Value) -> bool {
        self.state == FrameTrust::Trusted
            && request["frameId"] == self.top
            && request["request"]["url"] == RUNTIME_URL
            && request["request"]["method"] == "GET"
            && matches!(request["resourceType"].as_str(), Some("Fetch" | "XHR"))
    }
    pub(crate) fn authorizes(&self, request: &Value, tree: &Value) -> bool {
        self.eligible(request) && self.frame_is_trusted(&tree["frameTree"]["frame"])
    }
}
