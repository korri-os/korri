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

const CONFIG_SECTIONS: &[&str] = &[
    "host",
    "storage",
    "providers",
    "provider-links",
    "systems",
    "launchers",
    "runtimes",
    "profiles",
    "hooks",
];
const LIBRARY_SECTIONS: &[&str] = &["collections", "users", "library"];

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
    pub provider_links: BTreeMap<String, ProviderLinkPayload>,
    pub systems: BTreeMap<String, SystemPayload>,
    pub launchers: BTreeMap<String, AppPayload>,
    pub runtimes: BTreeMap<String, RuntimePayload>,
    pub profiles: BTreeMap<String, ProfilePayload>,
    pub hooks: BTreeMap<String, HookProfilePayload>,
    pub collections: BTreeMap<String, CollectionPayload>,
    pub users: BTreeMap<String, UserPayload>,
    pub library: BTreeMap<String, LibraryItemPayload>,
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
    #[serde(default, rename = "provider-links")]
    provider_links: SectionRecords<ProviderLinkPayload>,
    #[serde(default)]
    systems: SectionRecords<SystemPayload>,
    #[serde(default)]
    launchers: SectionRecords<AppPayload>,
    #[serde(default)]
    runtimes: SectionRecords<RuntimePayload>,
    #[serde(default)]
    profiles: SectionRecords<ProfilePayload>,
    #[serde(default)]
    hooks: SectionRecords<HookProfilePayload>,
    #[serde(default)]
    collections: SectionRecords<CollectionPayload>,
    #[serde(default)]
    users: SectionRecords<UserPayload>,
    #[serde(default)]
    library: SectionRecords<LibraryItemPayload>,
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

pub fn decode_config_pair(
    config_yaml: &str,
    library_yaml: &str,
) -> Result<ConfigSnapshot, ConfigSchemaError> {
    let config = decode_document("config.yaml", config_yaml)?;
    let library = decode_document("library.yaml", library_yaml)?;

    reject_wrong_file_sections("config.yaml", &config, LIBRARY_SECTIONS)?;
    reject_wrong_file_sections("library.yaml", &library, CONFIG_SECTIONS)?;

    validate_document_keys("config.yaml", &config)?;
    validate_document_keys("library.yaml", &library)?;
    validate_document_values("config.yaml", &config)?;
    validate_document_values("library.yaml", &library)?;

    Ok(ConfigSnapshot {
        host: config.host,
        storage: config.storage.records,
        providers: config.providers.records,
        provider_links: config.provider_links.records,
        systems: config.systems.records,
        launchers: config.launchers.records,
        runtimes: config.runtimes.records,
        profiles: config.profiles.records,
        hooks: config.hooks.records,
        collections: library.collections.records,
        users: library.users.records,
        library: library.library.records,
    })
}

fn decode_document(file: &'static str, yaml: &str) -> Result<RawDocument, ConfigSchemaError> {
    serde_yaml::from_str::<RawDocument>(yaml)
        .map_err(|source| ConfigSchemaError::Yaml { file, source })
}

fn reject_wrong_file_sections(
    file: &'static str,
    document: &RawDocument,
    forbidden: &[&str],
) -> Result<(), ConfigSchemaError> {
    for section in forbidden {
        if document.has_section(section) {
            return Err(ConfigSchemaError::WrongFileSection {
                file,
                section: (*section).to_owned(),
            });
        }
    }
    Ok(())
}

impl RawDocument {
    fn has_section(&self, section: &str) -> bool {
        match section {
            "host" => self.host.is_some(),
            "storage" => self.storage.present,
            "providers" => self.providers.present,
            "provider-links" => self.provider_links.present,
            "systems" => self.systems.present,
            "launchers" => self.launchers.present,
            "runtimes" => self.runtimes.present,
            "profiles" => self.profiles.present,
            "hooks" => self.hooks.present,
            "collections" => self.collections.present,
            "users" => self.users.present,
            "library" => self.library.present,
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
    for key in document.provider_links.keys() {
        validate_non_empty_key(file, "provider-links", key)?;
    }
    for key in document.systems.keys() {
        validate_non_empty_key(file, "systems", key)?;
    }
    for key in document.launchers.keys() {
        validate_non_empty_key(file, "launchers", key)?;
    }
    for key in document.runtimes.keys() {
        validate_non_empty_key(file, "runtimes", key)?;
    }
    for key in document.profiles.keys() {
        validate_non_empty_key(file, "profiles", key)?;
    }
    for key in document.hooks.keys() {
        validate_non_empty_key(file, "hooks", key)?;
    }
    for key in document.collections.keys() {
        validate_non_empty_key(file, "collections", key)?;
    }
    for key in document.users.keys() {
        validate_non_empty_key(file, "users", key)?;
    }
    for key in document.library.keys() {
        validate_local_playable_id(&format!("{file}.library[{key}]"), key)?;
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

    for (id, item) in document.library.iter() {
        if item.source.is_some() {
            return Err(ConfigSchemaError::Invalid {
                path: format!("{file}.library[{id}].source"),
                message: "library item source was removed; use provider-links[]".to_owned(),
            });
        }
        for release in &item.releases.0 {
            let release_path = format!("{file}.library[{id}].releases[{}]", release.id.0);
            if release.source.is_some() {
                return Err(ConfigSchemaError::Invalid {
                    path: format!("{release_path}.source"),
                    message: "release.source was removed; use provider-links[]".to_owned(),
                });
            }
            if release.app.is_some() {
                return Err(ConfigSchemaError::Invalid {
                    path: format!("{release_path}.app"),
                    message: "release.app was removed; use release.launch".to_owned(),
                });
            }
            if release.runtime.is_some() {
                return Err(ConfigSchemaError::Invalid {
                    path: format!("{release_path}.runtime"),
                    message: "release.runtime was removed; use release.launch.runtime".to_owned(),
                });
            }
            if release.apps.is_some() {
                return Err(ConfigSchemaError::Invalid {
                    path: format!("{release_path}.apps"),
                    message: "release.apps was removed; use release.launch".to_owned(),
                });
            }
            if release.identity.is_some() && !matches!(release.target, Some(Target::File { .. })) {
                return Err(ConfigSchemaError::Invalid {
                    path: format!("{release_path}.identity"),
                    message: "release identity hash tags may only be declared for file targets"
                        .to_owned(),
                });
            }
            if let Some(launch) = &release.launch {
                if launch.use_launcher.is_some() && launch.plugin.is_some() {
                    return Err(ConfigSchemaError::Invalid {
                        path: format!("{release_path}.launch"),
                        message: "release.launch cannot specify both use and plugin".to_owned(),
                    });
                }
            }
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
    if !snapshot.provider_links.is_empty() {
        push_issue(
            &mut issues,
            "provider-links",
            "provider links are not executable in this slice",
        );
    }
    if !snapshot.runtimes.is_empty() {
        push_issue(
            &mut issues,
            "runtimes",
            "runtime records are not executable in this slice",
        );
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
    if !snapshot.collections.is_empty() {
        push_issue(
            &mut issues,
            "collections",
            "collection records are not executable in this slice",
        );
    }
    if !snapshot.users.is_empty() {
        push_issue(
            &mut issues,
            "users",
            "user records are not executable in this slice",
        );
    }

    for (id, item) in &snapshot.library {
        item.collect_support_issues(&format!("library.{id}"), &mut issues);
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProviderLinkPayload {
    pub provider: ProviderIdString,
    pub playable: PlayableIdString,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub release: Option<NonEmptyString>,
    pub refs: NonEmptyVec<ProviderRef>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProviderRef {
    pub kind: ProviderRefKind,
    pub value: SafeRefValue,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub scope: Option<ProviderRefScope>,
    #[serde(default, rename = "targetPart", deserialize_with = "optional_non_null")]
    pub target_part: Option<NonEmptyString>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderRefKind {
    Url,
    ProviderItemId,
    ExternalId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ProviderRefScope {
    Playable,
    Release,
    TargetPart,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AppPayload {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub settings: Option<BTreeMap<String, Value>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub plugin: Option<ProviderIdString>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub command: Option<NonEmptyString>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub runtime: Option<NonEmptyString>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub args: Option<Vec<String>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub systems: Option<Vec<String>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub policy: Option<AppPolicy>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub inherit: Option<bool>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub presets: Option<BTreeMap<String, Value>>,
    #[serde(flatten)]
    pub inheritable: InheritableLayer,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AppPolicy {
    #[serde(
        default,
        rename = "allowedCommands",
        deserialize_with = "optional_non_null"
    )]
    pub allowed_commands: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RuntimePayload {
    pub kind: RuntimeKind,
    pub path: AbsolutePathString,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub title: Option<NonEmptyString>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub tool: Option<NonEmptyString>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub app: Option<NonEmptyString>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub supports: Option<RuntimeSupportsPayload>,
    #[serde(flatten)]
    pub inheritable: InheritableLayer,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeKind {
    LibretroCore,
    Tool,
    Emulator,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeSupportsPayload {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub systems: Option<Vec<NonEmptyString>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProfilePayload {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub app: Option<NonEmptyString>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub runtime: Option<NonEmptyString>,
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
#[serde(deny_unknown_fields)]
pub struct CollectionPayload {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub description: Option<String>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub items: Option<Vec<PlayableIdString>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub inherit: Option<bool>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub presets: Option<BTreeMap<String, Value>>,
    #[serde(default, rename = "byLauncher", deserialize_with = "optional_non_null")]
    pub by_launcher: Option<BTreeMap<String, InheritableLayer>>,
    #[serde(flatten)]
    pub inheritable: CollectionInheritableLayer,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UserPayload {
    #[serde(
        default,
        rename = "displayName",
        deserialize_with = "optional_non_null"
    )]
    pub display_name: Option<String>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub favorites: Option<Vec<PlayableIdString>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub hidden: Option<Vec<PlayableIdString>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub launch: Option<LaunchBlock>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub launcher: Option<String>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub inherit: Option<bool>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub presets: Option<BTreeMap<String, Value>>,
    #[serde(default, rename = "byLauncher", deserialize_with = "optional_non_null")]
    pub by_launcher: Option<BTreeMap<String, InheritableLayer>>,
    #[serde(flatten)]
    pub inheritable: InheritableLayer,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LibraryItemPayload {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "optional_non_null")]
    source: Option<Value>,
    #[serde(default, rename = "version-of", deserialize_with = "optional_non_null")]
    pub version_of: Option<PlayableIdString>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub relation: Option<NonEmptyString>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub collections: Option<Vec<NonEmptyString>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub display: Option<BTreeMap<String, Value>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub metadata: Option<BTreeMap<String, Value>>,
    #[serde(default, rename = "userData", deserialize_with = "optional_non_null")]
    pub user_data: Option<BTreeMap<String, Value>>,
    #[serde(default, deserialize_with = "optional_contains_non_null")]
    pub contains: Option<ContainsMap>,
    pub releases: ReleaseList,
    #[serde(flatten)]
    pub inheritable: InheritableLayer,
}

impl LibraryItemPayload {
    fn collect_support_issues(&self, path: &str, issues: &mut Vec<SupportIssue>) {
        if self.version_of.is_some() {
            push_issue(
                issues,
                &format!("{path}.version-of"),
                "version relationships are not executable in this slice",
            );
        }
        if self.relation.is_some() {
            push_issue(
                issues,
                &format!("{path}.relation"),
                "playable relations are not executable in this slice",
            );
        }
        if self.collections.is_some() {
            push_issue(
                issues,
                &format!("{path}.collections"),
                "collection membership is not executable in this slice",
            );
        }
        if self.display.is_some() {
            push_issue(
                issues,
                &format!("{path}.display"),
                "display metadata is not executable in this slice",
            );
        }
        if self.metadata.is_some() {
            push_issue(
                issues,
                &format!("{path}.metadata"),
                "metadata is not executable in this slice",
            );
        }
        if self.user_data.is_some() {
            push_issue(
                issues,
                &format!("{path}.userData"),
                "user data is not executable in this slice",
            );
        }
        if self.contains.is_some() {
            push_issue(
                issues,
                &format!("{path}.contains"),
                "contained playables are not executable in this slice",
            );
        }
        self.inheritable.collect_support_issues(path, issues);
        for (index, release) in self.releases.0.iter().enumerate() {
            release.collect_support_issues(&format!("{path}.releases[{index}]"), issues);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseList(pub Vec<LibraryReleasePayload>);

impl<'de> Deserialize<'de> for ReleaseList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let releases = Vec::<LibraryReleasePayload>::deserialize(deserializer)?;
        if releases.is_empty() {
            return Err(D::Error::custom(
                "library item must declare at least one release",
            ));
        }
        let mut ids = BTreeSet::new();
        for release in &releases {
            if !ids.insert(release.id.0.clone()) {
                return Err(D::Error::custom(format!(
                    "library item release id '{}' must be unique",
                    release.id.0
                )));
            }
        }
        Ok(Self(releases))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LibraryReleasePayload {
    pub id: LocalPlayableIdString,
    #[serde(default, deserialize_with = "optional_non_null")]
    source: Option<Value>,
    pub system: NonEmptyString,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub target: Option<Target>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub identity: Option<ReleaseIdentityTag>,
    #[serde(default, deserialize_with = "optional_non_null")]
    app: Option<Value>,
    #[serde(default, deserialize_with = "optional_non_null")]
    runtime: Option<Value>,
    #[serde(default, deserialize_with = "optional_non_null")]
    apps: Option<Value>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub display: Option<BTreeMap<String, Value>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub launch: Option<ReleaseLaunch>,
    #[serde(flatten)]
    pub inheritable: InheritableLayer,
}

impl LibraryReleasePayload {
    fn collect_support_issues(&self, path: &str, issues: &mut Vec<SupportIssue>) {
        if self.display.is_some() {
            push_issue(
                issues,
                &format!("{path}.display"),
                "release display metadata is not executable in this slice",
            );
        }
        self.inheritable.collect_support_issues(path, issues);
        if let Some(launch) = &self.launch {
            launch.collect_support_issues(&format!("{path}.launch"), issues);
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Target {
    #[serde(rename = "file")]
    File {
        storage: NonEmptyString,
        path: TargetString,
        #[serde(default, deserialize_with = "optional_non_null")]
        discovery: Option<FileTargetDiscovery>,
    },
    #[serde(rename = "file-set")]
    FileSet {
        storage: NonEmptyString,
        #[serde(default, deserialize_with = "optional_non_null")]
        root: Option<TargetString>,
        files: NonEmptyUniqueFileSetParts,
    },
    #[serde(rename = "executable")]
    Executable { path: TargetString },
    #[serde(rename = "url")]
    Url { value: TargetString },
    #[serde(rename = "provider-ref")]
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReleaseIdentityTag {
    pub kind: ReleaseIdentityKind,
    pub value: ArtifactIdString,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum ReleaseIdentityKind {
    Hash,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReleaseLaunch {
    #[serde(default, rename = "use", deserialize_with = "optional_non_null")]
    pub use_launcher: Option<NonEmptyString>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub plugin: Option<ProviderIdString>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub runtime: Option<NonEmptyString>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub input: Option<LaunchInput>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub settings: Option<BTreeMap<String, Value>>,
    #[serde(default, rename = "with", deserialize_with = "optional_non_null")]
    pub with_policy: Option<ProviderValueMap>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub env: Option<BTreeMap<String, String>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub cwd: Option<String>,
    #[serde(default, rename = "argsAppend", deserialize_with = "optional_non_null")]
    pub args_append: Option<Vec<String>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub overrides: Option<LaunchOverrides>,
}

impl ReleaseLaunch {
    fn collect_support_issues(&self, path: &str, issues: &mut Vec<SupportIssue>) {
        if self.plugin.is_some() {
            push_issue(
                issues,
                &format!("{path}.plugin"),
                "release launcher plugin selection is not executable in this slice",
            );
        }
        if self.input.is_some() {
            push_issue(
                issues,
                &format!("{path}.input"),
                "release input selection is not executable in this slice",
            );
        }
        if self.settings.is_some() {
            push_issue(
                issues,
                &format!("{path}.settings"),
                "release launch settings are not executable in this slice",
            );
        }
        if self.with_policy.is_some() {
            push_issue(
                issues,
                &format!("{path}.with"),
                "release launch companion policy is not executable in this slice",
            );
        }
        if self.env.is_some() {
            push_issue(
                issues,
                &format!("{path}.env"),
                "release launch environment is not executable in this slice",
            );
        }
        if self.cwd.is_some() {
            push_issue(
                issues,
                &format!("{path}.cwd"),
                "release launch working directory is not executable in this slice",
            );
        }
        if self.args_append.is_some() {
            push_issue(
                issues,
                &format!("{path}.argsAppend"),
                "release launch arguments are not executable in this slice",
            );
        }
        if self.overrides.is_some() {
            push_issue(
                issues,
                &format!("{path}.overrides"),
                "release launch overrides are not executable in this slice",
            );
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LaunchInput {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub part: Option<NonEmptyString>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub roles: Option<Vec<NonEmptyString>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LaunchOverrides {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub args: Option<ArgOverrides>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub config: Option<ConfigOverrides>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ArgOverrides {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub prepend: Option<Vec<String>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub append: Option<Vec<String>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub replace: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConfigOverrides {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub prepend: Option<String>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub append: Option<String>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub replace: Option<String>,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct CollectionInheritableLayer {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub launch: Option<LaunchPolicy>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub env: Option<BTreeMap<String, String>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub cwd: Option<String>,
    #[serde(default, rename = "argsAppend", deserialize_with = "optional_non_null")]
    pub args_append: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LaunchPolicy {
    #[serde(default, rename = "with", deserialize_with = "optional_non_null")]
    pub with_policy: Option<ProviderValueMap>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LaunchBlock {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub app: Option<String>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub module: Option<String>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub settings: Option<BTreeMap<String, Value>>,
    #[serde(default, rename = "with", deserialize_with = "optional_non_null")]
    pub with_policy: Option<ProviderValueMap>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub args: Option<Vec<String>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub env: Option<BTreeMap<String, String>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub cwd: Option<String>,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainsMap(pub BTreeMap<LocalPlayableIdString, ContainedPlayablePayload>);

impl<'de> Deserialize<'de> for ContainsMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let map =
            BTreeMap::<LocalPlayableIdString, ContainedPlayablePayload>::deserialize(deserializer)?;
        if map.is_empty() {
            return Err(D::Error::custom(
                "contains must name at least one local playable",
            ));
        }
        Ok(Self(map))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ContainedPlayablePayload {
    #[serde(default, deserialize_with = "optional_non_null")]
    pub title: Option<String>,
    #[serde(default, rename = "version-of", deserialize_with = "optional_non_null")]
    pub version_of: Option<PlayableIdString>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub relation: Option<NonEmptyString>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub collections: Option<Vec<NonEmptyString>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub display: Option<BTreeMap<String, Value>>,
    #[serde(default, deserialize_with = "optional_non_null")]
    pub metadata: Option<BTreeMap<String, Value>>,
    #[serde(default, rename = "userData", deserialize_with = "optional_non_null")]
    pub user_data: Option<BTreeMap<String, Value>>,
    #[serde(flatten)]
    pub inheritable: InheritableLayer,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NonEmptyString(pub String);

impl<'de> Deserialize<'de> for NonEmptyString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
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
pub struct LocalPlayableIdString(pub String);

impl<'de> Deserialize<'de> for LocalPlayableIdString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        validate_local_playable_id("local playable", &value).map_err(D::Error::custom)?;
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PlayableIdString(pub String);

impl<'de> Deserialize<'de> for PlayableIdString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        validate_playable_id("playable", &value).map_err(D::Error::custom)?;
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NonEmptyVec<T>(pub Vec<T>);

impl<'de, T> Deserialize<'de> for NonEmptyVec<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let values = Vec::<T>::deserialize(deserializer)?;
        if values.is_empty() {
            return Err(D::Error::custom("list must not be empty"));
        }
        Ok(Self(values))
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

fn optional_contains_non_null<'de, D>(deserializer: D) -> Result<Option<ContainsMap>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_yaml::Value::deserialize(deserializer)?;
    if matches!(value, serde_yaml::Value::Null) {
        return Err(D::Error::custom(
            "explicit null is not valid; omit the field instead",
        ));
    }
    ContainsMap::deserialize(value)
        .map(Some)
        .map_err(D::Error::custom)
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

fn validate_local_playable_id(path: &str, value: &str) -> Result<(), ConfigSchemaError> {
    if valid_playable_segment(value) {
        Ok(())
    } else {
        Err(invalid(
            path,
            "local playable ids must be lowercase path segments without slashes",
        ))
    }
}

fn validate_playable_id(path: &str, value: &str) -> Result<(), ConfigSchemaError> {
    let parts: Vec<_> = value.split('/').collect();
    let valid = match parts.as_slice() {
        [item] => valid_playable_segment(item),
        [item, contained] => valid_playable_segment(item) && valid_playable_segment(contained),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(invalid(
            path,
            "playable ids must be '<item-id>' or '<item-id>/<contained-id>'",
        ))
    }
}

fn valid_playable_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_lowercase() || first.is_ascii_digit())
        && segment != "."
        && segment != ".."
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
