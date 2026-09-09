//! HTTPS acquisition. Curl uses explicit protocol, credential, size and time
//! bounds; certificate validation is never disabled.
use crate::{catalog::Catalog, process};
use std::{path::Path, time::Duration};

pub const MAX_CATALOG_BYTES: u64 = 4 * 1024 * 1024;

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
        MAX_CATALOG_BYTES,
        Duration::from_secs(30),
    )?;
    Catalog::from_json(&std::fs::read(temporary.path()).map_err(|e| e.to_string())?)
        .map_err(|e| format!("catalog from {url}: {e}"))
}

/// Output belongs to the caller's private staging directory or private temporary
/// file. Only the host supplies the executable, CA bundle and transfer bounds.
pub fn download(
    curl: &Path,
    url: &SourceUrl,
    ca: Option<&Path>,
    output: &Path,
    max_bytes: u64,
    timeout: Duration,
) -> Result<(), String> {
    let mut args = vec![
        "--disable".into(),
        "--globoff".into(),
        "--fail".into(),
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
        "--max-filesize".into(),
        max_bytes.to_string(),
        "--connect-timeout".into(),
        "10".into(),
        "--max-time".into(),
        timeout.as_secs_f64().to_string(),
        "--output".into(),
        output.to_str().ok_or("output path is not UTF-8")?.into(),
    ];
    if let Some(ca) = ca {
        args.extend([
            "--cacert".into(),
            ca.to_str().ok_or("CA path is not UTF-8")?.into(),
        ]);
    }
    args.extend(["--url".into(), url.as_str().into()]);
    let result = process::run(curl, args, timeout + Duration::from_secs(2))?;
    if !result.success {
        return Err(format!(
            "HTTPS fetch failed: {}{}",
            result.stdout.trim(),
            result.stderr.trim()
        ));
    }
    if std::fs::metadata(output).map_err(|e| e.to_string())?.len() > max_bytes {
        return Err(format!("HTTPS response exceeds {max_bytes} bytes"));
    }
    Ok(())
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
