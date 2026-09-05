use hmac::{Hmac, Mac};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::Sha256;
use std::collections::{HashMap, HashSet};
use typeshare::typeshare;

use crate::{GameIdentity, SessionControlValue};

type HmacSha256 = Hmac<Sha256>;
const RETROARCH_CONTROL_CONTEXT: &[u8] = b"korri-retroarch-control-v3";
const RETROARCH_CONTROL_PORT_CONTEXT: &[u8] = b"korri-retroarch-control-port-v1";
const RETROARCH_CONTROL_PORT_BASE: u16 = 49152;
const RETROARCH_CONTROL_PORT_COUNT: u16 = 16384;

pub(crate) fn derive_retroarch_control_port(key: &[u8], launch_id: &str) -> u16 {
    let mut control_mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    control_mac.update(RETROARCH_CONTROL_PORT_CONTEXT);
    control_mac.update(launch_id.as_bytes());
    let bytes = control_mac.finalize().into_bytes();
    let offset = u16::from_be_bytes([bytes[0], bytes[1]]) % RETROARCH_CONTROL_PORT_COUNT;
    RETROARCH_CONTROL_PORT_BASE + offset
}

fn deserialize_optional_non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LocalGame {
    pub id: String,
    pub title: String,
    pub system: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<GameIdentity>,
    #[serde(
        default,
        rename = "coverAssetId",
        skip_serializing_if = "Option::is_none"
    )]
    pub cover_asset_id: Option<String>,
    #[serde(default, rename = "playStats", skip_serializing_if = "Option::is_none")]
    pub play_stats: Option<crate::play_log::PlayStats>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AndroidComponent {
    pub package_name: String,
    pub class_name: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionedFile {
    pub path: String,
    pub content: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LaunchContributorKind {
    Launcher,
    Transport,
    Runtime,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchRouteContributor {
    pub kind: LaunchContributorKind,
    pub id: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchExecutor {
    pub id: String,
    pub available: bool,
}

#[typeshare]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LaunchForegroundKind {
    Component,
    Package,
    /** Java resolves this marker to its own package plus Game component. */
    ArtemisGame,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LaunchForegroundRule {
    pub kind: LaunchForegroundKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class_name: Option<String>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LaunchContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /** CRC32 of the exact prepared content bytes; SHA identity remains separate. */
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_crc32: Option<String>,
    pub contributors: Vec<LaunchRouteContributor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor: Option<LaunchExecutor>,
    pub foreground: LaunchForegroundRule,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AndroidActiveLaunch {
    pub launch_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_crc32: Option<String>,
    pub contributors: Vec<LaunchRouteContributor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor: Option<LaunchExecutor>,
    pub foreground: LaunchForegroundRule,
}

impl AndroidActiveLaunch {
    pub fn from_context(launch_id: String, context: LaunchContext) -> Self {
        Self {
            launch_id,
            game_id: context.game_id,
            title: context.title,
            content_crc32: context.content_crc32,
            contributors: context.contributors,
            executor: context.executor,
            foreground: context.foreground,
        }
    }
}

impl LaunchContext {
    pub(crate) fn unresolved() -> Self {
        Self {
            game_id: None,
            title: None,
            content_crc32: None,
            contributors: Vec::new(),
            executor: None,
            foreground: LaunchForegroundRule {
                kind: LaunchForegroundKind::Package,
                package_name: Some(String::new()),
                class_name: None,
            },
        }
    }
}

#[typeshare]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LaunchDisposition {
    Fresh,
    Resume,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LaunchSpec {
    /** Identity created by korrid while preparing this exact launch. */
    pub launch_id: String,
    pub launcher_id: String,
    pub disposition: LaunchDisposition,
    pub context: LaunchContext,
    pub component: AndroidComponent,
    pub extras: HashMap<String, String>,
    pub directories: Vec<String>,
    pub files: Vec<ProvisionedFile>,
    #[serde(
        default,
        rename = "authorizedContentRoot",
        skip_serializing_if = "Option::is_none"
    )]
    pub authorized_content_root: Option<String>,
    /// Per-server HMAC. The portal transports it opaquely; native verifies it.
    pub integrity: String,
}

impl LaunchSpec {
    fn signing_bytes(&self) -> Vec<u8> {
        fn push(value: &str, bytes: &mut Vec<u8>) {
            bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
            bytes.extend_from_slice(value.as_bytes());
        }

        let mut bytes = Vec::new();
        push(&self.launch_id, &mut bytes);
        push(&self.launcher_id, &mut bytes);
        push(
            match self.disposition {
                LaunchDisposition::Fresh => "fresh",
                LaunchDisposition::Resume => "resume",
            },
            &mut bytes,
        );
        push(
            &serde_json::to_string(&self.context).expect("launch context serializes"),
            &mut bytes,
        );
        push(&self.component.package_name, &mut bytes);
        push(&self.component.class_name, &mut bytes);
        let mut extras: Vec<_> = self.extras.iter().collect();
        extras.sort_unstable_by_key(|(key, _)| *key);
        bytes.extend_from_slice(&(extras.len() as u64).to_be_bytes());
        for (key, value) in extras {
            push(key, &mut bytes);
            push(value, &mut bytes);
        }
        bytes.extend_from_slice(&(self.directories.len() as u64).to_be_bytes());
        for directory in &self.directories {
            push(directory, &mut bytes);
        }
        bytes.extend_from_slice(&(self.files.len() as u64).to_be_bytes());
        for file in &self.files {
            push(&file.path, &mut bytes);
            push(&file.content, &mut bytes);
        }
        push(
            self.authorized_content_root.as_deref().unwrap_or(""),
            &mut bytes,
        );
        bytes
    }

    pub(crate) fn with_launch_id(mut self, launch_id: String) -> Self {
        self.launch_id = launch_id;
        self
    }

    pub(crate) fn with_context(mut self, context: LaunchContext) -> Self {
        self.context = context;
        self
    }

    pub(crate) fn with_disposition(mut self, disposition: LaunchDisposition) -> Self {
        self.disposition = disposition;
        self
    }

    pub(crate) fn retroarch_control_token(&self, key: &[u8]) -> Option<String> {
        if self.launcher_id != "retroarch" || self.launch_id.is_empty() {
            return None;
        }
        let mut control_mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
        control_mac.update(RETROARCH_CONTROL_CONTEXT);
        control_mac.update(self.launch_id.as_bytes());
        Some(hex::encode(control_mac.finalize().into_bytes()))
    }

    pub(crate) fn retroarch_control_port(&self, key: &[u8]) -> Option<u16> {
        if self.launcher_id != "retroarch" || self.launch_id.is_empty() {
            return None;
        }
        Some(derive_retroarch_control_port(key, &self.launch_id))
    }

    pub(crate) fn sign(mut self, key: &[u8]) -> Self {
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
        mac.update(&self.signing_bytes());
        self.integrity = hex::encode(mac.finalize().into_bytes());
        self
    }

    pub(crate) fn verify(&self, key: &[u8]) -> bool {
        let Ok(signature) = hex::decode(&self.integrity) else {
            return false;
        };
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
        mac.update(&self.signing_bytes());
        mac.verify_slice(&signature).is_ok()
    }
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoonlightLaunchSpec {
    /** Fresh identity created by korrid for this native stream startup. */
    pub launch_id: String,
    pub transport_id: String,
    pub context: LaunchContext,
    pub implementation: crate::MoonlightImplementation,
    pub sunshine_app: String,
    pub host_uuid: String,
    pub app_id: u32,
    /** Per-server HMAC. The portal transports it opaquely; native consumes it once. */
    pub integrity: String,
}

impl MoonlightLaunchSpec {
    fn signing_bytes(&self) -> Vec<u8> {
        fn push(value: &[u8], bytes: &mut Vec<u8>) {
            bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
            bytes.extend_from_slice(value);
        }

        let mut bytes = Vec::new();
        push(self.launch_id.as_bytes(), &mut bytes);
        push(self.transport_id.as_bytes(), &mut bytes);
        push(
            &serde_json::to_vec(&self.context).expect("launch context serializes"),
            &mut bytes,
        );
        push(
            &serde_json::to_vec(&self.implementation).expect("Moonlight implementation serializes"),
            &mut bytes,
        );
        push(self.sunshine_app.as_bytes(), &mut bytes);
        push(self.host_uuid.as_bytes(), &mut bytes);
        push(&self.app_id.to_be_bytes(), &mut bytes);
        bytes
    }

    fn sign(mut self, key: &[u8]) -> Self {
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
        mac.update(&self.signing_bytes());
        self.integrity = hex::encode(mac.finalize().into_bytes());
        self
    }

    pub(crate) fn verify(&self, key: &[u8]) -> bool {
        let Ok(signature) = hex::decode(&self.integrity) else {
            return false;
        };
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
        mac.update(&self.signing_bytes());
        mac.verify_slice(&signature).is_ok()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MoonlightLaunchVerificationFailure {
    Integrity,
    Stale,
    Replay,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LaunchPublicationState {
    Reserved,
    Started,
    Published,
}

struct CurrentLaunchPublication {
    id: String,
    state: LaunchPublicationState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchPublicationReservationFailure {
    Stale,
    Replay,
    NotStarted,
    AlreadyPublished,
}

/** Latest-only reservation shared by native verification and publication. */
pub struct LaunchPublicationReservations {
    current: Option<CurrentLaunchPublication>,
}

impl LaunchPublicationReservations {
    pub fn new() -> Self {
        Self { current: None }
    }

    pub fn reserve(&mut self, launch_id: impl Into<String>) {
        self.current = Some(CurrentLaunchPublication {
            id: launch_id.into(),
            state: LaunchPublicationState::Reserved,
        });
    }

    pub fn cancel(&mut self, launch_id: &str) -> bool {
        let cancellable = self.current.as_ref().is_some_and(|current| {
            current.id == launch_id && current.state == LaunchPublicationState::Reserved
        });
        if cancellable {
            self.current = None;
        }
        cancellable
    }

    pub fn authorize(
        &mut self,
        launch_id: &str,
    ) -> Result<(), LaunchPublicationReservationFailure> {
        let Some(current) = self.current.as_mut() else {
            return Err(LaunchPublicationReservationFailure::Stale);
        };
        if current.id != launch_id {
            return Err(LaunchPublicationReservationFailure::Stale);
        }
        if current.state != LaunchPublicationState::Reserved {
            return Err(LaunchPublicationReservationFailure::Replay);
        }
        current.state = LaunchPublicationState::Started;
        Ok(())
    }

    pub fn publish(&mut self, launch_id: &str) -> Result<(), LaunchPublicationReservationFailure> {
        let Some(current) = self.current.as_mut() else {
            return Err(LaunchPublicationReservationFailure::Stale);
        };
        if current.id != launch_id {
            return Err(LaunchPublicationReservationFailure::Stale);
        }
        match current.state {
            LaunchPublicationState::Reserved => {
                Err(LaunchPublicationReservationFailure::NotStarted)
            }
            LaunchPublicationState::Started => {
                current.state = LaunchPublicationState::Published;
                Ok(())
            }
            LaunchPublicationState::Published => {
                Err(LaunchPublicationReservationFailure::AlreadyPublished)
            }
        }
    }
}

/** Per-server signer and one-use native verifier for Moonlight startup. */
pub struct MoonlightLaunchAuthority {
    key: Vec<u8>,
    reservations: LaunchPublicationReservations,
}

impl MoonlightLaunchAuthority {
    pub fn new(key: Vec<u8>) -> Self {
        Self {
            key,
            reservations: LaunchPublicationReservations::new(),
        }
    }

    pub fn prepare(
        &mut self,
        transport_id: impl Into<String>,
        implementation: crate::MoonlightImplementation,
        sunshine_app: impl Into<String>,
        host_uuid: impl Into<String>,
        app_id: u32,
        game_id: Option<String>,
        title: Option<String>,
    ) -> MoonlightLaunchSpec {
        let launch_id = hex::encode(rand::random::<[u8; 16]>());
        let transport_id = transport_id.into();
        let spec = MoonlightLaunchSpec {
            launch_id: launch_id.clone(),
            context: LaunchContext {
                game_id,
                title,
                content_crc32: None,
                contributors: vec![LaunchRouteContributor {
                    kind: LaunchContributorKind::Transport,
                    id: transport_id.clone(),
                }],
                // U6 installs the live Game executor. U4 carries its stable
                // identity but truthfully reports it unavailable.
                executor: Some(LaunchExecutor {
                    id: "android-moonlight".into(),
                    available: false,
                }),
                foreground: LaunchForegroundRule {
                    kind: LaunchForegroundKind::ArtemisGame,
                    package_name: None,
                    class_name: None,
                },
            },
            transport_id,
            implementation,
            sunshine_app: sunshine_app.into(),
            host_uuid: host_uuid.into(),
            app_id,
            integrity: String::new(),
        }
        .sign(&self.key);
        self.reservations.reserve(launch_id);
        spec
    }

    /** Invalidate only the named current reservation while it remains unused. */
    pub fn cancel(&mut self, launch_id: &str) -> bool {
        self.reservations.cancel(launch_id)
    }

    pub fn authorize(
        &mut self,
        spec: &MoonlightLaunchSpec,
    ) -> Result<(), MoonlightLaunchVerificationFailure> {
        if !spec.verify(&self.key) {
            return Err(MoonlightLaunchVerificationFailure::Integrity);
        }
        self.reservations
            .authorize(&spec.launch_id)
            .map_err(|failure| match failure {
                LaunchPublicationReservationFailure::Stale => {
                    MoonlightLaunchVerificationFailure::Stale
                }
                LaunchPublicationReservationFailure::Replay
                | LaunchPublicationReservationFailure::NotStarted
                | LaunchPublicationReservationFailure::AlreadyPublished => {
                    MoonlightLaunchVerificationFailure::Replay
                }
            })
    }

    pub fn publish(
        &mut self,
        spec: &MoonlightLaunchSpec,
    ) -> Result<(), LaunchPublicationReservationFailure> {
        if !spec.verify(&self.key) {
            return Err(LaunchPublicationReservationFailure::Stale);
        }
        self.reservations.publish(&spec.launch_id)
    }
}

#[typeshare]
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AndroidMoonlightEffect {
    Disconnect,
    QuitHost,
    ToggleKeyboard,
    ToggleFullKeyboard,
    SetFillMode,
    SetZoomMode,
    RotateScreen,
    ToggleHud,
    ToggleFloatingMenu,
    ToggleKeyboardController,
    SwitchTouchSensitivity,
    SetMouseMode,
    SetLocalCursor,
    SetSgsrEdgeThreshold,
    SetSgsrSharpness,
    SetFaceButtonFlip,
    SetRumble,
    SetPictureInPicture,
    SetStreamBitrateKbps,
    RestoreStreamBitrate,
    SetStreamFps,
    RestoreStreamFps,
    SetStreamWidth,
    RestoreStreamResolution,
}

/** Closed platform effect vocabulary. Plugins may refer only to integrations
 * represented here; no process, URL, intent, socket, or method name crosses. */
#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    content = "payload",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum PlatformEffect {
    AndroidMoonlight(AndroidMoonlightEffect),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlatformInstruction {
    pub launch_id: String,
    pub executor_id: String,
    pub generation: String,
    pub action_id: String,
    pub dismiss_on_success: bool,
    /** Cryptographically random, consumed once for the active launch. */
    pub nonce: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_non_null"
    )]
    pub value: Option<SessionControlValue>,
    pub effect: PlatformEffect,
    /** Per-server HMAC verified at the native platform edge. */
    pub integrity: String,
}

impl PlatformInstruction {
    fn signing_bytes(&self) -> Vec<u8> {
        fn push(value: &[u8], bytes: &mut Vec<u8>) {
            bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
            bytes.extend_from_slice(value);
        }

        let mut bytes = Vec::new();
        push(self.launch_id.as_bytes(), &mut bytes);
        push(self.executor_id.as_bytes(), &mut bytes);
        push(self.generation.as_bytes(), &mut bytes);
        push(self.action_id.as_bytes(), &mut bytes);
        push(&[u8::from(self.dismiss_on_success)], &mut bytes);
        push(self.nonce.as_bytes(), &mut bytes);
        push(
            &serde_json::to_vec(&self.value).expect("control values serialize"),
            &mut bytes,
        );
        push(
            &serde_json::to_vec(&self.effect).expect("platform effects serialize"),
            &mut bytes,
        );
        bytes
    }

    pub fn protect(
        launch_id: impl Into<String>,
        executor_id: impl Into<String>,
        generation: impl Into<String>,
        action_id: impl Into<String>,
        dismiss_on_success: bool,
        value: Option<SessionControlValue>,
        effect: PlatformEffect,
        key: &[u8],
    ) -> Self {
        let nonce: [u8; 32] = rand::random();
        let mut instruction = Self {
            launch_id: launch_id.into(),
            executor_id: executor_id.into(),
            generation: generation.into(),
            action_id: action_id.into(),
            dismiss_on_success,
            nonce: hex::encode(nonce),
            value,
            effect,
            integrity: String::new(),
        };
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
        mac.update(&instruction.signing_bytes());
        instruction.integrity = hex::encode(mac.finalize().into_bytes());
        instruction
    }

    fn verify(&self, key: &[u8]) -> bool {
        let Ok(signature) = hex::decode(&self.integrity) else {
            return false;
        };
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
        mac.update(&self.signing_bytes());
        mac.verify_slice(&signature).is_ok()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformInstructionVerificationFailure {
    Integrity,
    StaleSession,
    Replay,
}

/** Stateful native-edge authorization: integrity and launch binding are
 * checked before a nonce is atomically marked consumed. */
pub struct PlatformInstructionVerifier {
    active_launch_id: String,
    consumed_nonces: HashSet<String>,
}

impl PlatformInstructionVerifier {
    pub fn new(active_launch_id: impl Into<String>) -> Self {
        Self {
            active_launch_id: active_launch_id.into(),
            consumed_nonces: HashSet::new(),
        }
    }

    pub fn verify_current(
        &self,
        instruction: &PlatformInstruction,
        key: &[u8],
    ) -> Result<(), PlatformInstructionVerificationFailure> {
        if !instruction.verify(key) {
            return Err(PlatformInstructionVerificationFailure::Integrity);
        }
        if instruction.launch_id != self.active_launch_id {
            return Err(PlatformInstructionVerificationFailure::StaleSession);
        }
        if self.consumed_nonces.contains(&instruction.nonce) {
            return Err(PlatformInstructionVerificationFailure::Replay);
        }
        Ok(())
    }

    pub fn consume_nonce(
        &mut self,
        instruction: &PlatformInstruction,
    ) -> Result<(), PlatformInstructionVerificationFailure> {
        if !self.consumed_nonces.insert(instruction.nonce.clone()) {
            return Err(PlatformInstructionVerificationFailure::Replay);
        }
        Ok(())
    }

    pub fn authorize(
        &mut self,
        instruction: &PlatformInstruction,
        key: &[u8],
    ) -> Result<(), PlatformInstructionVerificationFailure> {
        self.verify_current(instruction, key)?;
        self.consume_nonce(instruction)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileProvisionMode {
    Direct,
    Deferred,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> LaunchSpec {
        LaunchSpec {
            launch_id: "launch-1".into(),
            launcher_id: "retroarch".into(),
            disposition: LaunchDisposition::Fresh,
            context: LaunchContext::unresolved(),
            component: AndroidComponent {
                package_name: "package".into(),
                class_name: "Activity".into(),
            },
            extras: HashMap::from([("ROM".into(), "/root/rom.gba".into())]),
            directories: vec!["/root/saves".into()],
            files: vec![ProvisionedFile {
                path: "/root/config".into(),
                content: "config".into(),
            }],
            authorized_content_root: Some("/root".into()),
            integrity: String::new(),
        }
    }

    #[test]
    fn launch_publication_reservation_is_latest_started_and_one_use() {
        let mut reservations = LaunchPublicationReservations::new();
        reservations.reserve("older");
        reservations.reserve("latest");

        assert_eq!(
            reservations.authorize("older"),
            Err(LaunchPublicationReservationFailure::Stale)
        );
        assert_eq!(
            reservations.publish("latest"),
            Err(LaunchPublicationReservationFailure::NotStarted)
        );
        assert!(reservations.authorize("latest").is_ok());
        assert_eq!(
            reservations.authorize("latest"),
            Err(LaunchPublicationReservationFailure::Replay)
        );
        assert!(reservations.publish("latest").is_ok());
        assert_eq!(
            reservations.publish("latest"),
            Err(LaunchPublicationReservationFailure::AlreadyPublished)
        );
    }

    #[test]
    fn consumed_start_failure_stays_unpublished_until_a_replacement_is_reserved() {
        let mut reservations = LaunchPublicationReservations::new();
        reservations.reserve("failed-start");
        assert!(reservations.authorize("failed-start").is_ok());
        assert!(reservations.current.as_ref().is_some_and(|current| {
            current.id == "failed-start" && current.state == LaunchPublicationState::Started
        }));

        reservations.reserve("replacement");
        assert_eq!(
            reservations.publish("failed-start"),
            Err(LaunchPublicationReservationFailure::Stale)
        );
        assert!(reservations.authorize("replacement").is_ok());
        assert!(reservations.publish("replacement").is_ok());
    }

    #[test]
    fn signed_moonlight_launches_reject_tamper_stale_and_replay() {
        let mut authority = MoonlightLaunchAuthority::new(b"private per-server key".to_vec());
        let first = authority.prepare(
            "@korri:moonlight/moonlight",
            crate::MoonlightImplementation::Artemis,
            "Korri Stream",
            "host-uuid",
            7,
            None,
            None,
        );
        assert_eq!(first.transport_id, "@korri:moonlight/moonlight");
        assert_eq!(first.sunshine_app, "Korri Stream");
        assert_eq!(first.host_uuid, "host-uuid");
        assert_eq!(first.app_id, 7);
        assert_eq!(first.launch_id.len(), 32);

        let second = authority.prepare(
            "@korri:moonlight/moonlight",
            crate::MoonlightImplementation::Artemis,
            "Korri Stream",
            "host-uuid",
            7,
            None,
            None,
        );
        assert_ne!(second.launch_id, first.launch_id);
        assert_eq!(
            authority
                .reservations
                .current
                .as_ref()
                .map(|launch| (launch.id.as_str(), launch.state)),
            Some((second.launch_id.as_str(), LaunchPublicationState::Reserved))
        );
        assert_eq!(
            authority.authorize(&first),
            Err(MoonlightLaunchVerificationFailure::Stale)
        );

        let mut tampered = second.clone();
        tampered.app_id = 8;
        assert_eq!(
            authority.authorize(&tampered),
            Err(MoonlightLaunchVerificationFailure::Integrity)
        );
        assert!(authority.authorize(&second).is_ok());
        assert!(authority
            .reservations
            .current
            .as_ref()
            .is_some_and(|launch| launch.state == LaunchPublicationState::Started));
        assert_eq!(
            authority.authorize(&second),
            Err(MoonlightLaunchVerificationFailure::Replay)
        );
        assert!(authority.publish(&second).is_ok());
        assert_eq!(
            authority.publish(&second),
            Err(LaunchPublicationReservationFailure::AlreadyPublished)
        );
    }

    #[test]
    fn exact_unused_moonlight_reservation_can_be_cancelled() {
        let mut authority = MoonlightLaunchAuthority::new(b"private per-server key".to_vec());
        let reservation = authority.prepare(
            "@korri:moonlight/moonlight",
            crate::MoonlightImplementation::Artemis,
            "Korri Stream",
            "host-uuid",
            7,
            None,
            None,
        );

        assert!(authority.cancel(&reservation.launch_id));
        assert_eq!(
            authority.authorize(&reservation),
            Err(MoonlightLaunchVerificationFailure::Stale)
        );
        assert!(!authority.cancel(&reservation.launch_id));
    }

    #[test]
    fn cancelling_an_older_moonlight_reservation_preserves_its_replacement() {
        let mut authority = MoonlightLaunchAuthority::new(b"private per-server key".to_vec());
        let older = authority.prepare(
            "@korri:moonlight/moonlight",
            crate::MoonlightImplementation::Artemis,
            "Korri Stream",
            "host-uuid",
            7,
            None,
            None,
        );
        let replacement = authority.prepare(
            "@korri:moonlight/moonlight",
            crate::MoonlightImplementation::Artemis,
            "Korri Stream",
            "host-uuid",
            7,
            None,
            None,
        );

        assert!(!authority.cancel(&older.launch_id));
        assert!(authority.authorize(&replacement).is_ok());
        assert!(!authority.cancel(&replacement.launch_id));
        assert_eq!(
            authority.authorize(&replacement),
            Err(MoonlightLaunchVerificationFailure::Replay)
        );
    }

    #[test]
    fn protected_platform_instructions_reject_tampering_and_replay() {
        let key = b"private per-server key";
        let instruction = PlatformInstruction::protect(
            "launch-1",
            "android-moonlight",
            "generation-a",
            "fill",
            false,
            Some(SessionControlValue::Toggle(true)),
            PlatformEffect::AndroidMoonlight(AndroidMoonlightEffect::SetFillMode),
            key,
        );
        let mut verifier = PlatformInstructionVerifier::new("launch-1");
        assert!(verifier.authorize(&instruction, key).is_ok());
        assert_eq!(
            verifier.authorize(&instruction, key),
            Err(PlatformInstructionVerificationFailure::Replay)
        );
        assert_eq!(
            PlatformInstructionVerifier::new("replacement-launch").authorize(&instruction, key),
            Err(PlatformInstructionVerificationFailure::StaleSession)
        );

        let mutations: Vec<Box<dyn Fn(&mut PlatformInstruction)>> = vec![
            Box::new(|value| value.executor_id = "replacement".into()),
            Box::new(|value| value.generation = "generation-b".into()),
            Box::new(|value| value.action_id = "quit".into()),
            Box::new(|value| value.dismiss_on_success = true),
            Box::new(|value| value.value = Some(SessionControlValue::Toggle(false))),
            Box::new(|value| value.launch_id = "launch-2".into()),
            Box::new(|value| value.nonce = "other".into()),
            Box::new(|value| {
                value.effect = PlatformEffect::AndroidMoonlight(AndroidMoonlightEffect::SetZoomMode)
            }),
        ];
        for mutate in mutations {
            let mut tampered = instruction.clone();
            mutate(&mut tampered);
            assert_eq!(
                PlatformInstructionVerifier::new(&tampered.launch_id).authorize(&tampered, key),
                Err(PlatformInstructionVerificationFailure::Integrity)
            );
        }
    }

    #[test]
    fn protected_jni_records_reject_unknown_top_level_and_nested_fields() {
        let key = b"private per-server key";
        let local = spec()
            .with_context(LaunchContext {
                game_id: Some("game".into()),
                title: Some("Game".into()),
                content_crc32: Some("d6141609".into()),
                contributors: vec![LaunchRouteContributor {
                    kind: LaunchContributorKind::Launcher,
                    id: "@korri:retroarch/retroarch".into(),
                }],
                executor: Some(LaunchExecutor {
                    id: "retroarch-control".into(),
                    available: false,
                }),
                foreground: LaunchForegroundRule {
                    kind: LaunchForegroundKind::Package,
                    package_name: Some("com.retroarch".into()),
                    class_name: None,
                },
            })
            .sign(key);
        let local_json = serde_json::to_value(&local).unwrap();
        let local_round_trip: LaunchSpec = serde_json::from_value(local_json.clone()).unwrap();
        assert_eq!(local_round_trip.signing_bytes(), local.signing_bytes());

        let mut local_top = local_json.clone();
        local_top["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<LaunchSpec>(local_top).is_err());
        for path in ["component", "context"] {
            let mut value = local_json.clone();
            value[path]["unexpected"] = serde_json::json!(true);
            assert!(
                serde_json::from_value::<LaunchSpec>(value).is_err(),
                "{path}"
            );
        }
        let mut local_contributor = local_json.clone();
        local_contributor["context"]["contributors"][0]["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<LaunchSpec>(local_contributor).is_err());
        let mut local_file = local_json.clone();
        local_file["files"][0]["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<LaunchSpec>(local_file).is_err());
        for path in ["executor", "foreground"] {
            let mut value = local_json.clone();
            value["context"][path]["unexpected"] = serde_json::json!(true);
            assert!(
                serde_json::from_value::<LaunchSpec>(value).is_err(),
                "{path}"
            );
        }

        let mut authority = MoonlightLaunchAuthority::new(key.to_vec());
        let moonlight = authority.prepare(
            "@korri:moonlight/moonlight",
            crate::MoonlightImplementation::Artemis,
            "Korri Stream",
            "host-uuid",
            7,
            None,
            None,
        );
        let moonlight_json = serde_json::to_value(&moonlight).unwrap();
        let moonlight_round_trip: MoonlightLaunchSpec =
            serde_json::from_value(moonlight_json.clone()).unwrap();
        assert_eq!(
            moonlight_round_trip.signing_bytes(),
            moonlight.signing_bytes()
        );
        let mut moonlight_top = moonlight_json.clone();
        moonlight_top["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<MoonlightLaunchSpec>(moonlight_top).is_err());
        let mut moonlight_nested = moonlight_json;
        moonlight_nested["context"]["foreground"]["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<MoonlightLaunchSpec>(moonlight_nested).is_err());

        let instruction = PlatformInstruction::protect(
            "launch-1",
            "android-moonlight",
            "generation-a",
            "fill",
            false,
            Some(SessionControlValue::Toggle(true)),
            PlatformEffect::AndroidMoonlight(AndroidMoonlightEffect::SetFillMode),
            key,
        );
        let instruction_json = serde_json::to_value(&instruction).unwrap();
        let instruction_round_trip: PlatformInstruction =
            serde_json::from_value(instruction_json.clone()).unwrap();
        assert_eq!(
            instruction_round_trip.signing_bytes(),
            instruction.signing_bytes()
        );
        let mut instruction_top = instruction_json.clone();
        instruction_top["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<PlatformInstruction>(instruction_top).is_err());
        let mut instruction_effect = instruction_json.clone();
        instruction_effect["effect"]["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<PlatformInstruction>(instruction_effect).is_err());
        let mut instruction_value = instruction_json.clone();
        instruction_value["value"]["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<PlatformInstruction>(instruction_value).is_err());
        let mut null_value = instruction_json;
        null_value["value"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<PlatformInstruction>(null_value).is_err());
    }

    #[test]
    fn integrity_rejects_tampered_launcher_instructions() {
        let key = b"private per-server key";
        let signed = spec().sign(key);
        assert!(signed.verify(key));
        assert!(!signed.extras.contains_key("KORRI_CONTROL_TOKEN"));
        let control_token = signed.retroarch_control_token(key).unwrap();
        assert_eq!(control_token.len(), 64);
        assert_ne!(
            control_token,
            signed
                .retroarch_control_token(b"another server key")
                .unwrap()
        );
        let replacement = spec().with_launch_id("launch-2".into()).sign(key);
        assert_ne!(
            control_token,
            replacement.retroarch_control_token(key).unwrap()
        );

        let mut tampered_disposition = signed.clone();
        tampered_disposition.disposition = LaunchDisposition::Resume;
        assert!(!tampered_disposition.verify(key));

        let mut tampered_content = signed.clone();
        tampered_content.files[0].content = "kiosk_mode_enable = false".into();
        assert!(!tampered_content.verify(key));

        let mut tampered_crc32 = signed.clone();
        tampered_crc32.context.content_crc32 = Some("deadbeef".into());
        assert!(!tampered_crc32.verify(key));

        let mut tampered_extra = signed.clone();
        tampered_extra
            .extras
            .insert("QUITFOCUS".into(), "true".into());
        assert!(!tampered_extra.verify(key));
        let mut tampered_directory = signed;
        tampered_directory.directories[0] = "/outside".into();
        assert!(!tampered_directory.verify(key));
    }
}
