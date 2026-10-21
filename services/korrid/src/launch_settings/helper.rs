//! One bounded JSON-line exchange per connection to the narrow root helper.
use super::{LaunchDeviceDefaults, LaunchDeviceFacts, LaunchPercentRange};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct Client {
    pub socket: PathBuf,
}
impl Default for Client {
    fn default() -> Self {
        Self {
            socket: std::env::var_os("KORRID_LIMITS_SOCKET")
                .map(PathBuf::from)
                .unwrap_or_else(|| "/run/korri-limits/limits.sock".into()),
        }
    }
}
#[derive(Serialize)]
#[serde(tag = "_tag", content = "payload")]
enum Request<'a> {
    Describe {},
    ApplyDevice(&'a LaunchDeviceDefaults),
    ApplySession {
        unit: &'a str,
        cpu: &'a LaunchPercentRange,
        gpu: &'a LaunchPercentRange,
    },
    EndSession {
        unit: &'a str,
    },
}
#[derive(Deserialize)]
#[serde(tag = "_tag", content = "payload")]
enum Response {
    Ok(serde_json::Value),
    Err(serde_json::Value),
}
impl Client {
    fn exchange(&self, request: Request<'_>) -> Result<serde_json::Value, String> {
        let mut stream = UnixStream::connect(&self.socket).map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| e.to_string())?;
        serde_json::to_writer(&mut stream, &request).map_err(|e| e.to_string())?;
        stream.write_all(b"\n").map_err(|e| e.to_string())?;
        let mut line = String::new();
        BufReader::new(stream.take(262_145))
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        if line.len() > 262_144 || !line.ends_with('\n') {
            return Err("invalid limits helper response framing".into());
        }
        match serde_json::from_str::<Response>(&line).map_err(|e| e.to_string())? {
            Response::Ok(value) => Ok(value),
            Response::Err(error) => Err(format!("limits helper refused request: {error}")),
        }
    }
    pub fn describe(&self) -> Result<LaunchDeviceFacts, String> {
        serde_json::from_value(self.exchange(Request::Describe {})?).map_err(|e| e.to_string())
    }
    pub fn apply_device(&self, values: &LaunchDeviceDefaults) -> Result<(), String> {
        self.exchange(Request::ApplyDevice(values)).map(|_| ())
    }
    pub fn apply_session(
        &self,
        unit: &str,
        cpu: &LaunchPercentRange,
        gpu: &LaunchPercentRange,
    ) -> Result<(), String> {
        self.exchange(Request::ApplySession { unit, cpu, gpu })
            .map(|_| ())
    }
    pub fn end_session(&self, unit: &str) -> Result<(), String> {
        self.exchange(Request::EndSession { unit }).map(|_| ())
    }
}
