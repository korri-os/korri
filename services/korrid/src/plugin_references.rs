//! Named native artifacts belong to the package that declares their file key.
//! Nix closure references carry native dependencies; Korri does not maintain a
//! second package-dependency graph.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One `accounts` entry of a plugin declaration (ADR 0089): a sign-in to an
/// outside service that the plugin's own program reports through the
/// account host protocol (`account_sign_in/protocol.rs`). `program` and
/// `logo` are keys of the plugin's own `files`, as a runner's `program` is.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AccountDeclaration {
    pub title: String,
    pub program: String,
    /// Finished copy for an optional setup step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offer: Option<String>,
    /// An SVG the plugin ships: the service's mark.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
}

/// The largest logo korrid publishes, in bytes.
pub const MAX_LOGO_BYTES: u64 = 64 * 1024;

/// The plugin host's generated account units for `plugin` (ADR 0089): the
/// socket `<name>.socket` and its service template `<name>@.service`. The
/// name holds the first 128 bits of the id's SHA-256, so it stays within a
/// unit name's 64 bytes.
pub fn account_unit(plugin: &str) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(plugin.as_bytes());
    format!("korri-account-{}", hex::encode(&digest[..16]))
}

/// The socket korrid connects to for one account job of `plugin`.
pub fn account_socket(plugin: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("/run/{}.sock", account_unit(plugin)))
}

fn copy_is_valid(text: &str, max: usize) -> bool {
    !text.trim().is_empty() && text.len() <= max && !text.chars().any(char::is_control)
}

/// The declaration's `accounts`, checked: at most one entry, a short title
/// and offer, and `program` and `logo` keys in the plugin's own `files`. The
/// logo is an `.svg` file. An absent export is an empty map.
pub fn accounts(
    declaration: &serde_json::Value,
    files: &BTreeMap<String, std::path::PathBuf>,
) -> Result<BTreeMap<String, AccountDeclaration>, String> {
    let Some(value) = declaration.get("accounts") else {
        return Ok(BTreeMap::new());
    };
    let accounts: BTreeMap<String, AccountDeclaration> =
        serde_json::from_value(value.clone()).map_err(|e| format!("invalid accounts: {e}"))?;
    // One sign-in per plugin: the surface knows it by the plugin's id, and the
    // plugin host generates one account socket per plugin.
    if accounts.len() > 1 {
        return Err("a plugin may declare at most one account".into());
    }
    for (key, account) in &accounts {
        let local = |file: &str| !file.contains(['/', '@']) && files.contains_key(file);
        if key.is_empty()
            || !key
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"-_".contains(&c))
        {
            return Err(format!("account {key} needs a lowercase key"));
        }
        if !copy_is_valid(&account.title, 64) {
            return Err(format!(
                "account {key} title must be 1 to 64 bytes without control characters"
            ));
        }
        if account
            .offer
            .as_deref()
            .is_some_and(|offer| !copy_is_valid(offer, 200))
        {
            return Err(format!(
                "account {key} offer must be 1 to 200 bytes without control characters"
            ));
        }
        if !local(&account.program) {
            return Err(format!(
                "account {key} requires file {} in its own package",
                account.program
            ));
        }
        if let Some(logo) = &account.logo {
            if !local(logo) || files[logo].extension().and_then(|e| e.to_str()) != Some("svg") {
                return Err(format!(
                    "account {key} logo {logo} must be an .svg file in its own package"
                ));
            }
        }
    }
    Ok(accounts)
}

#[derive(Deserialize)]
struct RunnerReference {
    program: Option<String>,
    core: Option<String>,
    #[serde(default)]
    platforms: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct ArtifactReference {
    /// ADR 0045: optional; derived from the owner and the key when absent.
    #[serde(default)]
    unpack: Option<String>,
}

#[derive(Deserialize)]
struct References {
    #[serde(default)]
    artifacts: BTreeMap<String, ArtifactReference>,
    #[serde(default)]
    runners: BTreeMap<String, RunnerReference>,
}

pub fn validate_files(
    declaration: serde_json::Value,
    files: &BTreeMap<String, std::path::PathBuf>,
) -> Result<(), String> {
    accounts(&declaration, files)?;
    let references: References = serde_json::from_value(declaration).map_err(|e| e.to_string())?;
    for (artifact, reference) in &references.artifacts {
        if let Some(key) = &reference.unpack {
            if key.contains(['/', '@']) || !files.contains_key(key) {
                return Err(format!(
                    "artifact {artifact} requires unpack file in its own package"
                ));
            }
        }
    }
    for (key, runner) in &references.runners {
        // ADR 0054: an Android release carries no Linux runner's files.
        if !crate::runner_platform::platforms_include(
            runner.platforms.as_deref(),
            crate::runner_platform::host_platform(),
        ) {
            continue;
        }
        for file in [runner.program.as_ref(), runner.core.as_ref()]
            .into_iter()
            .flatten()
        {
            if !files.contains_key(file) {
                return Err(format!(
                    "runner {key} requires file {file} in its own package"
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn file_keys_belong_to_the_contributions_own_package() {
        let data = json!({"runners":{"ra":{"program":"ra", "core":"core"}}});
        let files = BTreeMap::from([("ra".into(), std::path::PathBuf::from("/program"))]);
        assert_eq!(
            validate_files(data.clone(), &files).unwrap_err(),
            "runner ra requires file core in its own package"
        );
        let mut files = files;
        files.insert("core".into(), "/core".into());
        validate_files(data, &files).unwrap();
    }

    #[test]
    fn an_account_names_its_own_program_and_an_svg_logo() {
        let files = BTreeMap::from([
            (
                "host".into(),
                std::path::PathBuf::from("/nix/store/x/bin/host"),
            ),
            (
                "logo".into(),
                std::path::PathBuf::from("/nix/store/x/logo.svg"),
            ),
            (
                "png".into(),
                std::path::PathBuf::from("/nix/store/x/logo.png"),
            ),
        ]);
        let declared = |account: serde_json::Value| json!({"accounts": {"tailscale": account}});
        let accounts = accounts(
            &declared(
                json!({"title": "Tailscale", "program": "host", "logo": "logo",
                "offer": "Private network: reach your devices from anywhere"}),
            ),
            &files,
        )
        .unwrap();
        assert_eq!(accounts["tailscale"].program, "host");
        assert_eq!(
            super::accounts(&json!({}), &files).unwrap(),
            BTreeMap::new()
        );
        for (account, error) in [
            (
                json!({"title": "T", "program": "missing"}),
                "account tailscale requires file missing in its own package",
            ),
            (
                json!({"title": "T", "program": "@korri:x/host"}),
                "account tailscale requires file @korri:x/host in its own package",
            ),
            (
                json!({"title": "T", "program": "host", "logo": "png"}),
                "account tailscale logo png must be an .svg file in its own package",
            ),
            (
                json!({"title": "", "program": "host"}),
                "account tailscale title must be 1 to 64 bytes without control characters",
            ),
            (
                json!({"title": "T", "program": "host", "offer": "a\nb"}),
                "account tailscale offer must be 1 to 200 bytes without control characters",
            ),
        ] {
            assert_eq!(
                validate_files(declared(account), &files).unwrap_err(),
                error
            );
        }
        assert!(validate_files(
            declared(json!({"title": "T", "program": "host", "extra": 1})),
            &files
        )
        .unwrap_err()
        .contains("unknown field"));
        assert_eq!(
            validate_files(
                json!({"accounts": {"a": {"title": "A", "program": "host"}, "b": {"title": "B", "program": "host"}}}),
                &files
            )
            .unwrap_err(),
            "a plugin may declare at most one account"
        );
    }
}
