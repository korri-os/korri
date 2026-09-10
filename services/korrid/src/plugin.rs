//! Declaration-only plugins and their device-local registry.
//!
//! This is the narrow legacy plugin seam exercised by the Android application
//! checkpoint: a plugin identifies itself and contributes provider, system,
//! launcher, transport, runtime, file-release discovery, and contextual session-control records. Plugins
//! still perform no effects; this module only evaluates, validates, normalizes,
//! and announces their declarations.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path},
};

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
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AndroidLauncherRecord {
    pub package_name: String,
    pub class_name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LauncherRecord {
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub plugin: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub command: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub systems: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub android: Option<AndroidLauncherRecord>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub kind: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub program: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeSupportsRecord {
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub systems: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeRecord {
    pub id: String,
    pub kind: String,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub app: Option<String>,
    pub path: String,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub launcher: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub supports: Option<RuntimeSupportsRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileReleaseDiscoveryClaim {
    pub id: String,
    pub title: Option<String>,
    pub extensions: Vec<String>,
    pub system: String,
    pub launcher: String,
    pub runtime: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileReleaseDiscoveryContribution {
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub title: Option<String>,
    pub extensions: Vec<String>,
    pub system: String,
    pub launcher: String,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub runtime: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum AndroidTransportImplementation {
    Artemis,
}

impl AndroidTransportImplementation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Artemis => "artemis",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AndroidTransportRecord {
    pub implementation: AndroidTransportImplementation,
    pub sunshine_app: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TransportRecord {
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    pub android: Option<AndroidTransportRecord>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "kebab-case")]
pub enum SessionControlOwnerKind {
    Launcher,
    Transport,
    Runtime,
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum SessionControlEffect {
    #[serde(rename = "@korri:retroarch/open-menu")]
    RetroarchOpenMenu,
    #[serde(rename = "@korri:retroarch/quit")]
    RetroarchQuit,
    #[serde(rename = "@korri:moonlight/disconnect")]
    MoonlightDisconnect,
    #[serde(rename = "@korri:moonlight/quit-host")]
    MoonlightQuitHost,
    #[serde(rename = "@korri:moonlight/toggle-keyboard")]
    MoonlightToggleKeyboard,
    #[serde(rename = "@korri:moonlight/toggle-full-keyboard")]
    MoonlightToggleFullKeyboard,
    #[serde(rename = "@korri:moonlight/set-fill-mode")]
    MoonlightSetFillMode,
    #[serde(rename = "@korri:moonlight/set-zoom-mode")]
    MoonlightSetZoomMode,
    #[serde(rename = "@korri:moonlight/rotate-screen")]
    MoonlightRotateScreen,
    #[serde(rename = "@korri:moonlight/toggle-hud")]
    MoonlightToggleHud,
    #[serde(rename = "@korri:moonlight/toggle-floating-menu")]
    MoonlightToggleFloatingMenu,
    #[serde(rename = "@korri:moonlight/toggle-keyboard-controller")]
    MoonlightToggleKeyboardController,
    #[serde(rename = "@korri:moonlight/switch-touch-sensitivity")]
    MoonlightSwitchTouchSensitivity,
    #[serde(rename = "@korri:moonlight/set-mouse-mode")]
    MoonlightSetMouseMode,
    #[serde(rename = "@korri:moonlight/set-local-cursor")]
    MoonlightSetLocalCursor,
    #[serde(rename = "@korri:moonlight/set-sgsr-edge-threshold")]
    MoonlightSetSgsrEdgeThreshold,
    #[serde(rename = "@korri:moonlight/set-sgsr-sharpness")]
    MoonlightSetSgsrSharpness,
    #[serde(rename = "@korri:moonlight/set-face-button-flip")]
    MoonlightSetFaceButtonFlip,
    #[serde(rename = "@korri:moonlight/set-rumble")]
    MoonlightSetRumble,
    #[serde(rename = "@korri:moonlight/set-picture-in-picture")]
    MoonlightSetPictureInPicture,
    #[serde(rename = "@korri:moonlight/set-stream-bitrate-kbps")]
    MoonlightSetStreamBitrateKbps,
    #[serde(rename = "@korri:moonlight/restore-stream-bitrate")]
    MoonlightRestoreStreamBitrate,
    #[serde(rename = "@korri:moonlight/set-stream-fps")]
    MoonlightSetStreamFps,
    #[serde(rename = "@korri:moonlight/restore-stream-fps")]
    MoonlightRestoreStreamFps,
    #[serde(rename = "@korri:moonlight/set-stream-width")]
    MoonlightSetStreamWidth,
    #[serde(rename = "@korri:moonlight/restore-stream-resolution")]
    MoonlightRestoreStreamResolution,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SessionControlExecutor {
    AndroidMoonlight,
    RetroarchControl,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionControlPlatform {
    Android,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SessionControlIntegration {
    Moonlight,
    Retroarch,
}

impl SessionControlEffect {
    fn integration(self) -> SessionControlIntegration {
        match self {
            Self::RetroarchOpenMenu | Self::RetroarchQuit => SessionControlIntegration::Retroarch,
            Self::MoonlightDisconnect
            | Self::MoonlightQuitHost
            | Self::MoonlightToggleKeyboard
            | Self::MoonlightToggleFullKeyboard
            | Self::MoonlightSetFillMode
            | Self::MoonlightSetZoomMode
            | Self::MoonlightRotateScreen
            | Self::MoonlightToggleHud
            | Self::MoonlightToggleFloatingMenu
            | Self::MoonlightToggleKeyboardController
            | Self::MoonlightSwitchTouchSensitivity
            | Self::MoonlightSetMouseMode
            | Self::MoonlightSetLocalCursor
            | Self::MoonlightSetSgsrEdgeThreshold
            | Self::MoonlightSetSgsrSharpness
            | Self::MoonlightSetFaceButtonFlip
            | Self::MoonlightSetRumble
            | Self::MoonlightSetPictureInPicture
            | Self::MoonlightSetStreamBitrateKbps
            | Self::MoonlightRestoreStreamBitrate
            | Self::MoonlightSetStreamFps
            | Self::MoonlightRestoreStreamFps
            | Self::MoonlightSetStreamWidth
            | Self::MoonlightRestoreStreamResolution => SessionControlIntegration::Moonlight,
        }
    }

    pub fn executor(self) -> SessionControlExecutor {
        match self.integration() {
            SessionControlIntegration::Moonlight => SessionControlExecutor::AndroidMoonlight,
            SessionControlIntegration::Retroarch => SessionControlExecutor::RetroarchControl,
        }
    }

    pub(crate) fn retroarch_control_command(
        self,
    ) -> Option<crate::launcher::retroarch_control::RetroarchControlCommand> {
        use crate::launcher::retroarch_control::RetroarchControlCommand as Command;
        match self {
            Self::RetroarchOpenMenu => Some(Command::ShowMenu),
            Self::RetroarchQuit => Some(Command::Quit),
            _ => None,
        }
    }

    pub fn android_moonlight_effect(self) -> Option<crate::launcher::AndroidMoonlightEffect> {
        use crate::launcher::AndroidMoonlightEffect as Effect;
        Some(match self {
            Self::RetroarchOpenMenu | Self::RetroarchQuit => return None,
            Self::MoonlightDisconnect => Effect::Disconnect,
            Self::MoonlightQuitHost => Effect::QuitHost,
            Self::MoonlightToggleKeyboard => Effect::ToggleKeyboard,
            Self::MoonlightToggleFullKeyboard => Effect::ToggleFullKeyboard,
            Self::MoonlightSetFillMode => Effect::SetFillMode,
            Self::MoonlightSetZoomMode => Effect::SetZoomMode,
            Self::MoonlightRotateScreen => Effect::RotateScreen,
            Self::MoonlightToggleHud => Effect::ToggleHud,
            Self::MoonlightToggleFloatingMenu => Effect::ToggleFloatingMenu,
            Self::MoonlightToggleKeyboardController => Effect::ToggleKeyboardController,
            Self::MoonlightSwitchTouchSensitivity => Effect::SwitchTouchSensitivity,
            Self::MoonlightSetMouseMode => Effect::SetMouseMode,
            Self::MoonlightSetLocalCursor => Effect::SetLocalCursor,
            Self::MoonlightSetSgsrEdgeThreshold => Effect::SetSgsrEdgeThreshold,
            Self::MoonlightSetSgsrSharpness => Effect::SetSgsrSharpness,
            Self::MoonlightSetFaceButtonFlip => Effect::SetFaceButtonFlip,
            Self::MoonlightSetRumble => Effect::SetRumble,
            Self::MoonlightSetPictureInPicture => Effect::SetPictureInPicture,
            Self::MoonlightSetStreamBitrateKbps => Effect::SetStreamBitrateKbps,
            Self::MoonlightRestoreStreamBitrate => Effect::RestoreStreamBitrate,
            Self::MoonlightSetStreamFps => Effect::SetStreamFps,
            Self::MoonlightRestoreStreamFps => Effect::RestoreStreamFps,
            Self::MoonlightSetStreamWidth => Effect::SetStreamWidth,
            Self::MoonlightRestoreStreamResolution => Effect::RestoreStreamResolution,
        })
    }

    pub fn platform(self) -> SessionControlPlatform {
        match self.integration() {
            SessionControlIntegration::Moonlight | SessionControlIntegration::Retroarch => {
                SessionControlPlatform::Android
            }
        }
    }

    fn plugin_id(self) -> &'static str {
        match self.integration() {
            SessionControlIntegration::Moonlight => "@korri:moonlight",
            SessionControlIntegration::Retroarch => "@korri:retroarch",
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
    android: Option<BTreeMap<String, serde_json::Value>>,
    title: String,
    description: Option<String>,
    providers: BTreeMap<String, ProviderRecord>,
    systems: BTreeMap<String, SystemRecord>,
    launchers: BTreeMap<String, LauncherRecord>,
    transports: BTreeMap<String, TransportRecord>,
    runtimes: BTreeMap<String, RuntimeRecord>,
    session_controls: BTreeMap<String, SessionControlRecord>,
    file_release_discovery_claims: BTreeMap<String, FileReleaseDiscoveryClaim>,
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
    pub fn android(&self) -> Option<&BTreeMap<String, serde_json::Value>> {
        self.android.as_ref()
    }
}

#[derive(Clone, Debug)]
pub struct PluginRegistry {
    plugins: BTreeMap<String, Plugin>,
    installed: BTreeMap<String, crate::plugin_installation::EnabledPackage>,
    enabled_plugin_ids: BTreeSet<String>,
    registered_provider_ids: BTreeSet<String>,
    registered_system_ids: BTreeSet<String>,
    registered_launcher_ids: BTreeSet<String>,
    registered_transport_ids: BTreeSet<String>,
    registered_runtime_ids: BTreeSet<String>,
    registered_session_controls: BTreeMap<String, SessionControlRecord>,
    providers: BTreeMap<String, ProviderRecord>,
    systems: BTreeMap<String, SystemRecord>,
    launchers: BTreeMap<String, LauncherRecord>,
    transports: BTreeMap<String, TransportRecord>,
    runtimes: BTreeMap<String, RuntimeRecord>,
    session_controls: BTreeMap<String, SessionControlRecord>,
    file_release_discovery_claims: BTreeMap<String, FileReleaseDiscoveryClaim>,
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
        let mut registered_launcher_ids = BTreeSet::new();
        let mut registered_transport_ids = BTreeSet::new();
        let mut registered_runtime_ids = BTreeSet::new();
        let mut registered_session_controls = BTreeMap::new();
        let mut providers = BTreeMap::new();
        let mut systems = BTreeMap::new();
        let mut launchers = BTreeMap::new();
        let mut transports = BTreeMap::new();
        let mut runtimes = BTreeMap::new();
        let mut session_controls = BTreeMap::new();
        let mut file_release_discovery_claims = BTreeMap::new();

        for plugin in by_id.values() {
            registered_provider_ids
                .extend(plugin.providers.values().map(|record| record.id.clone()));
            registered_system_ids.extend(plugin.systems.values().map(|record| record.id.clone()));
            registered_launcher_ids
                .extend(plugin.launchers.values().map(|record| record.id.clone()));
            registered_transport_ids
                .extend(plugin.transports.values().map(|record| record.id.clone()));
            registered_runtime_ids.extend(plugin.runtimes.values().map(|record| record.id.clone()));
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
            for (local_id, record) in &plugin.launchers {
                insert_unique(
                    &mut launchers,
                    plugin_record_id(plugin_id, local_id),
                    record.clone(),
                )?;
            }
            for (local_id, record) in &plugin.transports {
                insert_unique(
                    &mut transports,
                    plugin_record_id(plugin_id, local_id),
                    record.clone(),
                )?;
            }
            for (local_id, record) in &plugin.runtimes {
                insert_unique(
                    &mut runtimes,
                    plugin_record_id(plugin_id, local_id),
                    record.clone(),
                )?;
            }
            for record in plugin.session_controls.values() {
                insert_unique(&mut session_controls, record.id.clone(), record.clone())?;
            }
        }

        let enabled_system_ids: BTreeSet<String> =
            systems.values().map(|record| record.id.clone()).collect();
        for plugin_id in &enabled_plugin_ids {
            let plugin = by_id
                .get(plugin_id)
                .expect("enabled plugin ids were validated above");
            for claim in plugin.file_release_discovery_claims.values() {
                validate_discovery_reference(
                    &registered_system_ids,
                    &claim.id,
                    "system",
                    &claim.system,
                )?;
                validate_discovery_reference(
                    &registered_launcher_ids,
                    &claim.id,
                    "launcher",
                    &claim.launcher,
                )?;
                if let Some(runtime) = &claim.runtime {
                    validate_discovery_reference(
                        &registered_runtime_ids,
                        &claim.id,
                        "runtime",
                        runtime,
                    )?;
                }

                let references_enabled = enabled_system_ids.contains(&claim.system)
                    && launchers.contains_key(&claim.launcher)
                    && claim
                        .runtime
                        .as_ref()
                        .is_none_or(|runtime| runtimes.contains_key(runtime));
                if references_enabled {
                    insert_unique(
                        &mut file_release_discovery_claims,
                        claim.id.clone(),
                        claim.clone(),
                    )?;
                }
            }
        }

        Ok(Self {
            installed: BTreeMap::new(),
            plugins: by_id,
            enabled_plugin_ids,
            registered_provider_ids,
            registered_system_ids,
            registered_launcher_ids,
            registered_transport_ids,
            registered_runtime_ids,
            registered_session_controls,
            providers,
            systems,
            launchers,
            transports,
            runtimes,
            session_controls,
            file_release_discovery_claims,
        })
    }

    pub fn from_installed(
        packages: Vec<crate::plugin_installation::EnabledPackage>,
    ) -> Result<Self, PluginError> {
        let mut plugins = Vec::new();
        let mut installed = BTreeMap::new();
        let mut declarations = Vec::new();
        for package in packages {
            let (namespace, _) = package
                .id
                .split_once(':')
                .ok_or_else(|| PluginError::InvalidPluginId(package.id.clone()))?;
            let source = std::fs::read_to_string(package.package.join("plugin.ts"))
                .map_err(|e| PluginError::Evaluation(e.to_string()))?;
            let json = script::eval_plugin_ts(&source).map_err(PluginError::Evaluation)?;
            let declaration: serde_json::Value = serde_json::from_str(&json)?;
            crate::plugin_references::validate_files(declaration.clone(), &package.files)
                .map_err(PluginError::Evaluation)?;
            declarations.push((package.id.clone(), declaration));
            let plugin = decode_plugin_declaration(namespace, &json)?;
            if plugin.id != package.id {
                return Err(PluginError::InvalidPluginId(package.id));
            }
            if installed.insert(package.id.clone(), package).is_some() {
                return Err(PluginError::DuplicatePluginId(plugin.id));
            }
            plugins.push(plugin);
        }
        crate::plugin_references::validate(declarations).map_err(PluginError::Evaluation)?;
        for package in installed.values() {
            for required in &package.requires {
                if !installed
                    .values()
                    .any(|candidate| candidate.package == *required)
                {
                    return Err(PluginError::Evaluation(format!(
                        "{} requires unavailable selection {}",
                        package.id,
                        required.display()
                    )));
                }
            }
        }
        let mut registry = Self::new(plugins, installed.keys().cloned())?;
        registry.installed = installed;
        for launcher in registry
            .launchers
            .values()
            .filter(|launcher| launcher.kind.is_some())
        {
            registry.native_launcher(&launcher.id)?;
        }
        for runtime in registry
            .runtimes
            .values()
            .filter(|runtime| runtime.launcher.is_some())
        {
            registry.native_launcher(runtime.launcher.as_deref().unwrap())?;
            registry.installed_file(&runtime.id, &runtime.path)?;
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

    pub fn native_launcher(
        &self,
        id: &str,
    ) -> Result<(&LauncherRecord, &LauncherRecord), PluginError> {
        let instance = self
            .launchers
            .get(id)
            .ok_or_else(|| PluginError::Evaluation(format!("launcher {id} is unavailable")))?;
        let kind_id = instance
            .kind
            .as_ref()
            .ok_or_else(|| PluginError::Evaluation(format!("launcher {id} has no native kind")))?;
        let kind = self.launchers.get(kind_id).ok_or_else(|| {
            PluginError::Evaluation(format!("launcher {id} requires missing kind {kind_id}"))
        })?;
        if kind.kind.as_deref() != Some(kind_id) {
            return Err(PluginError::Evaluation(format!(
                "launcher kind {kind_id} is not a kind's own instance"
            )));
        }
        self.installed_file(
            id,
            instance
                .program
                .as_deref()
                .ok_or_else(|| PluginError::Evaluation(format!("launcher {id} has no program")))?,
        )?;
        Ok((instance, kind))
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

    pub fn owns_registered_launcher_id(&self, id: &str) -> bool {
        self.registered_launcher_ids.contains(id)
    }

    pub fn owns_registered_transport_id(&self, id: &str) -> bool {
        self.registered_transport_ids.contains(id)
    }

    pub fn owns_registered_runtime_id(&self, id: &str) -> bool {
        self.registered_runtime_ids.contains(id)
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

    pub fn launchers(&self) -> &BTreeMap<String, LauncherRecord> {
        &self.launchers
    }

    pub fn transports(&self) -> &BTreeMap<String, TransportRecord> {
        &self.transports
    }

    pub fn runtimes(&self) -> &BTreeMap<String, RuntimeRecord> {
        &self.runtimes
    }

    pub fn session_controls(&self) -> &BTreeMap<String, SessionControlRecord> {
        &self.session_controls
    }

    pub fn file_release_discovery_claims(&self) -> &BTreeMap<String, FileReleaseDiscoveryClaim> {
        &self.file_release_discovery_claims
    }

    pub fn file_release_discovery_claims_for_extension(
        &self,
        extension: &str,
    ) -> Vec<&FileReleaseDiscoveryClaim> {
        let Ok(extension) = normalize_extension(extension) else {
            return Vec::new();
        };
        self.file_release_discovery_claims
            .values()
            .filter(|claim| {
                claim
                    .extensions
                    .iter()
                    .any(|candidate| candidate == &extension)
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
    #[error("discovery claim {claim_id} references unknown {kind} {referenced_id}")]
    UnknownDiscoveryReference {
        claim_id: String,
        kind: &'static str,
        referenced_id: String,
    },
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
    launchers: BTreeMap<String, LauncherRecord>,
    #[serde(default)]
    transports: BTreeMap<String, TransportRecord>,
    #[serde(default)]
    runtimes: BTreeMap<String, RuntimeRecord>,
    #[serde(default, rename = "sessionControls")]
    session_controls: BTreeMap<String, SessionControlRecord>,
    #[serde(default)]
    discovery: PluginDiscoveryContributions,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    config: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    android: Option<BTreeMap<String, serde_json::Value>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginDiscoveryContributions {
    #[serde(default)]
    file_releases: BTreeMap<String, FileReleaseDiscoveryContribution>,
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
    }

    for (local_id, launcher) in &declaration.launchers {
        if local_id.is_empty() {
            return Err(PluginError::EmptyContributionId { kind: "launcher" });
        }
        let expected_id = plugin_record_id(&id, local_id);
        if launcher.id != expected_id {
            return Err(PluginError::InvalidContribution {
                kind: "launcher",
                record_id: local_id.clone(),
                reason: format!("record id {} must be {expected_id}", launcher.id),
            });
        }
        if let Some(provider_id) = &launcher.plugin {
            if !is_provider_id(provider_id) {
                return Err(PluginError::InvalidPluginId(provider_id.clone()));
            }
        }
        if launcher.command.as_deref() == Some("") {
            return Err(PluginError::EmptyContributionId {
                kind: "launcher command",
            });
        }
        if let Some(android) = &launcher.android {
            if !is_android_identifier(&android.package_name, false)
                || !is_android_identifier(&android.class_name, true)
            {
                return Err(PluginError::InvalidContribution {
                    kind: "launcher",
                    record_id: local_id.clone(),
                    reason: "Android package and class names must be fully qualified identifiers"
                        .to_owned(),
                });
            }
        }
        if launcher.kind.is_some() != launcher.program.is_some()
            || (launcher.kind.is_some()
                && (launcher.android.is_some()
                    || launcher.command.is_some()
                    || launcher.plugin.is_some()))
            || launcher.program.as_deref() == Some("")
        {
            return Err(PluginError::InvalidContribution {
                kind: "launcher",
                record_id: local_id.clone(),
                reason: "native launchers require kind and program file key, without Android integration fields".to_owned(),
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
        if transport
            .android
            .as_ref()
            .is_some_and(|android| android.sunshine_app.trim().is_empty())
        {
            return Err(PluginError::InvalidContribution {
                kind: "transport",
                record_id: local_id.clone(),
                reason: "Android Sunshine app must not be empty".to_owned(),
            });
        }
    }

    for (local_id, runtime) in &declaration.runtimes {
        if local_id.is_empty() {
            return Err(PluginError::EmptyContributionId { kind: "runtime" });
        }
        let expected_id = plugin_record_id(&id, local_id);
        if runtime.id != expected_id {
            return Err(PluginError::InvalidContribution {
                kind: "runtime",
                record_id: local_id.clone(),
                reason: format!("record id {} must be {expected_id}", runtime.id),
            });
        }
        if runtime.kind.is_empty()
            || match &runtime.launcher {
                Some(launcher) => {
                    launcher.is_empty() || runtime.app.is_some() || runtime.path.is_empty()
                }
                None => {
                    runtime.app.as_deref().is_none_or(str::is_empty)
                        || !is_safe_absolute_path(&runtime.path)
                }
            }
        {
            return Err(PluginError::InvalidContribution {
                kind: "runtime",
                record_id: local_id.clone(),
                reason: "runtime requires a kind, and either Android app/absolute path or native launcher/file key"
                    .to_owned(),
            });
        }
    }

    let owned_launchers: BTreeSet<String> = declaration
        .launchers
        .values()
        .map(|record| record.id.clone())
        .collect();
    let owned_transports: BTreeSet<String> = declaration
        .transports
        .values()
        .map(|record| record.id.clone())
        .collect();
    let owned_runtimes: BTreeSet<String> = declaration
        .runtimes
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
            SessionControlOwnerKind::Launcher => owned_launchers.contains(&control.owner.id),
            SessionControlOwnerKind::Transport => owned_transports.contains(&control.owner.id),
            SessionControlOwnerKind::Runtime => owned_runtimes.contains(&control.owner.id),
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
        if control.effect.plugin_id() != id {
            return Err(PluginError::InvalidContribution {
                kind: "session control",
                record_id: local_id.clone(),
                reason: "effect belongs to another integration".to_owned(),
            });
        }
        validate_session_control(local_id, control)?;
        control.plugin_id = id.clone();
        control.local_id = local_id.clone();
    }

    let mut file_release_discovery_claims = BTreeMap::new();
    for (local_id, claim) in declaration.discovery.file_releases {
        if local_id.is_empty() {
            return Err(PluginError::EmptyContributionId {
                kind: "discovery file release",
            });
        }
        let expected_id = plugin_record_id(&id, &local_id);
        if claim.id != expected_id {
            return Err(PluginError::InvalidContribution {
                kind: "discovery file release",
                record_id: local_id,
                reason: format!("record id {} must be {expected_id}", claim.id),
            });
        }
        if claim.system.is_empty()
            || claim.launcher.is_empty()
            || claim.runtime.as_deref() == Some("")
        {
            return Err(PluginError::InvalidContribution {
                kind: "discovery file release",
                record_id: claim.id,
                reason: "system, launcher, and runtime references must be non-empty".to_owned(),
            });
        }
        let mut extensions = Vec::new();
        let mut seen_extensions = BTreeSet::new();
        for extension in claim.extensions {
            let normalized = normalize_extension(&extension).map_err(|reason| {
                PluginError::InvalidContribution {
                    kind: "discovery file release",
                    record_id: claim.id.clone(),
                    reason,
                }
            })?;
            if !seen_extensions.insert(normalized.clone()) {
                return Err(PluginError::InvalidContribution {
                    kind: "discovery file release",
                    record_id: claim.id,
                    reason: format!("duplicate normalized extension {normalized}"),
                });
            }
            extensions.push(normalized);
        }
        if extensions.is_empty() {
            return Err(PluginError::InvalidContribution {
                kind: "discovery file release",
                record_id: claim.id,
                reason: "at least one extension is required".to_owned(),
            });
        }
        file_release_discovery_claims.insert(
            expected_id.clone(),
            FileReleaseDiscoveryClaim {
                id: expected_id,
                title: claim.title,
                extensions,
                system: claim.system,
                launcher: claim.launcher,
                runtime: claim.runtime,
            },
        );
    }

    Ok(Plugin {
        config: declaration.config,
        android: declaration.android,
        id,
        title,
        description: declaration.description,
        providers,
        systems: declaration.systems,
        launchers: declaration.launchers,
        transports: declaration.transports,
        runtimes: declaration.runtimes,
        session_controls: declaration.session_controls,
        file_release_discovery_claims,
    })
}

fn validate_discovery_reference(
    registered_ids: &BTreeSet<String>,
    claim_id: &str,
    kind: &'static str,
    referenced_id: &str,
) -> Result<(), PluginError> {
    if registered_ids.contains(referenced_id) {
        Ok(())
    } else {
        Err(PluginError::UnknownDiscoveryReference {
            claim_id: claim_id.to_owned(),
            kind,
            referenced_id: referenced_id.to_owned(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SessionControlKind {
    Command,
    Toggle,
    Choice,
    Range,
}

impl SessionControlEffect {
    fn control_kind(self) -> SessionControlKind {
        match self {
            Self::MoonlightSetFillMode
            | Self::MoonlightSetZoomMode
            | Self::MoonlightSetFaceButtonFlip
            | Self::MoonlightSetRumble
            | Self::MoonlightSetPictureInPicture => SessionControlKind::Toggle,
            Self::MoonlightSetMouseMode => SessionControlKind::Choice,
            Self::MoonlightSetSgsrEdgeThreshold
            | Self::MoonlightSetSgsrSharpness
            | Self::MoonlightSetStreamBitrateKbps
            | Self::MoonlightSetStreamFps
            | Self::MoonlightSetStreamWidth => SessionControlKind::Range,
            Self::RetroarchOpenMenu
            | Self::RetroarchQuit
            | Self::MoonlightDisconnect
            | Self::MoonlightQuitHost
            | Self::MoonlightToggleKeyboard
            | Self::MoonlightToggleFullKeyboard
            | Self::MoonlightRotateScreen
            | Self::MoonlightToggleHud
            | Self::MoonlightToggleFloatingMenu
            | Self::MoonlightToggleKeyboardController
            | Self::MoonlightSwitchTouchSensitivity
            | Self::MoonlightSetLocalCursor
            | Self::MoonlightRestoreStreamBitrate
            | Self::MoonlightRestoreStreamFps
            | Self::MoonlightRestoreStreamResolution => SessionControlKind::Command,
        }
    }
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
    if declared_kind != control.effect.control_kind() {
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

fn normalize_extension(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    let normalized = trimmed
        .strip_prefix('.')
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    if normalized.is_empty() {
        return Err("extension must not be empty".to_owned());
    }
    if normalized
        .chars()
        .all(|character| character.is_ascii_alphanumeric())
    {
        Ok(normalized)
    } else {
        Err(format!("extension {value} contains unsupported characters"))
    }
}

fn is_android_identifier(value: &str, allow_dollar: bool) -> bool {
    value.split('.').all(|segment| {
        let mut chars = segment.chars();
        matches!(chars.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
            && chars.all(|character| {
                character.is_ascii_alphanumeric()
                    || character == '_'
                    || (allow_dollar && character == '$')
            })
    }) && value.contains('.')
}

fn is_safe_absolute_path(value: &str) -> bool {
    if value
        .chars()
        .any(|character| character.is_control() || matches!(character, '"' | '\\'))
    {
        return false;
    }
    let path = Path::new(value);
    path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::RootDir | Component::Normal(_)))
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
