pub mod cascade;
mod catalog;
pub use catalog::{GameId, GamePayload, ReleaseIdentity, ReleaseKey, ReleasePayload};
mod linux_routes;
pub mod resolver;
pub mod settings;
pub mod snapshot;
pub mod storage;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{
    de::{Error as DeError, MapAccess, Visitor},
    Deserialize, Deserializer,
};
use serde_json::Value;
use thiserror::Error;

pub(crate) const DEVICE_SECTIONS: &[&str] = &[
    "host",
    "storage",
    "providers",
    "systems",
    "families",
    "runners",
    "profiles",
    "hooks",
    "locations",
];
pub(crate) const GAMES_SECTIONS: &[&str] = &["games"];
pub(crate) const RELEASES_SECTIONS: &[&str] = &["releases"];

#[derive(Debug, Error)]
pub enum ConfigSchemaError {
    #[error("{file}: {source}")]
    Yaml {
        file: &'static str,
        #[source]
        source: serde_yaml::Error,
    },
    #[error("{file}: section '{section}' is not allowed in this fixed document")]
    WrongFileSection { file: &'static str, section: String },
    #[error("{path}: {message}")]
    Invalid { path: String, message: String },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConfigSnapshot {
    pub host: Option<HostPayload>,
    pub storage: BTreeMap<String, StoragePayload>,
    pub providers: BTreeMap<String, ProviderPayload>,
    pub systems: BTreeMap<String, SystemPayload>,
    pub families: BTreeMap<String, FamilyPayload>,
    pub runners: BTreeMap<String, RunnerPayload>,
    pub profiles: BTreeMap<String, ProfilePayload>,
    pub hooks: BTreeMap<String, HookProfilePayload>,
    pub games: BTreeMap<String, GamePayload>,
    pub releases: BTreeMap<String, ReleasePayload>,
    pub locations: BTreeMap<String, Vec<Location>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDocument {
    #[serde(default, deserialize_with = "optional_non_null")]
    host: Option<HostPayload>,
    #[serde(default)]
    storage: SectionRecords<StoragePayload>,
    #[serde(default)]
    providers: SectionRecords<ProviderPayload>,
    #[serde(default)]
    systems: SectionRecords<SystemPayload>,
    #[serde(default)]
    families: SectionRecords<FamilyPayload>,
    #[serde(default)]
    runners: SectionRecords<RunnerPayload>,
    #[serde(default)]
    profiles: SectionRecords<ProfilePayload>,
    #[serde(default)]
    hooks: SectionRecords<HookProfilePayload>,
    #[serde(default)]
    games: SectionRecords<GamePayload>,
    #[serde(default)]
    releases: SectionRecords<ReleasePayload>,
    #[serde(default)]
    locations: SectionRecords<Vec<Location>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SectionRecords<T> {
    present: bool,
    records: BTreeMap<String, T>,
}

impl<T> Default for SectionRecords<T> {
    fn default() -> Self {
        Self {
            present: false,
            records: BTreeMap::new(),
        }
    }
}

impl<T> std::ops::Deref for SectionRecords<T> {
    type Target = BTreeMap<String, T>;

    fn deref(&self) -> &Self::Target {
        &self.records
    }
}

impl<'de, T> Deserialize<'de> for SectionRecords<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SectionRecordsVisitor<T>(std::marker::PhantomData<T>);

        impl<'de, T> Visitor<'de> for SectionRecordsVisitor<T>
        where
            T: Deserialize<'de>,
        {
            type Value = SectionRecords<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a section record map")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut records = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, T>()? {
                    if records.contains_key(&key) {
                        return Err(A::Error::custom(format!("duplicate record key '{key}'")));
                    }
                    records.insert(key, value);
                }
                Ok(SectionRecords {
                    present: true,
                    records,
                })
            }
        }

        deserializer.deserialize_map(SectionRecordsVisitor(std::marker::PhantomData))
    }
}

/// The minimum layer has exactly three documents. No legacy paths or opinions are read.
pub fn decode_config_documents(
    device_yaml: &str,
    games_yaml: &str,
    releases_yaml: &str,
) -> Result<ConfigSnapshot, ConfigSchemaError> {
    let device = decode_document("device.yaml", device_yaml)?;
    let games = decode_document("catalog/games.yaml", games_yaml)?;
    let releases = decode_document("catalog/releases.yaml", releases_yaml)?;
    for (file, document, allowed) in [
        ("device.yaml", &device, DEVICE_SECTIONS),
        ("catalog/games.yaml", &games, GAMES_SECTIONS),
        ("catalog/releases.yaml", &releases, RELEASES_SECTIONS),
    ] {
        for section in DEVICE_SECTIONS
            .iter()
            .chain(GAMES_SECTIONS)
            .chain(RELEASES_SECTIONS)
        {
            if !allowed.contains(section) && document.has_section(section) {
                return Err(ConfigSchemaError::WrongFileSection {
                    file,
                    section: (*section).into(),
                });
            }
        }
        validate_document_keys(file, document)?;
        validate_document_values(file, document)?;
    }
    let snapshot = ConfigSnapshot {
        host: device.host,
        storage: device.storage.records,
        providers: device.providers.records,
        systems: device.systems.records,
        families: device.families.records,
        runners: device.runners.records,
        profiles: device.profiles.records,
        hooks: device.hooks.records,
        locations: device.locations.records,
        games: games.games.records,
        releases: releases.releases.records,
    };
    catalog::validate(&snapshot)?;
    Ok(snapshot)
}

fn decode_document(file: &'static str, yaml: &str) -> Result<RawDocument, ConfigSchemaError> {
    serde_yaml::from_str::<RawDocument>(yaml)
        .map_err(|source| ConfigSchemaError::Yaml { file, source })
}

impl RawDocument {
    fn has_section(&self, section: &str) -> bool {
        match section {
            "host" => self.host.is_some(),
            "storage" => self.storage.present,
            "providers" => self.providers.present,
            "systems" => self.systems.present,
            "families" => self.families.present,
            "runners" => self.runners.present,
            "profiles" => self.profiles.present,
            "hooks" => self.hooks.present,
            "games" => self.games.present,
            "releases" => self.releases.present,
            "locations" => self.locations.present,
            _ => false,
        }
    }
}

fn validate_document_keys(
    file: &'static str,
    document: &RawDocument,
) -> Result<(), ConfigSchemaError> {
    for key in document.storage.keys() {
        validate_non_empty_key(file, "storage", key)?;
    }
    for key in document.providers.keys() {
        validate_provider_id(&format!("{file}.providers[{key}]"), key)?;
    }
    for key in document.systems.keys() {
        validate_non_empty_key(file, "systems", key)?;
    }
    for key in document.families.keys() {
        validate_provider_id(&format!("{file}.families[{key}]"), key)?;
    }
    for key in document.runners.keys() {
        validate_non_empty_key(file, "runners", key)?;
    }
    for key in document.profiles.keys() {
        validate_non_empty_key(file, "profiles", key)?;
    }
    for key in document.hooks.keys() {
        validate_non_empty_key(file, "hooks", key)?;
    }
    Ok(())
}

fn validate_non_empty_key(
    file: &'static str,
    section: &str,
    key: &str,
) -> Result<(), ConfigSchemaError> {
    if key.is_empty() {
        return Err(ConfigSchemaError::Invalid {
            path: format!("{file}.{section}"),
            message: "record keys must be non-empty".to_owned(),
        });
    }
    Ok(())
}

fn validate_document_values(
    file: &'static str,
    document: &RawDocument,
) -> Result<(), ConfigSchemaError> {
    for (id, provider) in document.providers.iter() {
        if provider.kind.is_some() {
            return Err(ConfigSchemaError::Invalid {
                path: format!("{file}.providers[{id}].kind"),
                message: "providers no longer carry kind classifications".to_owned(),
            });
        }
    }

    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupportIssue {
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("unsupported legacy-readable configuration: {0}")]
pub struct UnsupportedConfigError(UnsupportedIssues);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnsupportedIssues(pub Vec<SupportIssue>);

impl fmt::Display for UnsupportedIssues {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, issue) in self.0.iter().enumerate() {
            if index > 0 {
                write!(formatter, "; ")?;
            }
            write!(formatter, "{} ({})", issue.path, issue.message)?;
        }
        Ok(())
    }
}

pub fn classify_snapshot_support(snapshot: &ConfigSnapshot) -> Result<(), UnsupportedConfigError> {
    let mut issues = Vec::new();

    if let Some(host) = &snapshot.host {
        host.collect_support_issues("host", &mut issues);
    }
    if !snapshot.profiles.is_empty() {
        push_issue(
            &mut issues,
            "profiles",
            "profile records are not executable in this slice",
        );
    }
    if !snapshot.hooks.is_empty() {
        push_issue(
            &mut issues,
            "hooks",
            "hook profiles are not executable in this slice",
        );
    }
    for (id, release) in &snapshot.releases {
        release
            .inheritable
            .collect_support_issues(&format!("releases.{id}"), &mut issues);
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(UnsupportedConfigError(UnsupportedIssues(issues)))
    }
}

fn push_issue(issues: &mut Vec<SupportIssue>, path: &str, message: &str) {
    issues.push(SupportIssue {
        path: path.to_owned(),
        message: message.to_owned(),
    });
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HostPayload {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub launch: Option<LaunchPolicy>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub moonlight: Option<BTreeMap<String, Value>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub preferences: Option<Preferences>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub plugin: Option<ProviderValueMap>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub relays: Option<Vec<String>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub env: Option<BTreeMap<String, String>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub cwd: Option<String>,
    #[serde(default, rename = "argsAppend", deserialize_with = "optional_non_null")]
    pub args_append: Option<Vec<String>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub patches: Option<Vec<String>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub hooks: Option<HostHooksPolicy>,
}

impl HostPayload {
    fn collect_support_issues(&self, path: &str, issues: &mut Vec<SupportIssue>) {
        if self.launch.is_some() {
            push_issue(
                issues,
                &format!("{path}.launch"),
                "host launch policy is not executable in this slice",
            );
        }
        if self.moonlight.is_some() {
            push_issue(
                issues,
                &format!("{path}.moonlight"),
                "host moonlight policy is not executable in this slice",
            );
        }
        if self.preferences.is_some() {
            push_issue(
                issues,
                &format!("{path}.preferences"),
                "host preferences are not executable in this slice",
            );
        }
        if let Some(plugin) = &self.plugin {
            for (id, value) in plugin {
                if !value.is_boolean() {
                    push_issue(
                        issues,
                        &format!("{path}.plugin.{}", id.0),
                        "plugin enablement must be true or false",
                    );
                }
            }
        }
        if self.env.is_some() {
            push_issue(
                issues,
                &format!("{path}.env"),
                "host environment policy is not executable in this slice",
            );
        }
        if self.cwd.is_some() {
            push_issue(
                issues,
                &format!("{path}.cwd"),
                "host working directory policy is not executable in this slice",
            );
        }
        if self.args_append.is_some() {
            push_issue(
                issues,
                &format!("{path}.argsAppend"),
                "host argument policy is not executable in this slice",
            );
        }
        if self.patches.is_some() {
            push_issue(
                issues,
                &format!("{path}.patches"),
                "host patches are not executable in this slice",
            );
        }
        if self.hooks.is_some() {
            push_issue(
                issues,
                &format!("{path}.hooks"),
                "host hooks are not executable in this slice",
            );
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StoragePayload {
    pub root: NonEmptyString,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub path: Option<BTreeMap<String, NonEmptyString>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProviderPayload {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "optional_non_null")]
    kind: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SystemPayload {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub runner: Option<NonEmptyString>,
    #[serde(default)]
    pub families: cascade::RunnerConfigs,
    #[serde(default)]
    pub runners: cascade::RunnerConfigs,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub manufacturer: Option<String>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub aliases: Option<Vec<String>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub metadata: Option<BTreeMap<String, Value>>,
}

pub type FamilyPayload = cascade::RunnerConfig;
pub type RunnerPayload = cascade::RunnerConfig;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProfilePayload {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub runner: Option<NonEmptyString>,
    #[serde(flatten)]
    pub inheritable: InheritableLayer,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HookProfilePayload {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub before: Option<Vec<HookBeforeStep>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub after: Option<Vec<HookAfterStep>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(untagged, deny_unknown_fields)]
pub enum Location {
    File {
        storage: NonEmptyString,
        path: TargetString,
        #[serde(default, deserialize_with = "optional_non_null")]
        discovery: Option<FileTargetDiscovery>,
    },
    FileSet {
        storage: NonEmptyString,
        #[serde(default, deserialize_with = "optional_non_null")]
        root: Option<TargetString>,
        files: NonEmptyUniqueFileSetParts,
    },
    Executable {
        path: TargetString,
    },
    Url {
        value: TargetString,
    },
    ProviderRef {
        provider: ProviderIdString,
        #[serde(rename = "ref")]
        provider_ref: NonEmptyString,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FileTargetDiscovery {
    #[serde(rename = "first-seen-at")]
    pub first_seen_at: NonEmptyString,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NonEmptyUniqueFileSetParts(pub Vec<FileSetPart>);

impl<'de> Deserialize<'de> for NonEmptyUniqueFileSetParts {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let files = Vec::<FileSetPart>::deserialize(deserializer)?;
        if files.is_empty() {
            return Err(D::Error::custom(
                "file-set targets must declare at least one file",
            ));
        }
        let mut ids = BTreeSet::new();
        for file in &files {
            if !ids.insert(file.id.0.clone()) {
                return Err(D::Error::custom(format!(
                    "file-set target file id '{}' must be unique",
                    file.id.0
                )));
            }
        }
        Ok(Self(files))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FileSetPart {
    pub id: NonEmptyString,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub role: Option<NonEmptyString>,
    pub path: TargetString,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct InheritableLayer {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub launch: Option<LaunchPolicy>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub moonlight: Option<BTreeMap<String, Value>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub preferences: Option<Preferences>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub plugin: Option<ProviderValueMap>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub env: Option<BTreeMap<String, String>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub cwd: Option<String>,
    #[serde(default, rename = "argsAppend", deserialize_with = "optional_non_null")]
    pub args_append: Option<Vec<String>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub patches: Option<Vec<String>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub hooks: Option<HooksPolicy>,
}

impl InheritableLayer {
    fn collect_support_issues(&self, path: &str, issues: &mut Vec<SupportIssue>) {
        if self.launch.is_some() {
            push_issue(
                issues,
                &format!("{path}.launch"),
                "inheritable launch policy is not executable in this slice",
            );
        }
        if self.moonlight.is_some() {
            push_issue(
                issues,
                &format!("{path}.moonlight"),
                "moonlight policy is not executable in this slice",
            );
        }
        if self.preferences.is_some() {
            push_issue(
                issues,
                &format!("{path}.preferences"),
                "preferences are not executable in this slice",
            );
        }
        if self.plugin.is_some() {
            push_issue(
                issues,
                &format!("{path}.plugin"),
                "plugin policy is not executable in this slice",
            );
        }
        if self.env.is_some() {
            push_issue(
                issues,
                &format!("{path}.env"),
                "environment policy is not executable in this slice",
            );
        }
        if self.cwd.is_some() {
            push_issue(
                issues,
                &format!("{path}.cwd"),
                "working-directory policy is not executable in this slice",
            );
        }
        if self.args_append.is_some() {
            push_issue(
                issues,
                &format!("{path}.argsAppend"),
                "argument policy is not executable in this slice",
            );
        }
        if self.patches.is_some() {
            push_issue(
                issues,
                &format!("{path}.patches"),
                "patches are not executable in this slice",
            );
        }
        if self.hooks.is_some() {
            push_issue(
                issues,
                &format!("{path}.hooks"),
                "hooks are not executable in this slice",
            );
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LaunchPolicy {
    #[serde(default, rename = "with", deserialize_with = "optional_non_null")]
    pub with_policy: Option<ProviderValueMap>,
}

pub type ProviderValueMap = BTreeMap<ProviderIdString, Value>;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub launch: Option<LaunchPreferences>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LaunchPreferences {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub video: Option<LaunchVideoPreferences>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub audio: Option<LaunchAudioPreferences>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LaunchVideoPreferences {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub fullscreen: Option<bool>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub resolution: Option<LaunchResolutionPreferences>,
    #[serde(
        default,
        rename = "aspect-ratio",
        deserialize_with = "optional_non_null"
    )]
    pub aspect_ratio: Option<NonEmptyString>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LaunchResolutionPreferences {
    pub width: PositiveInt,
    pub height: PositiveInt,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LaunchAudioPreferences {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub volume: Option<Volume>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HooksPolicy {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub before: Option<Vec<HookBeforeStep>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub after: Option<Vec<HookAfterStep>>,
    #[serde(default, rename = "use", deserialize_with = "optional_non_null")]
    pub use_profiles: Option<Vec<NonEmptyString>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HostHooksPolicy {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub before: Option<Vec<HookBeforeStep>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub after: Option<Vec<HookAfterStep>>,
    #[serde(default, rename = "use", deserialize_with = "optional_non_null")]
    pub use_profiles: Option<Vec<NonEmptyString>>,
    #[serde(
        default,
        rename = "trust-removable",
        deserialize_with = "optional_non_null"
    )]
    pub trust_removable: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HookBeforeStep {
    pub run: NonEmptyString,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub name: Option<NonEmptyString>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub timeout: Option<PositiveInt>,
    #[serde(default, rename = "on-failure", deserialize_with = "optional_non_null")]
    pub on_failure: Option<HookBeforeFailurePolicy>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum HookBeforeFailurePolicy {
    Abort,
    Warn,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HookAfterStep {
    pub run: NonEmptyString,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub name: Option<NonEmptyString>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub timeout: Option<PositiveInt>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NonEmptyString(pub String);

impl<'de> Deserialize<'de> for NonEmptyString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_yaml::Value::deserialize(deserializer)?;
        let serde_yaml::Value::String(value) = value else {
            return Err(D::Error::custom("value must be a non-empty string"));
        };
        if value.is_empty() {
            return Err(D::Error::custom("value must be non-empty"));
        }
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TargetString(pub String);

impl<'de> Deserialize<'de> for TargetString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = NonEmptyString::deserialize(deserializer)?.0;
        if value.starts_with('/') {
            return Err(D::Error::custom(
                "release target URI/string values must not be absolute paths",
            ));
        }
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct AbsolutePathString(pub String);

impl<'de> Deserialize<'de> for AbsolutePathString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = NonEmptyString::deserialize(deserializer)?.0;
        if !value.starts_with('/') {
            return Err(D::Error::custom("runtime path must be absolute"));
        }
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ProviderIdString(pub String);

impl<'de> Deserialize<'de> for ProviderIdString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        validate_provider_id("provider", &value).map_err(D::Error::custom)?;
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SafeRefValue(pub String);

impl<'de> Deserialize<'de> for SafeRefValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = NonEmptyString::deserialize(deserializer)?.0;
        if value.len() > 2048 {
            return Err(D::Error::custom(
                "provider ref values must be 2048 characters or fewer",
            ));
        }
        if value.chars().any(|character| {
            let code_point = character as u32;
            code_point < 32 || code_point == 127
        }) {
            return Err(D::Error::custom(
                "provider ref values must not contain control characters",
            ));
        }
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ArtifactIdString(pub String);

impl<'de> Deserialize<'de> for ArtifactIdString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let valid = value.strip_prefix("sha256:").is_some_and(|digest| {
            digest.len() == 64
                && digest
                    .chars()
                    .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        });
        if !valid {
            return Err(D::Error::custom(
                "artifact ids must be sha256:<64 lowercase hex characters>",
            ));
        }
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PositiveInt(pub u64);

impl<'de> Deserialize<'de> for PositiveInt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        if value == 0 {
            return Err(D::Error::custom("positive integer required"));
        }
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Volume(pub f64);

impl<'de> Deserialize<'de> for Volume {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = f64::deserialize(deserializer)?;
        if !value.is_finite() || !(0.0..=100.0).contains(&value) {
            return Err(D::Error::custom(
                "preferences.launch.audio.volume must be in [0, 100]",
            ));
        }
        Ok(Self(value))
    }
}

impl Eq for Volume {}

fn optional_non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let value = serde_yaml::Value::deserialize(deserializer)?;
    if matches!(value, serde_yaml::Value::Null) {
        return Err(D::Error::custom(
            "explicit null is not valid; omit the field instead",
        ));
    }
    T::deserialize(value).map(Some).map_err(D::Error::custom)
}

fn validate_provider_id(path: &str, value: &str) -> Result<(), ConfigSchemaError> {
    let Some(without_at) = value.strip_prefix('@') else {
        return Err(invalid(
            path,
            "provider ids must be plugin-owned ids like '@korri:example'",
        ));
    };
    let Some((namespace, name)) = without_at.split_once(':') else {
        return Err(invalid(
            path,
            "provider ids must be plugin-owned ids like '@korri:example'",
        ));
    };
    if namespace.contains(':')
        || !valid_provider_segment(namespace)
        || !valid_provider_segment(name)
    {
        return Err(invalid(
            path,
            "provider ids must be plugin-owned ids like '@korri:example'",
        ));
    }
    Ok(())
}

fn valid_provider_segment(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | '_' | '-')
        })
}

fn invalid(path: &str, message: &str) -> ConfigSchemaError {
    ConfigSchemaError::Invalid {
        path: path.to_owned(),
        message: message.to_owned(),
    }
}

#[cfg(test)]
#[path = "../../tests/fixtures/readable.rs"]
pub(crate) mod test_fixtures;
