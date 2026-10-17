//! Declaration-only plugins and their device-local registry.
//!
//! Plugins declare systems and their content rules, groups, games, providers,
//! families, runners, transports and contextual controls. Plugins perform no
//! effects; this module evaluates, validates and combines their declarations.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer};
use thiserror::Error;

use crate::script;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderRecord {
    pub id: String,
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderContribution {
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    title: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SystemRecord {
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub aliases: Option<Vec<String>>,
    /// What one game of this system looks like (ADR 0001, contract section 1).
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub content: Option<ContentRecord>,
}

/// `systems.<id>.content`: extensions, ordered layouts, evidence and header
/// rules (ADR 0001, ADR 0005, contract section 1).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ContentRecord {
    pub extensions: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub layout: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub evidence: Option<EvidenceRecord>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub header: Option<Vec<HeaderRule>>,
}

/// Content evidence ranked below a declared hash (brief 2026-10-02 ranking).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRecord {
    #[serde(default)]
    pub bytes: Vec<BytesRule>,
    #[serde(default)]
    pub names: Vec<String>,
}

/// Bytes at an offset in the first 64 KiB, lowercase hex.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BytesRule {
    pub offset: u64,
    pub hex: String,
}

/// A clrmamepro-style header rule (research note, ADR 0005). Exactly one of
/// `bytes` or `remainder` is set.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HeaderRule {
    pub size: u64,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub bytes: Option<BytesRule>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub remainder: Option<RemainderRule>,
}

/// The rule applies when the file size modulo `divisor` equals `value`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RemainderRule {
    pub divisor: u64,
    pub value: u64,
}

/// Every enabled plugin's rules for one system id, combined (contract
/// section 1: unions; layouts, title and header precedence by plugin id).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContentRules {
    pub system: String,
    pub title: Option<String>,
    pub extensions: Vec<String>,
    /// Ordered; the default `**/{title}.{ext}` when no plugin gives one.
    pub layouts: Vec<String>,
    pub evidence: EvidenceRecord,
    pub header: Vec<HeaderRule>,
}

/// `groups.<id>`: related systems one source can hold (ADR 0001). Only
/// plugins declare groups; groups combine by id.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GroupRecord {
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub title: Option<String>,
    pub systems: Vec<String>,
}

/// `games.<key>`: a declared game (ADR 0003). Its provider identity is
/// `{ provider: <plugin id>, ref: <key> }` (ADR 0002).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclaredGame {
    pub provider: String,
    pub key: String,
    pub title: String,
    pub releases: BTreeMap<String, DeclaredRelease>,
}

/// One declared release: hashes (`sha256:` or `sha1:`) and an optional
/// prepared file relative to an account root.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeclaredRelease {
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub system: Option<String>,
    pub hashes: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub file: Option<String>,
}

/// A declared release found by one of its hashes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeclaredReleaseRef<'a> {
    pub game: &'a DeclaredGame,
    pub release_key: &'a str,
    pub release: &'a DeclaredRelease,
}

/// What a source's `system` key names (ADR 0001).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PairingTarget<'a> {
    System(&'a ContentRules),
    Group {
        group: &'a GroupRecord,
        members: Vec<&'a ContentRules>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FamilyRecord {
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub title: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModifierRecord {
    pub id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RunnerRecord {
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub family: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub command: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub systems: Option<Vec<String>>,
    /// Any one of these whole-file library identities admits the native runner.
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub releases: Option<Vec<crate::config::ArtifactIdString>>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub program: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub core: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TransportRecord {
    pub id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "kebab-case")]
pub enum SessionControlOwnerKind {
    Runner,
    Transport,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SessionControlOwner {
    pub kind: SessionControlOwnerKind,
    pub id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SessionControlOption {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum SessionControlDeclarationInteraction {
    Command,
    Toggle {
        #[serde(
            default,
            rename = "trueLabel",
            deserialize_with = "deserialize_optional_non_null"
        )]
        true_label: Option<String>,
        #[serde(
            default,
            rename = "falseLabel",
            deserialize_with = "deserialize_optional_non_null"
        )]
        false_label: Option<String>,
    },
    Choice {
        options: Vec<SessionControlOption>,
    },
    Range {
        min: f64,
        max: f64,
        step: f64,
    },
}

/// Closed first-party effects that korrid has a concrete host executor for.
/// A surface submits only the control ID; it never supplies this value.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum SessionControlEffect {
    #[serde(rename = "@korri:retroarch/open-menu")]
    RetroarchOpenMenu,
    #[serde(rename = "@korri:retroarch/quit")]
    RetroarchQuit,
}

impl SessionControlEffect {
    fn family(self) -> &'static str {
        match self {
            Self::RetroarchOpenMenu | Self::RetroarchQuit => "@korri:retroarch",
        }
    }

    pub(crate) fn retroarch_command(self) -> crate::host::retroarch_control::RetroarchCommand {
        match self {
            Self::RetroarchOpenMenu => crate::host::retroarch_control::RetroarchCommand::OpenMenu,
            Self::RetroarchQuit => crate::host::retroarch_control::RetroarchCommand::Quit,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionControlRecord {
    #[serde(skip)]
    pub plugin_id: String,
    #[serde(skip)]
    pub local_id: String,
    pub id: String,
    pub owner: SessionControlOwner,
    pub label: String,
    pub order: u16,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub description: Option<String>,
    pub interaction: SessionControlDeclarationInteraction,
    pub effect: SessionControlEffect,
    #[serde(default)]
    pub destructive: bool,
    #[serde(default)]
    pub dismiss_on_success: bool,
}

#[derive(Clone, Debug)]
pub struct Plugin {
    id: String,
    config: Option<BTreeMap<String, serde_json::Value>>,
    title: String,
    description: Option<String>,
    providers: BTreeMap<String, ProviderRecord>,
    systems: BTreeMap<String, SystemRecord>,
    families: BTreeMap<String, FamilyRecord>,
    runners: BTreeMap<String, RunnerRecord>,
    modifiers: BTreeMap<String, ModifierRecord>,
    transports: BTreeMap<String, TransportRecord>,
    session_controls: BTreeMap<String, SessionControlRecord>,
    groups: BTreeMap<String, GroupRecord>,
    declared_games: BTreeMap<String, DeclaredGame>,
}

impl Plugin {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn config(&self) -> Option<&BTreeMap<String, serde_json::Value>> {
        self.config.as_ref()
    }
}

#[derive(Clone, Debug)]
pub struct PluginRegistry {
    plugins: BTreeMap<String, Plugin>,
    installed: BTreeMap<String, crate::plugin_installation::EnabledPackage>,
    enabled_plugin_ids: BTreeSet<String>,
    registered_provider_ids: BTreeSet<String>,
    registered_system_ids: BTreeSet<String>,
    registered_family_ids: BTreeSet<String>,
    registered_runner_ids: BTreeSet<String>,
    registered_modifier_ids: BTreeSet<String>,
    registered_transport_ids: BTreeSet<String>,
    registered_session_controls: BTreeMap<String, SessionControlRecord>,
    providers: BTreeMap<String, ProviderRecord>,
    systems: BTreeMap<String, SystemRecord>,
    families: BTreeMap<String, FamilyRecord>,
    runners: BTreeMap<String, RunnerRecord>,
    modifiers: BTreeMap<String, ModifierRecord>,
    transports: BTreeMap<String, TransportRecord>,
    session_controls: BTreeMap<String, SessionControlRecord>,
    content_rules: BTreeMap<String, ContentRules>,
    groups: BTreeMap<String, GroupRecord>,
    declared_games: BTreeMap<String, DeclaredGame>,
}

impl PluginRegistry {
    pub fn new(
        plugins: Vec<Plugin>,
        enabled_plugin_ids: impl IntoIterator<Item = String>,
    ) -> Result<Self, PluginError> {
        let requested_enabled: BTreeSet<String> = enabled_plugin_ids.into_iter().collect();
        let mut by_id = BTreeMap::new();

        for plugin in plugins {
            let plugin_id = plugin.id.clone();
            if by_id.insert(plugin_id.clone(), plugin).is_some() {
                return Err(PluginError::DuplicatePluginId(plugin_id));
            }
        }

        let enabled_plugin_ids = requested_enabled;
        let mut registered_provider_ids = BTreeSet::new();
        let mut registered_system_ids = BTreeSet::new();
        let mut registered_family_ids = BTreeSet::new();
        let mut registered_runner_ids = BTreeSet::new();
        let mut registered_modifier_ids = BTreeSet::new();
        let mut registered_transport_ids = BTreeSet::new();
        let mut registered_session_controls = BTreeMap::new();
        let mut providers = BTreeMap::new();
        let mut systems = BTreeMap::new();
        let mut families = BTreeMap::new();
        let mut runners = BTreeMap::new();
        let mut modifiers = BTreeMap::new();
        let mut transports = BTreeMap::new();
        let mut session_controls = BTreeMap::new();
        let mut content_rules: BTreeMap<String, ContentRules> = BTreeMap::new();
        let mut groups: BTreeMap<String, GroupRecord> = BTreeMap::new();
        let mut declared_games = BTreeMap::new();

        for plugin in by_id.values() {
            registered_provider_ids
                .extend(plugin.providers.values().map(|record| record.id.clone()));
            registered_system_ids.extend(plugin.systems.values().map(|record| record.id.clone()));
            registered_family_ids.extend(plugin.families.values().map(|record| record.id.clone()));
            registered_runner_ids.extend(plugin.runners.values().map(|record| record.id.clone()));
            registered_modifier_ids
                .extend(plugin.modifiers.values().map(|record| record.id.clone()));
            registered_transport_ids
                .extend(plugin.transports.values().map(|record| record.id.clone()));
            for record in plugin.session_controls.values() {
                insert_unique(
                    &mut registered_session_controls,
                    record.id.clone(),
                    record.clone(),
                )?;
            }
        }

        for plugin_id in &enabled_plugin_ids {
            let plugin = by_id
                .get(plugin_id)
                .ok_or_else(|| PluginError::UnknownEnabledPlugin(plugin_id.clone()))?;

            for (record_id, record) in &plugin.providers {
                insert_unique(&mut providers, record_id.clone(), record.clone())?;
            }
            for (local_id, record) in &plugin.systems {
                insert_unique(
                    &mut systems,
                    plugin_record_id(plugin_id, local_id),
                    record.clone(),
                )?;
            }
            for record in plugin.families.values() {
                insert_unique(&mut families, record.id.clone(), record.clone())?;
            }
            for (local_id, record) in &plugin.transports {
                insert_unique(
                    &mut transports,
                    plugin_record_id(plugin_id, local_id),
                    record.clone(),
                )?;
            }
            for record in plugin.modifiers.values() {
                insert_unique(&mut modifiers, record.id.clone(), record.clone())?;
            }
            for (local_id, record) in &plugin.runners {
                insert_unique(
                    &mut runners,
                    plugin_record_id(plugin_id, local_id),
                    record.clone(),
                )?;
            }
            for record in plugin.session_controls.values() {
                insert_unique(&mut session_controls, record.id.clone(), record.clone())?;
            }
        }

        // BTreeSet iteration supplies the contract's plugin-id precedence.
        for plugin_id in &enabled_plugin_ids {
            let plugin = &by_id[plugin_id];
            for system in plugin.systems.values() {
                let rules =
                    content_rules
                        .entry(system.id.clone())
                        .or_insert_with(|| ContentRules {
                            system: system.id.clone(),
                            ..ContentRules::default()
                        });
                if rules.title.is_none() {
                    rules.title = system.title.clone();
                }
                if let Some(content) = &system.content {
                    union(&mut rules.extensions, &content.extensions);
                    if let Some(layouts) = &content.layout {
                        union(&mut rules.layouts, layouts);
                    }
                    if let Some(evidence) = &content.evidence {
                        union(&mut rules.evidence.bytes, &evidence.bytes);
                        union(&mut rules.evidence.names, &evidence.names);
                    }
                    if let Some(header) = &content.header {
                        union(&mut rules.header, header);
                    }
                }
            }
            for group in plugin.groups.values() {
                let merged = groups
                    .entry(group.id.clone())
                    .or_insert_with(|| GroupRecord {
                        id: group.id.clone(),
                        title: None,
                        systems: Vec::new(),
                    });
                if merged.title.is_none() {
                    merged.title = group.title.clone();
                }
                union(&mut merged.systems, &group.systems);
            }
            declared_games.extend(plugin.declared_games.clone());
        }
        for rules in content_rules.values_mut() {
            if rules.layouts.is_empty() {
                rules.layouts.push("**/{title}.{ext}".into());
            }
        }
        for group in groups.values_mut() {
            if registered_system_ids.contains(&group.id) {
                return Err(PluginError::GroupSystemCollision(group.id.clone()));
            }
            group
                .systems
                .retain(|system| content_rules.contains_key(system));
        }

        Ok(Self {
            installed: BTreeMap::new(),
            plugins: by_id,
            enabled_plugin_ids,
            registered_provider_ids,
            registered_system_ids,
            registered_family_ids,
            registered_runner_ids,
            registered_modifier_ids,
            registered_transport_ids,
            registered_session_controls,
            providers,
            systems,
            families,
            runners,
            modifiers,
            transports,
            session_controls,
            content_rules,
            groups,
            declared_games,
        })
    }

    pub fn from_installed(
        packages: Vec<crate::plugin_installation::EnabledPackage>,
    ) -> Result<Self, PluginError> {
        let mut plugins = Vec::new();
        let mut installed = BTreeMap::new();
        for package in packages {
            let (namespace, _) = package
                .id
                .split_once(':')
                .ok_or_else(|| PluginError::InvalidPluginId(package.id.clone()))?;
            let source = script::source::SourceSnapshot::package_plugin(
                &package.package,
                &package.entry,
                &package.sources,
            )
            .map_err(PluginError::Evaluation)?;
            let json = script::eval_plugin_snapshot(&source).map_err(PluginError::Evaluation)?;
            let declaration: serde_json::Value = serde_json::from_str(&json)?;
            crate::plugin_references::validate_files(declaration.clone(), &package.files)
                .map_err(PluginError::Evaluation)?;
            let plugin = decode_plugin_declaration(namespace, &json)?;
            if plugin.id != package.id {
                return Err(PluginError::InvalidPluginId(package.id));
            }
            if installed.insert(package.id.clone(), package).is_some() {
                return Err(PluginError::DuplicatePluginId(plugin.id));
            }
            plugins.push(plugin);
        }
        let mut registry = Self::new(plugins, installed.keys().cloned())?;
        registry.installed = installed;
        for runner in registry.runners.values() {
            if let Some(program) = &runner.program {
                registry.installed_file(&runner.id, program)?;
            }
            if runner.program.is_some() {
                if let Some(core) = &runner.core {
                    registry.installed_file(&runner.id, core)?;
                }
            }
        }
        Ok(registry)
    }

    pub fn installed_file(
        &self,
        contribution: &str,
        key: &str,
    ) -> Result<std::path::PathBuf, PluginError> {
        let (owner, _) = contribution
            .split_once('/')
            .ok_or_else(|| PluginError::InvalidPluginId(contribution.into()))?;
        self.installed
            .get(owner)
            .and_then(|package| package.files.get(key))
            .cloned()
            .ok_or_else(|| {
                PluginError::Evaluation(format!(
                    "{contribution} requires file {key} from its own installed package {owner}"
                ))
            })
    }

    pub fn native_runner(&self, id: &str) -> Result<&RunnerRecord, PluginError> {
        let runner = self
            .runners
            .get(id)
            .ok_or_else(|| PluginError::Evaluation(format!("runner {id} is unavailable")))?;
        let program = runner
            .program
            .as_deref()
            .ok_or_else(|| PluginError::Evaluation(format!("runner {id} has no program")))?;
        self.installed_file(id, program)?;
        if let Some(core) = &runner.core {
            self.installed_file(id, core)?;
        }
        Ok(runner)
    }

    pub fn enabled_modifier(&self, id: &str) -> Result<(), PluginError> {
        if !self.modifiers.contains_key(id) {
            return Err(PluginError::Evaluation(format!(
                "modifier {id} is not declared by an installed, enabled package"
            )));
        }
        self.installed_package(id).map(|_| ())
    }

    pub fn installed_package(
        &self,
        contribution: &str,
    ) -> Result<&crate::plugin_installation::EnabledPackage, PluginError> {
        let (owner, _) = contribution
            .split_once('/')
            .ok_or_else(|| PluginError::InvalidPluginId(contribution.into()))?;
        self.installed
            .get(owner)
            .ok_or_else(|| PluginError::Evaluation(format!("{owner} is not installed and enabled")))
    }

    pub fn registered_plugin_ids(&self) -> Vec<&str> {
        self.plugins.keys().map(String::as_str).collect()
    }

    pub fn plugin_title(&self, plugin_id: &str) -> Option<&str> {
        self.plugins.get(plugin_id).map(Plugin::title)
    }

    pub fn enabled_plugin_ids(&self) -> Vec<&str> {
        self.enabled_plugin_ids.iter().map(String::as_str).collect()
    }

    pub fn owns_registered_provider_id(&self, id: &str) -> bool {
        self.registered_provider_ids.contains(id)
    }

    pub fn owns_registered_system_id(&self, id: &str) -> bool {
        self.registered_system_ids.contains(id)
    }

    pub fn owns_registered_runner_id(&self, id: &str) -> bool {
        self.registered_runner_ids.contains(id)
    }

    pub fn owns_registered_modifier_id(&self, id: &str) -> bool {
        self.registered_modifier_ids.contains(id)
    }

    pub fn owns_registered_family_id(&self, id: &str) -> bool {
        self.registered_family_ids.contains(id)
    }

    pub fn owns_registered_transport_id(&self, id: &str) -> bool {
        self.registered_transport_ids.contains(id)
    }

    pub fn owns_registered_session_control_id(&self, id: &str) -> bool {
        self.registered_session_controls.contains_key(id)
    }

    pub fn registered_session_controls(&self) -> &BTreeMap<String, SessionControlRecord> {
        &self.registered_session_controls
    }

    pub fn providers(&self) -> &BTreeMap<String, ProviderRecord> {
        &self.providers
    }

    pub fn systems(&self) -> &BTreeMap<String, SystemRecord> {
        &self.systems
    }

    pub fn families(&self) -> &BTreeMap<String, FamilyRecord> {
        &self.families
    }

    pub fn runners(&self) -> &BTreeMap<String, RunnerRecord> {
        &self.runners
    }

    pub fn transports(&self) -> &BTreeMap<String, TransportRecord> {
        &self.transports
    }

    pub fn session_controls(&self) -> &BTreeMap<String, SessionControlRecord> {
        &self.session_controls
    }

    /// Merged content rules for one enabled system, independent of runners.
    pub fn content_rules(&self, system: &str) -> Option<&ContentRules> {
        self.content_rules.get(system)
    }

    pub fn groups(&self) -> &BTreeMap<String, GroupRecord> {
        &self.groups
    }

    pub fn group(&self, id: &str) -> Option<&GroupRecord> {
        self.groups.get(id)
    }

    pub fn pairing_target(&self, id: &str) -> Option<PairingTarget<'_>> {
        if let Some(rules) = self.content_rules(id) {
            return Some(PairingTarget::System(rules));
        }
        self.group(id).map(|group| PairingTarget::Group {
            group,
            members: group
                .systems
                .iter()
                .filter_map(|id| self.content_rules(id))
                .collect(),
        })
    }

    /// Enabled declared games keyed `<plugin id>/<game key>` (ADR 0003).
    pub fn declared_games(&self) -> &BTreeMap<String, DeclaredGame> {
        &self.declared_games
    }

    pub fn declared_hash(&self, hash: &str) -> Vec<DeclaredReleaseRef<'_>> {
        self.declared_games
            .values()
            .flat_map(|game| {
                game.releases.iter().filter_map(move |(key, release)| {
                    release
                        .hashes
                        .iter()
                        .any(|candidate| candidate.eq_ignore_ascii_case(hash))
                        .then_some(DeclaredReleaseRef {
                            game,
                            release_key: key,
                            release,
                        })
                })
            })
            .collect()
    }
}

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("plugin evaluation failed: {0}")]
    Evaluation(String),
    #[error("invalid plugin declaration: {0}")]
    InvalidDeclaration(#[from] serde_json::Error),
    #[error("invalid plugin id {0}")]
    InvalidPluginId(String),
    #[error("invalid {kind} contribution {record_id}: {reason}")]
    InvalidContribution {
        kind: &'static str,
        record_id: String,
        reason: String,
    },
    #[error("invalid {kind} contribution key: local ids must not be empty")]
    EmptyContributionId { kind: &'static str },
    #[error("duplicate plugin id {0}")]
    DuplicatePluginId(String),
    #[error("enabled plugin {0} is not registered")]
    UnknownEnabledPlugin(String),
    #[error("duplicate contributed record id {0}")]
    DuplicateContribution(String),
    #[error("group id {0} is also a system id")]
    GroupSystemCollision(String),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginDeclaration {
    #[serde(skip)]
    namespace: String,
    name: String,
    #[serde(default)]
    services: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    title: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    description: Option<String>,
    #[serde(default)]
    providers: BTreeMap<String, ProviderContribution>,
    #[serde(default)]
    systems: BTreeMap<String, SystemRecord>,
    #[serde(default)]
    families: BTreeMap<String, FamilyRecord>,
    #[serde(default)]
    runners: BTreeMap<String, RunnerRecord>,
    #[serde(default)]
    modifiers: BTreeMap<String, ModifierRecord>,
    #[serde(default)]
    transports: BTreeMap<String, TransportRecord>,
    #[serde(default, rename = "controls")]
    session_controls: BTreeMap<String, SessionControlRecord>,
    #[serde(default)]
    groups: BTreeMap<String, GroupRecord>,
    #[serde(default)]
    games: BTreeMap<String, DeclaredGameContribution>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    config: Option<BTreeMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclaredGameContribution {
    title: String,
    releases: BTreeMap<String, DeclaredRelease>,
}

/// The caller supplies publisher identity from verified packaging or trusted
/// compiled composition. Plugin source cannot select its own publisher.
pub fn load_plugin_source(namespace: &str, source: &str) -> Result<Plugin, PluginError> {
    let declaration_json = script::eval_plugin_ts(source).map_err(PluginError::Evaluation)?;
    decode_plugin_declaration(namespace, &declaration_json)
}

pub fn decode_plugin_declaration(
    namespace: &str,
    declaration_json: &str,
) -> Result<Plugin, PluginError> {
    let mut declaration: PluginDeclaration = serde_json::from_str(declaration_json)?;
    declaration.namespace = namespace.to_owned();
    normalize_plugin(declaration)
}

fn normalize_plugin(mut declaration: PluginDeclaration) -> Result<Plugin, PluginError> {
    let id = format!("{}:{}", declaration.namespace, declaration.name);
    if !is_provider_id(&id) {
        return Err(PluginError::InvalidPluginId(id));
    }

    // Native services belong to the Linux host, not the data registry. Accept
    // the named export without changing any existing contribution record.
    let mut services = BTreeSet::new();
    for service in &declaration.services {
        if service.len() > 64 || !is_provider_segment(service) || !services.insert(service) {
            return Err(PluginError::InvalidContribution {
                kind: "service",
                record_id: service.clone(),
                reason: "invalid or repeated service name".into(),
            });
        }
    }
    let title = declaration
        .title
        .unwrap_or_else(|| titleize(&declaration.name));
    let mut providers = BTreeMap::new();

    for (record_id, contribution) in declaration.providers {
        if !is_provider_id(&record_id) {
            return Err(PluginError::InvalidContribution {
                kind: "provider",
                record_id,
                reason: "map key is not a provider id".to_owned(),
            });
        }
        if let Some(contributed_id) = contribution.id {
            if contributed_id != record_id {
                return Err(PluginError::InvalidContribution {
                    kind: "provider",
                    record_id,
                    reason: format!("record id {contributed_id} does not match its map key"),
                });
            }
        }
        providers.insert(
            record_id.clone(),
            ProviderRecord {
                id: record_id,
                title: contribution.title,
            },
        );
    }

    match providers.get_mut(&id) {
        Some(own_provider) => {
            if own_provider.title.is_none() {
                own_provider.title = Some(title.clone());
            }
        }
        None => {
            providers.insert(
                id.clone(),
                ProviderRecord {
                    id: id.clone(),
                    title: Some(title.clone()),
                },
            );
        }
    }

    for (local_id, system) in &declaration.systems {
        if local_id.is_empty() {
            return Err(PluginError::EmptyContributionId { kind: "system" });
        }
        if system.id != *local_id {
            return Err(PluginError::InvalidContribution {
                kind: "system",
                record_id: local_id.clone(),
                reason: format!("record id {} does not match its local key", system.id),
            });
        }
        if let Some(content) = &system.content {
            validate_content(local_id, content)?;
        }
    }

    // A family record enters the registry under its global id, so naming a
    // family is a claim on an id every other plugin also resolves. Only the
    // publisher who owns the namespace may make that claim. Joining a family
    // stays open: a runner still references any family id it likes.
    let namespace_prefix = format!("{}:", declaration.namespace);
    for (local_id, family) in &declaration.families {
        if local_id.is_empty() {
            return Err(PluginError::EmptyContributionId { kind: "family" });
        }
        if !is_provider_id(&family.id) {
            return Err(PluginError::InvalidContribution {
                kind: "family",
                record_id: local_id.clone(),
                reason: format!("family id {} must be scoped", family.id),
            });
        }
        if !family.id.starts_with(&namespace_prefix) {
            return Err(PluginError::InvalidContribution {
                kind: "family",
                record_id: local_id.clone(),
                reason: format!(
                    "family id {} is outside the publisher namespace {}",
                    family.id, declaration.namespace
                ),
            });
        }
    }

    for (local_id, modifier) in &declaration.modifiers {
        let expected = plugin_record_id(&id, local_id);
        if !is_provider_segment(local_id) || modifier.id != expected {
            return Err(PluginError::InvalidContribution {
                kind: "modifier",
                record_id: local_id.clone(),
                reason: format!(
                    "modifier id {} must be {expected} with a valid local key",
                    modifier.id
                ),
            });
        }
    }

    for (local_id, runner) in &declaration.runners {
        if local_id.is_empty() {
            return Err(PluginError::EmptyContributionId { kind: "runner" });
        }
        let expected_id = plugin_record_id(&id, local_id);
        if runner.id != expected_id {
            return Err(PluginError::InvalidContribution {
                kind: "runner",
                record_id: local_id.clone(),
                reason: format!("record id {} must be {expected_id}", runner.id),
            });
        }
        if runner
            .family
            .as_ref()
            .is_some_and(|family| !is_provider_id(family))
        {
            return Err(PluginError::InvalidPluginId(runner.family.clone().unwrap()));
        }
        if runner.command.as_deref() == Some("")
            || runner.program.as_deref() == Some("")
            || runner.core.as_deref() == Some("")
        {
            return Err(PluginError::InvalidContribution {
                kind: "runner",
                record_id: local_id.clone(),
                reason: "command and named file keys must not be empty".to_owned(),
            });
        }
        let native = runner.program.is_some();
        if let Some(releases) = &runner.releases {
            let invalid = |reason: &str| PluginError::InvalidContribution {
                kind: "runner",
                record_id: local_id.clone(),
                reason: reason.to_owned(),
            };
            if !native {
                return Err(invalid("releases require a native runner"));
            }
            if runner.systems.is_some() {
                return Err(invalid("runner cannot declare both systems and releases"));
            }
            if releases.is_empty() {
                return Err(invalid("releases must contain at least one hash"));
            }
            let mut seen = BTreeSet::new();
            if releases.iter().any(|release| !seen.insert(release)) {
                return Err(invalid("releases must not contain duplicate hashes"));
            }
        }
        if native == runner.command.is_some() {
            return Err(PluginError::InvalidContribution {
                kind: "runner",
                record_id: local_id.clone(),
                reason: "runner must declare exactly one program or command".to_owned(),
            });
        }
    }

    for (local_id, transport) in &declaration.transports {
        if local_id.is_empty() {
            return Err(PluginError::EmptyContributionId { kind: "transport" });
        }
        let expected_id = plugin_record_id(&id, local_id);
        if transport.id != expected_id {
            return Err(PluginError::InvalidContribution {
                kind: "transport",
                record_id: local_id.clone(),
                reason: format!("record id {} must be {expected_id}", transport.id),
            });
        }
    }

    let owned_runners: BTreeSet<String> = declaration
        .runners
        .values()
        .map(|record| record.id.clone())
        .collect();
    let owned_transports: BTreeSet<String> = declaration
        .transports
        .values()
        .map(|record| record.id.clone())
        .collect();

    let mut session_control_orders = BTreeMap::new();
    for (local_id, control) in &mut declaration.session_controls {
        if local_id.is_empty() {
            return Err(PluginError::EmptyContributionId {
                kind: "session control",
            });
        }
        if !is_plugin_record_id(&control.id, &id) {
            return Err(PluginError::InvalidContribution {
                kind: "session control",
                record_id: local_id.clone(),
                reason: format!("record id {} must belong to {id}", control.id),
            });
        }
        let owns_context = match control.owner.kind {
            SessionControlOwnerKind::Runner => owned_runners.contains(&control.owner.id),
            SessionControlOwnerKind::Transport => owned_transports.contains(&control.owner.id),
        };
        if !owns_context {
            return Err(PluginError::InvalidContribution {
                kind: "session control",
                record_id: local_id.clone(),
                reason: format!(
                    "owner {} is not a {:?} contribution of {id}",
                    control.owner.id, control.owner.kind
                ),
            });
        }
        if let Some(existing) = session_control_orders.insert(
            (control.owner.kind, control.owner.id.clone(), control.order),
            local_id.clone(),
        ) {
            return Err(PluginError::InvalidContribution {
                kind: "session control",
                record_id: local_id.clone(),
                reason: format!(
                    "order {} collides with session control {existing}",
                    control.order
                ),
            });
        }
        let effect_family = control.effect.family();
        let in_family = match control.owner.kind {
            SessionControlOwnerKind::Runner => {
                declaration
                    .runners
                    .values()
                    .find(|runner| runner.id == control.owner.id)
                    .and_then(|runner| runner.family.as_deref())
                    == Some(effect_family)
            }
            SessionControlOwnerKind::Transport => id == effect_family,
        };
        if !in_family {
            return Err(PluginError::InvalidContribution {
                kind: "session control",
                record_id: local_id.clone(),
                reason: format!("effect requires membership of family {effect_family}"),
            });
        }
        validate_session_control(local_id, control)?;
        control.plugin_id = id.clone();
        control.local_id = local_id.clone();
    }

    for (key, group) in &declaration.groups {
        if key.is_empty() || group.id != *key {
            return Err(invalid_contribution(
                "group",
                key,
                "id must match a non-empty map key",
            ));
        }
    }
    let mut declared_games = BTreeMap::new();
    for (key, mut game) in declaration.games {
        if !is_provider_segment(&key) {
            return Err(invalid_contribution(
                "game",
                &key,
                "key must be a local playable-id segment",
            ));
        }
        for (release_key, release) in &mut game.releases {
            if !is_provider_segment(release_key) {
                return Err(invalid_contribution(
                    "game",
                    &key,
                    "release key must be a local playable-id segment",
                ));
            }
            if release.hashes.is_empty()
                || release.hashes.iter().any(|hash| !is_declared_hash(hash))
            {
                return Err(invalid_contribution(
                    "game",
                    &key,
                    "release requires sha256:<64 hex> or sha1:<40 hex> hashes",
                ));
            }
            // Hash identity is case-independent. Store the same lowercase form
            // that the scanner computes, including for uppercase published lists.
            for hash in &mut release.hashes {
                hash.make_ascii_lowercase();
            }
            if let Some(file) = &release.file {
                if file.is_empty()
                    || file.starts_with('/')
                    || std::path::Path::new(file)
                        .components()
                        .any(|part| matches!(part, std::path::Component::ParentDir))
                    || file.contains('\0')
                {
                    return Err(invalid_contribution(
                        "game",
                        &key,
                        "file must be relative to an account root without traversal",
                    ));
                }
            }
        }
        declared_games.insert(
            plugin_record_id(&id, &key),
            DeclaredGame {
                provider: id.clone(),
                key,
                title: game.title,
                releases: game.releases,
            },
        );
    }

    Ok(Plugin {
        config: declaration.config,
        id,
        title,
        description: declaration.description,
        providers,
        systems: declaration.systems,
        families: declaration.families,
        runners: declaration.runners,
        modifiers: declaration.modifiers,
        transports: declaration.transports,
        session_controls: declaration.session_controls,
        groups: declaration.groups,
        declared_games,
    })
}

fn union<T: PartialEq + Clone>(target: &mut Vec<T>, values: &[T]) {
    for value in values {
        if !target.contains(value) {
            target.push(value.clone());
        }
    }
}

fn invalid_contribution(kind: &'static str, id: &str, reason: &str) -> PluginError {
    PluginError::InvalidContribution {
        kind,
        record_id: id.into(),
        reason: reason.into(),
    }
}

fn is_declared_hash(hash: &str) -> bool {
    let Some((algorithm, value)) = hash.split_once(':') else {
        return false;
    };
    let length = match algorithm {
        "sha256" => 64,
        "sha1" => 40,
        _ => return false,
    };
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_content(id: &str, content: &ContentRecord) -> Result<(), PluginError> {
    let invalid = |reason: &str| invalid_contribution("system content", id, reason);
    for extension in &content.extensions {
        if !extension.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        }) {
            return Err(invalid(
                "extensions must be lowercase suffixes without a leading dot",
            ));
        }
        if matches!(
            extension.rsplit('.').next(),
            Some("zip" | "7z" | "rar" | "gz" | "tar")
        ) {
            return Err(invalid("archive extensions are not supported"));
        }
    }
    if let Some(layouts) = &content.layout {
        for layout in layouts {
            validate_layout(layout, &content.extensions).map_err(invalid)?;
        }
    }
    if let Some(evidence) = &content.evidence {
        for bytes in &evidence.bytes {
            validate_bytes(bytes).map_err(invalid)?;
        }
    }
    if let Some(headers) = &content.header {
        for rule in headers {
            match (&rule.bytes, &rule.remainder) {
                (Some(bytes), None) => validate_bytes(bytes).map_err(invalid)?,
                (None, Some(remainder))
                    if remainder.divisor > 0 && remainder.value < remainder.divisor => {}
                (None, Some(_)) => {
                    return Err(invalid(
                        "header remainder requires a positive divisor and a value below it",
                    ))
                }
                _ => return Err(invalid("header requires exactly one of bytes or remainder")),
            }
        }
    }
    Ok(())
}

fn validate_bytes(rule: &BytesRule) -> Result<(), &'static str> {
    if rule.hex.is_empty()
        || !rule.hex.len().is_multiple_of(2)
        || rule.hex.len() > 128
        || !rule
            .hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("hex must be non-empty lowercase even-length hex of at most 64 bytes");
    }
    let size = (rule.hex.len() / 2) as u64;
    if rule.offset >= 65536 || size > 65536 - rule.offset {
        return Err("byte evidence must fit within the first 64 KiB");
    }
    Ok(())
}

fn validate_layout(layout: &str, extensions: &[String]) -> Result<(), &'static str> {
    if layout.ends_with('/') {
        return Err("folder layouts are not supported");
    }
    if layout
        .split('/')
        .any(|part| part.contains("**") && part != "**")
    {
        return Err("** must occupy a whole path segment");
    }
    let mut rest = layout;
    let mut titles = 0;
    while let Some(start) = rest.find(['{', '}']) {
        if rest.as_bytes()[start] != b'{' {
            return Err("unmatched layout brace");
        }
        let Some(end) = rest[start..].find('}') else {
            return Err("unmatched layout brace");
        };
        match &rest[start + 1..start + end] {
            "title" => titles += 1,
            "disc" | "version" | "region" => {}
            "ext" if !extensions.is_empty() => {}
            "ext" => return Err("{ext} requires system extensions"),
            _ => return Err("unknown layout token"),
        }
        rest = &rest[start + end + 1..];
    }
    if titles != 1 {
        return Err("layout requires {title} exactly once");
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SessionControlKind {
    Command,
    Toggle,
    Choice,
    Range,
}

fn validate_session_control(
    local_id: &str,
    control: &SessionControlRecord,
) -> Result<(), PluginError> {
    let invalid = |reason: &str| PluginError::InvalidContribution {
        kind: "session control",
        record_id: local_id.to_owned(),
        reason: reason.to_owned(),
    };

    if control.label.trim().is_empty() {
        return Err(invalid("label must not be empty"));
    }
    if control
        .description
        .as_ref()
        .is_some_and(|description| description.trim().is_empty())
    {
        return Err(invalid("description must not be empty"));
    }

    let declared_kind = match &control.interaction {
        SessionControlDeclarationInteraction::Command => SessionControlKind::Command,
        SessionControlDeclarationInteraction::Toggle {
            true_label,
            false_label,
        } => {
            if true_label.as_ref().is_some_and(String::is_empty)
                || false_label.as_ref().is_some_and(String::is_empty)
            {
                return Err(invalid("toggle display labels must not be empty"));
            }
            SessionControlKind::Toggle
        }
        SessionControlDeclarationInteraction::Choice { options } => {
            if options.is_empty() {
                return Err(invalid("choice controls must declare at least one option"));
            }
            let mut values = BTreeSet::new();
            for option in options {
                if option.value.trim().is_empty() || option.label.trim().is_empty() {
                    return Err(invalid("choice option values and labels must not be empty"));
                }
                if !values.insert(option.value.as_str()) {
                    return Err(invalid("choice option values must be unique"));
                }
            }
            SessionControlKind::Choice
        }
        SessionControlDeclarationInteraction::Range { min, max, step } => {
            if !min.is_finite()
                || !max.is_finite()
                || !step.is_finite()
                || *min > *max
                || *step <= 0.0
                || min + step == *min
            {
                return Err(invalid("range bounds and step are invalid"));
            }
            SessionControlKind::Range
        }
    };
    if declared_kind != SessionControlKind::Command {
        return Err(invalid(
            "control form does not match the allowlisted effect",
        ));
    }
    Ok(())
}

fn insert_unique<T>(
    records: &mut BTreeMap<String, T>,
    id: String,
    record: T,
) -> Result<(), PluginError> {
    if records.insert(id.clone(), record).is_some() {
        return Err(PluginError::DuplicateContribution(id));
    }
    Ok(())
}

fn plugin_record_id(plugin_id: &str, local_id: &str) -> String {
    format!("{plugin_id}/{local_id}")
}

fn is_plugin_record_id(value: &str, plugin_id: &str) -> bool {
    value
        .strip_prefix(plugin_id)
        .and_then(|suffix| suffix.strip_prefix('/'))
        .is_some_and(|local_id| {
            !local_id.is_empty()
                && local_id.chars().all(|character| {
                    character.is_ascii_lowercase()
                        || character.is_ascii_digit()
                        || matches!(character, '-' | '_' | '.')
                })
        })
}

fn is_provider_id(value: &str) -> bool {
    let Some(without_at) = value.strip_prefix('@') else {
        return false;
    };
    let Some((namespace, name)) = without_at.split_once(':') else {
        return false;
    };
    !namespace.contains(':') && is_provider_segment(namespace) && is_provider_segment(name)
}

fn is_provider_segment(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | '_' | '-')
        })
}

fn deserialize_optional_non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

fn titleize(id: &str) -> String {
    id.split(['-', '_', '.'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().chain(characters).collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn plugin(name: &str, exports: Value) -> Result<Plugin, PluginError> {
        let mut declaration = exports.as_object().unwrap().clone();
        declaration.insert("name".into(), json!(name));
        decode_plugin_declaration("@test", &Value::Object(declaration).to_string())
    }

    fn content(value: Value) -> Result<Plugin, PluginError> {
        plugin(
            "rules",
            json!({"systems": {"nes": {"id": "nes", "content": value}}}),
        )
    }

    #[test]
    fn exports_use_the_new_names_without_compatibility() {
        load_plugin_source("@test", "export const name = 'rules'; export const groups = {}; export const games = {}; export const controls = {};").unwrap();
        for export in ["discovery", "sessionControls"] {
            let source = format!("export const name = 'rules'; export const {export} = {{}};");
            assert!(load_plugin_source("@test", &source).is_err(), "{export}");
            assert!(plugin("rules", json!({export: {}})).is_err(), "{export}");
        }
    }

    #[test]
    fn content_extensions_are_lowercase_compound_suffixes_not_archives() {
        for extension in ["nes", "p8.png", "7abc"] {
            content(json!({"extensions": [extension]})).unwrap();
        }
        for extension in [
            "", ".nes", "NES", " nes", "p8..png", "p8/png", "p8.*", "zip", "7z", "rar", "gz", "tar",
        ] {
            assert!(
                content(json!({"extensions": [extension]})).is_err(),
                "{extension}"
            );
        }
    }

    #[test]
    fn layouts_validate_tokens_titles_and_file_releases() {
        for layout in [
            "**/{title}.{ext}",
            "{title} ({region})/{disc}-{version}.{ext}",
            "*/{title}.nes",
        ] {
            content(json!({"extensions": ["nes"], "layout": [layout]})).unwrap();
        }
        for layout in [
            "**/{name}.{ext}",
            "{title}/{title}.{ext}",
            "game.{ext}",
            "{title}/",
            "{title}.{unknown}",
            "{title}.{ext",
            "{title}.ext}",
            "foo**/{title}.{ext}",
        ] {
            assert!(
                content(json!({"extensions": ["nes"], "layout": [layout]})).is_err(),
                "{layout}"
            );
        }
        assert!(content(json!({"extensions": [], "layout": ["{title}.{ext}"]})).is_err());
    }

    #[test]
    fn byte_rules_are_bounded_lowercase_even_hex() {
        for (offset, hex) in [
            (0, "4e45531a".to_owned()),
            (65472, "ff".repeat(64)),
            (65535, "ff".into()),
        ] {
            content(json!({"extensions": ["nes"], "evidence": {"bytes": [{"offset": offset, "hex": hex}]}})).unwrap();
        }
        for rule in [
            json!({"offset": 0, "hex": ""}),
            json!({"offset": 0, "hex": "AA"}),
            json!({"offset": 0, "hex": "f"}),
            json!({"offset": 0, "hex": "gg"}),
            json!({"offset": 0, "hex": "ff".repeat(65)}),
            json!({"offset": 65536, "hex": "ff"}),
            json!({"offset": 65535, "hex": "ffff"}),
            json!({"offset": u64::MAX, "hex": "ff"}),
        ] {
            for field in ["evidence", "header"] {
                let value = if field == "evidence" {
                    json!({"extensions": ["nes"], "evidence": {"bytes": [rule]}})
                } else {
                    json!({"extensions": ["nes"], "header": [{"size": 16, "bytes": rule}]})
                };
                assert!(content(value).is_err(), "{field}: {rule}");
            }
        }
    }

    #[test]
    fn header_rules_have_exactly_one_condition() {
        for rule in [
            json!({"size": 16, "bytes": {"offset": 0, "hex": "4e45531a"}}),
            json!({"size": 512, "remainder": {"divisor": 1024, "value": 512}}),
        ] {
            content(json!({"extensions": ["nes"], "header": [rule]})).unwrap();
        }
        for rule in [
            json!({"size": 16}),
            json!({"size": 16, "bytes": {"offset": 0, "hex": "00"}, "remainder": {"divisor": 1024, "value": 512}}),
            json!({"size": 16, "remainder": {"divisor": 0, "value": 0}}),
            json!({"size": 16, "remainder": {"divisor": 4, "value": 4}}),
        ] {
            assert!(
                content(json!({"extensions": ["nes"], "header": [rule]})).is_err(),
                "{rule}"
            );
        }
    }

    #[test]
    fn declared_hashes_and_local_keys_are_validated() {
        let declare = |game: &str, release: &str, hashes: Value| {
            plugin(
                "games",
                json!({"games": {game: {"title": "Game", "releases": {release: {"hashes": hashes}}}}}),
            )
        };
        for hash in [
            format!("sha256:{}", "a".repeat(64)),
            format!("sha1:{}", "f".repeat(40)),
        ] {
            declare("game-1.a_b", "release-1.a_b", json!([hash])).unwrap();
        }
        for hashes in [
            json!([]),
            json!(["sha256:a"]),
            json!([format!("sha256:{}", "g".repeat(64))]),
            json!([format!("sha1:{}", "a".repeat(41))]),
            json!([format!("md5:{}", "a".repeat(32))]),
            json!([null]),
        ] {
            assert!(
                declare("game", "release", hashes.clone()).is_err(),
                "{hashes}"
            );
        }
        for key in [
            "",
            ".",
            "..",
            "_game",
            "-game",
            "Game",
            "game/release",
            "game space",
        ] {
            assert!(
                declare(key, "release", json!([format!("sha1:{}", "f".repeat(40))])).is_err(),
                "game {key}"
            );
            assert!(
                declare("game", key, json!([format!("sha1:{}", "f".repeat(40))])).is_err(),
                "release {key}"
            );
        }
    }

    #[test]
    fn registry_merges_enabled_content_in_plugin_id_order_without_runners() {
        let a = plugin("a", json!({"systems": {"nes": {"id": "nes", "title": "First", "content": {"extensions": ["nes"], "layout": ["{title}.{ext}", "**/{title}.{ext}"], "evidence": {"bytes": [{"offset": 0, "hex": "00"}], "names": ["first"]}, "header": [{"size": 16, "bytes": {"offset": 0, "hex": "00"}}]}}, "gb": {"id": "gb"}}})).unwrap();
        let b = plugin("b", json!({"systems": {"nes": {"id": "nes", "title": "Second", "content": {"extensions": ["nes", "fds"], "layout": ["**/{title}.{ext}", "*/{title}.{ext}"], "evidence": {"bytes": [{"offset": 0, "hex": "00"}, {"offset": 1, "hex": "ff"}], "names": ["first", "second"]}, "header": [{"size": 16, "bytes": {"offset": 0, "hex": "00"}}, {"size": 512, "remainder": {"divisor": 1024, "value": 512}}]}}}})).unwrap();
        let disabled = plugin(
            "disabled",
            json!({"systems": {"nes": {"id": "nes", "content": {"extensions": ["ignored"]}}}}),
        )
        .unwrap();
        let registry =
            PluginRegistry::new(vec![b, disabled, a], ["@test:b".into(), "@test:a".into()])
                .unwrap();
        let rules = registry.content_rules("nes").unwrap();
        assert_eq!(rules.title.as_deref(), Some("First"));
        assert_eq!(rules.extensions, ["nes", "fds"]);
        assert_eq!(
            rules.layouts,
            ["{title}.{ext}", "**/{title}.{ext}", "*/{title}.{ext}"]
        );
        assert_eq!(rules.evidence.bytes.len(), 2);
        assert_eq!(rules.evidence.names, ["first", "second"]);
        assert_eq!(
            rules.header.iter().map(|r| r.size).collect::<Vec<_>>(),
            [16, 512]
        );
        assert_eq!(
            registry.content_rules("gb").unwrap().layouts,
            ["**/{title}.{ext}"]
        );
        assert!(registry.runners().is_empty());
        assert!(matches!(
            registry.pairing_target("nes"),
            Some(PairingTarget::System(_))
        ));
        assert!(registry.content_rules("missing").is_none());
        assert!(registry.pairing_target("missing").is_none());
    }

    #[test]
    fn groups_merge_drop_undeclared_members_and_cannot_shadow_systems() {
        let a = plugin("a", json!({"systems": {"gb": {"id": "gb"}}, "groups": {"gameboy": {"id": "gameboy", "title": "First", "systems": ["gb", "gbc", "missing"]}}})).unwrap();
        let b = plugin("b", json!({"systems": {"gbc": {"id": "gbc", "content": {"extensions": ["gbc"]}}}, "groups": {"gameboy": {"id": "gameboy", "title": "Second", "systems": ["gbc", "gb"]}}})).unwrap();
        let disabled = plugin(
            "disabled",
            json!({"systems": {"missing": {"id": "missing"}}}),
        )
        .unwrap();
        let registry = PluginRegistry::new(
            vec![b, a.clone(), disabled],
            ["@test:b".into(), "@test:a".into()],
        )
        .unwrap();
        let group = registry.group("gameboy").unwrap();
        assert_eq!(group.title.as_deref(), Some("First"));
        assert_eq!(group.systems, ["gb", "gbc"]);
        assert!(
            matches!(registry.pairing_target("gameboy"), Some(PairingTarget::Group { members, .. }) if members.len() == 2)
        );
        let conflict = plugin(
            "conflict",
            json!({"groups": {"gb": {"id": "gb", "systems": []}}}),
        )
        .unwrap();
        assert!(PluginRegistry::new(
            vec![a, conflict],
            ["@test:a".into(), "@test:conflict".into()]
        )
        .is_err());
        assert!(plugin(
            "bad",
            json!({"groups": {"gb": {"id": "other", "systems": []}}})
        )
        .is_err());
    }

    #[test]
    fn new_exports_decode_real_module_records_and_canonicalize_hashes() {
        let hash = format!("sha256:{}", "A".repeat(64));
        let source = format!(
            r#"
            export const name = "games";
            export const systems = {{nes: {{id: "nes", content: {{extensions: ["nes"], layout: ["**/{{title}}.{{ext}}"], evidence: {{names: ["^Game"]}}, header: [{{size: 16, bytes: {{offset: 0, hex: "4e45531a"}}}}]}}}}}};
            export const groups = {{nintendo: {{id: "nintendo", systems: ["nes"]}}}};
            export const games = {{game: {{title: "Game", releases: {{original: {{hashes: ["{hash}"], system: "nes"}}}}}}}};
        "#
        );
        let plugin = load_plugin_source("@test", &source).unwrap();
        let registry = PluginRegistry::new(vec![plugin], ["@test:games".into()]).unwrap();
        assert_eq!(registry.groups().len(), 1);
        assert_eq!(
            registry.groups()["nintendo"],
            *registry.group("nintendo").unwrap()
        );
        assert_eq!(registry.content_rules("nes").unwrap().header.len(), 1);
        assert_eq!(registry.declared_hash(&hash.to_lowercase()).len(), 1);
        assert_eq!(registry.declared_hash(&hash).len(), 1);
    }

    #[test]
    fn default_layout_is_only_added_when_no_enabled_plugin_supplies_one() {
        let a = plugin(
            "a",
            json!({"systems": {"nes": {"id": "nes", "content": {"extensions": ["nes"]}}}}),
        )
        .unwrap();
        let b = plugin("b", json!({"systems": {"nes": {"id": "nes", "title": "NES", "content": {"extensions": ["fds"], "layout": ["{title}.{ext}"]}}}})).unwrap();
        let default = PluginRegistry::new(vec![a.clone()], ["@test:a".into()]).unwrap();
        assert_eq!(
            default.content_rules("nes").unwrap().layouts,
            ["**/{title}.{ext}"]
        );
        let merged = PluginRegistry::new(vec![b, a], ["@test:a".into(), "@test:b".into()]).unwrap();
        assert_eq!(
            merged.content_rules("nes").unwrap().layouts,
            ["{title}.{ext}"]
        );
        assert_eq!(
            merged.content_rules("nes").unwrap().title.as_deref(),
            Some("NES")
        );
    }

    #[test]
    fn new_records_reject_unknown_fields_nulls_and_escaping_prepared_files() {
        for value in [
            json!({"extensions": ["nes"], "unknown": true}),
            json!({"extensions": ["nes"], "layout": null}),
            json!({"extensions": ["nes"], "evidence": {"bytes": null}}),
            json!({"extensions": ["nes"], "header": [{"size": 16, "bytes": null}]}),
        ] {
            assert!(content(value.clone()).is_err(), "{value}");
        }
        let hash = format!("sha1:{}", "f".repeat(40));
        for file in [
            "",
            "/outside/game",
            "../game",
            "safe/../../game",
            "safe\0game",
        ] {
            assert!(plugin("games", json!({"games": {"game": {"title": "Game", "releases": {"prepared": {"hashes": [hash], "file": file}}}}})).is_err(), "{file}");
        }
        assert!(plugin(
            "groups",
            json!({"groups": {"gb": {"id": "gb", "systems": null}}})
        )
        .is_err());
        assert!(plugin(
            "games",
            json!({"games": {"game": {"title": "Game", "unknown": true, "releases": {}}}})
        )
        .is_err());
    }

    #[test]
    fn an_enabled_group_cannot_shadow_a_disabled_registered_system() {
        let system = plugin("system", json!({"systems": {"gb": {"id": "gb"}}})).unwrap();
        let group = plugin(
            "group",
            json!({"groups": {"gb": {"id": "gb", "systems": []}}}),
        )
        .unwrap();
        assert!(matches!(
            PluginRegistry::new(vec![system, group], ["@test:group".into()]),
            Err(PluginError::GroupSystemCollision(_))
        ));
    }

    #[test]
    fn declared_games_keep_provider_and_hash_queries_return_every_release() {
        let hash = format!("sha1:{}", "a".repeat(40));
        let exports = json!({"games": {"jak1": {"title": "Jak", "releases": {"prepared": {"hashes": [hash], "file": "opengoal/GAME.CGO"}, "original": {"hashes": [hash], "system": "ps2"}}}}});
        let a = plugin("a", exports.clone()).unwrap();
        let b = plugin("b", exports.clone()).unwrap();
        let disabled = plugin("disabled", exports).unwrap();
        let registry =
            PluginRegistry::new(vec![b, disabled, a], ["@test:b".into(), "@test:a".into()])
                .unwrap();
        assert_eq!(
            registry
                .declared_games()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["@test:a/jak1", "@test:b/jak1"]
        );
        let game = &registry.declared_games()["@test:a/jak1"];
        assert_eq!(game.provider, "@test:a");
        assert_eq!(game.key, "jak1");
        assert_eq!(game.releases["prepared"].system, None);
        assert_eq!(
            game.releases["prepared"].file.as_deref(),
            Some("opengoal/GAME.CGO")
        );
        assert_eq!(registry.declared_hash(&hash).len(), 4);
        assert_eq!(registry.declared_hash(&hash)[0].release_key, "original");
        assert!(registry.declared_hash("sha1:unknown").is_empty());
    }
}
