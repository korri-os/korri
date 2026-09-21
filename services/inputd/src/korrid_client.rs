use std::{fmt, io, path::PathBuf, sync::Arc, time::Duration};

use serde::Deserialize;

use crate::virtual_targets::InputOwner;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    sync::Mutex,
};

const STATUS_REQUEST: &[u8] = br#"{"_tag":"app.session.status","payload":{}}"#;
const MAX_LAUNCH_ID_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalControlLimits {
    pub operation_timeout: Duration,
    pub status_attempts: usize,
    pub retry_delay: Duration,
    pub max_response_bytes: usize,
}

impl Default for LocalControlLimits {
    fn default() -> Self {
        Self {
            operation_timeout: Duration::from_millis(750),
            status_attempts: 2,
            retry_delay: Duration::from_millis(50),
            max_response_bytes: 16 * 1024,
        }
    }
}

#[derive(Clone, Debug)]
pub struct KorridClient {
    socket_path: PathBuf,
    limits: LocalControlLimits,
    panel_guard: Arc<Mutex<()>>,
}

impl KorridClient {
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
            limits: LocalControlLimits::default(),
            panel_guard: Arc::new(Mutex::new(())),
        }
    }

    pub fn with_limits(socket_path: impl Into<PathBuf>, limits: LocalControlLimits) -> Self {
        Self {
            socket_path: socket_path.into(),
            limits,
            panel_guard: Arc::new(Mutex::new(())),
        }
    }

    pub async fn status(&self) -> Result<SessionStatus, LocalControlError> {
        let attempts = self.limits.status_attempts.max(1);
        for attempt in 0..attempts {
            match self.request(STATUS_REQUEST).await {
                Ok(response) => return parse_status(&response),
                Err(error) if error.retryable_read() && attempt + 1 < attempts => {
                    tokio::time::sleep(self.limits.retry_delay).await;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("status always returns from a positive attempt count")
    }

    pub async fn toggle_panel_exact(&self) -> Result<ExactPanelOutcome, LocalControlError> {
        self.toggle_panel_exact_with(|| async { true }).await
    }

    /// Runs one complete Home transaction. The guard covers the status read,
    /// exact freezer mutation, Portal ownership, and portal focus. Leave must
    /// freeze the observed launch before it enters Portal. A failed Portal
    /// transition thaws and refocuses that same launch before Game ownership
    /// is restored.
    pub async fn toggle_panel_exact_with<F, Fut>(
        &self,
        enter_portal: F,
    ) -> Result<ExactPanelOutcome, LocalControlError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let _panel_guard = self.panel_guard.lock().await;
        match self.status().await? {
            SessionStatus::Running { launch_id } | SessionStatus::FocusFailed { launch_id } => {
                match self
                    .change_freezer(&launch_id, "app.session.freeze", "freeze")
                    .await?
                {
                    ExactPanelOutcome::Opened => {}
                    // The observed launch ended or was replaced before its
                    // exact freeze. Do not enter Portal or retarget the new
                    // launch. Reconciliation will observe the current state.
                    ExactPanelOutcome::NoActive | ExactPanelOutcome::AlreadyStopping => {
                        return Ok(ExactPanelOutcome::LeaveRefused);
                    }
                    other => return Ok(other),
                }
                if !enter_portal().await {
                    // Portal routing or focus can fail after a partial effect.
                    // Thaw and refocus the exact frozen launch before the caller
                    // restores Game ownership.
                    return match self
                        .change_freezer(&launch_id, "app.session.thaw", "thaw")
                        .await?
                    {
                        ExactPanelOutcome::Returned => Ok(ExactPanelOutcome::LeaveRefused),
                        other => Ok(other),
                    };
                }
                Ok(ExactPanelOutcome::Opened)
            }
            SessionStatus::Frozen { launch_id } => {
                self.change_freezer(&launch_id, "app.session.thaw", "thaw")
                    .await
            }
            SessionStatus::Stopping { .. } => Ok(ExactPanelOutcome::AlreadyStopping),
            SessionStatus::NoActive | SessionStatus::Completed => Ok(ExactPanelOutcome::NoActive),
            SessionStatus::RecoveryBlocked => Ok(ExactPanelOutcome::RecoveryBlocked),
        }
    }

    async fn change_freezer(
        &self,
        launch_id: &str,
        expected_tag: &str,
        operation: &str,
    ) -> Result<ExactPanelOutcome, LocalControlError> {
        validate_launch_id(launch_id)?;
        let request = serde_json::json!({
            "_tag": expected_tag,
            "payload": { "expectedLaunchId": launch_id }
        });
        // Like exact stop, a freezer mutation is attempted once. A transport
        // failure cannot prove korrid did not receive it.
        let response = self.request(request.to_string().as_bytes()).await?;
        parse_panel_change(&response, expected_tag, operation)
    }

    pub async fn stop_active_exact(&self) -> Result<ExactStopOutcome, LocalControlError> {
        let launch_id = match self.status().await? {
            SessionStatus::Running { launch_id }
            | SessionStatus::Frozen { launch_id }
            | SessionStatus::FocusFailed { launch_id } => launch_id,
            SessionStatus::Stopping { .. } => return Ok(ExactStopOutcome::AlreadyStopping),
            SessionStatus::NoActive => return Ok(ExactStopOutcome::NoActive),
            SessionStatus::Completed => return Ok(ExactStopOutcome::Completed),
            SessionStatus::RecoveryBlocked => return Ok(ExactStopOutcome::RecoveryBlocked),
        };
        validate_launch_id(&launch_id)?;
        let request = serde_json::json!({
            "_tag": "app.session.stop",
            "payload": { "expectedLaunchId": launch_id }
        });
        // Mutation is intentionally attempted once. A transport failure is not
        // evidence that korrid did not receive the stop.
        let response = self.request(request.to_string().as_bytes()).await?;
        parse_stop(&response)
    }

    async fn request(&self, body: &[u8]) -> Result<Vec<u8>, LocalControlError> {
        tokio::time::timeout(self.limits.operation_timeout, self.request_inner(body))
            .await
            .map_err(|_| LocalControlError::TimedOut)?
    }

    async fn request_inner(&self, body: &[u8]) -> Result<Vec<u8>, LocalControlError> {
        let mut stream = UnixStream::connect(&self.socket_path)
            .await
            .map_err(LocalControlError::Connect)?;
        let head = format!(
            "POST /rpc HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream
            .write_all(head.as_bytes())
            .await
            .map_err(LocalControlError::Write)?;
        stream
            .write_all(body)
            .await
            .map_err(LocalControlError::Write)?;
        stream.shutdown().await.map_err(LocalControlError::Write)?;

        let mut response = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let count = stream
                .read(&mut buffer)
                .await
                .map_err(LocalControlError::Read)?;
            if count == 0 {
                break;
            }
            if response.len().saturating_add(count) > self.limits.max_response_bytes {
                return Err(LocalControlError::ResponseTooLarge);
            }
            response.extend_from_slice(&buffer[..count]);
        }
        parse_http_response(&response, self.limits.max_response_bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionStatus {
    Running { launch_id: String },
    Frozen { launch_id: String },
    FocusFailed { launch_id: String },
    Stopping { launch_id: String },
    NoActive,
    Completed,
    RecoveryBlocked,
}

impl SessionStatus {
    pub fn input_owner(&self) -> InputOwner {
        match self {
            Self::Running { .. } => InputOwner::Game,
            Self::Frozen { .. }
            | Self::FocusFailed { .. }
            | Self::Stopping { .. }
            | Self::NoActive
            | Self::Completed
            | Self::RecoveryBlocked => InputOwner::Portal,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactPanelOutcome {
    Opened,
    Returned,
    FocusFailed,
    LeaveRefused,
    NoActive,
    AlreadyStopping,
    RecoveryBlocked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExactStopOutcome {
    NoActive,
    StaleIdentity,
    AlreadyStopping,
    Completed,
    RecoveryBlocked,
}

#[derive(Debug)]
pub enum LocalControlError {
    Connect(io::Error),
    Write(io::Error),
    Read(io::Error),
    TimedOut,
    ResponseTooLarge,
    InvalidHttpResponse,
    HttpStatus(u16),
    InvalidTreaty(String),
    Rejected { code: String, message: String },
}

impl LocalControlError {
    fn retryable_read(&self) -> bool {
        matches!(
            self,
            Self::Connect(_)
                | Self::Write(_)
                | Self::Read(_)
                | Self::TimedOut
                | Self::InvalidHttpResponse
        )
    }
}

impl fmt::Display for LocalControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect(error) => write!(formatter, "local control connect failed: {error}"),
            Self::Write(error) => write!(formatter, "local control request failed: {error}"),
            Self::Read(error) => write!(formatter, "local control response failed: {error}"),
            Self::TimedOut => formatter.write_str("local control operation timed out"),
            Self::ResponseTooLarge => formatter.write_str("local control response exceeded limit"),
            Self::InvalidHttpResponse => {
                formatter.write_str("local control HTTP response is invalid")
            }
            Self::HttpStatus(status) => write!(formatter, "local control returned HTTP {status}"),
            Self::InvalidTreaty(message) => {
                write!(formatter, "local control treaty mismatch: {message}")
            }
            Self::Rejected { code, message } => {
                write!(formatter, "local control rejected {code}: {message}")
            }
        }
    }
}

impl std::error::Error for LocalControlError {}

fn parse_http_response(
    response: &[u8],
    max_response_bytes: usize,
) -> Result<Vec<u8>, LocalControlError> {
    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or(LocalControlError::InvalidHttpResponse)?;
    let headers = std::str::from_utf8(&response[..header_end])
        .map_err(|_| LocalControlError::InvalidHttpResponse)?;
    let mut lines = headers.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| {
            let mut fields = line.split_ascii_whitespace();
            match (fields.next(), fields.next()) {
                (Some("HTTP/1.1"), Some(value)) => value.parse::<u16>().ok(),
                _ => None,
            }
        })
        .ok_or(LocalControlError::InvalidHttpResponse)?;

    let mut content_length = None;
    for line in lines {
        let (name, value) = line
            .split_once(':')
            .ok_or(LocalControlError::InvalidHttpResponse)?;
        if name.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return Err(LocalControlError::InvalidHttpResponse);
            }
            let value = value.trim();
            if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(LocalControlError::InvalidHttpResponse);
            }
            let parsed = value
                .parse::<usize>()
                .map_err(|_| LocalControlError::InvalidHttpResponse)?;
            if parsed > max_response_bytes {
                return Err(LocalControlError::ResponseTooLarge);
            }
            content_length = Some(parsed);
        }
    }
    let content_length = content_length.ok_or(LocalControlError::InvalidHttpResponse)?;
    let body_start = header_end + 4;
    let framed_end = body_start
        .checked_add(content_length)
        .ok_or(LocalControlError::ResponseTooLarge)?;
    if framed_end != response.len() {
        return Err(LocalControlError::InvalidHttpResponse);
    }
    if status != 200 {
        return Err(LocalControlError::HttpStatus(status));
    }
    Ok(response[body_start..framed_end].to_vec())
}

#[derive(Deserialize)]
struct ResponseEnvelope {
    #[serde(rename = "_tag")]
    tag: String,
    outcome: Outcome,
}

#[derive(Deserialize)]
#[serde(tag = "_tag", content = "payload")]
enum Outcome {
    Ok(serde_json::Value),
    Err(Failure),
}

#[derive(Deserialize)]
struct Failure {
    code: String,
    message: String,
}

#[derive(Deserialize)]
struct StatusPayload {
    active: Option<ActivePayload>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActivePayload {
    launch_id: String,
    phase: Option<String>,
}

#[derive(Deserialize)]
struct StopPayload {
    phase: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FreezePayload {
    launch_id: String,
    state: String,
}

fn parse_status(body: &[u8]) -> Result<SessionStatus, LocalControlError> {
    let response = parse_envelope(body, "app.session.status")?;
    match response {
        Outcome::Ok(payload) => {
            let payload: StatusPayload = serde_json::from_value(payload)
                .map_err(|error| LocalControlError::InvalidTreaty(error.to_string()))?;
            let Some(active) = payload.active else {
                return Ok(SessionStatus::NoActive);
            };
            validate_launch_id(&active.launch_id)?;
            match active.phase.as_deref() {
                Some("running") => Ok(SessionStatus::Running {
                    launch_id: active.launch_id,
                }),
                Some("frozen") => Ok(SessionStatus::Frozen {
                    launch_id: active.launch_id,
                }),
                Some("focus-failed") => Ok(SessionStatus::FocusFailed {
                    launch_id: active.launch_id,
                }),
                Some("stopping") => Ok(SessionStatus::Stopping {
                    launch_id: active.launch_id,
                }),
                phase => Err(LocalControlError::InvalidTreaty(format!(
                    "unknown active phase {phase:?}"
                ))),
            }
        }
        Outcome::Err(failure) => match failure.code.as_str() {
            "NoActiveSession" | "StaleLaunchIdentity" | "SelectedRemoteSessionReplaced" => {
                Ok(SessionStatus::NoActive)
            }
            "SessionCompleted" => Ok(SessionStatus::Completed),
            "HostRecoveryBlocked" => Ok(SessionStatus::RecoveryBlocked),
            _ => Err(LocalControlError::Rejected {
                code: failure.code,
                message: failure.message,
            }),
        },
    }
}

fn parse_panel_change(
    body: &[u8],
    expected_tag: &str,
    operation: &str,
) -> Result<ExactPanelOutcome, LocalControlError> {
    let response = parse_envelope(body, expected_tag)?;
    match response {
        Outcome::Ok(payload) => {
            let payload: FreezePayload = serde_json::from_value(payload)
                .map_err(|error| LocalControlError::InvalidTreaty(error.to_string()))?;
            validate_launch_id(&payload.launch_id)?;
            match (operation, payload.state.as_str()) {
                ("freeze", "frozen") => Ok(ExactPanelOutcome::Opened),
                ("thaw", "running") => Ok(ExactPanelOutcome::Returned),
                _ => Err(LocalControlError::InvalidTreaty(format!(
                    "{operation} returned freezer state {:?}",
                    payload.state
                ))),
            }
        }
        Outcome::Err(failure) => match failure.code.as_str() {
            "HostFocusFailed" if operation == "thaw" => Ok(ExactPanelOutcome::FocusFailed),
            "NoActiveSession"
            | "SessionCompleted"
            | "StaleLaunchIdentity"
            | "SelectedRemoteSessionReplaced" => Ok(ExactPanelOutcome::NoActive),
            "SessionStopping" => Ok(ExactPanelOutcome::AlreadyStopping),
            "HostRecoveryBlocked" => Ok(ExactPanelOutcome::RecoveryBlocked),
            _ => Err(LocalControlError::Rejected {
                code: failure.code,
                message: failure.message,
            }),
        },
    }
}

fn parse_stop(body: &[u8]) -> Result<ExactStopOutcome, LocalControlError> {
    let response = parse_envelope(body, "app.session.stop")?;
    match response {
        Outcome::Ok(payload) => {
            let payload: StopPayload = serde_json::from_value(payload)
                .map_err(|error| LocalControlError::InvalidTreaty(error.to_string()))?;
            match payload.phase.as_str() {
                "stopped" => Ok(ExactStopOutcome::Completed),
                "pending" => Ok(ExactStopOutcome::AlreadyStopping),
                phase => Err(LocalControlError::InvalidTreaty(format!(
                    "unknown stop phase {phase:?}"
                ))),
            }
        }
        Outcome::Err(failure) => match failure.code.as_str() {
            "NoActiveSession" => Ok(ExactStopOutcome::NoActive),
            "StaleLaunchIdentity" | "SelectedRemoteSessionReplaced" => {
                Ok(ExactStopOutcome::StaleIdentity)
            }
            "HostRecoveryBlocked" => Ok(ExactStopOutcome::RecoveryBlocked),
            "SessionCompleted" => Ok(ExactStopOutcome::Completed),
            _ => Err(LocalControlError::Rejected {
                code: failure.code,
                message: failure.message,
            }),
        },
    }
}

fn parse_envelope(body: &[u8], expected_tag: &str) -> Result<Outcome, LocalControlError> {
    let response: ResponseEnvelope = serde_json::from_slice(body)
        .map_err(|error| LocalControlError::InvalidTreaty(error.to_string()))?;
    if response.tag != expected_tag {
        return Err(LocalControlError::InvalidTreaty(format!(
            "expected {expected_tag}, got {}",
            response.tag
        )));
    }
    Ok(response.outcome)
}

fn validate_launch_id(launch_id: &str) -> Result<(), LocalControlError> {
    if launch_id.is_empty()
        || launch_id.len() > MAX_LAUNCH_ID_BYTES
        || !launch_id.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(LocalControlError::InvalidTreaty(
            "launchId is not a bounded opaque identifier".into(),
        ));
    }
    Ok(())
}
