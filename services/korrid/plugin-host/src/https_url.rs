//! One URL policy for published archive references and repository acquisition.
use url::Url;

pub fn parse(raw: &str) -> Result<Url, String> {
    if raw.len() > 4096 || raw.chars().any(char::is_control) {
        return Err("HTTPS URL exceeds 4096 bytes or contains control characters".into());
    }
    let url = Url::parse(raw).map_err(|e| format!("invalid HTTPS URL: {e}"))?;
    if url.scheme() != "https" || url.host_str().is_none_or(str::is_empty) {
        return Err("URL must use HTTPS with a non-empty host".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("HTTPS URL must not contain credentials".into());
    }
    if url.fragment().is_some() {
        return Err("HTTPS URL must not contain a fragment".into());
    }
    if url.as_str().len() > 4096 {
        return Err("normalized HTTPS URL exceeds 4096 bytes".into());
    }
    Ok(url)
}
