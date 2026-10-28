use crate::{client::Client, error::CliError, output::Output};
use clap::{ArgMatches, Command};
use korrid::{RpcRequest, RpcResponse};

pub trait Module: Sync {
    fn command(&self) -> Command;
    fn run(&self, args: &ArgMatches, context: &Context) -> Result<Output, CliError>;
}

pub struct Context {
    pub client: Client,
    /// Exact peer key, or None for this device. No remote transport lives here.
    pub device: Option<String>,
    pub device_flag: bool,
    pub yes: bool,
}

impl Context {
    pub fn collect(&self, requests: &[RpcRequest]) -> Result<Vec<RpcResponse>, CliError> {
        let mut responses = Vec::new();
        for request in requests {
            let response = match self.rpc(request) {
                Ok(response) => response,
                Err(error) if error.exit == 4 && responses.is_empty() => return Err(error),
                Err(error) => korrid::cli::failure_response(request, error.failure)
                    .map_err(|_| CliError::usage("Could not encode the failed section."))?,
            };
            responses.push(response);
        }
        Ok(responses)
    }

    pub fn plugin_rpc(
        &self,
        request: &crate::plugin_wire::RpcRequest,
    ) -> Result<crate::plugin_wire::RpcResponse, CliError> {
        self.client
            .plugin_request_on(self.device.as_deref(), request)
    }

    pub fn rpc(&self, request: &RpcRequest) -> Result<RpcResponse, CliError> {
        self.client.request_on(self.device.as_deref(), request)
    }
}
