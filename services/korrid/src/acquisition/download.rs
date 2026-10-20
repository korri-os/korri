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
    /// The cap for a game with no expected hash (`MAX_GAME_BYTES`).
    game_cap: u64,
    /// Test-only DNS answers, so declared DNS names reach a loopback server.
    #[cfg(test)]
    names: std::collections::BTreeMap<String, IpAddr>,
}
impl Default for DownloadEngine {
    fn default() -> Self {
        Self {
            stall_window: Duration::from_secs(60),
            retry_delay: Duration::from_secs(2),
            addresses: policy::AddressPolicy::Public,
            game_cap: MAX_GAME_BYTES,
            #[cfg(test)]
            names: Default::default(),
        }
    }
}

/// Legacy `MAX_FALLBACK_DOWNLOAD_BYTES`: the cap for a game with no expected hash.
pub(super) const MAX_GAME_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Body text one provider turn response may carry (ADR 0014).
pub(super) const MAX_TEXT_BYTES: u64 = 4 * 1024 * 1024;

/// A GET a provider asked for, after the turn runner admitted it.
pub(super) struct TextRequest<'a> {
    pub url: Url,
    pub headers: &'a [(String, String)],
    pub timeout: Option<Duration>,
}

/// A text response: final URL, lower-cased headers (repeats joined with ", ").
pub(super) struct TextResponse {
    pub status: u16,
    pub url: String,
    pub headers: std::collections::BTreeMap<String, String>,
    pub text: String,
}

/// A downloaded game in its private staging directory. Dropping it removes
/// the file, so every failure and cancellation leaves no partial bytes.
pub(super) struct DownloadedGame {
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
    pub sha1: String,
}
impl Drop for DownloadedGame {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Prefix of every game staging file; startup removes leftovers by it.
pub(super) const STAGING_PREFIX: &str = ".acquire-";
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
            game_cap: MAX_GAME_BYTES,
            names: Default::default(),
        }
    }

    #[cfg(test)]
    pub(super) fn with_game_cap(mut self, cap: u64) -> Self {
        self.game_cap = cap;
        self
    }

    #[cfg(test)]
    pub(super) fn with_name(mut self, name: &str, ip: IpAddr) -> Self {
        self.names.insert(name.into(), ip);
        self
    }

    async fn client(&self, url: &Url) -> Result<Client, Failure> {
        policy::validate(url, self.addresses)?;
        let host = url
            .host_str()
            .ok_or(Failure::UnsafeUrl)?
            .trim_matches(['[', ']']);
        let port = url.port_or_known_default().ok_or(Failure::UnsafeUrl)?;
        #[cfg(test)]
        let named = self.names.get(host).copied();
        #[cfg(not(test))]
        let named: Option<IpAddr> = None;
        let addresses: Vec<SocketAddr> = if let Some(ip) = named.or_else(|| host.parse().ok()) {
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
        let url = Url::parse(value).map_err(|_| Failure::UnsafeUrl)?;
        self.send(url, policy::Hosts::Any, |request| {
            if offset > 0 {
                request.header(header::RANGE, format!("bytes={offset}-"))
            } else {
                request
            }
        })
        .await
        .map(|(_, response)| response)
    }

    /// One GET with manual redirects. Every hop passes the SSRF policy and,
    /// for provider traffic, the owning plugin's declared hosts.
    async fn send(
        &self,
        mut url: Url,
        hosts: policy::Hosts<'_>,
        configure: impl Fn(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
    ) -> Result<(Url, Response), Failure> {
        policy::declared(&url, hosts)?;
        for hops in 0..=5 {
            let client = self.client(&url).await?;
            let request = configure(
                client
                    .get(url.clone())
                    .header(header::ACCEPT_ENCODING, "identity"),
            );
            let response = request.send().await.map_err(|error| {
                if error.is_timeout() {
                    Failure::Stalled
                } else {
                    Failure::Network
                }
            })?;
            if !response.status().is_redirection() {
                return Ok((url, response));
            }
            let location = response
                .headers()
                .get(header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or(Failure::Redirect)?;
            url = policy::redirect(&url, location, hops, self.addresses)?;
            policy::declared(&url, hosts)?;
        }
        Err(Failure::Redirect)
    }

    /// A provider turn request: GET, body text capped at 4 MiB, decoded as
    /// UTF-8 with replacement. An HTTP error status is a response.
    pub(super) async fn fetch_text(
        &self,
        request: TextRequest<'_>,
        hosts: &[String],
    ) -> Result<TextResponse, Failure> {
        let work = async {
            let (url, mut response) = self
                .send(request.url, policy::Hosts::Only(hosts), |mut builder| {
                    for (name, value) in request.headers {
                        builder = builder.header(name, value);
                    }
                    builder
                })
                .await?;
            if response
                .content_length()
                .is_some_and(|length| length > MAX_TEXT_BYTES)
            {
                return Err(Failure::TooLarge);
            }
            let mut headers = std::collections::BTreeMap::<String, String>::new();
            for (name, value) in response.headers() {
                let value = String::from_utf8_lossy(value.as_bytes()).into_owned();
                headers
                    .entry(name.as_str().to_ascii_lowercase())
                    .and_modify(|joined| {
                        joined.push_str(", ");
                        joined.push_str(&value);
                    })
                    .or_insert(value);
            }
            let status = response.status().as_u16();
            let mut body = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(transfer)? {
                if body.len() as u64 + chunk.len() as u64 > MAX_TEXT_BYTES {
                    return Err(Failure::TooLarge);
                }
                body.extend_from_slice(&chunk);
            }
            Ok(TextResponse {
                status,
                url: url.to_string(),
                headers,
                text: String::from_utf8_lossy(&body).into_owned(),
            })
        };
        match request.timeout {
            Some(limit) => tokio::time::timeout(limit, work)
                .await
                .map_err(|_| Failure::Stalled)?,
            None => work.await,
        }
    }

    /// A resolved game with no expected hash: one attempt, streamed to a
    /// private random file, hashed while writing, capped at 2 GiB. Empty bodies
    /// and web pages are refused (legacy `rejectNonArtifactPayload`).
    pub(super) async fn fetch_game(
        &self,
        url: &str,
        hosts: &[String],
        directory: &Path,
        mut progress: impl FnMut(Progress),
    ) -> Result<DownloadedGame, Failure> {
        let url = Url::parse(url).map_err(|_| Failure::UnsafeUrl)?;
        let (_, mut response) = self.send(url, policy::Hosts::Only(hosts), |b| b).await?;
        if !response.status().is_success() || response.status() == StatusCode::PARTIAL_CONTENT {
            return Err(Failure::Network);
        }
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if content_type.starts_with("text/html") || content_type.starts_with("application/xhtml") {
            return Err(Failure::WebPage);
        }
        let total = response.content_length();
        if total.is_some_and(|total| total > self.game_cap) {
            return Err(Failure::TooLarge);
        }
        let mut name = [0u8; 12];
        rand::RngCore::fill_bytes(&mut rand::rng(), &mut name);
        let mut staged = DownloadedGame {
            path: directory.join(format!("{STAGING_PREFIX}{}", hex::encode(name))),
            bytes: 0,
            sha256: String::new(),
            sha1: String::new(),
        };
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&staged.path)?;
        let (mut sha256, mut sha1) = (Sha256::new(), sha1::Sha1::new());
        let mut lead = Vec::with_capacity(512);
        let mut done = 0u64;
        let mut samples = VecDeque::from([(Instant::now(), 0u64)]);
        let (mut window_start, mut window_bytes) = (Instant::now(), 0u64);
        progress(Progress {
            done,
            total,
            rate: None,
        });
        let mut tick = tokio::time::interval(self.stall_window.min(Duration::from_millis(200)));
        loop {
            let chunk = tokio::select! {
                chunk = response.chunk() => Some(chunk.map_err(transfer)?),
                _ = tick.tick() => None,
            };
            if let Some(chunk) = chunk {
                let Some(chunk) = chunk else {
                    break;
                };
                done += chunk.len() as u64;
                if done > self.game_cap {
                    return Err(Failure::TooLarge);
                }
                if lead.len() < 512 {
                    let take = (512 - lead.len()).min(chunk.len());
                    lead.extend_from_slice(&chunk[..take]);
                    if lead.len() == 512 && html_lead(&lead) {
                        return Err(Failure::WebPage);
                    }
                }
                output.write_all(&chunk)?;
                sha256.update(&chunk);
                sha1::Digest::update(&mut sha1, &chunk);
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
        if done == 0 {
            return Err(Failure::Empty);
        }
        if html_lead(&lead) {
            return Err(Failure::WebPage);
        }
        output.sync_all()?;
        staged.bytes = done;
        staged.sha256 = hex::encode(sha256.finalize());
        staged.sha1 = hex::encode(sha1::Digest::finalize(sha1));
        Ok(staged)
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

fn transfer(error: reqwest::Error) -> Failure {
    if error.is_timeout() {
        Failure::Stalled
    } else {
        Failure::Network
    }
}

/// Legacy `HTML_LEAD` over the first 512 bytes, after an optional BOM.
fn html_lead(lead: &[u8]) -> bool {
    let text = String::from_utf8_lossy(lead);
    let text = text
        .trim_start_matches('\u{feff}')
        .trim_start()
        .to_ascii_lowercase();
    let Some(rest) = text.strip_prefix('<') else {
        return false;
    };
    if let Some(after) = rest.strip_prefix("!doctype") {
        return after.starts_with(char::is_whitespace);
    }
    ["html", "head", "body", "script"].iter().any(|tag| {
        rest.strip_prefix(tag)
            .is_some_and(|after| after.starts_with(|c: char| c.is_whitespace() || c == '>'))
    })
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
