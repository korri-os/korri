//! Binding edge. This is the only module that starts or calls korrid.
//!
//! It starts the real brain through `korrid::start_local_server`, the same
//! public embedded entry point the Android edge uses, with `EmbeddedPlatform::
//! default()`: no relays, no reach transport, no viewer, no owner signer. It
//! then speaks the real typed wire: `korrid::RpcRequest` out, `korrid::
//! RpcResponse` back, over `POST /rpc` on 127.0.0.1 with the per-run bearer
//! capability. The capability lives only in this process's memory.

use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

use korrid::{
    CatalogSnapshot, CatalogSnapshotOutcome, CatalogSnapshotRequest, EmbeddedPlatform, Health,
    HealthOutcome, HealthRequest, IdentityStatus, IdentityStatusOutcome, IdentityStatusRequest,
    PeerList, PeerListOutcome, PeerListRequest, RpcRequest, RpcResponse,
};
use reqwest::{blocking::Client, StatusCode};
use zeroize::Zeroizing;

/// Names a running process may carry without changing what korrid does.
/// `KORRI_ROOT` locates the checkout for the Nix task. `KORRI_BUILD_ID` is the
/// existing compile-time source stamp (`clients/cli/package.nix`).
const ALLOWED_KORRI_NAMES: [&str; 2] = ["KORRI_ROOT", "KORRI_BUILD_ID"];

/// korrid reads several `KORRID_*`/`KORRI_*` names at runtime, for example
/// `KORRID_UPSTREAMS` (upstreams.rs `from_env_or_file`), which would point the
/// embedded brain at real peers even with a fresh scratch root. Refuse every
/// such inherited name rather than guess which ones are safe. Returns the
/// offending names only, never a value.
pub fn inherited_korri_environment() -> Vec<String> {
    let mut names: Vec<String> = std::env::vars_os()
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .filter(|name| name.starts_with("KORRID_") || name.starts_with("KORRI_"))
        .filter(|name| !ALLOWED_KORRI_NAMES.contains(&name.as_str()))
        .collect();
    names.sort();
    names
}

/// No browser takes part. korrid requires an allowed origin; this is the same
/// reserved-TLD value korrid's own embedded tests use (`embedded_tests.rs`).
/// A `.example` name cannot serve a page, and the RPC still needs the bearer.
const NO_BROWSER_ORIGIN: &str = "https://portal.example";

/// Total time one refresh (all four requests) may take. The shutdown wait in
/// main.rs is longer, so a refresh in flight always finishes first.
pub const INSPECTION_BUDGET: Duration = Duration::from_secs(4);

/// Why one request did not produce its typed outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// korrid answered HTTP 401 or 403: the capability was not accepted.
    Unauthorized(u16),
    /// No answer from the loopback port (refused, reset, timed out).
    Unreachable(String),
    /// korrid answered with its typed `Err(RpcFailure)`.
    Rejected { code: String, message: String },
    /// HTTP status, body or response tag that the typed contract does not allow.
    Unexpected(String),
}

/// One refresh: four real requests and their typed outcomes.
pub struct Inspection {
    pub health: Result<Health, Problem>,
    pub identity: Result<IdentityStatus, Problem>,
    pub catalog: Result<CatalogSnapshot, Problem>,
    pub peers: Result<PeerList, Problem>,
    pub elapsed: Duration,
}

/// Which bearer value a refresh sends. `Wrong` exists only for the visual
/// proof of the rejected state; it sends a fresh random value to the real server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Credential {
    Real,
    Wrong,
}

/// Result of sending `system.health` with each credential shape.
#[derive(Debug)]
pub struct AuthProbe {
    pub without_bearer: Option<u16>,
    pub wrong_bearer: Option<u16>,
    pub real_bearer: Option<u16>,
}

/// The running embedded korrid and its disposable storage.
///
/// No `Debug`: the capability must not reach a formatter.
pub struct Brain {
    port: u16,
    capability: Zeroizing<String>,
    wrong_capability: Zeroizing<String>,
    client: Client,
    running: bool,
    // Drop order: the server stops (in `Drop`) before these directories go.
    readable: Option<tempfile::TempDir>,
    private: Option<tempfile::TempDir>,
}

impl Brain {
    pub fn start() -> Result<Self, String> {
        let readable = tempfile::Builder::new()
            .prefix("korri-slint-spike-storage-")
            .tempdir()
            .map_err(|error| format!("scratch storage: {error}"))?;
        let private = tempfile::Builder::new()
            .prefix("korri-slint-spike-private-")
            .tempdir()
            .map_err(|error| format!("scratch private state: {error}"))?;
        // korrid's federation store requires exactly 0700 on the private root
        // (federation/store.rs `private_directory`); embedded_tests.rs sets it
        // the same way after creating its temporary directory.
        std::fs::set_permissions(private.path(), std::fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("scratch private state mode: {error}"))?;
        let readable_path = readable
            .path()
            .to_str()
            .ok_or("scratch storage path is not UTF-8")?
            .to_owned();
        let private_path = private
            .path()
            .to_str()
            .ok_or("scratch private path is not UTF-8")?
            .to_owned();
        let port = korrid::start_local_server(
            NO_BROWSER_ORIGIN,
            &readable_path,
            &private_path,
            EmbeddedPlatform::default(),
        )
        .map_err(|error| error.to_string())?;
        let mut brain = Self {
            port,
            capability: Zeroizing::new(String::new()),
            wrong_capability: Zeroizing::new(korrid::generate_rpc_capability()),
            client: Client::builder()
                // A proxy from the environment would receive the bearer.
                .no_proxy()
                .build()
                .map_err(|error| format!("HTTP client: {error}"))?,
            running: true,
            readable: Some(readable),
            private: Some(private),
        };
        match korrid::local_server_capability() {
            Some(capability) => brain.capability = Zeroizing::new(capability),
            None => return Err("korrid started without a capability".into()),
        }
        Ok(brain)
    }

    /// Stop the server but keep the port, so a refresh meets a closed port.
    /// Used only by the offline proof.
    pub fn stop_server(&mut self) -> Result<(), String> {
        if self.running {
            korrid::stop_local_server().map_err(|error| error.to_string())?;
            self.running = false;
        }
        Ok(())
    }

    /// Stop korrid, then delete both scratch directories.
    pub fn shutdown(mut self) -> Result<(), String> {
        self.stop_server()?;
        for dir in [self.readable.take(), self.private.take()]
            .into_iter()
            .flatten()
        {
            dir.close()
                .map_err(|error| format!("remove scratch: {error}"))?;
        }
        Ok(())
    }

    pub fn inspect(&self, credential: Credential) -> Inspection {
        let started = Instant::now();
        let deadline = started + INSPECTION_BUDGET;
        let health = self
            .call(RpcRequest::Health(HealthRequest {}), credential, deadline)
            .and_then(|response| match response {
                RpcResponse::Health(HealthOutcome::Ok(health)) => Ok(health),
                RpcResponse::Health(HealthOutcome::Err(failure)) => Err(rejected(failure)),
                _ => Err(mismatch("system.health")),
            });
        let identity = self
            .call(
                RpcRequest::IdentityStatus(IdentityStatusRequest {}),
                credential,
                deadline,
            )
            .and_then(|response| match response {
                RpcResponse::IdentityStatus(IdentityStatusOutcome::Ok(status)) => Ok(status),
                RpcResponse::IdentityStatus(IdentityStatusOutcome::Err(failure)) => {
                    Err(rejected(failure))
                }
                _ => Err(mismatch("system.identity.status")),
            });
        let catalog = self
            .call(
                RpcRequest::CatalogSnapshot(CatalogSnapshotRequest {}),
                credential,
                deadline,
            )
            .and_then(|response| match response {
                RpcResponse::CatalogSnapshot(CatalogSnapshotOutcome::Ok(snapshot)) => Ok(snapshot),
                RpcResponse::CatalogSnapshot(CatalogSnapshotOutcome::Err(failure)) => {
                    Err(rejected(failure))
                }
                _ => Err(mismatch("app.catalog.snapshot")),
            });
        let peers = self
            .call(
                RpcRequest::PeerList(PeerListRequest {}),
                credential,
                deadline,
            )
            .and_then(|response| match response {
                RpcResponse::PeerList(PeerListOutcome::Ok(list)) => Ok(list),
                RpcResponse::PeerList(PeerListOutcome::Err(failure)) => Err(rejected(failure)),
                _ => Err(mismatch("app.peer.list")),
            });
        Inspection {
            health,
            identity,
            catalog,
            peers,
            elapsed: started.elapsed(),
        }
    }

    /// Send `system.health` without a bearer, with a wrong one, and with the
    /// real one, all inside one INSPECTION_BUDGET. Returns only HTTP status
    /// codes; `None` means no answer in time.
    pub fn probe_authorization(&self) -> AuthProbe {
        let deadline = Instant::now() + INSPECTION_BUDGET;
        let send = |bearer: Option<&str>| {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let mut request = self
                .client
                .post(self.url())
                .timeout(remaining)
                .json(&RpcRequest::Health(HealthRequest {}));
            if let Some(bearer) = bearer {
                request = request.bearer_auth(bearer);
            }
            request
                .send()
                .ok()
                .map(|response| response.status().as_u16())
        };
        AuthProbe {
            without_bearer: send(None),
            wrong_bearer: send(Some(self.wrong_capability.as_str())),
            real_bearer: send(Some(self.capability.as_str())),
        }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}/rpc", self.port)
    }

    fn call(
        &self,
        request: RpcRequest,
        credential: Credential,
        deadline: Instant,
    ) -> Result<RpcResponse, Problem> {
        // One budget for the whole refresh, not one per request.
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(Problem::Unreachable(budget_spent()));
        }
        let bearer = match credential {
            Credential::Real => self.capability.as_str(),
            Credential::Wrong => self.wrong_capability.as_str(),
        };
        let response = self
            .client
            .post(self.url())
            .timeout(remaining)
            .bearer_auth(bearer)
            .json(&request)
            .send()
            .map_err(|error| Problem::Unreachable(transport_reason(&error)))?;
        match response.status() {
            StatusCode::OK => response
                .json::<RpcResponse>()
                .map_err(|_| Problem::Unexpected("response did not match RpcResponse".into())),
            status @ (StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) => {
                Err(Problem::Unauthorized(status.as_u16()))
            }
            status => Err(Problem::Unexpected(format!("HTTP {}", status.as_u16()))),
        }
    }
}

impl Drop for Brain {
    fn drop(&mut self) {
        if self.running {
            let _ = korrid::stop_local_server();
        }
        // TempDir fields then remove the scratch directories.
    }
}

fn budget_spent() -> String {
    format!(
        "the {} s budget for one refresh ran out",
        INSPECTION_BUDGET.as_secs()
    )
}

fn rejected(failure: korrid::RpcFailure) -> Problem {
    Problem::Rejected {
        code: failure.code,
        message: failure.message,
    }
}

fn mismatch(tag: &str) -> Problem {
    Problem::Unexpected(format!("korrid answered a different request than {tag}"))
}

fn transport_reason(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        budget_spent()
    } else if error.is_connect() {
        "connection refused".into()
    } else {
        "connection failed".into()
    }
}
