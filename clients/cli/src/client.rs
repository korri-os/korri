use crate::error::CliError;
use korrid::{cli::ForwardRequest, remote_signer::REQUEST_TIMEOUT, RpcRequest, RpcResponse};
use std::{io::Read, path::Path, time::Duration};

const RESPONSE_LIMIT: u64 = 8 * 1024 * 1024;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

fn signer_wait(request: &RpcRequest) -> Option<Duration> {
    let requests = match request {
        // coordinator::switch signs the old owner's revocation. That owner
        // may use NIP-46 even when the replacement is a local backup.
        RpcRequest::IdentityLocalSwitch(_) => 1,
        // connect + sign the new binding + revoke the old binding are serial.
        RpcRequest::IdentityNip46Switch(_) => 3,
        RpcRequest::SettingsUpdate(request) if request.setting_id == "person-name" => 1,
        _ => return None,
    };
    // send_request's 120 s deadline starts AFTER publishing. Keep the existing
    // 30 s transport/storage allowance for each exchange as well as that wait.
    Some((REQUEST_TIMEOUT + DEFAULT_TIMEOUT) * requests)
}

pub struct Client {
    http: reqwest::blocking::Client,
}

impl Client {
    pub fn new(socket: &Path) -> Result<Self, CliError> {
        let http = reqwest::blocking::Client::builder()
            .unix_socket(socket)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .map_err(|_| {
                CliError::failure(
                    "ClientUnavailable",
                    "Could not create the local client.",
                    "Check the installed Korri version.",
                )
            })?;
        Ok(Self { http })
    }

    /// Issue a one-use receipt on this device only. Never forward selected paths.
    pub fn select_folder(&self, path: &Path) -> Result<String, CliError> {
        let response = self
            .http
            .post("http://localhost/folder-selection")
            .json(path)
            .send()
            .map_err(|_| CliError::unreachable())?;
        let status = response.status();
        let mut bytes = Vec::new();
        response
            .take(RESPONSE_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| CliError::unreachable())?;
        if bytes.len() as u64 > RESPONSE_LIMIT {
            return Err(CliError::usage(
                "Korrid returned too much folder-selection data.",
            ));
        }
        if !status.is_success() {
            let failure: korrid::RpcFailure = serde_json::from_slice(&bytes).map_err(|_| {
                CliError::failure(
                    "FolderSelectionUnavailable",
                    "Korrid cannot select this library folder.",
                    "Check the daemon version and use local access on this device.",
                )
            })?;
            return Err(CliError::rpc(failure));
        }
        serde_json::from_slice(&bytes).map_err(|_| {
            CliError::failure(
                "InvalidResponse",
                "Korrid returned an invalid folder receipt.",
                "Check that korri and korrid use the same version.",
            )
        })
    }

    pub fn request(&self, request: &RpcRequest) -> Result<RpcResponse, CliError> {
        self.send("/rpc", request, request)
    }

    pub fn request_on(
        &self,
        device: Option<&str>,
        request: &RpcRequest,
    ) -> Result<RpcResponse, CliError> {
        match device {
            None => self.request(request),
            Some(device) => self.send(
                "/forward",
                &ForwardRequest {
                    device_public_key: device.into(),
                    request: request.clone(),
                },
                request,
            ),
        }
    }

    fn send(
        &self,
        route: &str,
        body: &impl serde::Serialize,
        request: &RpcRequest,
    ) -> Result<RpcResponse, CliError> {
        // reqwest connects only to the Unix socket. No caller can supply a URL,
        // bearer capability, signing key, or remote endpoint.
        let mut pending = self
            .http
            .post(format!("http://localhost{route}"))
            .json(body);
        if let Some(wait) = signer_wait(request) {
            pending = pending.timeout(wait);
        }
        let response = pending.send().map_err(|_| CliError::unreachable())?;
        if response.status() == reqwest::StatusCode::FORBIDDEN {
            return Err(CliError::failure(
                "AuthorizationDenied",
                "This operation requires local authority on the selected device. Use SSH or local access on that device.",
                "Use SSH or local access on that device.",
            ));
        }
        if !response.status().is_success() {
            return Err(CliError::failure(
                "RpcRejected",
                "Korrid refused the request.",
                "Check that korri and korrid use the same version.",
            ));
        }
        let mut bytes = Vec::new();
        response
            .take(RESPONSE_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| CliError::unreachable())?;
        if bytes.len() as u64 > RESPONSE_LIMIT {
            return Err(CliError::failure(
                "InvalidResponse",
                "Korrid returned too much data.",
                "Check the installed Korri version.",
            ));
        }
        let invalid = || {
            CliError::failure(
                "InvalidResponse",
                "Korrid returned an invalid response.",
                "Check that korri and korrid use the same version.",
            )
        };
        let response: RpcResponse = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let request_tag = serde_json::to_value(request).map_err(|_| invalid())?;
        let response_tag = serde_json::to_value(&response).map_err(|_| invalid())?;
        if request_tag["_tag"] != response_tag["_tag"] {
            return Err(invalid());
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use korrid::*;
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
    };

    const DELAYED_SERVER: &str = r#"
"""One delayed native RPC reply over a real Unix HTTP socket."""

import http.server
import json
import socketserver
import sys
import time

socket_path, response, delay = sys.argv[1:]
# Fail before readiness if the supplied wire response is not JSON.
json.loads(response)


class Handler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        self.rfile.read(int(self.headers["Content-Length"]))
        time.sleep(float(delay))
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(response.encode())))
        self.end_headers()
        try:
            self.wfile.write(response.encode())
        except BrokenPipeError:
            pass  # The ordinary short-deadline request is expected to disconnect.

    def log_message(self, *_args):
        pass


with socketserver.UnixStreamServer(socket_path, Handler) as server:
    print("ready", flush=True)
    server.handle_request()
"#;

    struct DelayedServer {
        child: Child,
        _root: tempfile::TempDir,
        client: Client,
    }

    impl DelayedServer {
        fn start(response: &RpcResponse) -> Self {
            let root = tempfile::tempdir().unwrap();
            let socket = root.path().join("cli.sock");
            let mut child = Command::new("python3")
                .args(["-c", DELAYED_SERVER])
                .arg(&socket)
                .arg(serde_json::to_string(response).unwrap())
                .arg("0.15")
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let mut ready = String::new();
            BufReader::new(child.stdout.take().unwrap())
                .read_line(&mut ready)
                .unwrap();
            assert_eq!(ready, "ready\n");
            // Inject a shorter reqwest default, not a new production setting or
            // alternate timeout policy. The request override stays unchanged.
            let http = reqwest::blocking::Client::builder()
                .unix_socket(socket)
                .no_proxy()
                .timeout(Duration::from_millis(30))
                .build()
                .unwrap();
            Self {
                child,
                _root: root,
                client: Client { http },
            }
        }
    }

    impl Drop for DelayedServer {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn signer_failure() -> RpcFailure {
        RpcFailure {
            code: "SignerDenied".into(),
            message: "The signer refused the request.".into(),
        }
    }

    #[test]
    fn identity_switch_waits_past_the_http_default_for_the_daemons_result() {
        let requests = [
            (
                RpcRequest::IdentityLocalSwitch(IdentityLocalSwitchRequest {
                    encrypted_secret: "encrypted-backup".into(),
                    password: "password".into(),
                    data_disposition: IdentityDataDisposition::Transfer,
                    trust_loss_confirmed: true,
                }),
                RpcResponse::IdentityLocalSwitch(IdentitySwitchOutcome::Err(signer_failure())),
            ),
            (
                RpcRequest::IdentityNip46Switch(IdentityNip46SwitchRequest {
                    bunker_uri: "bunker://signer".into(),
                    data_disposition: IdentityDataDisposition::Transfer,
                    trust_loss_confirmed: true,
                }),
                RpcResponse::IdentityNip46Switch(IdentitySwitchOutcome::Err(signer_failure())),
            ),
        ];
        for ((request, expected), exchanges) in requests.into_iter().zip([1, 3]) {
            assert_eq!(
                signer_wait(&request),
                Some(
                    (korrid::remote_signer::REQUEST_TIMEOUT + Duration::from_secs(30)) * exchanges
                )
            );
            let server = DelayedServer::start(&expected);
            let actual = server.client.request(&request).unwrap();
            assert_eq!(
                serde_json::to_value(actual).unwrap(),
                serde_json::to_value(expected).unwrap()
            );
        }
    }

    #[test]
    fn person_name_waits_for_its_remote_signer_too() {
        let expected = RpcResponse::SettingsUpdate(SettingsUpdateOutcome::Err(signer_failure()));
        let server = DelayedServer::start(&expected);
        let request = RpcRequest::SettingsUpdate(SettingsUpdateRequest {
            expected_revision: "current".into(),
            setting_id: "person-name".into(),
            value: "Person".into(),
        });
        assert_eq!(
            signer_wait(&request),
            Some(REQUEST_TIMEOUT + DEFAULT_TIMEOUT)
        );
        let actual = server.client.request(&request).unwrap();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }

    #[test]
    fn ordinary_request_still_uses_the_http_default_timeout() {
        assert_eq!(DEFAULT_TIMEOUT, Duration::from_secs(30));
        assert!(signer_wait(&RpcRequest::Health(HealthRequest {})).is_none());
        assert!(
            signer_wait(&RpcRequest::SettingsUpdate(SettingsUpdateRequest {
                expected_revision: "current".into(),
                setting_id: "device-name".into(),
                value: "Device".into(),
            }))
            .is_none()
        );
        let server = DelayedServer::start(&RpcResponse::Health(HealthOutcome::Ok(Health {
            version: "test".into(),
        })));
        assert_eq!(
            server
                .client
                .request(&RpcRequest::Health(HealthRequest {}))
                .unwrap_err()
                .exit,
            4
        );
    }
}
