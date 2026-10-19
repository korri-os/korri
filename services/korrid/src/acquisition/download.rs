//! Async reqwest transport. Bytes are streamed to disk and hashed, never buffered
//! as a complete artifact. Every hop pins a checked DNS answer; proxies are off.
use super::{policy, Failure};
use crate::plugin::ArtifactFile;
use reqwest::{header, redirect::Policy, Client, Response, StatusCode};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::{IpAddr, SocketAddr},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use url::Url;

#[derive(Clone)]
pub(super) struct DownloadEngine {
    stall_window: Duration,
    retry_delay: Duration,
    addresses: policy::AddressPolicy,
}
impl Default for DownloadEngine {
    fn default() -> Self {
        Self {
            stall_window: Duration::from_secs(60),
            retry_delay: Duration::from_secs(2),
            addresses: policy::AddressPolicy::Public,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Progress {
    pub done: u64,
    pub total: Option<u64>,
    pub rate: Option<f64>,
}

impl DownloadEngine {
    #[cfg(test)]
    pub(super) fn for_test(stall_window: Duration) -> Self {
        Self {
            stall_window,
            retry_delay: Duration::from_millis(1),
            addresses: policy::AddressPolicy::AllowLoopback,
        }
    }

    async fn client(&self, url: &Url) -> Result<Client, Failure> {
        policy::validate(url, self.addresses)?;
        let host = url
            .host_str()
            .ok_or(Failure::UnsafeUrl)?
            .trim_matches(['[', ']']);
        let port = url.port_or_known_default().ok_or(Failure::UnsafeUrl)?;
        let addresses: Vec<SocketAddr> = if let Ok(ip) = host.parse::<IpAddr>() {
            vec![SocketAddr::new(ip, port)]
        } else {
            tokio::time::timeout(
                Duration::from_secs(30),
                tokio::net::lookup_host((host, port)),
            )
            .await
            .map_err(|_| Failure::Network)?
            .map_err(|_| Failure::Network)?
            .collect()
        };
        if addresses.is_empty() {
            return Err(Failure::Network);
        }
        for address in &addresses {
            if !self.addresses.allows(address.ip()) {
                return Err(Failure::UnsafeUrl);
            }
        }
        Client::builder()
            .redirect(Policy::none())
            .no_proxy()
            .connect_timeout(Duration::from_secs(30))
            .read_timeout(self.stall_window)
            .resolve_to_addrs(host, &addresses)
            .build()
            .map_err(|_| Failure::Network)
    }

    async fn response(&self, value: &str, offset: u64) -> Result<Response, Failure> {
        let mut url = Url::parse(value).map_err(|_| Failure::UnsafeUrl)?;
        for hops in 0..=5 {
            let client = self.client(&url).await?;
            let mut request = client
                .get(url.clone())
                .header(header::ACCEPT_ENCODING, "identity");
            if offset > 0 {
                request = request.header(header::RANGE, format!("bytes={offset}-"));
            }
            let response = request.send().await.map_err(|error| {
                if error.is_timeout() {
                    Failure::Stalled
                } else {
                    Failure::Network
                }
            })?;
            if !response.status().is_redirection() {
                return Ok(response);
            }
            let location = response
                .headers()
                .get(header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or(Failure::Redirect)?;
            url = policy::redirect(&url, location, hops, self.addresses)?;
        }
        Err(Failure::Redirect)
    }

    pub(super) async fn fetch(
        &self,
        file: &ArtifactFile,
        directory: &Path,
        mut progress: impl FnMut(Progress),
    ) -> Result<PathBuf, Failure> {
        let hex = file.sha256.strip_prefix("sha256:").ok_or(Failure::Hash)?;
        let verified = directory.join(hex);
        let part = directory.join(format!("{hex}.part"));
        if verified.exists() {
            let (hash, bytes) = hash_file_async(&verified).await?;
            if hex::encode(hash.finalize()) == hex {
                progress(Progress {
                    done: bytes,
                    total: Some(bytes),
                    rate: None,
                });
                return Ok(verified);
            }
            fs::remove_file(&verified)?;
        }
        let mut last = Failure::Network;
        for attempt in 0..=5 {
            if attempt > 0 {
                tokio::time::sleep(self.retry_delay).await;
            }
            match self.attempt(file, &part, &mut progress).await {
                Ok(()) => {
                    fs::rename(&part, &verified)?;
                    return Ok(verified);
                }
                Err(
                    error @ (Failure::UnsafeUrl
                    | Failure::UnsafePath
                    | Failure::Storage
                    | Failure::NoSpace
                    | Failure::Redirect),
                ) => return Err(error),
                Err(error) => last = error,
            }
        }
        Err(last)
    }

    async fn attempt(
        &self,
        file: &ArtifactFile,
        part: &Path,
        progress: &mut impl FnMut(Progress),
    ) -> Result<(), Failure> {
        let (mut hash, mut done) = if part.exists() {
            hash_file_async(part).await?
        } else {
            (Sha256::new(), 0)
        };
        let expected = file.sha256.trim_start_matches("sha256:");
        if done > 0 && hex::encode(hash.clone().finalize()) == expected {
            return Ok(());
        }
        let mut response = self.response(&file.url, done).await?;
        if response.status() == StatusCode::RANGE_NOT_SATISFIABLE {
            if part.exists() {
                fs::remove_file(part)?;
            }
            return Err(Failure::Network);
        }
        let total = if response.status() == StatusCode::PARTIAL_CONTENT {
            let value = response
                .headers()
                .get(header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .ok_or(Failure::Network)?;
            content_range(value, done)?
        } else if response.status() == StatusCode::OK {
            // A server may ignore Range. Never append its complete response.
            done = 0;
            hash = Sha256::new();
            response.content_length()
        } else {
            return Err(Failure::Network);
        };
        let mut output = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(done == 0)
            .append(done > 0)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(part)?;
        let mut samples = VecDeque::from([(Instant::now(), done)]);
        let (mut window_start, mut window_bytes) = (Instant::now(), done);
        progress(Progress {
            done,
            total,
            rate: None,
        });
        let mut tick = tokio::time::interval(self.stall_window.min(Duration::from_millis(200)));
        loop {
            let chunk = tokio::select! {
                chunk = response.chunk() => Some(chunk.map_err(|error| if error.is_timeout() { Failure::Stalled } else { Failure::Network })?),
                _ = tick.tick() => None,
            };
            if let Some(chunk) = chunk {
                let Some(chunk) = chunk else {
                    break;
                };
                output.write_all(&chunk)?;
                hash.update(&chunk);
                done += chunk.len() as u64;
            }
            let now = Instant::now();
            if now.duration_since(window_start) >= self.stall_window {
                if (done - window_bytes) as f64 / now.duration_since(window_start).as_secs_f64()
                    < 1024.0
                {
                    return Err(Failure::Stalled);
                }
                window_start = now;
                window_bytes = done;
            }
            samples.push_back((now, done));
            while samples.len() > 1 && now.duration_since(samples[0].0) > Duration::from_secs(5) {
                samples.pop_front();
            }
            let (start, bytes) = samples[0];
            let elapsed = now.duration_since(start).as_secs_f64();
            progress(Progress {
                done,
                total,
                rate: (elapsed > 0.0).then_some((done - bytes) as f64 / elapsed.max(f64::EPSILON)),
            });
        }
        output.sync_all()?;
        if hex::encode(hash.finalize()) != expected {
            drop(output);
            fs::remove_file(part)?;
            return Err(Failure::Hash);
        }
        Ok(())
    }
}

fn content_range(value: &str, offset: u64) -> Result<Option<u64>, Failure> {
    let (range, total) = value
        .strip_prefix("bytes ")
        .and_then(|v| v.split_once('/'))
        .ok_or(Failure::Network)?;
    let (start, end) = range.split_once('-').ok_or(Failure::Network)?;
    let start = start.parse::<u64>().map_err(|_| Failure::Network)?;
    let end = end.parse::<u64>().map_err(|_| Failure::Network)?;
    let total = if total == "*" {
        None
    } else {
        Some(total.parse::<u64>().map_err(|_| Failure::Network)?)
    };
    if start != offset || end < start || total.is_some_and(|total| end >= total) {
        return Err(Failure::Network);
    }
    Ok(total)
}

fn hash_file(path: &Path) -> Result<(Sha256, u64), Failure> {
    let mut file = File::options()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(Failure::Storage);
    }
    let mut hash = Sha256::new();
    let mut bytes = 0;
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
        bytes += count as u64;
    }
    Ok((hash, bytes))
}

async fn hash_file_async(path: &Path) -> Result<(Sha256, u64), Failure> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || hash_file(&path))
        .await
        .map_err(|_| Failure::Storage)?
}
