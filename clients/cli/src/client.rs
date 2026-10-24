use crate::error::CliError;
use korrid::{cli::ForwardRequest, RpcRequest, RpcResponse};
use std::{io::Read, path::Path, time::Duration};

const RESPONSE_LIMIT: u64 = 8 * 1024 * 1024;

pub struct Client {
    http: reqwest::blocking::Client,
}

impl Client {
    pub fn new(socket: &Path) -> Result<Self, CliError> {
        let http = reqwest::blocking::Client::builder()
            .unix_socket(socket)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
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
        let response = self
            .http
            .post(format!("http://localhost{route}"))
            .json(body)
            .send()
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
