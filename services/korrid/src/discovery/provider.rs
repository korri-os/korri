//! Closed first-party manifest discovery (ADR 0011). Plugins receive text, not I/O.
use std::{
    collections::BTreeSet,
    ffi::CString,
    fs::{self, File, OpenOptions},
    io::Read,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::OpenOptionsExt,
    },
    path::Path,
};

use serde::{Deserialize, Serialize};
use typeshare::typeshare;

use super::{
    identify::AccountRoot,
    scanner::{DiscoveryDiagnostic, DiscoveryDiagnosticCode},
};
use crate::{
    plugin::PluginRegistry,
    script::{self, source::SourceSnapshot},
};

use crate::plugin_discovery::STEAM_INSTALLED_APPS;
const MAX_FILES: usize = 4096;
const MAX_FILE_BYTES: usize = 64 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 10_000;

#[typeshare]
#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseDiscoveryInput {
    pub files: Vec<ReleaseDiscoveryFile>,
}

#[typeshare]
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseDiscoveryFile {
    pub relative_path: String,
    pub text: String,
}

/// Legacy provider-ref-release observation, restricted to this first real consumer.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderRefReleaseObservation {
    pub kind: ProviderReleaseKind,
    pub confidence: ProviderReleaseConfidence,
    pub source: ProviderReleaseSource,
    pub target: ProviderReleaseTarget,
    pub release: ProviderRelease,
    pub launch: ProviderReleaseLaunch,
}

#[typeshare(serialized_as = "ProviderReleaseKindWire")]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProviderReleaseKind {
    #[serde(rename = "provider-ref-release")]
    ProviderRefRelease,
}

#[typeshare(serialized_as = "ProviderReleaseConfidenceWire")]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProviderReleaseConfidence {
    #[serde(rename = "high")]
    High,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderReleaseSource {
    pub relative_path: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderReleaseTarget {
    pub provider: String,
    #[serde(rename = "ref")]
    pub provider_ref: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderRelease {
    pub id: String,
    pub title: String,
    pub system: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderReleaseLaunch {
    #[serde(rename = "use")]
    pub runner: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProviderScan {
    pub id: String,
    pub observations: Vec<ProviderRefReleaseObservation>,
}

fn diagnostic(diagnostics: &mut Vec<DiscoveryDiagnostic>, path: Option<&str>, message: &str) {
    eprintln!("korrid discovery: {message}");
    if diagnostics.len() < 1000 {
        diagnostics.push(DiscoveryDiagnostic {
            code: DiscoveryDiagnosticCode::EntryUnavailable,
            storage_id: Some(STEAM_INSTALLED_APPS.into()),
            path: path.map(str::to_owned),
            message: message.into(),
        });
    }
}

pub fn scan(
    accounts: &[AccountRoot],
    registry: &PluginRegistry,
    diagnostics: &mut Vec<DiscoveryDiagnostic>,
) -> Vec<ProviderScan> {
    let mut result = ProviderScan {
        id: STEAM_INSTALLED_APPS.into(),
        ..Default::default()
    };
    if !registry
        .discovery_providers()
        .any(|p| p.id == STEAM_INSTALLED_APPS)
    {
        // A disabled/removed plugin withdraws only its discovery-owned locations.
        return vec![result];
    }
    let source = (|| {
        let package = registry
            .installed_package(STEAM_INSTALLED_APPS)
            .map_err(|e| e.to_string())?;
        SourceSnapshot::package_plugin(&package.package, &package.entry, &package.sources)
    })();
    let source = match source {
        Ok(source) => source,
        Err(error) => {
            diagnostic(diagnostics, None, &error);
            return vec![];
        }
    };
    let mut count = 0;
    let mut seen = BTreeSet::new();
    for account in accounts {
        let directory = match manifest_directory(&account.root) {
            Ok(directory) => directory,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                diagnostic(
                    diagnostics,
                    None,
                    &format!("manifest directory unavailable: {error}"),
                );
                // Like an unreadable paired source, do not withdraw prior locations.
                return vec![];
            }
        };
        let names = match manifest_names(&directory) {
            Ok(names) => names,
            Err(error) => {
                diagnostic(diagnostics, None, &error);
                return vec![];
            }
        };
        for name in names {
            if count == MAX_FILES {
                diagnostic(
                    diagnostics,
                    None,
                    "4096 manifest file cap reached; remaining files skipped",
                );
                break;
            }
            count += 1;
            let relative_path = format!("steamapps/{name}");
            let text = match read_manifest(&directory, &name) {
                Ok(text) => text,
                Err(error) => {
                    diagnostic(diagnostics, Some(&relative_path), &error);
                    continue;
                }
            };
            // One manifest per call keeps even maximally escaped text below the
            // existing 512 KiB callback input limit. No readText callback is exposed.
            match discover(
                &source,
                ReleaseDiscoveryFile {
                    relative_path: relative_path.clone(),
                    text,
                },
            ) {
                Ok(observations) => {
                    for observation in observations {
                        if seen.insert(observation.target.provider_ref.clone()) {
                            result.observations.push(observation);
                        }
                    }
                }
                Err(error) => {
                    diagnostic(diagnostics, Some(&relative_path), &error);
                    // Bad plugin output is a failed scan, never evidence of uninstall.
                    return vec![];
                }
            }
        }
    }
    vec![result]
}

fn discover(
    source: &SourceSnapshot,
    file: ReleaseDiscoveryFile,
) -> Result<Vec<ProviderRefReleaseObservation>, String> {
    let expected_path = file.relative_path.clone();
    let expected_ref = manifest_appid(
        expected_path
            .strip_prefix("steamapps/")
            .ok_or("invalid manifest path")?,
    )
    .ok_or("invalid manifest path")?
    .to_owned();
    let input = serde_json::to_string(&ReleaseDiscoveryInput { files: vec![file] })
        .map_err(|e| e.to_string())?;
    let output = script::call_plugin_operation_snapshot(source, "discovery.discover", &input)
        .map_err(|e| e.to_string())?;
    let observations: Vec<ProviderRefReleaseObservation> =
        serde_json::from_str(&output).map_err(|e| format!("invalid provider observations: {e}"))?;
    if observations.len() > 1
        || observations.iter().any(|o| {
            o.target.provider != "@korri:steam"
                || o.target.provider_ref != expected_ref
                || !o.target.provider_ref.bytes().all(|b| b.is_ascii_digit())
                || o.release.title.trim().is_empty()
                || o.release.id != "steam"
                || o.release.system != "steam"
                || o.launch.runner != "@korri:steam/steam"
                || o.source.relative_path != expected_path
        })
    {
        return Err("invalid Steam provider release target, title, source or release".into());
    }
    Ok(observations)
}

fn manifest_appid(name: &str) -> Option<&str> {
    let id = name.strip_prefix("appmanifest_")?.strip_suffix(".acf")?;
    (!id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())).then_some(id)
}

fn child(parent: &File, name: &str, directory: bool) -> std::io::Result<File> {
    let name = CString::new(name).map_err(std::io::Error::other)?;
    let flags = libc::O_RDONLY
        | libc::O_CLOEXEC
        | libc::O_NOFOLLOW
        | libc::O_NONBLOCK
        | if directory { libc::O_DIRECTORY } else { 0 };
    let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn manifest_directory(account: &Path) -> std::io::Result<File> {
    let account = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(account)?;
    let (_, glob) = crate::plugin_discovery::LOCATIONS
        .iter()
        .find(|(id, _)| *id == STEAM_INSTALLED_APPS)
        .expect("admitted first-party location");
    let (relative, _) = glob.rsplit_once('/').expect("first-party manifest glob");
    let mut directory = account;
    for part in relative.split('/') {
        directory = child(&directory, part, true)?;
    }
    Ok(directory)
}

fn manifest_names(directory: &File) -> Result<Vec<String>, String> {
    let entries = fs::read_dir(format!("/proc/self/fd/{}", directory.as_raw_fd()))
        .map_err(|e| e.to_string())?;
    let mut names = Vec::new();
    for (index, entry) in entries.enumerate() {
        if index == MAX_DIRECTORY_ENTRIES {
            return Err("manifest directory exceeds 10000 entries".into());
        }
        let entry = entry.map_err(|e| e.to_string())?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if manifest_appid(&name).is_some() {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

fn read_manifest(directory: &File, name: &str) -> Result<String, String> {
    let mut file = child(directory, name, false).map_err(|e| e.to_string())?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES as u64 {
        return Err("manifest must be a regular file of at most 65536 bytes; skipped".into());
    }
    let mut bytes = vec![0; meta.len() as usize];
    file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() != meta.len() {
        return Err("manifest changed while reading; skipped".into());
    }
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
