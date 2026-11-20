//! Named native artifacts belong to the package that declares their file key.
//! Nix closure references carry native dependencies; Korri does not maintain a
//! second package-dependency graph.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The `account` export of a plugin declaration (ADR 0089, "Account sign-in
/// declaration"): a sign-in to an outside service that the plugin's own
/// program reports through the account host protocol
/// (`account_sign_in/protocol.rs`). `program` and `logo` are keys of the
/// plugin's own `files`, as a runner's `program` is.
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
/// The longest title and offer, in bytes.
const MAX_TITLE_BYTES: usize = 64;
const MAX_OFFER_BYTES: usize = 200;

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

/// The service an account program runs beside: the plugin's first declared
/// native unit, as its systemd unit name. One rule for korrid (which waits
/// for it) and the plugin host (which generates the units, and also checks
/// that the unit is a service). `None` when the first unit is a socket or
/// there is none.
pub fn account_service(services: &[String]) -> Option<String> {
    let first = services.first()?;
    if first.ends_with(".socket") {
        return None;
    }
    Some(if first.ends_with(".service") {
        first.clone()
    } else {
        format!("{first}.service")
    })
}

fn copy_is_valid(text: &str, max: usize) -> bool {
    !text.trim().is_empty() && text.len() <= max && !text.chars().any(char::is_control)
}

/// The declaration's `account`, checked: a short title and offer, and
/// `program` and `logo` keys in the plugin's own `files`. The logo is an
/// `.svg` file. One per plugin: the surface knows a sign-in by the plugin's
/// id, and the plugin host generates one account socket per plugin.
pub fn account(
    declaration: &serde_json::Value,
    files: &BTreeMap<String, std::path::PathBuf>,
) -> Result<Option<AccountDeclaration>, String> {
    let Some(value) = declaration.get("account") else {
        return Ok(None);
    };
    let account: AccountDeclaration =
        serde_json::from_value(value.clone()).map_err(|e| format!("invalid account: {e}"))?;
    let local = |file: &str| !file.contains(['/', '@']) && files.contains_key(file);
    if !copy_is_valid(&account.title, MAX_TITLE_BYTES) {
        return Err(format!(
            "account title must be 1 to {MAX_TITLE_BYTES} bytes without control characters"
        ));
    }
    if account
        .offer
        .as_deref()
        .is_some_and(|offer| !copy_is_valid(offer, MAX_OFFER_BYTES))
    {
        return Err(format!(
            "account offer must be 1 to {MAX_OFFER_BYTES} bytes without control characters"
        ));
    }
    if !local(&account.program) {
        return Err(format!(
            "account requires file {} in its own package",
            account.program
        ));
    }
    if let Some(logo) = &account.logo {
        if !local(logo) || files[logo].extension().and_then(|e| e.to_str()) != Some("svg") {
            return Err(format!(
                "account logo {logo} must be an .svg file in its own package"
            ));
        }
    }
    Ok(Some(account))
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
    account(&declaration, files)?;
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
            ("host".into(), "/nix/store/x/bin/host".into()),
            ("logo".into(), "/nix/store/x/logo.svg".into()),
            ("png".into(), "/nix/store/x/logo.png".into()),
        ]);
        let declared = |account: serde_json::Value| json!({ "account": account });
        let account = account(
            &declared(
                json!({"title": "Tailscale", "program": "host", "logo": "logo",
                "offer": "Private network: reach your devices from anywhere"}),
            ),
            &files,
        )
        .unwrap()
        .unwrap();
        assert_eq!(account.program, "host");
        assert_eq!(super::account(&json!({}), &files).unwrap(), None);
        for (account, error) in [
            (
                json!({"title": "T", "program": "missing"}),
                "account requires file missing in its own package",
            ),
            (
                json!({"title": "T", "program": "@korri:x/host"}),
                "account requires file @korri:x/host in its own package",
            ),
            (
                json!({"title": "T", "program": "host", "logo": "png"}),
                "account logo png must be an .svg file in its own package",
            ),
            (
                json!({"title": "", "program": "host"}),
                "account title must be 1 to 64 bytes without control characters",
            ),
            (
                json!({"title": "T", "program": "host", "offer": "a\nb"}),
                "account offer must be 1 to 200 bytes without control characters",
            ),
            (
                json!({"title": "T", "program": "host", "offer": "x".repeat(201)}),
                "account offer must be 1 to 200 bytes without control characters",
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
        // The keyed form is gone: a map is not one account.
        assert!(validate_files(
            declared(json!({"tailscale": {"title": "T", "program": "host"}})),
            &files
        )
        .is_err());
    }

    #[test]
    fn the_account_service_is_the_first_declared_unit_when_it_is_a_service() {
        let names = |n: &[&str]| n.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            account_service(&names(&["tailscaled", "other"])).as_deref(),
            Some("tailscaled.service")
        );
        assert_eq!(
            account_service(&names(&["a.service"])).as_deref(),
            Some("a.service")
        );
        assert_eq!(account_service(&names(&["a.socket", "a"])), None);
        assert_eq!(account_service(&[]), None);
    }
}
