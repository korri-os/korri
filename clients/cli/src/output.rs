use crate::error::CliError;
use korrid::{RpcFailure, RpcResponse};

pub enum Responses {
    One(Box<RpcResponse>),
    Many(Vec<RpcResponse>),
}

pub struct Output {
    pub responses: Responses,
    pub human: String,
}

impl Output {
    pub fn one(response: RpcResponse, human: String) -> Self {
        Self {
            responses: Responses::One(Box::new(response)),
            human,
        }
    }
    pub fn many(responses: Vec<RpcResponse>, human: String) -> Self {
        Self {
            responses: Responses::Many(responses),
            human,
        }
    }
    pub fn print(&self, json: bool) -> Result<u8, CliError> {
        let value = match &self.responses {
            Responses::One(response) => serde_json::to_value(response),
            Responses::Many(responses) => serde_json::to_value(responses),
        }
        .map_err(|_| {
            CliError::failure(
                "OutputUnavailable",
                "Could not encode the response.",
                "Check the installed Korri version.",
            )
        })?;
        if json {
            println!("{value}");
        } else {
            println!("{}", self.human);
        }
        let responses: &[RpcResponse] = match &self.responses {
            Responses::One(response) => std::slice::from_ref(response.as_ref()),
            Responses::Many(responses) => responses,
        };
        let failures: Vec<_> = responses.iter().filter_map(failure).collect();
        Ok(
            if failures
                .iter()
                .any(|failure| failure.code == "KorridUnreachable")
            {
                4
            } else if failures.is_empty() {
                0
            } else {
                1
            },
        )
    }
}

/// Exit status follows both outer Err outcomes and canonical failure-bearing
/// payloads. The response itself is always serialized unchanged.
pub fn failure(response: &RpcResponse) -> Option<RpcFailure> {
    use korrid::{acquisition::*, *};
    match response {
        RpcResponse::SessionStatus(SessionStatusOutcome::Ok(status)) => {
            return status.observation_failure.clone()
        }
        RpcResponse::LocalGamesList(LocalGamesListOutcome::Ok(games)) => {
            return games.failures.as_ref().and_then(|v| v.first()).cloned()
        }
        RpcResponse::AcquisitionAcquire(AcquireJobOutcome::Ok(job))
        | RpcResponse::AcquisitionAcquireStatus(AcquireJobOutcome::Ok(job))
        | RpcResponse::AcquisitionAcquireCancel(AcquireJobOutcome::Ok(job)) => {
            if let AcquireJobState::Failed { reason } = &job.state {
                return Some(reason.clone());
            }
        }
        RpcResponse::AcquisitionStatus(AcquisitionOutcome::Ok(snapshot))
        | RpcResponse::AcquisitionStart(AcquisitionOutcome::Ok(snapshot))
        | RpcResponse::AcquisitionCancel(AcquisitionOutcome::Ok(snapshot))
        | RpcResponse::AcquisitionDownload(AcquisitionOutcome::Ok(snapshot))
        | RpcResponse::AcquisitionRemove(AcquisitionOutcome::Ok(snapshot)) => {
            return snapshot
                .artifacts
                .iter()
                .find_map(|artifact| match &artifact.state {
                    ArtifactState::Failed { reason } => Some(reason.clone()),
                    _ => None,
                });
        }
        RpcResponse::WifiConnect(korrid::network::NetworkStatusOutcome::Ok(snapshot))
        | RpcResponse::WifiConnectHidden(korrid::network::NetworkStatusOutcome::Ok(snapshot)) => {
            if let korrid::network::Wifi::Available {
                attempt: korrid::network::WifiAttempt::Failed { message, .. },
                ..
            } = &snapshot.wifi
            {
                return Some(RpcFailure {
                    code: "WifiConnectFailed".into(),
                    message: message.clone(),
                });
            }
        }
        _ => {}
    }
    let value = serde_json::to_value(response).ok()?;
    (value["outcome"]["_tag"] == "Err")
        .then(|| serde_json::from_value(value["outcome"]["payload"].clone()).ok())
        .flatten()
}

pub fn text(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_control() { '�' } else { c })
        .collect()
}

pub fn unknown(section: &str, response: &RpcResponse) -> String {
    match failure(response) {
        Some(failure) => format!(
            "{section}: unknown ({})\nNext: Check korri status and try again.",
            text(&failure.message)
        ),
        None => format!("{section}: unknown"),
    }
}
