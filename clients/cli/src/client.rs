use crate::error::CliError;
use korrid::{cli::ForwardRequest, remote_signer::REQUEST_TIMEOUT, RpcRequest, RpcResponse};
use std::{io::Read, path::Path, time::Duration};

const RESPONSE_LIMIT: u64 = 8 * 1024 * 1024;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

fn request_wait(request: &RpcRequest, default: Duration) -> Option<Duration> {
    if korrid::plugin_repositories::is_admin_request(request) {
        return None;
    }
    let requests = match request {
        // coordinator::switch signs the old owner's revocation. That owner
        // may use NIP-46 even when the replacement is a local backup.
        RpcRequest::IdentityLocalSwitch(_) => 1,
        // connect + sign the new binding + revoke the old binding are serial.
        RpcRequest::IdentityNip46Switch(_) => 3,
        RpcRequest::SettingsUpdate(request) if request.setting_id == "person-name" => 1,
        _ => return Some(default),
    };
    // send_request's 120 s deadline starts AFTER publishing. Keep the existing
    // 30 s transport/storage allowance for each exchange as well as that wait.
    Some((REQUEST_TIMEOUT + DEFAULT_TIMEOUT) * requests)
}

pub struct Client {
    http: reqwest::blocking::Client,
    default_timeout: Duration,
}

impl Client {
    pub fn new(socket: &Path) -> Result<Self, CliError> {
        let http = reqwest::blocking::Client::builder()
            .unix_socket(socket)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(None)
            .connect_timeout(DEFAULT_TIMEOUT)
            .build()
            .map_err(|_| {
                CliError::failure(
                    "ClientUnavailable",
                    "Could not create the local client.",
                    "Check the installed Korri version.",
                )
            })?;
        Ok(Self {
            http,
            default_timeout: DEFAULT_TIMEOUT,
        })
    }

    /// Issue a one-use receipt on this device only. Never forward selected paths.
    pub fn select_folder(&self, path: &Path) -> Result<String, CliError> {
        let response = self
            .http
            .post("http://localhost/folder-selection")
            .json(path)
            .timeout(self.default_timeout)
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
            .json(body)
            .build()
            .map_err(|_| CliError::unreachable())?;
        *pending.timeout_mut() = request_wait(request, self.default_timeout);
        let response = self
            .http
            .execute(pending)
            .map_err(|_| CliError::unreachable())?;
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
        fn start(response: &impl serde::Serialize) -> Self {
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
            // Scale the existing client default, not the request policy or a
            // production setting. The same selector receives this default.
            let http = reqwest::blocking::Client::builder()
                .unix_socket(socket)
                .no_proxy()
                .timeout(None)
                .connect_timeout(DEFAULT_TIMEOUT)
                .build()
                .unwrap();
            Self {
                child,
                _root: root,
                client: Client {
                    http,
                    default_timeout: Duration::from_millis(30),
                },
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
                request_wait(&request, DEFAULT_TIMEOUT),
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
            request_wait(&request, DEFAULT_TIMEOUT),
            Some(REQUEST_TIMEOUT + DEFAULT_TIMEOUT)
        );
        let actual = server.client.request(&request).unwrap();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }

    #[test]
    fn ordinary_request_still_uses_the_default_timeout() {
        assert_eq!(DEFAULT_TIMEOUT, Duration::from_secs(30));
        assert!(
            request_wait(&RpcRequest::Health(HealthRequest {}), DEFAULT_TIMEOUT)
                == Some(DEFAULT_TIMEOUT)
        );
        assert!(
            request_wait(
                &RpcRequest::SettingsUpdate(SettingsUpdateRequest {
                    expected_revision: "current".into(),
                    setting_id: "device-name".into(),
                    value: "Device".into(),
                }),
                DEFAULT_TIMEOUT
            ) == Some(DEFAULT_TIMEOUT)
        );
        for device in [None, Some("peer")] {
            let server = DelayedServer::start(&RpcResponse::Health(HealthOutcome::Ok(Health {
                version: "test".into(),
            })));
            assert_eq!(
                server
                    .client
                    .request_on(device, &RpcRequest::Health(HealthRequest {}))
                    .unwrap_err()
                    .exit,
                4
            );
        }
    }

    #[test]
    fn plugin_admin_waits_past_the_default_locally_and_when_forwarded() {
        for (tag, payload) in [
            ("system.plugins.repositories", serde_json::json!({})),
            (
                "system.plugins.repository.add",
                serde_json::json!({"url":"https://example.com"}),
            ),
            (
                "system.plugins.repository.remove",
                serde_json::json!({"url":"https://example.com"}),
            ),
            (
                "system.plugins.catalog",
                serde_json::json!({"url":"https://example.com"}),
            ),
            (
                "system.plugins.inspect",
                serde_json::json!({"url":"https://example.com","pluginId":"@test:a","releaseVersion":"v1"}),
            ),
            (
                "system.plugins.install",
                serde_json::json!({"url":"https://example.com","pluginId":"@test:a","releaseVersion":"v1","approval":"reviewed"}),
            ),
            (
                "system.plugins.setEnabled",
                serde_json::json!({"id":"@test:a","enabled":true}),
            ),
        ] {
            let request: RpcRequest =
                serde_json::from_value(serde_json::json!({"_tag":tag,"payload":payload})).unwrap();
            assert_eq!(request_wait(&request, DEFAULT_TIMEOUT), None);
            let expected = korrid::cli::failure_response(&request, signer_failure()).unwrap();
            for device in [None, Some("peer")] {
                let server = DelayedServer::start(&expected);
                let result = server.client.request_on(device, &request).unwrap();
                assert_eq!(
                    serde_json::to_value(result).unwrap(),
                    serde_json::to_value(&expected).unwrap()
                );
            }
        }
    }
    #[test]
    fn new_plugin_owner_and_upload_calls_wait_for_the_target_without_retry() {
        use korrid::plugin_admin::*;
        let requests = [
            RpcRequest::PluginPublishers(PluginPublishersRequest {}),
            RpcRequest::PluginTrustPublisher(PluginTrustPublisherRequest {
                namespace: "@author".into(),
                public_key: "author:public".into(),
            }),
            RpcRequest::PluginRevokePublisher(PluginRevokePublisherRequest {
                namespace: "@author".into(),
            }),
            RpcRequest::PluginArchiveBegin(PluginArchiveBeginRequest {
                sha256: "00".repeat(32),
                size: 1,
            }),
            RpcRequest::PluginArchiveChunk(PluginArchiveChunkRequest {
                upload_id: "upload".into(),
                offset: 0,
                data: "YQ==".into(),
            }),
            RpcRequest::PluginArchiveFinish(PluginArchiveUploadRequest {
                upload_id: "upload".into(),
            }),
            RpcRequest::PluginArchiveInspect(PluginArchiveUploadRequest {
                upload_id: "upload".into(),
            }),
            RpcRequest::PluginArchiveInstall(PluginArchiveUploadInstallRequest {
                upload_id: "upload".into(),
                approval: "reviewed".into(),
            }),
        ];
        for request in requests {
            let tag = serde_json::to_value(&request).unwrap()["_tag"].clone();
            let expected: RpcResponse = serde_json::from_value(serde_json::json!({ "_tag":tag, "outcome":{"_tag":"Err","payload":{"code":"PluginHostUnavailable","message":"Host unavailable"}} })).unwrap();
            for device in [None, Some("peer")] {
                let server = DelayedServer::start(&expected);
                let result = server.client.request_on(device, &request).unwrap();
                assert_eq!(
                    serde_json::to_value(result).unwrap(),
                    serde_json::to_value(&expected).unwrap()
                );
            }
        }
    }
}
