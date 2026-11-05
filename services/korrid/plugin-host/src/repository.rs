//! HTTPS acquisition. Curl uses explicit protocol and credential policy;
//! metadata transfers also have size and time bounds. Certificate validation
//! is never disabled.
use crate::{catalog::Catalog, process};
use std::{path::Path, time::Duration};

pub const MAX_CATALOG_BYTES: u64 = 4 * 1024 * 1024;
/// An archive transfer slower than this for `ARCHIVE_STALL` has stalled. The
/// plugin prepare programs use the same values.
pub const STALL_BYTES_PER_SECOND: u32 = 1024;
pub const ARCHIVE_STALL: Duration = Duration::from_secs(60);

/// Catalogs and release evidence are small metadata with fixed bounds. Package
/// archives have none: the archive SHA-256, content addressing and the bound
/// publisher key establish integrity, not size or transfer time. A stalled
/// archive transfer still fails, so it cannot hold the host lock forever.
#[derive(Clone, Copy, Debug)]
pub enum TransferBounds {
    Limited { max_bytes: u64, timeout: Duration },
    Unlimited { stall: Duration },
}

pub struct Configuration {
    pub curl: std::path::PathBuf,
    pub official: Option<SourceUrl>,
    pub ca_bundle: Option<std::path::PathBuf>,
}

impl Configuration {
    pub fn from_host(curl: &Path) -> Result<Self, String> {
        Ok(Self {
            curl: crate::package::tools(curl)?,
            official: crate::source_store::official(Path::new(crate::storage::OFFICIAL_CATALOG))?,
            ca_bundle: None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SourceUrl(String);

impl SourceUrl {
    pub fn parse(raw: &str) -> Result<Self, String> {
        crate::https_url::parse(raw).map(|url| Self(url.to_string()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SourceUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub fn fetch_catalog(curl: &Path, url: &SourceUrl, ca: Option<&Path>) -> Result<Catalog, String> {
    let temporary = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    download(
        curl,
        url,
        ca,
        temporary.path(),
        TransferBounds::Limited {
            max_bytes: MAX_CATALOG_BYTES,
            timeout: Duration::from_secs(30),
        },
    )?;
    Catalog::from_json(&std::fs::read(temporary.path()).map_err(|e| e.to_string())?)
        .map_err(|e| format!("catalog from {url}: {e}"))
}

/// Output belongs to the caller's private staging directory or private temporary
/// file. Only the host supplies the executable, CA bundle and transfer bounds.
/// Connection setup always has its 10-second limit.
pub fn download(
    curl: &Path,
    url: &SourceUrl,
    ca: Option<&Path>,
    output: &Path,
    bounds: TransferBounds,
) -> Result<(), String> {
    transfer(curl, url, ca, output, bounds, false).map(|_| ())
}

/// Only a real HTTP 404 means absent. Transport, TLS and server errors fail
/// closed; they must never make a shared path look publisher-owned.
pub(crate) fn download_optional(
    curl: &Path,
    url: &SourceUrl,
    ca: Option<&Path>,
    output: &Path,
) -> Result<bool, String> {
    transfer(
        curl,
        url,
        ca,
        output,
        TransferBounds::Limited {
            max_bytes: 1024 * 1024,
            timeout: Duration::from_secs(30),
        },
        true,
    )
}

/// Fetch many small metadata files, each optional like `download_optional`,
/// in one curl process. Curl runs the transfers concurrently and reuses its
/// connections (HTTP/2 multiplexing), instead of one TLS setup per file. Each
/// result is `Some(true)` for HTTP 200, `Some(false)` for a real 404 and `None`
/// for any other outcome; the caller retries a `None` alone so that its error
/// is reported exactly as `download_optional` reports it.
pub(crate) fn download_many_optional(
    curl: &Path,
    transfers: &[(SourceUrl, std::path::PathBuf)],
    ca: Option<&Path>,
) -> Result<Vec<Option<bool>>, String> {
    const PARALLEL: usize = 16;
    const MAX_BYTES: u64 = 1024 * 1024;
    const TIMEOUT: Duration = Duration::from_secs(30);
    if transfers.is_empty() {
        return Ok(Vec::new());
    }
    let mut args = policy_args();
    args.extend([
        "--parallel".into(),
        "--parallel-max".into(),
        PARALLEL.to_string(),
        "--write-out".into(),
        "%{response_code} %{filename_effective}\n".into(),
        "--max-filesize".into(),
        MAX_BYTES.to_string(),
        "--max-time".into(),
        TIMEOUT.as_secs_f64().to_string(),
    ]);
    if let Some(ca) = ca {
        args.extend([
            "--cacert".into(),
            ca.to_str().ok_or("CA path is not UTF-8")?.into(),
        ]);
    }
    for (url, output) in transfers {
        let output = output.to_str().ok_or("output path is not UTF-8")?;
        if output.contains('\n') {
            return Err("output path contains a newline".into());
        }
        args.extend([
            "--url".into(),
            url.as_str().into(),
            "--output".into(),
            output.into(),
        ]);
    }
    // Every slot of the parallel pool may take the full transfer time.
    let rounds = transfers.len().div_ceil(PARALLEL) as u32;
    let result = process::run(curl, args, TIMEOUT * rounds + Duration::from_secs(2))?;
    let mut statuses = std::collections::BTreeMap::new();
    for line in result.stdout.lines() {
        if let Some((status, file)) = line.split_once(' ') {
            statuses.insert(file.to_owned(), status.to_owned());
        }
    }
    transfers
        .iter()
        .map(|(_, output)| {
            let status = output.to_str().and_then(|file| statuses.get(file));
            Ok(match status.map(String::as_str) {
                Some("200")
                    if std::fs::metadata(output).is_ok_and(|meta| meta.len() <= MAX_BYTES) =>
                {
                    Some(true)
                }
                Some("404") => Some(false),
                _ => None,
            })
        })
        .collect()
}

/// Protocol, redirect and credential policy shared by every transfer.
fn policy_args() -> Vec<String> {
    vec![
        "--disable".into(),
        "--globoff".into(),
        "--silent".into(),
        "--show-error".into(),
        "--proto".into(),
        "=https".into(),
        "--proto-redir".into(),
        "=https".into(),
        "--location".into(),
        "--max-redirs".into(),
        "3".into(),
        "--disallow-username-in-url".into(),
        "--netrc-file".into(),
        "/dev/null".into(),
        "--no-netrc".into(),
        "--no-netrc-optional".into(),
        "--connect-timeout".into(),
        "10".into(),
    ]
}

fn transfer(
    curl: &Path,
    url: &SourceUrl,
    ca: Option<&Path>,
    output: &Path,
    bounds: TransferBounds,
    optional: bool,
) -> Result<bool, String> {
    let mut args = policy_args();
    args.extend([
        "--output".into(),
        output.to_str().ok_or("output path is not UTF-8")?.into(),
    ]);
    if optional {
        args.extend(["--write-out".into(), "%{http_code}".into()]);
    } else {
        args.push("--fail".into());
    }
    match bounds {
        TransferBounds::Limited { max_bytes, timeout } => args.extend([
            "--max-filesize".into(),
            max_bytes.to_string(),
            "--max-time".into(),
            timeout.as_secs_f64().to_string(),
        ]),
        TransferBounds::Unlimited { stall } => args.extend([
            "--speed-limit".into(),
            STALL_BYTES_PER_SECOND.to_string(),
            "--speed-time".into(),
            stall.as_secs().max(1).to_string(),
        ]),
    }
    if let Some(ca) = ca {
        args.extend([
            "--cacert".into(),
            ca.to_str().ok_or("CA path is not UTF-8")?.into(),
        ]);
    }
    args.extend(["--url".into(), url.as_str().into()]);
    let result = match bounds {
        TransferBounds::Limited { timeout, .. } => {
            process::run(curl, args, timeout + Duration::from_secs(2))?
        }
        TransferBounds::Unlimited { .. } => process::run_no_deadline(curl, args)?,
    };
    if !result.success {
        return Err(format!(
            "HTTPS fetch failed: {}{}",
            result.stdout.trim(),
            result.stderr.trim()
        ));
    }
    if let TransferBounds::Limited { max_bytes, .. } = bounds {
        if std::fs::metadata(output).map_err(|e| e.to_string())?.len() > max_bytes {
            return Err(format!("HTTPS response exceeds {max_bytes} bytes"));
        }
    }
    if optional {
        return match result.stdout.trim() {
            "200" => Ok(true),
            "404" => Ok(false),
            status => Err(format!("upstream returned HTTP {status}")),
        };
    }
    Ok(true)
}

pub fn verify_archive_hash(path: &Path, expected: &str) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    crate::catalog::validate_sha256_hex(expected, "archive_sha256")?;
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    if hex::encode(digest.finalize()) != expected {
        return Err("archive SHA256 does not match the selected catalog record".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_and_normalizes_source_urls() {
        assert_eq!(
            SourceUrl::parse("https://EXAMPLE.com").unwrap().as_str(),
            "https://example.com/"
        );
        for raw in [
            "http://example.com",
            "ftp://example.com",
            "https://user:pass@example.com",
            "https://example.com/#x",
            "https://",
            "https://example.com/a\0b",
        ] {
            assert!(SourceUrl::parse(raw).is_err(), "{raw}");
        }
        assert!(SourceUrl::parse(&format!("https://example.com/{}", "a".repeat(4096))).is_err());
    }
}
