//! Embedded-capable korrid server core: contracts, dispatch, and lifecycle.

use axum::{
    extract::State,
    http::{header, HeaderMap, Method, StatusCode},
    routing::post,
    Json, Router,
};
use federation::coordinator::{
    Discovery, DiscoveryControl, DiscoveryInputs, DiscoveryTiming, FederationResources,
};
use serde::{Deserialize, Serialize};
use std::{
    net::{Ipv4Addr, SocketAddrV4, TcpListener as StdTcpListener},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    thread::JoinHandle,
};
use tokio::sync::oneshot;
use typeshare::typeshare;

pub mod authorization;
pub mod catalog_cli;
pub mod config;
pub mod discovery;
pub mod enrichment;
pub mod federation;
mod game_assets;
pub mod game_routes;
pub mod identity;
pub mod identity_cli;
mod peer_rpc;
pub mod play_log;
pub mod portal_access;
pub mod relay;
pub mod remote_signer;

pub use play_log::{PlayEntry, PlayLog};
use portal_access::{PortalAccess, PortalPermission};

fn installed_routes_unsupported() -> RpcFailure {
    RpcFailure {
        code: "OperationUnsupported".into(),
        message: "installed Linux routes are unavailable on this device".into(),
    }
}

pub const VERSION: &str = "korrid-v0";
const ANDROID_BUNDLED_PORTAL_ORIGIN: &str = "https://appassets.androidplatform.net";

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CatalogSnapshotRequest {}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub enum GameIdentity {
    Hash(String),
    Provider(GameProviderIdentity),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GameProviderIdentity {
    pub provider: String,
    #[serde(rename = "ref")]
    pub provider_ref: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_public_key: Option<String>,
    pub label: String,
    pub is_local: bool,
}

/// Derived, read-only view of one person's play history for one game.
/// Mirrors the legacy `PlayStats` record: never authored, always computed
/// from the play log. `lastPlayed` is absent when the game was never played.
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayStats {
    /// RFC 3339 UTC end time of the newest play, absent when never played.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_played: Option<String>,
    pub play_count: u32,
    pub total_playtime_seconds: f64,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<GameIdentity>,
    pub source: GameSource,
    /// The host's dynamic catalog producer supports installed-runner selection.
    /// Static host.toml commands do not, even when source.isLocal is true.
    pub supports_runner_selection: bool,
    /// Play statistics for the authenticated person who asked. A host
    /// derives them from its own play log; a brain forwards what the peer
    /// returned for the brain's own identity.
    #[serde(default, rename = "playStats", skip_serializing_if = "Option::is_none")]
    pub play_stats: Option<PlayStats>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CatalogHostFailure {
    pub host: String,
    pub code: String,
    pub message: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CatalogSnapshot {
    pub games: Vec<Game>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failures: Option<Vec<CatalogHostFailure>>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPrepareRequest {
    pub game_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPrepared {
    pub game_id: String,
    /** Identity created by korrid while preparing this exact launch. */
    pub launch_id: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MoonlightResolveRequest {}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoonlightLaunchPrepareRequest {
    pub host_uuid: String,
    pub app_id: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoonlightLaunchCancelRequest {
    pub launch_id: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoonlightLaunchCancelled {
    pub launch_id: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoonlightCertificateAttestRequest {
    pub host_uuid: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoonlightCertificateAttested {
    pub matched: bool,
}

#[typeshare]
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoonlightCertificateProvisionRequest {
    pub host_uuid: String,
    pub client_certificate: String,
}

impl std::fmt::Debug for MoonlightCertificateProvisionRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MoonlightCertificateProvisionRequest")
            .field("host_uuid", &self.host_uuid)
            .field("client_certificate", &"[redacted]")
            .finish()
    }
}

#[typeshare]
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoonlightCertificateProvisioned {
    pub server_certificate: String,
}

impl std::fmt::Debug for MoonlightCertificateProvisioned {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MoonlightCertificateProvisioned")
            .field("server_certificate", &"[redacted]")
            .finish()
    }
}

#[typeshare]
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoonlightCertificateRevokeRequest {
    pub host_uuid: String,
    pub client_certificate: String,
}

impl std::fmt::Debug for MoonlightCertificateRevokeRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MoonlightCertificateRevokeRequest")
            .field("host_uuid", &self.host_uuid)
            .field("client_certificate", &"[redacted]")
            .finish()
    }
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoonlightCertificateRevoked {
    pub removed: bool,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum MoonlightCertificateAttestOutcome {
    Ok(MoonlightCertificateAttested),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum MoonlightCertificateProvisionOutcome {
    Ok(MoonlightCertificateProvisioned),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum MoonlightCertificateRevokeOutcome {
    Ok(MoonlightCertificateRevoked),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MoonlightImplementation {
    Artemis,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedMoonlight {
    pub transport_id: String,
    pub implementation: MoonlightImplementation,
    pub sunshine_app: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum MoonlightResolveOutcome {
    Available(ResolvedMoonlight),
    Unavailable(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum MoonlightLaunchPrepareOutcome {
    Ok(launcher::MoonlightLaunchSpec),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum MoonlightLaunchCancelOutcome {
    Ok(MoonlightLaunchCancelled),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "payload", rename_all = "kebab-case")]
pub enum SessionControlInteraction {
    Command,
    Toggle {
        value: bool,
        #[serde(rename = "trueLabel")]
        true_label: String,
        #[serde(rename = "falseLabel")]
        false_label: String,
    },
    Choice {
        value: String,
        options: Vec<SessionControlChoice>,
    },
    Range {
        value: f64,
        min: f64,
        max: f64,
        step: f64,
    },
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SessionControlChoice {
    pub value: String,
    pub label: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionControl {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    pub destructive: bool,
    pub dismiss_on_success: bool,
    pub interaction: SessionControlInteraction,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SessionControlGroup {
    pub id: String,
    pub label: String,
    pub controls: Vec<SessionControl>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionControls {
    pub launch_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub groups: Vec<SessionControlGroup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retroarch_telemetry: Option<RetroarchSessionTelemetry>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetroarchSessionTelemetry {
    pub content_basename: String,
    pub crc32: String,
    pub menu_alive: bool,
    pub menu_selection: u32,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionControlsRequest {
    pub launch_id: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum SessionControlValue {
    Toggle(bool),
    Choice(String),
    Range(f64),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionControlInvokeRequest {
    pub launch_id: String,
    pub control_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<SessionControlValue>,
}

#[typeshare]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SessionControlFailureReason {
    StaleSession,
    UnknownControl,
    Disabled,
    InvalidValue,
    Unavailable,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionControlFailure {
    pub reason: SessionControlFailureReason,
    pub message: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SessionControlsOutcome {
    Ok(SessionControls),
    Err(SessionControlFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionControlCompleted {
    pub launch_id: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SessionControlInvokeResult {
    Completed(SessionControlCompleted),
    PlatformInstruction(launcher::PlatformInstruction),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SessionControlInvokeOutcome {
    Ok(SessionControlInvokeResult),
    Err(SessionControlFailure),
}

/** Strict process-local publication from the live Artemis Game edge. */
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoonlightExecutorState {
    pub launch_id: String,
    pub executor_id: String,
    pub generation: String,
    pub effects: Vec<MoonlightExecutorEffectState>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MoonlightExecutorEffectState {
    pub effect: launcher::AndroidMoonlightEffect,
    pub fulfillable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<SessionControlValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<MoonlightExecutorRangeState>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MoonlightExecutorRangeState {
    pub min: f64,
    pub max: f64,
    pub step: f64,
}

impl MoonlightExecutorState {
    fn effect(
        &self,
        effect: launcher::AndroidMoonlightEffect,
    ) -> Option<&MoonlightExecutorEffectState> {
        self.effects.iter().find(|entry| entry.effect == effect)
    }

    fn is_strict(&self) -> bool {
        use launcher::AndroidMoonlightEffect as Effect;
        let expected = [
            Effect::Disconnect,
            Effect::QuitHost,
            Effect::ToggleKeyboard,
            Effect::ToggleFullKeyboard,
            Effect::SetFillMode,
            Effect::SetZoomMode,
            Effect::RotateScreen,
            Effect::ToggleHud,
            Effect::ToggleFloatingMenu,
            Effect::ToggleKeyboardController,
            Effect::SwitchTouchSensitivity,
            Effect::SetMouseMode,
            Effect::SetLocalCursor,
            Effect::SetSgsrEdgeThreshold,
            Effect::SetSgsrSharpness,
            Effect::SetFaceButtonFlip,
            Effect::SetRumble,
            Effect::SetPictureInPicture,
            Effect::SetStreamBitrateKbps,
            Effect::RestoreStreamBitrate,
            Effect::SetStreamFps,
            Effect::RestoreStreamFps,
            Effect::SetStreamWidth,
            Effect::RestoreStreamResolution,
        ];
        if self.effects.len() != expected.len() {
            return false;
        }
        let mut seen = std::collections::BTreeSet::new();
        self.effects.iter().all(|entry| {
            seen.insert(entry.effect)
                && if !entry.fulfillable {
                    entry.value.is_none() && entry.range.is_none()
                } else {
                    let needs_live_range = matches!(
                        entry.effect,
                        Effect::SetStreamBitrateKbps
                            | Effect::SetStreamFps
                            | Effect::SetStreamWidth
                    );
                    if entry.range.is_some() != needs_live_range {
                        return false;
                    }
                    match (entry.effect, &entry.value) {
                        (
                            Effect::SetFillMode
                            | Effect::SetZoomMode
                            | Effect::SetFaceButtonFlip
                            | Effect::SetRumble
                            | Effect::SetPictureInPicture,
                            Some(SessionControlValue::Toggle(_)),
                        ) => true,
                        (Effect::SetMouseMode, Some(SessionControlValue::Choice(value))) => {
                            matches!(value.as_str(), "0" | "1" | "2" | "3" | "4" | "5")
                        }
                        (Effect::SetSgsrSharpness, Some(SessionControlValue::Range(value))) => {
                            valid_range_value(*value, 0.0, 50.0, 1.0)
                        }
                        (Effect::SetSgsrEdgeThreshold, Some(SessionControlValue::Range(value))) => {
                            valid_range_value(*value, 1.0, 32.0, 1.0)
                        }
                        (
                            effect @ (Effect::SetStreamBitrateKbps
                            | Effect::SetStreamFps
                            | Effect::SetStreamWidth),
                            Some(SessionControlValue::Range(value)),
                        ) => {
                            let Some(range) = &entry.range else {
                                return false;
                            };
                            let outer = match effect {
                                Effect::SetStreamBitrateKbps => (500.0, 150000.0, 1.0),
                                Effect::SetStreamFps => (1.0, 240.0, 1.0),
                                _ => (2.0, 8192.0, 2.0),
                            };
                            range.min >= outer.0
                                && range.max <= outer.1
                                && strict_dynamic_integer_range(
                                    effect, *value, range.min, range.max, range.step,
                                )
                        }
                        (
                            Effect::Disconnect
                            | Effect::QuitHost
                            | Effect::ToggleKeyboard
                            | Effect::ToggleFullKeyboard
                            | Effect::RotateScreen
                            | Effect::ToggleHud
                            | Effect::ToggleFloatingMenu
                            | Effect::ToggleKeyboardController
                            | Effect::SwitchTouchSensitivity
                            | Effect::SetLocalCursor
                            | Effect::RestoreStreamBitrate
                            | Effect::RestoreStreamFps
                            | Effect::RestoreStreamResolution,
                            None,
                        ) => true,
                        _ => false,
                    }
                }
        }) && expected.iter().all(|effect| seen.contains(effect))
            && [
                (Effect::SetStreamBitrateKbps, Effect::RestoreStreamBitrate),
                (Effect::SetStreamFps, Effect::RestoreStreamFps),
                (Effect::SetStreamWidth, Effect::RestoreStreamResolution),
            ]
            .iter()
            .all(|(set, restore)| {
                self.effect(*set).is_some_and(|entry| {
                    self.effect(*restore).is_some_and(|other| {
                        entry.fulfillable == other.fulfillable
                            && other.value.is_none()
                            && other.range.is_none()
                    })
                })
            })
    }
}

fn ulp_at(value: f64) -> f64 {
    let magnitude = value.abs();
    if magnitude < f64::MIN_POSITIVE {
        return f64::from_bits(1);
    }
    let exponent = ((magnitude.to_bits() >> 52) & 0x7ff) as i32 - 1023;
    2.0_f64.powi(exponent - 52)
}

fn exact_integer(value: f64) -> bool {
    value.is_finite() && value.fract() == 0.0
}

fn strict_dynamic_integer_range(
    effect: launcher::AndroidMoonlightEffect,
    value: f64,
    min: f64,
    max: f64,
    step: f64,
) -> bool {
    use launcher::AndroidMoonlightEffect as Effect;
    if !exact_integer(value) || !exact_integer(min) || !exact_integer(max) || !exact_integer(step) {
        return false;
    }
    let expected_step = if effect == Effect::SetStreamWidth {
        2.0
    } else {
        1.0
    };
    if step != expected_step {
        return false;
    }
    if effect == Effect::SetStreamWidth
        && (value % 2.0 != 0.0 || min % 2.0 != 0.0 || max % 2.0 != 0.0)
    {
        return false;
    }
    valid_range_value(value, min, max, step)
}

fn valid_range_value(value: f64, min: f64, max: f64, step: f64) -> bool {
    if !value.is_finite()
        || !min.is_finite()
        || !max.is_finite()
        || !step.is_finite()
        || step <= 0.0
        || min + step == min
        || min > max
        || value < min
        || value > max
    {
        return false;
    }
    if value == min || value == max {
        return true;
    }

    let offset = value - min;
    let steps = if offset.is_finite() {
        offset / step
    } else {
        value / step - min / step
    };
    if !steps.is_finite() {
        return false;
    }
    let nearest = steps.round().mul_add(step, min);
    if !nearest.is_finite() {
        return false;
    }

    // Subtraction, division, rounding, and the fused grid reconstruction can
    // each move the result by an ULP. Compare in value space, but cap that
    // tolerance to the declared step so absolute magnitude cannot admit a
    // material fraction of one step.
    let arithmetic_tolerance = 4.0 * ulp_at(value).max(ulp_at(min)).max(ulp_at(nearest));
    let grid_tolerance = step * 1e-9;
    (value - nearest).abs() <= arithmetic_tolerance.min(grid_tolerance)
}

/** Validate the invocation against the current materialized control before an
 * integration effect can be selected or protected. */
pub fn validate_session_control_invocation(
    active_launch_id: &str,
    request: &SessionControlInvokeRequest,
    control: &SessionControl,
) -> Result<(), SessionControlFailure> {
    if request.launch_id != active_launch_id {
        return Err(SessionControlFailure {
            reason: SessionControlFailureReason::StaleSession,
            message: "The gameplay session changed. Reopen the overlay and try again.".into(),
        });
    }
    if request.control_id != control.id {
        return Err(SessionControlFailure {
            reason: SessionControlFailureReason::UnknownControl,
            message: "That gameplay control is no longer available.".into(),
        });
    }
    if !control.enabled {
        return Err(SessionControlFailure {
            reason: SessionControlFailureReason::Disabled,
            message: control
                .disabled_reason
                .clone()
                .unwrap_or_else(|| "That gameplay control is currently unavailable.".into()),
        });
    }

    let valid = match (&control.interaction, &request.value) {
        (SessionControlInteraction::Command, None) => true,
        (SessionControlInteraction::Toggle { .. }, Some(SessionControlValue::Toggle(_))) => true,
        (
            SessionControlInteraction::Choice { options, .. },
            Some(SessionControlValue::Choice(value)),
        ) => options.iter().any(|option| option.value == *value),
        (
            SessionControlInteraction::Range {
                value: current,
                min,
                max,
                step,
            },
            Some(SessionControlValue::Range(submitted)),
        ) => {
            valid_range_value(*current, *min, *max, *step)
                && valid_range_value(*submitted, *min, *max, *step)
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(SessionControlFailure {
            reason: SessionControlFailureReason::InvalidValue,
            message: "That value is not valid for this gameplay control.".into(),
        })
    }
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionStatusRequest {}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveSession {
    pub launch_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<ActiveSession>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStopRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force: Option<bool>,
    /** Required by every host surface for an exact stop. */
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_launch_id: Option<String>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStopPhase {
    Stopped,
    Pending,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionStopResult {
    pub phase: SessionStopPhase,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFreezeRequest {
    /** Required by every host surface for an exact freeze. */
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_launch_id: Option<String>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionThawRequest {
    /** Required by every host surface for an exact thaw. */
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_launch_id: Option<String>,
}

#[typeshare]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SessionFreezerState {
    Frozen,
    Running,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFreezeResult {
    pub launch_id: String,
    /** Freezer state of the exact launch after the request. */
    pub state: SessionFreezerState,
    /** False when the launch was already in the requested state. */
    pub changed: bool,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PeerListRequest {}

#[typeshare]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PeerListState {
    Loading,
    Ready,
    Failed,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerListEntry {
    pub device_public_key: String,
    pub label: String,
    pub state: PeerListState,
    /** Local state observation time in Unix seconds, not endpoint issue time. */
    pub updated_at: typeshare::U53,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PeerList {
    /** Verified peers in ascending device-public-key order. */
    pub peers: Vec<PeerListEntry>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatusRequest {
    /** Selects exactly one native peer by its expected device public key. */
    pub device_public_key: String,
}

#[typeshare]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SourceCatalogState {
    Available,
    Unavailable,
}

#[typeshare]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SourceStreamControlState {
    Enabled,
    Disabled,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    /** Whether the host can produce a catalog snapshot right now. */
    pub catalog: SourceCatalogState,
    /** Whether the protected Sunshine certificate control is reachable. */
    pub stream_control: SourceStreamControlState,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LocalGamesListRequest {}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LocalGames {
    pub games: Vec<launcher::LocalGame>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failures: Option<Vec<RpcFailure>>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalGameLaunchRequest {
    pub game_id: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum LocalGamesListOutcome {
    Ok(LocalGames),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum LocalGameLaunchOutcome {
    Ok(launcher::LaunchSpec),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HealthRequest {}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DiscoverySnapshotRequest {}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryRegisterReceiptRequest {
    pub receipt: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryRemoveLocationRequest {
    pub location_id: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DiscoveryRescanRequest {}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum DiscoveryState {
    Idle {},
    Scanning {},
    Enriching {},
    Problem {},
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryLocationSummary {
    pub id: String,
    pub label: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryDiagnostic {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_id: Option<String>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverySnapshot {
    pub generation: String,
    pub state: DiscoveryState,
    pub locations: Vec<DiscoveryLocationSummary>,
    pub diagnostics: Vec<DiscoveryDiagnostic>,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum DiscoverySnapshotOutcome {
    Ok(DiscoverySnapshot),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SettingsSnapshotRequest {}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSetting {
    pub id: String,
    pub title: String,
    pub enabled: bool,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    pub revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    pub plugins: Vec<PluginSetting>,
    pub steam_grid_db_credential: config::settings::SecretSettingStatus,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamGridDbCredentialSetRequest {
    pub token: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SteamGridDbCredentialClearRequest {}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SensitiveSettingResult {
    pub status: config::settings::SecretSettingStatus,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsUpdateRequest {
    pub expected_revision: String,
    pub setting_id: String,
    /** Text transport keeps the surface treaty generic. Plugin values are
     * exactly "true" or "false"; device-name values are the name itself. */
    pub value: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SettingsSnapshotOutcome {
    Ok(SettingsSnapshot),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SettingsUpdateOutcome {
    Ok(SettingsSnapshot),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SensitiveSettingOutcome {
    Ok(SensitiveSettingResult),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Health {
    pub version: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RpcFailure {
    pub code: String,
    pub message: String,
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum CatalogSnapshotOutcome {
    Ok(CatalogSnapshot),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SessionPrepareOutcome {
    Ok(SessionPrepared),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SessionStatusOutcome {
    Ok(SessionStatus),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SessionStopOutcome {
    Ok(SessionStopResult),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SessionFreezeOutcome {
    Ok(SessionFreezeResult),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum SourceStatusOutcome {
    Ok(SourceStatus),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum PeerListOutcome {
    Ok(PeerList),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum HealthOutcome {
    Ok(Health),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum RpcRequest {
    #[serde(rename = "app.local-games.routes")]
    GameRoutes(game_routes::GameRoutesRequest),
    #[serde(rename = "app.local-games.runner.set")]
    GameRunnerSet(game_routes::GameRunnerSetRequest),
    #[serde(rename = "app.local-games.launch.selected")]
    SelectedGameLaunch(game_routes::SelectedGameLaunchRequest),
    #[serde(rename = "app.catalog.snapshot")]
    CatalogSnapshot(CatalogSnapshotRequest),
    #[serde(rename = "app.moonlight.certificate.attest")]
    MoonlightCertificateAttest(MoonlightCertificateAttestRequest),
    #[serde(rename = "app.moonlight.certificate.provision")]
    MoonlightCertificateProvision(MoonlightCertificateProvisionRequest),
    #[serde(rename = "app.moonlight.certificate.revoke")]
    MoonlightCertificateRevoke(MoonlightCertificateRevokeRequest),
    #[serde(rename = "app.session.prepare")]
    SessionPrepare(SessionPrepareRequest),
    #[serde(rename = "app.session.status")]
    SessionStatus(SessionStatusRequest),
    #[serde(rename = "app.session.stop")]
    SessionStop(SessionStopRequest),
    #[serde(rename = "app.session.freeze")]
    SessionFreeze(SessionFreezeRequest),
    #[serde(rename = "app.session.thaw")]
    SessionThaw(SessionThawRequest),
    #[serde(rename = "app.source.status")]
    SourceStatus(SourceStatusRequest),
    #[serde(rename = "app.peer.list")]
    PeerList(PeerListRequest),
    #[serde(rename = "app.local-games.list")]
    LocalGamesList(LocalGamesListRequest),
    #[serde(rename = "system.health")]
    Health(HealthRequest),
    #[serde(rename = "app.discovery.snapshot")]
    DiscoverySnapshot(DiscoverySnapshotRequest),
    #[serde(rename = "app.discovery.registerReceipt")]
    DiscoveryRegisterReceipt(DiscoveryRegisterReceiptRequest),
    #[serde(rename = "app.discovery.removeLocation")]
    DiscoveryRemoveLocation(DiscoveryRemoveLocationRequest),
    #[serde(rename = "app.discovery.rescan")]
    DiscoveryRescan(DiscoveryRescanRequest),
    #[serde(rename = "system.settings.snapshot")]
    SettingsSnapshot(SettingsSnapshotRequest),
    #[serde(rename = "system.settings.update")]
    SettingsUpdate(SettingsUpdateRequest),
    #[serde(rename = "system.settings.steamgriddbCredential.set")]
    SteamGridDbCredentialSet(SteamGridDbCredentialSetRequest),
    #[serde(rename = "system.settings.steamgriddbCredential.clear")]
    SteamGridDbCredentialClear(SteamGridDbCredentialClearRequest),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "outcome")]
pub enum RpcResponse {
    #[serde(rename = "app.local-games.routes")]
    GameRoutes(game_routes::GameRoutesOutcome),
    #[serde(rename = "app.local-games.runner.set")]
    GameRunnerSet(game_routes::GameRunnerSetOutcome),
    #[serde(rename = "app.local-games.launch.selected")]
    SelectedGameLaunch(game_routes::SelectedGameLaunchOutcome),
    #[serde(rename = "app.catalog.snapshot")]
    CatalogSnapshot(CatalogSnapshotOutcome),

    #[serde(rename = "app.moonlight.certificate.attest")]
    MoonlightCertificateAttest(MoonlightCertificateAttestOutcome),
    #[serde(rename = "app.moonlight.certificate.provision")]
    MoonlightCertificateProvision(MoonlightCertificateProvisionOutcome),
    #[serde(rename = "app.moonlight.certificate.revoke")]
    MoonlightCertificateRevoke(MoonlightCertificateRevokeOutcome),
    #[serde(rename = "app.session.prepare")]
    SessionPrepare(SessionPrepareOutcome),
    #[serde(rename = "app.session.status")]
    SessionStatus(SessionStatusOutcome),
    #[serde(rename = "app.session.stop")]
    SessionStop(SessionStopOutcome),
    #[serde(rename = "app.session.freeze")]
    SessionFreeze(SessionFreezeOutcome),
    #[serde(rename = "app.session.thaw")]
    SessionThaw(SessionFreezeOutcome),
    #[serde(rename = "app.source.status")]
    SourceStatus(SourceStatusOutcome),
    #[serde(rename = "app.peer.list")]
    PeerList(PeerListOutcome),
    #[serde(rename = "app.local-games.list")]
    LocalGamesList(LocalGamesListOutcome),
    #[serde(rename = "system.health")]
    Health(HealthOutcome),
    #[serde(rename = "app.discovery.snapshot")]
    DiscoverySnapshot(DiscoverySnapshotOutcome),
    #[serde(rename = "app.discovery.registerReceipt")]
    DiscoveryRegisterReceipt(DiscoverySnapshotOutcome),
    #[serde(rename = "app.discovery.removeLocation")]
    DiscoveryRemoveLocation(DiscoverySnapshotOutcome),
    #[serde(rename = "app.discovery.rescan")]
    DiscoveryRescan(DiscoverySnapshotOutcome),
    #[serde(rename = "system.settings.snapshot")]
    SettingsSnapshot(SettingsSnapshotOutcome),
    #[serde(rename = "system.settings.update")]
    SettingsUpdate(SettingsUpdateOutcome),
    #[serde(rename = "system.settings.steamgriddbCredential.set")]
    SteamGridDbCredentialSet(SensitiveSettingOutcome),
    #[serde(rename = "system.settings.steamgriddbCredential.clear")]
    SteamGridDbCredentialClear(SensitiveSettingOutcome),
}

#[derive(Clone, Debug)]
struct TrackedActiveLaunch {
    launch: launcher::AndroidActiveLaunch,
    started_at: std::time::Instant,
    started_epoch_seconds: u64,
}

#[derive(Clone)]
struct BrainRuntime {
    upstream: upstreams::UpstreamRegistry,
    local_storage_root: PathBuf,
    private_state_root: PathBuf,
    /// The verified owner of this brain device at construction. `None`
    /// while the device is unowned, revoked, or invalid.
    local_owner_public_key: Option<String>,
    local_file_provision: launcher::FileProvisionMode,
    local_launch_signing_key: Vec<u8>,
    local_launch_reservations: Arc<Mutex<launcher::LaunchPublicationReservations>>,
    moonlight_launch_authority: Arc<Mutex<launcher::MoonlightLaunchAuthority>>,
    active_android_launch: Arc<Mutex<Option<TrackedActiveLaunch>>>,
    moonlight_executor_state: Arc<Mutex<Option<MoonlightExecutorState>>>,
    registry_source: plugin_policy::RegistrySource,
    config_snapshot: config::snapshot::ConfigSnapshotCoordinator,
    discovery: discovery::DiscoveryLifecycleCoordinator,
    /** Serialises revision-check + replace; external file-manager edits are
     * detected by the revision inside this same critical section. */
    settings_write_lock: Arc<Mutex<()>>,
}

impl BrainRuntime {
    fn local_owner_public_key(&self) -> Option<&str> {
        self.local_owner_public_key.as_deref()
    }
}

/// The verified owner key of the local device, read once at construction.
/// A device that cannot load its identity has no owner.
fn brain_owner_public_key(private_state_root: &Path) -> Option<String> {
    let identity = identity::DeviceIdentity::load_or_create(private_state_root).ok()?;
    match identity.state() {
        identity::IdentityState::Owned {
            owner_public_key, ..
        } => Some(owner_public_key.clone()),
        _ => None,
    }
}

#[derive(Clone)]
enum ServerMode {
    Brain(BrainRuntime),
    Host(host::HostRuntime),
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum RpcSurface {
    Lan,
    LocalControl,
}

#[derive(Clone)]
struct AppState {
    // Retain the actual shared directory for snapshot-only B5 consumers.
    federation: Option<federation::FederationDirectory>,
    federation_wake: Option<DiscoveryControl>,
    mode: ServerMode,
    portal_access: Option<PortalAccess>,
    rpc_surface: RpcSurface,
}

fn active_session_conflict() -> RpcFailure {
    RpcFailure {
        code: "ActiveSessionConflict".into(),
        message: "An active RetroArch session must end before another local route can start."
            .into(),
    }
}

fn local_launch_failure(error: launcher::LaunchError) -> RpcFailure {
    let code = match &error {
        launcher::LaunchError::UnknownGame(_) => "LocalGameNotFound",
        launcher::LaunchError::RomMissing(_) => "LocalRomMissing",
        launcher::LaunchError::StorageAccess(_) => "LocalStorageUnavailable",
        launcher::LaunchError::Config(_) => "LocalConfigWriteFailed",
        launcher::LaunchError::ConfigUnauthorized(_) => "LocalConfigUnauthorized",
        launcher::LaunchError::RouteUnavailable(_) => "LocalRouteUnavailable",
        launcher::LaunchError::RouteCollision(_) => "LocalRouteCollision",
    };
    RpcFailure {
        code: code.into(),
        message: error.to_string(),
    }
}

fn snapshot_diagnostic_failure(diagnostic: &config::snapshot::SnapshotDiagnostic) -> RpcFailure {
    RpcFailure {
        code: snapshot_diagnostic_code(diagnostic.code).into(),
        message: diagnostic.message.clone(),
    }
}

fn snapshot_diagnostic_code(code: config::snapshot::SnapshotDiagnosticCode) -> &'static str {
    match code {
        config::snapshot::SnapshotDiagnosticCode::LocalConfigReloadFailed => {
            "LocalConfigReloadFailed"
        }
        config::snapshot::SnapshotDiagnosticCode::LocalConfigUnsupported => {
            "LocalConfigUnsupported"
        }
        config::snapshot::SnapshotDiagnosticCode::LocalConfigUnauthorized => {
            "LocalConfigUnauthorized"
        }
    }
}

fn route_diagnostic_failure(diagnostic: &config::resolver::RouteDiagnostic) -> RpcFailure {
    RpcFailure {
        code: route_diagnostic_code(diagnostic.code).into(),
        message: diagnostic.message.clone(),
    }
}

fn route_diagnostic_code(code: config::resolver::RouteDiagnosticCode) -> &'static str {
    match code {
        config::resolver::RouteDiagnosticCode::LocalRomMissing => "LocalRomMissing",
        config::resolver::RouteDiagnosticCode::LocalRouteUnavailable => "LocalRouteUnavailable",
        config::resolver::RouteDiagnosticCode::LocalRouteCollision => "LocalRouteCollision",
    }
}

fn upstream_failure(error: upstreams::UpstreamError) -> RpcFailure {
    RpcFailure {
        code: error.code().into(),
        message: error.to_string(),
    }
}

/// Pure mapping from the legacy status union to the korrid-shaped outcome.
fn session_status_outcome(
    result: Result<upstream::UpstreamSessionStatus, upstreams::UpstreamError>,
) -> SessionStatusOutcome {
    match result {
        Ok(upstream::UpstreamSessionStatus::SessionStatus { active }) => {
            SessionStatusOutcome::Ok(SessionStatus {
                active: active.map(|active| ActiveSession {
                    launch_id: active.launch_id,
                    host: active.host,
                    game_id: active.game_id,
                    title: active.title,
                    phase: active.phase,
                }),
            })
        }
        Ok(upstream::UpstreamSessionStatus::SessiondNotConfigured {}) => {
            SessionStatusOutcome::Err(RpcFailure {
                code: "SessiondNotConfigured".into(),
                message: "host session daemon is not configured".into(),
            })
        }
        Ok(upstream::UpstreamSessionStatus::HostUnavailable {}) => {
            SessionStatusOutcome::Err(RpcFailure {
                code: "HostUnavailable".into(),
                message: "host is unavailable".into(),
            })
        }
        Err(error) => SessionStatusOutcome::Err(upstream_failure(error)),
    }
}

/// Pure mapping from the legacy stop union to the korrid-shaped outcome.
fn session_stop_outcome(
    result: Result<upstream::UpstreamSessionStop, upstreams::UpstreamError>,
) -> SessionStopOutcome {
    match result {
        Ok(upstream::UpstreamSessionStop::Stopped { .. }) => {
            SessionStopOutcome::Ok(SessionStopResult {
                phase: SessionStopPhase::Stopped,
            })
        }
        Ok(upstream::UpstreamSessionStop::StopPending { .. }) => {
            SessionStopOutcome::Ok(SessionStopResult {
                phase: SessionStopPhase::Pending,
            })
        }
        Ok(upstream::UpstreamSessionStop::NothingToStop {}) => {
            SessionStopOutcome::Ok(SessionStopResult {
                phase: SessionStopPhase::Stopped,
            })
        }
        Ok(upstream::UpstreamSessionStop::ConfirmationRequired { action }) => {
            SessionStopOutcome::Err(RpcFailure {
                code: "ConfirmationRequired".into(),
                message: action.unwrap_or_else(|| "session stop requires confirmation".into()),
            })
        }
        Ok(upstream::UpstreamSessionStop::SessiondNotConfigured {}) => {
            SessionStopOutcome::Err(RpcFailure {
                code: "SessiondNotConfigured".into(),
                message: "host session daemon is not configured".into(),
            })
        }
        Ok(upstream::UpstreamSessionStop::HostUnavailable {}) => {
            SessionStopOutcome::Err(RpcFailure {
                code: "HostUnavailable".into(),
                message: "host is unavailable".into(),
            })
        }
        Err(error) => SessionStopOutcome::Err(upstream_failure(error)),
    }
}

fn host_session_status_outcome(
    result: Result<host::control::HostSessionStatus, RpcFailure>,
) -> SessionStatusOutcome {
    use host::control::HostSessionStatus;
    match result {
        Ok(HostSessionStatus::Running { launch_id, game_id }) => {
            SessionStatusOutcome::Ok(SessionStatus {
                active: Some(ActiveSession {
                    launch_id,
                    host: None,
                    game_id,
                    title: None,
                    phase: Some("running".into()),
                }),
            })
        }
        Ok(HostSessionStatus::Frozen { launch_id, game_id }) => {
            SessionStatusOutcome::Ok(SessionStatus {
                active: Some(ActiveSession {
                    launch_id,
                    host: None,
                    game_id,
                    title: None,
                    phase: Some("frozen".into()),
                }),
            })
        }
        Ok(HostSessionStatus::Stopping { launch_id, game_id }) => {
            SessionStatusOutcome::Ok(SessionStatus {
                active: Some(ActiveSession {
                    launch_id,
                    host: None,
                    game_id,
                    title: None,
                    phase: Some("stopping".into()),
                }),
            })
        }
        Ok(HostSessionStatus::Completed { launch_id }) => SessionStatusOutcome::Err(RpcFailure {
            code: "SessionCompleted".into(),
            message: format!("host launch {launch_id} completed"),
        }),
        Ok(HostSessionStatus::NoActive) => SessionStatusOutcome::Err(RpcFailure {
            code: "NoActiveSession".into(),
            message: "no host launch is active".into(),
        }),
        Ok(HostSessionStatus::RecoveryBlocked) => SessionStatusOutcome::Err(RpcFailure {
            code: "HostRecoveryBlocked".into(),
            message: "host recovery identity requires administrator resolution".into(),
        }),
        Err(failure) => SessionStatusOutcome::Err(failure),
    }
}

fn host_session_stop_outcome(
    result: Result<host::control::HostSessionStop, RpcFailure>,
) -> SessionStopOutcome {
    use host::control::HostSessionStop;
    match result {
        Ok(HostSessionStop::Completed { .. }) => SessionStopOutcome::Ok(SessionStopResult {
            phase: SessionStopPhase::Stopped,
        }),
        Ok(HostSessionStop::AlreadyStopping { .. }) => SessionStopOutcome::Ok(SessionStopResult {
            phase: SessionStopPhase::Pending,
        }),
        Ok(HostSessionStop::NoActive) => SessionStopOutcome::Err(RpcFailure {
            code: "NoActiveSession".into(),
            message: "no host launch is active".into(),
        }),
        Ok(HostSessionStop::StaleIdentity { .. }) => SessionStopOutcome::Err(RpcFailure {
            code: "StaleLaunchIdentity".into(),
            message: "expectedLaunchId does not identify the active host launch".into(),
        }),
        Ok(HostSessionStop::RecoveryBlocked) => SessionStopOutcome::Err(RpcFailure {
            code: "HostRecoveryBlocked".into(),
            message: "host recovery identity requires administrator resolution".into(),
        }),
        Err(failure) => SessionStopOutcome::Err(failure),
    }
}

fn host_session_freeze_outcome(
    result: Result<host::control::HostSessionFreezeChange, RpcFailure>,
    state: SessionFreezerState,
) -> SessionFreezeOutcome {
    use host::control::HostSessionFreezeChange;
    match result {
        Ok(HostSessionFreezeChange::Changed { launch_id }) => {
            SessionFreezeOutcome::Ok(SessionFreezeResult {
                launch_id,
                state,
                changed: true,
            })
        }
        Ok(HostSessionFreezeChange::Unchanged { launch_id }) => {
            SessionFreezeOutcome::Ok(SessionFreezeResult {
                launch_id,
                state,
                changed: false,
            })
        }
        Ok(HostSessionFreezeChange::NoActive) => SessionFreezeOutcome::Err(RpcFailure {
            code: "NoActiveSession".into(),
            message: "no host launch is active".into(),
        }),
        Ok(HostSessionFreezeChange::StaleIdentity { .. }) => {
            SessionFreezeOutcome::Err(RpcFailure {
                code: "StaleLaunchIdentity".into(),
                message: "expectedLaunchId does not identify the active host launch".into(),
            })
        }
        Ok(HostSessionFreezeChange::Stopping { .. }) => SessionFreezeOutcome::Err(RpcFailure {
            code: "SessionStopping".into(),
            message: "the host launch is stopping".into(),
        }),
        // The game is running again; only its window stayed behind. A separate
        // code stops a caller from retrying a freezer change that already
        // succeeded, and lets a surface say what actually failed.
        Ok(HostSessionFreezeChange::FocusFailed { message, .. }) => {
            SessionFreezeOutcome::Err(RpcFailure {
                code: "HostFocusFailed".into(),
                message,
            })
        }
        Ok(HostSessionFreezeChange::HelperFailed { message, .. }) => {
            SessionFreezeOutcome::Err(RpcFailure {
                code: "HostFreezerFailed".into(),
                message,
            })
        }
        Ok(HostSessionFreezeChange::RecoveryBlocked) => SessionFreezeOutcome::Err(RpcFailure {
            code: "HostRecoveryBlocked".into(),
            message: "host recovery identity requires administrator resolution".into(),
        }),
        Err(failure) => SessionFreezeOutcome::Err(failure),
    }
}

fn source_status_outcome(
    result: Result<SourceStatus, upstreams::UpstreamError>,
) -> SourceStatusOutcome {
    match result {
        Ok(status) => SourceStatusOutcome::Ok(status),
        Err(error) => SourceStatusOutcome::Err(upstream_failure(error)),
    }
}

fn session_freeze_outcome(
    result: Result<SessionFreezeResult, upstreams::UpstreamError>,
) -> SessionFreezeOutcome {
    match result {
        Ok(result) => SessionFreezeOutcome::Ok(result),
        Err(error) => SessionFreezeOutcome::Err(upstream_failure(error)),
    }
}

fn exact_host_launch_id(
    expected_launch_id: Option<&str>,
    verb: &str,
) -> Result<String, RpcFailure> {
    expected_launch_id
        .map(str::to_owned)
        .ok_or_else(|| RpcFailure {
            code: "ExpectedLaunchIdRequired".into(),
            message: format!("expectedLaunchId is required for exact host {verb}"),
        })
}


/// Whether a brain caller acts for the brain's own owner. Local surfaces act
/// for the device owner. A peer principal must present the same owner as
/// the brain device; a household or guest pass holder is a different person
/// even when the brain owner issued the pass.
fn brain_caller_is_owner(
    brain: &BrainRuntime,
    authorization: &authorization::AuthorizationContext,
) -> bool {
    match authorization {
        authorization::AuthorizationContext::LocalBrowser
        | authorization::AuthorizationContext::LocalUnixControl => true,
        authorization::AuthorizationContext::Peer(principal) => match principal {
            authorization::Principal::OwnerDevice {
                owner_public_key, ..
            } => brain
                .local_owner_public_key()
                .is_some_and(|local| local == *owner_public_key),
            _ => false,
        },
        authorization::AuthorizationContext::TrustReconciliation => false,
    }
}

fn strip_play_stats(mut snapshot: CatalogSnapshot) -> CatalogSnapshot {
    for game in &mut snapshot.games {
        game.play_stats = None;
    }
    snapshot
}

fn peer_list(state: &AppState) -> PeerListOutcome {
    let result = (|| {
        let peers = state
            .federation
            .as_ref()
            .map(federation::FederationDirectory::snapshot)
            .transpose()?
            .unwrap_or_default();
        let labels = match &state.mode {
            ServerMode::Brain(brain) => brain.upstream.configured_peer_labels(),
            ServerMode::Host(_) => Default::default(),
        };
        let peers = peers
            .into_iter()
            .map(|peer| {
                let label = labels
                    .get(&peer.device_public_key)
                    .cloned()
                    .or_else(|| {
                        peer.current_endpoint
                            .as_ref()
                            .or(peer.remembered_endpoint.as_ref())
                            .and_then(|endpoint| endpoint.label.clone())
                    })
                    .unwrap_or_else(|| peer.device_public_key.clone());
                let (state, last_error) = match peer.state {
                    federation::PeerState::Loading => (PeerListState::Loading, None),
                    federation::PeerState::Ready => (PeerListState::Ready, None),
                    // Native candidate operations already sanitize this at the producer.
                    federation::PeerState::Failed { error } => (PeerListState::Failed, Some(error)),
                };
                Ok(PeerListEntry {
                    device_public_key: peer.device_public_key,
                    label,
                    state,
                    updated_at: peer
                        .updated_at
                        .try_into()
                        .map_err(|_| federation::FederationError::Bounds)?,
                    last_error,
                })
            })
            .collect::<Result<Vec<_>, federation::FederationError>>()?;
        Ok::<_, federation::FederationError>(PeerList { peers })
    })();
    match result {
        Ok(peers) => PeerListOutcome::Ok(peers),
        Err(_) => PeerListOutcome::Err(RpcFailure {
            code: "PeerListUnavailable".into(),
            message: "peer directory unavailable".into(),
        }),
    }
}

async fn dispatch(
    state: &AppState,
    authorization: &authorization::AuthorizationContext,
    request: RpcRequest,
) -> Result<RpcResponse, authorization::AuthorizationDenied> {
    authorization::authorize(authorization, &request)?;
    let response = match request {
        RpcRequest::GameRoutes(request) => RpcResponse::GameRoutes(match &state.mode {
            ServerMode::Host(host) => host
                .game_routes(request.game_id)
                .await
                .map(game_routes::GameRoutesOutcome::Ok)
                .unwrap_or_else(game_routes::GameRoutesOutcome::Err),
            ServerMode::Brain(_) => {
                game_routes::GameRoutesOutcome::Err(installed_routes_unsupported())
            }
        }),
        RpcRequest::GameRunnerSet(request) => RpcResponse::GameRunnerSet(match &state.mode {
            ServerMode::Host(host) => host
                .set_game_runner(request)
                .await
                .map(game_routes::GameRunnerSetOutcome::Ok)
                .unwrap_or_else(game_routes::GameRunnerSetOutcome::Err),
            ServerMode::Brain(_) => {
                game_routes::GameRunnerSetOutcome::Err(installed_routes_unsupported())
            }
        }),
        RpcRequest::SelectedGameLaunch(request) => {
            RpcResponse::SelectedGameLaunch(match &state.mode {
                ServerMode::Host(host) => host
                    .prepare_selected(
                        request,
                        authorization.person_public_key(host.owner_public_key()),
                    )
                    .await
                    .map(game_routes::SelectedGameLaunchOutcome::Ok)
                    .unwrap_or_else(game_routes::SelectedGameLaunchOutcome::Err),
                ServerMode::Brain(_) => {
                    game_routes::SelectedGameLaunchOutcome::Err(installed_routes_unsupported())
                }
            })
        }
        RpcRequest::PeerList(_) => RpcResponse::PeerList(peer_list(state)),
        RpcRequest::CatalogSnapshot(_) => {
            let outcome = match &state.mode {
                ServerMode::Brain(brain) => brain
                    .upstream
                    .catalog_snapshot()
                    .await
                    .map(|snapshot| {
                        // The second federation hop authenticates as this
                        // brain's own device, so the peer derives play
                        // statistics for the brain owner. Only a caller
                        // proven to be that owner may see them.
                        if brain_caller_is_owner(brain, authorization) {
                            snapshot
                        } else {
                            strip_play_stats(snapshot)
                        }
                    })
                    .map(CatalogSnapshotOutcome::Ok)
                    .unwrap_or_else(|error| CatalogSnapshotOutcome::Err(upstream_failure(error))),
                ServerMode::Host(host) => host
                    .catalog_snapshot_for(authorization.person_public_key(host.owner_public_key()))
                    .await
                    .map(CatalogSnapshotOutcome::Ok)
                    .unwrap_or_else(CatalogSnapshotOutcome::Err),
            };
            RpcResponse::CatalogSnapshot(outcome)
        }
        RpcRequest::MoonlightCertificateAttest(request) => {
            let outcome = match host::moonlight_certificate::validate_host_uuid(&request.host_uuid)
            {
                Err(failure) => MoonlightCertificateAttestOutcome::Err(failure),
                Ok(()) => match (&state.mode, state.rpc_surface) {
                    (ServerMode::Brain(brain), RpcSurface::Lan) => brain
                        .upstream
                        .moonlight_certificate_attest(&request.host_uuid)
                        .await
                        .map(MoonlightCertificateAttestOutcome::Ok)
                        .unwrap_or_else(|error| {
                            MoonlightCertificateAttestOutcome::Err(upstream_failure(error))
                        }),
                    (ServerMode::Host(host), RpcSurface::Lan) => host
                        .moonlight_certificate_attest(&request.host_uuid)
                        .await
                        .map(|matched| {
                            MoonlightCertificateAttestOutcome::Ok(MoonlightCertificateAttested {
                                matched,
                            })
                        })
                        .unwrap_or_else(MoonlightCertificateAttestOutcome::Err),
                    (_, RpcSurface::LocalControl) => {
                        MoonlightCertificateAttestOutcome::Err(RpcFailure {
                            code: "OperationUnsupported".into(),
                            message: "certificate attestation is unavailable on the local control listener".into(),
                        })
                    }
                },
            };
            RpcResponse::MoonlightCertificateAttest(outcome)
        }
        RpcRequest::MoonlightCertificateProvision(request) => {
            let outcome = match host::moonlight_certificate::validate_host_uuid(&request.host_uuid)
                .and_then(|()| {
                    host::moonlight_certificate::validate_single_pem(&request.client_certificate)
                }) {
                Err(failure) => MoonlightCertificateProvisionOutcome::Err(failure),
                Ok(()) => match (&state.mode, state.rpc_surface) {
                    (ServerMode::Brain(brain), RpcSurface::Lan) => brain
                        .upstream
                        .moonlight_certificate_provision(
                            &request.host_uuid,
                            &request.client_certificate,
                        )
                        .await
                        .map(MoonlightCertificateProvisionOutcome::Ok)
                        .unwrap_or_else(|error| {
                            MoonlightCertificateProvisionOutcome::Err(upstream_failure(error))
                        }),
                    (ServerMode::Host(host), RpcSurface::Lan) => host
                        .moonlight_certificate_provision(
                            &request.host_uuid,
                            &request.client_certificate,
                        )
                        .await
                        .map(MoonlightCertificateProvisionOutcome::Ok)
                        .unwrap_or_else(MoonlightCertificateProvisionOutcome::Err),
                    (_, RpcSurface::LocalControl) => {
                        MoonlightCertificateProvisionOutcome::Err(RpcFailure {
                            code: "OperationUnsupported".into(),
                            message:
                                "certificate provision is unavailable on the local control listener"
                                    .into(),
                        })
                    }
                },
            };
            RpcResponse::MoonlightCertificateProvision(outcome)
        }
        RpcRequest::MoonlightCertificateRevoke(request) => {
            let outcome = match host::moonlight_certificate::validate_host_uuid(&request.host_uuid)
                .and_then(|()| {
                    host::moonlight_certificate::validate_single_pem(&request.client_certificate)
                }) {
                Err(failure) => MoonlightCertificateRevokeOutcome::Err(failure),
                Ok(()) => match (&state.mode, state.rpc_surface) {
                    (ServerMode::Brain(brain), RpcSurface::Lan) => brain
                        .upstream
                        .moonlight_certificate_revoke(
                            &request.host_uuid,
                            &request.client_certificate,
                        )
                        .await
                        .map(MoonlightCertificateRevokeOutcome::Ok)
                        .unwrap_or_else(|error| {
                            MoonlightCertificateRevokeOutcome::Err(upstream_failure(error))
                        }),
                    (ServerMode::Host(host), RpcSurface::Lan) => host
                        .moonlight_certificate_revoke(
                            &request.host_uuid,
                            &request.client_certificate,
                        )
                        .await
                        .map(|removed| {
                            MoonlightCertificateRevokeOutcome::Ok(MoonlightCertificateRevoked {
                                removed,
                            })
                        })
                        .unwrap_or_else(MoonlightCertificateRevokeOutcome::Err),
                    (_, RpcSurface::LocalControl) => {
                        MoonlightCertificateRevokeOutcome::Err(RpcFailure {
                            code: "OperationUnsupported".into(),
                            message:
                                "certificate revoke is unavailable on the local control listener"
                                    .into(),
                        })
                    }
                },
            };
            RpcResponse::MoonlightCertificateRevoke(outcome)
        }
        RpcRequest::SessionPrepare(request) => {
            let outcome = match (&state.mode, state.rpc_surface) {
                (ServerMode::Brain(brain), RpcSurface::Lan) => brain
                    .upstream
                    .prepare_stream(&request.game_id, request.host.as_deref())
                    .await
                    .map(SessionPrepareOutcome::Ok)
                    .unwrap_or_else(|error| SessionPrepareOutcome::Err(upstream_failure(error))),
                (ServerMode::Host(host), RpcSurface::Lan) => host
                    .prepare(
                        &request.game_id,
                        authorization.person_public_key(host.owner_public_key()),
                    )
                    .await
                    .map(SessionPrepareOutcome::Ok)
                    .unwrap_or_else(SessionPrepareOutcome::Err),
                (ServerMode::Brain(_), RpcSurface::LocalControl)
                | (ServerMode::Host(_), RpcSurface::LocalControl) => {
                    SessionPrepareOutcome::Err(RpcFailure {
                        code: "OperationUnsupported".into(),
                        message: "session prepare is unavailable on the local control listener"
                            .into(),
                    })
                }
            };
            RpcResponse::SessionPrepare(outcome)
        }
        RpcRequest::SessionStatus(_) => match (&state.mode, state.rpc_surface) {
            (ServerMode::Brain(brain), RpcSurface::Lan) => RpcResponse::SessionStatus(
                session_status_outcome(brain.upstream.session_status().await),
            ),
            (ServerMode::Host(host), RpcSurface::Lan)
            | (ServerMode::Host(host), RpcSurface::LocalControl) => {
                RpcResponse::SessionStatus(host_session_status_outcome(host.session_status().await))
            }
            (ServerMode::Brain(_), RpcSurface::LocalControl) => {
                RpcResponse::SessionStatus(SessionStatusOutcome::Err(RpcFailure {
                    code: "SessionStatusUnsupported".into(),
                    message: "session status is unavailable on this listener".into(),
                }))
            }
        },
        RpcRequest::SessionStop(request) => match (&state.mode, state.rpc_surface) {
            (ServerMode::Brain(brain), RpcSurface::Lan) => {
                RpcResponse::SessionStop(session_stop_outcome(
                    brain
                        .upstream
                        .session_stop(
                            request.expected_launch_id.as_deref(),
                            request.force.unwrap_or(false),
                        )
                        .await,
                ))
            }
            (ServerMode::Host(host), RpcSurface::Lan)
            | (ServerMode::Host(host), RpcSurface::LocalControl) => {
                let outcome = request
                    .expected_launch_id
                    .as_deref()
                    .ok_or_else(|| RpcFailure {
                        code: "ExpectedLaunchIdRequired".into(),
                        message: "expectedLaunchId is required for exact host stop".into(),
                    })
                    .map(|expected| expected.to_owned());
                let outcome = match outcome {
                    Ok(expected) => host.session_stop(&expected).await,
                    Err(failure) => Err(failure),
                };
                RpcResponse::SessionStop(host_session_stop_outcome(outcome))
            }
            (ServerMode::Brain(_), RpcSurface::LocalControl) => {
                RpcResponse::SessionStop(SessionStopOutcome::Err(RpcFailure {
                    code: "SessionStopUnsupported".into(),
                    message: "session stop is unavailable on this listener".into(),
                }))
            }
        },
        RpcRequest::SessionFreeze(request) => {
            let outcome = match (&state.mode, state.rpc_surface) {
                (ServerMode::Brain(brain), RpcSurface::Lan) => session_freeze_outcome(
                    brain
                        .upstream
                        .session_freeze(request.expected_launch_id.as_deref())
                        .await,
                ),
                (ServerMode::Host(host), RpcSurface::Lan)
                | (ServerMode::Host(host), RpcSurface::LocalControl) => {
                    let outcome =
                        match exact_host_launch_id(request.expected_launch_id.as_deref(), "freeze")
                        {
                            Ok(expected) => host.session_freeze(&expected).await,
                            Err(failure) => Err(failure),
                        };
                    host_session_freeze_outcome(outcome, SessionFreezerState::Frozen)
                }
                (ServerMode::Brain(_), RpcSurface::LocalControl) => {
                    SessionFreezeOutcome::Err(RpcFailure {
                        code: "SessionFreezeUnsupported".into(),
                        message: "session freeze is unavailable on this listener".into(),
                    })
                }
            };
            RpcResponse::SessionFreeze(outcome)
        }
        RpcRequest::SessionThaw(request) => {
            let outcome = match (&state.mode, state.rpc_surface) {
                (ServerMode::Brain(brain), RpcSurface::Lan) => session_freeze_outcome(
                    brain
                        .upstream
                        .session_thaw(request.expected_launch_id.as_deref())
                        .await,
                ),
                (ServerMode::Host(host), RpcSurface::Lan)
                | (ServerMode::Host(host), RpcSurface::LocalControl) => {
                    let outcome =
                        match exact_host_launch_id(request.expected_launch_id.as_deref(), "thaw") {
                            Ok(expected) => host.session_thaw(&expected).await,
                            Err(failure) => Err(failure),
                        };
                    host_session_freeze_outcome(outcome, SessionFreezerState::Running)
                }
                (ServerMode::Brain(_), RpcSurface::LocalControl) => {
                    SessionFreezeOutcome::Err(RpcFailure {
                        code: "SessionThawUnsupported".into(),
                        message: "session thaw is unavailable on this listener".into(),
                    })
                }
            };
            RpcResponse::SessionThaw(outcome)
        }
        RpcRequest::SourceStatus(request) => {
            let outcome = match (&state.mode, state.rpc_surface) {
                (ServerMode::Brain(brain), RpcSurface::Lan) => source_status_outcome(
                    brain
                        .upstream
                        .source_status(&request.device_public_key)
                        .await,
                ),
                (ServerMode::Host(host), RpcSurface::Lan)
                | (ServerMode::Host(host), RpcSurface::LocalControl) => host
                    .source_status(&request.device_public_key)
                    .await
                    .map(SourceStatusOutcome::Ok)
                    .unwrap_or_else(SourceStatusOutcome::Err),
                (ServerMode::Brain(_), RpcSurface::LocalControl) => {
                    SourceStatusOutcome::Err(RpcFailure {
                        code: "SourceStatusUnsupported".into(),
                        message: "source status is unavailable on this listener".into(),
                    })
                }
            };
            RpcResponse::SourceStatus(outcome)
        }
        RpcRequest::LocalGamesList(_) => match &state.mode {
            ServerMode::Brain(brain) => {
                let config_state = brain.config_snapshot.reload();
                let registry = match brain.registry_source.registry() {
                    Ok(registry) => registry,
                    Err(error) => {
                        return Ok(RpcResponse::LocalGamesList(LocalGamesListOutcome::Err(
                            RpcFailure {
                                code: "PluginPolicyInvalid".into(),
                                message: error.to_string(),
                            },
                        )));
                    }
                };
                let catalog = launcher::local_games_with_cover_assets(
                    &brain.local_storage_root,
                    Some(&brain.private_state_root),
                    &config_state,
                    &registry,
                );
                let mut failures = Vec::new();
                if let Some(diagnostic) = &config_state.diagnostic {
                    failures.push(snapshot_diagnostic_failure(diagnostic));
                }
                failures.extend(catalog.diagnostics.iter().map(route_diagnostic_failure));
                RpcResponse::LocalGamesList(LocalGamesListOutcome::Ok(LocalGames {
                    games: catalog.games,
                    failures: (!failures.is_empty()).then_some(failures),
                }))
            }
            ServerMode::Host(_) => {
                RpcResponse::LocalGamesList(LocalGamesListOutcome::Err(RpcFailure {
                    code: "OperationUnsupported".into(),
                    message: "local games are available only from the Android brain".into(),
                }))
            }
        },
        RpcRequest::Health(_) => RpcResponse::Health(HealthOutcome::Ok(Health {
            version: VERSION.into(),
        })),
        RpcRequest::DiscoverySnapshot(_) => match &state.mode {
            ServerMode::Brain(brain) => RpcResponse::DiscoverySnapshot(
                DiscoverySnapshotOutcome::Ok(discovery_snapshot(brain.discovery.snapshot())),
            ),
            ServerMode::Host(_) => {
                RpcResponse::DiscoverySnapshot(DiscoverySnapshotOutcome::Err(RpcFailure {
                    code: "OperationUnsupported".into(),
                    message: "discovery is available only from the Android brain".into(),
                }))
            }
        },
        RpcRequest::DiscoveryRegisterReceipt(request) => match &state.mode {
            ServerMode::Brain(brain) => RpcResponse::DiscoveryRegisterReceipt(
                brain
                    .discovery
                    .register_receipt(&request.receipt)
                    .map(discovery_snapshot)
                    .map(DiscoverySnapshotOutcome::Ok)
                    .unwrap_or_else(|error| {
                        DiscoverySnapshotOutcome::Err(folder_selection_failure(error))
                    }),
            ),
            ServerMode::Host(_) => {
                RpcResponse::DiscoveryRegisterReceipt(DiscoverySnapshotOutcome::Err(RpcFailure {
                    code: "OperationUnsupported".into(),
                    message: "discovery is available only from the Android brain".into(),
                }))
            }
        },
        RpcRequest::DiscoveryRemoveLocation(request) => match &state.mode {
            ServerMode::Brain(brain) => {
                RpcResponse::DiscoveryRemoveLocation(DiscoverySnapshotOutcome::Ok(
                    discovery_snapshot(brain.discovery.remove_location(request.location_id)),
                ))
            }
            ServerMode::Host(_) => {
                RpcResponse::DiscoveryRemoveLocation(DiscoverySnapshotOutcome::Err(RpcFailure {
                    code: "OperationUnsupported".into(),
                    message: "discovery is available only from the Android brain".into(),
                }))
            }
        },
        RpcRequest::DiscoveryRescan(_) => match &state.mode {
            ServerMode::Brain(brain) => RpcResponse::DiscoveryRescan(DiscoverySnapshotOutcome::Ok(
                discovery_snapshot(brain.discovery.rescan()),
            )),
            ServerMode::Host(_) => {
                RpcResponse::DiscoveryRescan(DiscoverySnapshotOutcome::Err(RpcFailure {
                    code: "OperationUnsupported".into(),
                    message: "discovery is available only from the Android brain".into(),
                }))
            }
        },
        RpcRequest::SettingsSnapshot(_) => match &state.mode {
            ServerMode::Brain(brain) => RpcResponse::SettingsSnapshot(
                config::settings::read_with_registry_source(
                    &brain.local_storage_root,
                    &brain.registry_source,
                )
                .and_then(|readable| {
                    config::settings::read_sensitive(&brain.private_state_root)
                        .map(|sensitive| settings_snapshot(readable, sensitive))
                })
                .map(SettingsSnapshotOutcome::Ok)
                .unwrap_or_else(|error| SettingsSnapshotOutcome::Err(settings_failure(error))),
            ),
            ServerMode::Host(_) => {
                RpcResponse::SettingsSnapshot(SettingsSnapshotOutcome::Err(RpcFailure {
                    code: "OperationUnsupported".into(),
                    message: "settings are available only from the Android brain".into(),
                }))
            }
        },
        RpcRequest::SettingsUpdate(request) => match &state.mode {
            ServerMode::Brain(brain) => {
                let change = if request.setting_id == config::settings::DEVICE_NAME_SETTING_ID {
                    Ok(config::settings::SettingChange::DeviceName(request.value))
                } else {
                    request
                        .value
                        .parse::<bool>()
                        .map(|enabled| config::settings::SettingChange::PluginEnabled {
                            id: request.setting_id,
                            enabled,
                        })
                        .map_err(|_| {
                            config::settings::SettingsError::Invalid(
                                "plugin value must be true or false".into(),
                            )
                        })
                };
                let outcome = change.and_then(|change| {
                    config::settings::update_with_registry_source(
                        &brain.local_storage_root,
                        &brain.private_state_root,
                        &brain.settings_write_lock,
                        &request.expected_revision,
                        change,
                        &brain.registry_source,
                    )
                });
                if outcome.is_ok() {
                    brain.config_snapshot.reload();
                    if let Some(wake) = &state.federation_wake {
                        let _ = wake.wake();
                    }
                }
                RpcResponse::SettingsUpdate(
                    outcome
                        .and_then(|readable| {
                            config::settings::read_sensitive(&brain.private_state_root)
                                .map(|sensitive| settings_snapshot(readable, sensitive))
                        })
                        .map(SettingsUpdateOutcome::Ok)
                        .unwrap_or_else(|error| {
                            SettingsUpdateOutcome::Err(settings_failure(error))
                        }),
                )
            }
            ServerMode::Host(_) => {
                RpcResponse::SettingsUpdate(SettingsUpdateOutcome::Err(RpcFailure {
                    code: "OperationUnsupported".into(),
                    message: "settings are available only from the Android brain".into(),
                }))
            }
        },
        RpcRequest::SteamGridDbCredentialSet(request) => match &state.mode {
            ServerMode::Brain(brain) => {
                let _write = brain
                    .settings_write_lock
                    .lock()
                    .expect("settings write lock poisoned");
                RpcResponse::SteamGridDbCredentialSet(
                    config::settings::set_steamgriddb_credential(
                        &brain.private_state_root,
                        &request.token,
                    )
                    .and_then(|status| {
                        crate::enrichment::SteamGridDbEnricher::clear_non_assigned_attempts(
                            &brain.private_state_root,
                        )
                        .map_err(|diagnostic| {
                            config::settings::SettingsError::Storage(diagnostic.message)
                        })?;
                        Ok(status)
                    })
                    .map(|status| SensitiveSettingOutcome::Ok(SensitiveSettingResult { status }))
                    .unwrap_or_else(|error| SensitiveSettingOutcome::Err(settings_failure(error))),
                )
            }
            ServerMode::Host(_) => {
                RpcResponse::SteamGridDbCredentialSet(SensitiveSettingOutcome::Err(RpcFailure {
                    code: "OperationUnsupported".into(),
                    message: "settings are available only from the Android brain".into(),
                }))
            }
        },
        RpcRequest::SteamGridDbCredentialClear(_) => {
            match &state.mode {
                ServerMode::Brain(brain) => {
                    let _write = brain
                        .settings_write_lock
                        .lock()
                        .expect("settings write lock poisoned");
                    RpcResponse::SteamGridDbCredentialClear(
                    config::settings::clear_steamgriddb_credential(&brain.private_state_root)
                        .and_then(|status| {
                            crate::enrichment::SteamGridDbEnricher::clear_non_assigned_attempts(
                                &brain.private_state_root,
                            )
                            .map_err(|diagnostic| config::settings::SettingsError::Storage(diagnostic.message))?;
                            Ok(status)
                        })
                        .map(|status| {
                            SensitiveSettingOutcome::Ok(SensitiveSettingResult { status })
                        })
                        .unwrap_or_else(|error| {
                            SensitiveSettingOutcome::Err(settings_failure(error))
                        }),
                )
                }
                ServerMode::Host(_) => RpcResponse::SteamGridDbCredentialClear(
                    SensitiveSettingOutcome::Err(RpcFailure {
                        code: "OperationUnsupported".into(),
                        message: "settings are available only from the Android brain".into(),
                    }),
                ),
            }
        }
    };
    Ok(response)
}

fn settings_snapshot(
    settings: config::settings::ReadableSettings,
    sensitive: config::settings::SensitiveSettings,
) -> SettingsSnapshot {
    SettingsSnapshot {
        revision: settings.revision,
        device_name: settings.device_name,
        plugins: settings
            .plugins
            .into_iter()
            .map(|plugin| PluginSetting {
                id: plugin.id,
                title: plugin.title,
                enabled: plugin.enabled,
            })
            .collect(),
        steam_grid_db_credential: sensitive.steam_grid_db_credential,
    }
}

fn discovery_snapshot(snapshot: discovery::DiscoverySnapshot) -> DiscoverySnapshot {
    DiscoverySnapshot {
        generation: snapshot.generation,
        state: match snapshot.state {
            discovery::DiscoveryPhase::Idle => DiscoveryState::Idle {},
            discovery::DiscoveryPhase::Scanning => DiscoveryState::Scanning {},
            discovery::DiscoveryPhase::Enriching => DiscoveryState::Enriching {},
            discovery::DiscoveryPhase::Problem => DiscoveryState::Problem {},
        },
        locations: snapshot
            .locations
            .into_iter()
            .map(|location| DiscoveryLocationSummary {
                id: location.id,
                label: location.label,
            })
            .collect(),
        diagnostics: snapshot
            .diagnostics
            .into_iter()
            .map(|diagnostic| DiscoveryDiagnostic {
                code: diagnostic.code,
                message: diagnostic.message,
                location_id: diagnostic.location_id,
            })
            .collect(),
    }
}

fn folder_selection_failure(error: discovery::FolderSelectionGrantError) -> RpcFailure {
    let code = match &error {
        discovery::FolderSelectionGrantError::InvalidPath(_) => "FolderSelectionInvalid",
        discovery::FolderSelectionGrantError::Unknown => "FolderSelectionReceiptUnknown",
        discovery::FolderSelectionGrantError::Expired => "FolderSelectionReceiptExpired",
    };
    RpcFailure {
        code: code.into(),
        message: error.to_string(),
    }
}

fn settings_failure(error: config::settings::SettingsError) -> RpcFailure {
    let code = match &error {
        config::settings::SettingsError::Conflict => "SettingsConflict",
        config::settings::SettingsError::Invalid(_) => "SettingsInvalid",
        config::settings::SettingsError::Storage(_) => "SettingsStorageUnavailable",
        config::settings::SettingsError::Candidate(_) => "SettingsCandidateInvalid",
    };
    RpcFailure {
        code: code.into(),
        message: error.to_string(),
    }
}

async fn rpc(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RpcRequest>,
) -> Result<Json<RpcResponse>, StatusCode> {
    if let Some(access) = &state.portal_access {
        access.authorize(&headers, &request)?;
    }
    let authorization = match state.rpc_surface {
        RpcSurface::Lan => authorization::AuthorizationContext::LocalBrowser,
        RpcSurface::LocalControl => authorization::AuthorizationContext::LocalUnixControl,
    };
    dispatch(&state, &authorization, request)
        .await
        .map(Json)
        .map_err(|_| StatusCode::FORBIDDEN)
}

fn default_local_storage_root() -> PathBuf {
    std::env::var_os("KORRI_LOCAL_STORAGE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("korri"))
}

fn default_private_state_root() -> PathBuf {
    std::env::var_os("KORRID_PRIVATE_STATE_ROOT")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_STATE_HOME")
                .map(PathBuf::from)
                .map(|root| root.join("korri"))
        })
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|root| root.join(".local/state/korri"))
        })
        .unwrap_or_else(|| std::env::temp_dir().join("korri-state"))
}

/// Build the localhost router protected by a per-server bearer capability.
/// The exact portal origin is the only browser origin allowed to send it.
pub fn router_with_capability(rpc_capability: &str, allowed_origin: &str) -> Router {
    router_with_capability_and_roots(
        rpc_capability,
        allowed_origin,
        default_local_storage_root(),
        default_private_state_root(),
    )
}

pub fn router_with_capability_and_roots(
    rpc_capability: &str,
    allowed_origin: &str,
    local_storage_root: impl AsRef<Path>,
    private_state_root: impl AsRef<Path>,
) -> Router {
    router_with_capability_and_federation(
        rpc_capability,
        allowed_origin,
        local_storage_root,
        private_state_root,
        None,
        None,
    )
}

pub fn router_with_capability_and_federation(
    rpc_capability: &str,
    allowed_origin: &str,
    local_storage_root: impl AsRef<Path>,
    private_state_root: impl AsRef<Path>,
    resources: Option<FederationResources>,
    wake: Option<DiscoveryControl>,
) -> Router {
    let local_storage_root = local_storage_root.as_ref().to_owned();
    let signing_key = generate_launch_signing_key();
    let local_launch_reservations =
        Arc::new(Mutex::new(launcher::LaunchPublicationReservations::new()));
    let moonlight_launch_authority = Arc::new(Mutex::new(launcher::MoonlightLaunchAuthority::new(
        signing_key.clone(),
    )));
    let config_snapshot = config::snapshot::ConfigSnapshotCoordinator::new(&local_storage_root);
    router_with_capability_local_root_provision_and_grants(
        rpc_capability,
        allowed_origin,
        local_storage_root,
        private_state_root,
        launcher::FileProvisionMode::Direct,
        signing_key,
        local_launch_reservations,
        moonlight_launch_authority,
        Arc::new(Mutex::new(None)),
        Arc::new(Mutex::new(None)),
        plugin_policy::RegistrySource::Installed,
        config_snapshot,
        discovery::FolderSelectionGrantStore::default(),
        None,
        resources,
        wake,
    )
}

pub fn router_with_capability_and_local_root(
    rpc_capability: &str,
    allowed_origin: &str,
    local_storage_root: impl AsRef<Path>,
) -> Router {
    let local_storage_root = local_storage_root.as_ref().to_owned();
    let signing_key = generate_launch_signing_key();
    let local_launch_reservations =
        Arc::new(Mutex::new(launcher::LaunchPublicationReservations::new()));
    let moonlight_launch_authority = Arc::new(Mutex::new(launcher::MoonlightLaunchAuthority::new(
        signing_key.clone(),
    )));
    let config_snapshot = config::snapshot::ConfigSnapshotCoordinator::new(&local_storage_root);
    router_with_capability_local_root_and_provision(
        rpc_capability,
        allowed_origin,
        local_storage_root,
        default_private_state_root(),
        launcher::FileProvisionMode::Direct,
        signing_key,
        local_launch_reservations,
        moonlight_launch_authority,
        Arc::new(Mutex::new(None)),
        Arc::new(Mutex::new(None)),
        plugin_policy::RegistrySource::Installed,
        config_snapshot,
    )
}

/** Test-only Android router. Production selects this platform only through
 * the Android-gated JNI server entrypoint. */
#[cfg(test)]
fn test_router_with_capability_and_local_root(
    rpc_capability: &str,
    allowed_origin: &str,
    local_storage_root: impl AsRef<Path>,
) -> Router {
    test_router_with_provision(
        rpc_capability,
        allowed_origin,
        local_storage_root,
        launcher::FileProvisionMode::Deferred,
    )
}

#[cfg(test)]
fn test_router_with_provision(
    rpc_capability: &str,
    allowed_origin: &str,
    local_storage_root: impl AsRef<Path>,
    provision: launcher::FileProvisionMode,
) -> Router {
    let local_storage_root = local_storage_root.as_ref().to_owned();
    let signing_key = generate_launch_signing_key();
    let local_launch_reservations =
        Arc::new(Mutex::new(launcher::LaunchPublicationReservations::new()));
    let moonlight_launch_authority = Arc::new(Mutex::new(launcher::MoonlightLaunchAuthority::new(
        signing_key.clone(),
    )));
    let config_snapshot = config::snapshot::ConfigSnapshotCoordinator::new(&local_storage_root);
    router_with_capability_local_root_and_provision(
        rpc_capability,
        allowed_origin,
        local_storage_root.clone(),
        local_storage_root.join(".private-test"),
        provision,
        signing_key,
        local_launch_reservations,
        moonlight_launch_authority,
        Arc::new(Mutex::new(None)),
        Arc::new(Mutex::new(None)),
        plugin_policy::RegistrySource::Selected(Arc::new(
            crate::plugin_test_fixtures::installed(&local_storage_root),
        )),
        config_snapshot,
    )
}

fn router_with_capability_local_root_and_provision(
    rpc_capability: &str,
    allowed_origin: &str,
    local_storage_root: impl AsRef<Path>,
    private_state_root: impl AsRef<Path>,
    local_file_provision: launcher::FileProvisionMode,
    local_launch_signing_key: Vec<u8>,
    local_launch_reservations: Arc<Mutex<launcher::LaunchPublicationReservations>>,
    moonlight_launch_authority: Arc<Mutex<launcher::MoonlightLaunchAuthority>>,
    active_android_launch: Arc<Mutex<Option<TrackedActiveLaunch>>>,
    moonlight_executor_state: Arc<Mutex<Option<MoonlightExecutorState>>>,
    registry_source: plugin_policy::RegistrySource,
    config_snapshot: config::snapshot::ConfigSnapshotCoordinator,
) -> Router {
    router_with_capability_local_root_provision_and_grants(
        rpc_capability,
        allowed_origin,
        local_storage_root,
        private_state_root,
        local_file_provision,
        local_launch_signing_key,
        local_launch_reservations,
        moonlight_launch_authority,
        active_android_launch,
        moonlight_executor_state,
        registry_source,
        config_snapshot,
        discovery::FolderSelectionGrantStore::default(),
        None,
        None,
        None,
    )
}

fn router_with_capability_local_root_provision_and_grants(
    rpc_capability: &str,
    allowed_origin: &str,
    local_storage_root: impl AsRef<Path>,
    private_state_root: impl AsRef<Path>,
    local_file_provision: launcher::FileProvisionMode,
    local_launch_signing_key: Vec<u8>,
    local_launch_reservations: Arc<Mutex<launcher::LaunchPublicationReservations>>,
    moonlight_launch_authority: Arc<Mutex<launcher::MoonlightLaunchAuthority>>,
    active_android_launch: Arc<Mutex<Option<TrackedActiveLaunch>>>,
    moonlight_executor_state: Arc<Mutex<Option<MoonlightExecutorState>>>,
    registry_source: plugin_policy::RegistrySource,
    config_snapshot: config::snapshot::ConfigSnapshotCoordinator,
    folder_selection_grants: discovery::FolderSelectionGrantStore,
    configured_upstream: Option<upstreams::UpstreamRegistry>,
    resources: Option<FederationResources>,
    federation_wake: Option<DiscoveryControl>,
) -> Router {
    let (state, _) = brain_app_state(
        rpc_capability,
        allowed_origin,
        local_storage_root,
        private_state_root,
        local_file_provision,
        local_launch_signing_key,
        local_launch_reservations,
        moonlight_launch_authority,
        active_android_launch,
        moonlight_executor_state,
        registry_source,
        config_snapshot,
        folder_selection_grants,
        configured_upstream,
        resources,
        federation_wake,
    );
    portal_router_from_state(state)
}

#[allow(clippy::too_many_arguments)]
fn brain_app_state(
    rpc_capability: &str,
    allowed_origin: &str,
    local_storage_root: impl AsRef<Path>,
    private_state_root: impl AsRef<Path>,
    local_file_provision: launcher::FileProvisionMode,
    local_launch_signing_key: Vec<u8>,
    local_launch_reservations: Arc<Mutex<launcher::LaunchPublicationReservations>>,
    moonlight_launch_authority: Arc<Mutex<launcher::MoonlightLaunchAuthority>>,
    active_android_launch: Arc<Mutex<Option<TrackedActiveLaunch>>>,
    moonlight_executor_state: Arc<Mutex<Option<MoonlightExecutorState>>>,
    registry_source: plugin_policy::RegistrySource,
    config_snapshot: config::snapshot::ConfigSnapshotCoordinator,
    folder_selection_grants: discovery::FolderSelectionGrantStore,
    configured_upstream: Option<upstreams::UpstreamRegistry>,
    resources: Option<FederationResources>,
    federation_wake: Option<DiscoveryControl>,
) -> (AppState, ()) {
    let local_storage_root = local_storage_root.as_ref().to_owned();
    let private_state_root = private_state_root.as_ref().to_owned();
    let settings_write_lock = Arc::new(Mutex::new(()));
    let discovery = discovery::DiscoveryLifecycleCoordinator::new_with_registry_source(
        &local_storage_root,
        &private_state_root,
        settings_write_lock.clone(),
        folder_selection_grants.clone(),
        registry_source.clone(),
    );
    #[cfg(not(test))]
    let resources = resources.or_else(|| {
        Some(FederationResources::open(&private_state_root).expect("open federation authority"))
    });
    let upstream = configured_upstream.unwrap_or_else(|| {
        if let Some(resources) = &resources {
            upstreams::UpstreamRegistry::from_env_or_file(
                &local_storage_root.join("upstreams.json"),
                resources.credentials.clone(),
            )
            .with_federation(resources.directory.clone())
        } else {
            #[cfg(test)]
            {
                upstreams::UpstreamRegistry::from_env_or_file_for_tests(
                    &local_storage_root.join("upstreams.json"),
                )
            }
            #[cfg(not(test))]
            {
                unreachable!("production federation composed above")
            }
        }
    });
    let local_owner_public_key = brain_owner_public_key(&private_state_root);
    let mut portal_access =
        PortalAccess::new(rpc_capability, allowed_origin, PortalPermission::Full);
    let state = AppState {
        federation: resources.map(|resources| resources.directory),
        federation_wake,
        mode: ServerMode::Brain(BrainRuntime {
            upstream,
            local_storage_root,
            private_state_root,
            local_owner_public_key,
            local_file_provision,
            local_launch_signing_key,
            local_launch_reservations,
            moonlight_launch_authority,
            active_android_launch,
            moonlight_executor_state,
            registry_source,
            config_snapshot,
            discovery,
            settings_write_lock,
        }),
        portal_access: Some(portal_access),
        rpc_surface: RpcSurface::Lan,
    };
    (state, ())
}

fn portal_router_from_state(state: AppState) -> Router {
    let access = state
        .portal_access
        .as_ref()
        .expect("portal router requires token authority");
    let allowed_origins = tower_http::cors::AllowOrigin::list(access.allowed_origins().to_vec());
    let cors = tower_http::cors::CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_methods([Method::POST])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);
    Router::new()
        .route("/rpc", post(rpc))
        .layer(cors)
        .with_state(state)
}

/// Build the LAN-facing native host router.
/// Production accepts only signed and encrypted peer RPC on `/peer-rpc`.
pub fn host_router(config_path: impl AsRef<Path>) -> Router {
    #[cfg(test)]
    {
        host_routers_with_in_memory_units(config_path).0
    }
    #[cfg(not(test))]
    {
        host_router_with_storage(config_path, None::<PathBuf>)
    }
}

pub fn host_router_with_storage(
    config_path: impl AsRef<Path>,
    storage_root: Option<impl Into<PathBuf>>,
) -> Router {
    #[cfg(test)]
    {
        let private = tempfile::tempdir().expect("private host state").keep();
        let runtime = host::HostRuntime::from_paths_with_backend(
            config_path.as_ref(),
            storage_root.map(Into::into),
            private,
            Arc::new(host::control::InMemoryLaunchUnitBackend::default()),
        );
        plain_host_routers(runtime).0
    }
    #[cfg(not(test))]
    {
        host_routers_with_storage_and_private(
            config_path,
            storage_root,
            PathBuf::from("korri-state"),
            None,
        )
        .0
    }
}

/// Build network and private-control routers over one shared device runtime.
/// Optional portal authority enables the same browser `/rpc` handler used by
/// the embedded brain, alongside encrypted `/peer-rpc` on the existing router.
/// It does not grant the browser the private-control socket's authority.
pub fn host_routers_with_storage_and_private(
    config_path: impl AsRef<Path>,
    storage_root: Option<impl Into<PathBuf>>,
    private_state_root: impl Into<PathBuf>,
    portal_access: Option<PortalAccess>,
) -> (Router, Router) {
    let private_state_root = private_state_root.into();
    let resources =
        FederationResources::open(&private_state_root).expect("open federation authority");
    host_routers_with_federation(
        config_path,
        storage_root,
        private_state_root,
        resources,
        portal_access,
    )
}

/// Build the same routers over federation resources the caller already owns,
/// so one process shares a single directory, credentials and authorization
/// with its discovery coordinator.
pub fn host_routers_with_federation(
    config_path: impl AsRef<Path>,
    storage_root: Option<impl Into<PathBuf>>,
    private_state_root: impl Into<PathBuf>,
    resources: FederationResources,
    portal_access: Option<PortalAccess>,
) -> (Router, Router) {
    let private_state_root = private_state_root.into();
    let runtime = host::HostRuntime::from_paths_with_private_state(
        config_path.as_ref(),
        storage_root.map(Into::into),
        private_state_root.clone(),
    );
    secure_host_routers_with_federation(runtime, &private_state_root, resources, portal_access)
}

#[cfg(test)]
fn host_routers_with_in_memory_units(config_path: impl AsRef<Path>) -> (Router, Router) {
    let private = tempfile::tempdir().expect("private host state").keep();
    let runtime = host::HostRuntime::from_paths_with_backend(
        config_path.as_ref(),
        None,
        private,
        Arc::new(host::control::InMemoryLaunchUnitBackend::default()),
    );
    plain_host_routers(runtime)
}

#[cfg(test)]
fn host_router_with_in_memory_units(config_path: impl AsRef<Path>) -> Router {
    host_routers_with_in_memory_units(config_path).0
}

#[cfg(test)]
fn secure_host_router_with_in_memory_units_at(
    config_path: impl AsRef<Path>,
    private_state_root: &Path,
    now: u64,
) -> Router {
    let runtime = host::HostRuntime::from_paths_with_backend(
        config_path.as_ref(),
        None,
        private_state_root.to_owned(),
        Arc::new(host::control::InMemoryLaunchUnitBackend::default()),
    );
    let (lan, _) = app_states(runtime);
    peer_rpc::PeerRpcServer::new_at(lan, private_state_root, now)
        .expect("load or create peer RPC identity")
        .router(None)
}

fn app_states(runtime: host::HostRuntime) -> (AppState, AppState) {
    let lan = AppState {
        federation: None,
        federation_wake: None,
        mode: ServerMode::Host(runtime.clone()),
        portal_access: None,
        rpc_surface: RpcSurface::Lan,
    };
    let local = AppState {
        federation: None,
        federation_wake: None,
        mode: ServerMode::Host(runtime),
        portal_access: None,
        rpc_surface: RpcSurface::LocalControl,
    };
    (lan, local)
}

#[cfg(test)]
fn plain_host_routers(runtime: host::HostRuntime) -> (Router, Router) {
    let (lan, local) = app_states(runtime);
    (
        Router::new().route("/rpc", post(rpc)).with_state(lan),
        Router::new().route("/rpc", post(rpc)).with_state(local),
    )
}

#[cfg(test)]
pub(crate) fn plain_host_routers_for_tests(runtime: host::HostRuntime) -> (Router, Router) {
    plain_host_routers(runtime)
}

/// Give the browser its own `/rpc` router over the LAN state, carrying the
/// portal's capability and origin without the private-control authority.
fn portal_router_for(lan: &AppState, portal_access: Option<PortalAccess>) -> Option<Router> {
    portal_access.map(|access| {
        let mut browser = lan.clone();
        browser.portal_access = Some(access);
        portal_router_from_state(browser)
    })
}

#[cfg(test)]
fn secure_host_routers(
    runtime: host::HostRuntime,
    private_state_root: &Path,
    portal_access: Option<PortalAccess>,
) -> (Router, Router) {
    let (lan, local) = app_states(runtime);
    let portal = portal_router_for(&lan, portal_access);
    let peer = peer_rpc::PeerRpcServer::new(lan, private_state_root)
        .expect("load or create peer RPC identity");
    (
        peer.router(portal),
        Router::new().route("/rpc", post(rpc)).with_state(local),
    )
}

fn secure_host_routers_with_federation(
    runtime: host::HostRuntime,
    private_state_root: &Path,
    resources: FederationResources,
    portal_access: Option<PortalAccess>,
) -> (Router, Router) {
    let (mut lan, mut local) = app_states(runtime);
    lan.federation = Some(resources.directory.clone());
    local.federation = Some(resources.directory);
    let portal = portal_router_for(&lan, portal_access);
    let peer = peer_rpc::PeerRpcServer::with_shared_authority(
        lan,
        private_state_root,
        resources.credentials,
        resources.authorization,
    )
    .expect("load peer replay authority");
    (
        peer.router(portal),
        Router::new().route("/rpc", post(rpc)).with_state(local),
    )
}

/// Generate an unguessable capability for one server lifetime.
pub fn generate_rpc_capability() -> String {
    let bytes: [u8; 32] = rand::random();
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/** Create the identity korrid carries through one prepared launch. */
pub fn generate_launch_id() -> String {
    let bytes: [u8; 16] = rand::random();
    hex::encode(bytes)
}

fn generate_launch_signing_key() -> Vec<u8> {
    let bytes: [u8; 32] = rand::random();
    bytes.to_vec()
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("korrid server is already running")]
    AlreadyRunning,
    #[error("korrid server is not running")]
    NotRunning,
    #[error("failed to start korrid server: {details}")]
    StartFailed { details: String },
}

struct ServerHandle {
    port: u16,
    rpc_capability: String,
    private_state_root: PathBuf,
    launch_signing_key: Vec<u8>,
    local_launch_reservations: Arc<Mutex<launcher::LaunchPublicationReservations>>,
    active_android_launch: Arc<Mutex<Option<TrackedActiveLaunch>>>,
    moonlight_executor_state: Arc<Mutex<Option<MoonlightExecutorState>>>,
    platform_instruction_verifier: Option<launcher::PlatformInstructionVerifier>,
    moonlight_launch_authority: Arc<Mutex<launcher::MoonlightLaunchAuthority>>,
    moonlight_config_snapshot: config::snapshot::ConfigSnapshotCoordinator,
    registry_source: plugin_policy::RegistrySource,
    folder_selection_grants: discovery::FolderSelectionGrantStore,
    upstream: upstreams::UpstreamRegistry,
    federation: FederationResources,
    federation_wake: DiscoveryControl,
    stop: oneshot::Sender<()>,
    thread: JoinHandle<()>,
}

static SERVER: OnceLock<Mutex<Option<ServerHandle>>> = OnceLock::new();

fn server_slot() -> &'static Mutex<Option<ServerHandle>> {
    SERVER.get_or_init(|| Mutex::new(None))
}

pub fn korrid_version() -> String {
    VERSION.into()
}

/// Starts the standalone localhost brain. Platform-native integrations are
/// unavailable through this public host entrypoint.
pub fn start_local_server(
    allowed_origin: &str,
    local_storage_root: &str,
    private_state_root: &str,
) -> Result<u16, ServerError> {
    start_local_server_for_platform(
        allowed_origin,
        local_storage_root,
        private_state_root,
        plugin_policy::RegistrySource::Installed,
    )
}

/** Android production reaches Artemis only through this target-gated JNI
 * entrypoint, never through caller-provided platform data. */
#[cfg(target_os = "android")]
pub(crate) fn start_embedded_android_server(
    allowed_origin: &str,
    local_storage_root: &str,
    private_state_root: &str,
) -> Result<u16, ServerError> {
    start_local_server_for_platform(
        allowed_origin,
        local_storage_root,
        private_state_root,
        plugin_policy::RegistrySource::Installed,
    )
}

fn start_local_server_for_platform(
    allowed_origin: &str,
    local_storage_root: &str,
    private_state_root: &str,
    registry_source: plugin_policy::RegistrySource,
) -> Result<u16, ServerError> {
    let mut slot = server_slot().lock().expect("server mutex poisoned");
    if slot.is_some() {
        return Err(ServerError::AlreadyRunning);
    }

    let listener =
        StdTcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).map_err(|error| {
            ServerError::StartFailed {
                details: error.to_string(),
            }
        })?;
    listener
        .set_nonblocking(true)
        .map_err(|error| ServerError::StartFailed {
            details: error.to_string(),
        })?;
    let port = listener
        .local_addr()
        .map_err(|error| ServerError::StartFailed {
            details: error.to_string(),
        })?
        .port();
    let rpc_capability = generate_rpc_capability();
    let server_capability = rpc_capability.clone();
    let launch_signing_key = generate_launch_signing_key();
    let server_signing_key = launch_signing_key.clone();
    let local_launch_reservations =
        Arc::new(Mutex::new(launcher::LaunchPublicationReservations::new()));
    let server_local_launch_reservations = Arc::clone(&local_launch_reservations);
    let moonlight_launch_authority = Arc::new(Mutex::new(launcher::MoonlightLaunchAuthority::new(
        launch_signing_key.clone(),
    )));
    let server_moonlight_launch_authority = Arc::clone(&moonlight_launch_authority);
    let active_android_launch = Arc::new(Mutex::new(None));
    let router_active_android_launch = Arc::clone(&active_android_launch);
    let moonlight_executor_state = Arc::new(Mutex::new(None));
    let router_moonlight_executor_state = Arc::clone(&moonlight_executor_state);
    let folder_selection_grants = discovery::FolderSelectionGrantStore::default();
    let server_folder_selection_grants = folder_selection_grants.clone();
    let allowed_origin = allowed_origin.to_owned();
    let local_storage_root = local_storage_root.to_owned();
    let private_state_root = private_state_root.to_owned();
    let handle_private_state_root = PathBuf::from(&private_state_root);
    let moonlight_config_snapshot =
        config::snapshot::ConfigSnapshotCoordinator::new(&local_storage_root);
    let server_config_snapshot = moonlight_config_snapshot.clone();
    let server_registry_source = registry_source.clone();
    let federation =
        FederationResources::open(Path::new(&private_state_root)).map_err(|error| {
            ServerError::StartFailed {
                details: error.to_string(),
            }
        })?;
    let upstream = upstreams::UpstreamRegistry::from_env_or_file(
        &Path::new(&local_storage_root).join("upstreams.json"),
        federation.credentials.clone(),
    )
    .with_federation(federation.directory.clone());
    let server_federation = federation.clone();
    let relay_config = moonlight_config_snapshot.clone();
    let (federation_wake, discovery) = Discovery::new(
        federation.directory.clone(),
        federation.credentials.clone(),
        Arc::new(move || DiscoveryInputs::android(&relay_config)),
        Arc::new(relay::WebSocketRelayTransport::new()),
        DiscoveryTiming::default(),
        Arc::new(peer_rpc::unix_time),
    )
    .start();
    let server_federation_wake = federation_wake.clone();
    let server_upstream = upstream.clone();
    let (stop, stopped) = oneshot::channel();
    let thread = std::thread::Builder::new()
        .name("korrid".into())
        .spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("create Tokio runtime");
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener)
                    .expect("convert localhost listener");
                let shutdown_discovery = server_federation_wake.clone();
                let server = axum::serve(
                    listener,
                    router_with_capability_local_root_provision_and_grants(
                        &server_capability,
                        &allowed_origin,
                        &local_storage_root,
                        &private_state_root,
                        launcher::FileProvisionMode::Deferred,
                        server_signing_key,
                        server_local_launch_reservations,
                        server_moonlight_launch_authority,
                        router_active_android_launch,
                        router_moonlight_executor_state,
                        server_registry_source.clone(),
                        server_config_snapshot,
                        server_folder_selection_grants,
                        Some(server_upstream),
                        Some(server_federation),
                        Some(server_federation_wake.clone()),
                    ),
                )
                .with_graceful_shutdown(async move {
                    let _ = stopped.await;
                    // Keep existing RPC draining, but do not wait for it to
                    // cancel a blocked relay operation.
                    shutdown_discovery.cancel();
                });
                // Both tasks live inside this runtime and are joined before it exits.
                let discovery = tokio::spawn(discovery);
                let result = server.await;
                server_federation_wake.cancel();
                discovery.await.expect("join relay discovery");
                result.expect("serve korrid");
            });
        })
        .map_err(|error| ServerError::StartFailed {
            details: error.to_string(),
        })?;

    *slot = Some(ServerHandle {
        port,
        rpc_capability,
        private_state_root: handle_private_state_root,
        launch_signing_key,
        local_launch_reservations,
        active_android_launch,
        moonlight_executor_state,
        platform_instruction_verifier: None,
        moonlight_launch_authority,
        moonlight_config_snapshot,
        registry_source,
        folder_selection_grants,
        upstream,
        federation,
        federation_wake,
        stop,
        thread,
    });
    Ok(port)
}

pub fn stop_local_server() -> Result<(), ServerError> {
    let handle = server_slot()
        .lock()
        .expect("server mutex poisoned")
        .take()
        .ok_or(ServerError::NotRunning)?;
    let _ = handle.stop.send(());
    handle.thread.join().map_err(|_| ServerError::StartFailed {
        details: "server thread panicked".into(),
    })?;
    Ok(())
}

fn running_private_state_root() -> Result<PathBuf, ServerError> {
    server_slot()
        .lock()
        .expect("server mutex poisoned")
        .as_ref()
        .map(|handle| handle.private_state_root.clone())
        .ok_or(ServerError::NotRunning)
}

pub fn local_identity_status_json() -> Result<String, ServerError> {
    let root = running_private_state_root()?;
    let identity = identity::DeviceIdentity::load_or_create(&root).map_err(|error| {
        ServerError::StartFailed {
            details: error.to_string(),
        }
    })?;
    serde_json::to_string(identity.state()).map_err(|error| ServerError::StartFailed {
        details: error.to_string(),
    })
}

pub fn local_owner_binding_template(created_at: u64) -> Result<String, ServerError> {
    let root = running_private_state_root()?;
    let identity = identity::DeviceIdentity::load_or_create(&root).map_err(|error| {
        ServerError::StartFailed {
            details: error.to_string(),
        }
    })?;
    identity
        .owner_statement_template(identity::OwnerStatementStatus::Owned, created_at)
        .map_err(|error| ServerError::StartFailed {
            details: error.to_string(),
        })
}

pub fn apply_local_owner_binding(
    unsigned_template_json: &str,
    expected_owner_public_key: &str,
    signed_event_json: &str,
) -> Result<String, ServerError> {
    let (root, credentials, wake) = {
        let slot = server_slot().lock().expect("server mutex poisoned");
        let server = slot.as_ref().ok_or(ServerError::NotRunning)?;
        (
            server.private_state_root.clone(),
            server.federation.credentials.clone(),
            server.federation_wake.clone(),
        )
    };
    let mut identity = identity::DeviceIdentity::load_or_create(&root).map_err(|error| {
        ServerError::StartFailed {
            details: error.to_string(),
        }
    })?;
    identity
        .apply_signed_owner_binding(
            unsigned_template_json,
            expected_owner_public_key,
            signed_event_json,
        )
        .map_err(|error| ServerError::StartFailed {
            details: error.to_string(),
        })?;
    // Persist first, then replace inside the original credentials mutex. Existing
    // clients and discovery see the binding without rotating browser authority.
    credentials
        .reload_identity(&root)
        .map_err(|error| ServerError::StartFailed {
            details: error.to_string(),
        })?;
    wake.wake().map_err(|error| ServerError::StartFailed {
        details: error.to_string(),
    })?;
    serde_json::to_string(identity.state()).map_err(|error| ServerError::StartFailed {
        details: error.to_string(),
    })
}

pub fn local_server_port() -> Option<u16> {
    server_slot()
        .lock()
        .expect("server mutex poisoned")
        .as_ref()
        .map(|server| server.port)
}

pub fn local_server_capability() -> Option<String> {
    server_slot()
        .lock()
        .expect("server mutex poisoned")
        .as_ref()
        .map(|server| server.rpc_capability.clone())
}

pub fn moonlight_host_candidates() -> Result<Vec<upstreams::MoonlightHostCandidate>, ServerError> {
    let upstream = server_slot()
        .lock()
        .expect("server mutex poisoned")
        .as_ref()
        .map(|server| server.upstream.clone())
        .ok_or(ServerError::NotRunning)?;
    upstream
        .moonlight_host_candidates()
        .map_err(|error| ServerError::StartFailed {
            details: error.to_string(),
        })
}

pub fn issue_folder_selection_receipt(
    canonical_approved_path: &str,
) -> Result<String, discovery::FolderSelectionGrantError> {
    let store = server_slot()
        .lock()
        .expect("server mutex poisoned")
        .as_ref()
        .map(|server| server.folder_selection_grants.clone())
        .ok_or(discovery::FolderSelectionGrantError::Unknown)?;
    store
        .issue_approved_path(canonical_approved_path)
        .map(|grant| grant.token)
}

pub mod host;
pub mod launcher;
pub mod plugin;
pub mod plugin_installation;
pub mod plugin_policy;
mod plugin_references;
/// Real plugin declarations used as test data. Shipped with the library so
/// that integration tests can build the same registry a unit test builds.
pub mod plugin_test_fixtures;
pub mod script;
#[cfg(test)]
mod settings_secrets_tests;
pub mod upstream;
pub mod upstream_native;
pub mod upstreams;

#[cfg(test)]
mod tests {
    mod peer_list {
        include!("peer_list_tests.rs");
    }
    mod federation_lifecycle {
        include!("federation/lifecycle_tests.rs");
    }
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{header, Request, StatusCode},
    };
    use nostr::{
        event::{EventBuilder, FinalizeEvent, Kind, Tag},
        key::Keys,
        types::Timestamp,
    };
    use tower::ServiceExt;

    const PNG_1X1: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 4,
        0, 0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 156, 99, 250, 207, 0, 0, 2, 7,
        1, 2, 154, 28, 49, 113, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];

    /// The embedded server is a process singleton, so the tests that start it
    /// must not overlap. Poisoning is irrelevant here: the guard only orders
    /// them, so a panicking test still hands the next one a usable lock.
    static EMBEDDED_SERVER_TEST_LOCK: Mutex<()> = Mutex::new(());

    pub(super) fn embedded_server_guard() -> std::sync::MutexGuard<'static, ()> {
        EMBEDDED_SERVER_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write_wl4_plugin_config(root: &Path) {
        config::test_fixtures::gba(root);
    }
    fn write_checkpoint_android_config(root: &Path) {
        config::test_fixtures::android(root);
    }



    fn control(interaction: SessionControlInteraction) -> SessionControl {
        SessionControl {
            id: "control".into(),
            label: "Control".into(),
            description: None,
            enabled: true,
            disabled_reason: None,
            destructive: false,
            dismiss_on_success: false,
            interaction,
        }
    }

    fn invocation(value: Option<SessionControlValue>) -> SessionControlInvokeRequest {
        SessionControlInvokeRequest {
            launch_id: "current".into(),
            control_id: "control".into(),
            value,
        }
    }

    fn moonlight_executor_state(launch_id: &str) -> MoonlightExecutorState {
        use launcher::AndroidMoonlightEffect as Effect;
        let value = |effect| match effect {
            Effect::SetFillMode
            | Effect::SetZoomMode
            | Effect::SetFaceButtonFlip
            | Effect::SetPictureInPicture => Some(SessionControlValue::Toggle(false)),
            Effect::SetRumble => Some(SessionControlValue::Toggle(true)),
            Effect::SetMouseMode => Some(SessionControlValue::Choice("0".into())),
            Effect::SetSgsrSharpness => Some(SessionControlValue::Range(20.0)),
            Effect::SetSgsrEdgeThreshold => Some(SessionControlValue::Range(8.0)),
            Effect::SetStreamBitrateKbps => Some(SessionControlValue::Range(12345.0)),
            Effect::SetStreamFps => Some(SessionControlValue::Range(60.0)),
            Effect::SetStreamWidth => Some(SessionControlValue::Range(1920.0)),
            _ => None,
        };
        let effects = [
            Effect::Disconnect,
            Effect::QuitHost,
            Effect::ToggleKeyboard,
            Effect::ToggleFullKeyboard,
            Effect::SetFillMode,
            Effect::SetZoomMode,
            Effect::RotateScreen,
            Effect::ToggleHud,
            Effect::ToggleFloatingMenu,
            Effect::ToggleKeyboardController,
            Effect::SwitchTouchSensitivity,
            Effect::SetMouseMode,
            Effect::SetLocalCursor,
            Effect::SetSgsrEdgeThreshold,
            Effect::SetSgsrSharpness,
            Effect::SetFaceButtonFlip,
            Effect::SetRumble,
            Effect::SetPictureInPicture,
            Effect::SetStreamBitrateKbps,
            Effect::RestoreStreamBitrate,
            Effect::SetStreamFps,
            Effect::RestoreStreamFps,
            Effect::SetStreamWidth,
            Effect::RestoreStreamResolution,
        ]
        .into_iter()
        .map(|effect| MoonlightExecutorEffectState {
            effect,
            fulfillable: true,
            value: value(effect),
            range: match effect {
                Effect::SetStreamBitrateKbps => Some(MoonlightExecutorRangeState {
                    min: 500.0,
                    max: 150000.0,
                    step: 1.0,
                }),
                Effect::SetStreamFps => Some(MoonlightExecutorRangeState {
                    min: 1.0,
                    max: 120.0,
                    step: 1.0,
                }),
                Effect::SetStreamWidth => Some(MoonlightExecutorRangeState {
                    min: 2.0,
                    max: 1920.0,
                    step: 2.0,
                }),
                _ => None,
            },
        })
        .collect();
        MoonlightExecutorState {
            launch_id: launch_id.into(),
            executor_id: "android-moonlight".into(),
            generation: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            effects,
        }
    }

    #[test]
    fn moonlight_live_ranges_are_strict_and_unfulfillable_effects_have_no_payload() {
        use launcher::AndroidMoonlightEffect as Effect;
        let mut state = moonlight_executor_state("launch");
        assert!(state.is_strict());
        let bitrate = state
            .effects
            .iter_mut()
            .find(|entry| entry.effect == Effect::SetStreamBitrateKbps)
            .unwrap();
        bitrate.range.as_mut().unwrap().max = 150500.0;
        assert!(!state.is_strict());
        let mut state = moonlight_executor_state("launch");
        let fps = state
            .effects
            .iter_mut()
            .find(|entry| entry.effect == Effect::SetStreamFps)
            .unwrap();
        fps.fulfillable = false;
        fps.value = None;
        fps.range = None;
        assert!(!state.is_strict());
        let restore = state
            .effects
            .iter_mut()
            .find(|entry| entry.effect == Effect::RestoreStreamFps)
            .unwrap();
        restore.fulfillable = false;
        assert!(state.is_strict());
        for (set, restore) in [
            (Effect::SetStreamBitrateKbps, Effect::RestoreStreamBitrate),
            (Effect::SetStreamFps, Effect::RestoreStreamFps),
            (Effect::SetStreamWidth, Effect::RestoreStreamResolution),
        ] {
            let mut state = moonlight_executor_state("launch");
            let entry = state
                .effects
                .iter_mut()
                .find(|entry| entry.effect == set)
                .unwrap();
            entry.fulfillable = false;
            entry.value = None;
            entry.range = None;
            assert!(!state.is_strict());
            let entry = state
                .effects
                .iter_mut()
                .find(|entry| entry.effect == restore)
                .unwrap();
            entry.fulfillable = false;
            assert!(state.is_strict());
        }
    }

    #[test]
    fn moonlight_dynamic_ranges_reject_fractional_and_odd_integer_facts() {
        use launcher::AndroidMoonlightEffect as Effect;
        let mut malformed = Vec::new();

        let mut state = moonlight_executor_state("launch");
        state
            .effects
            .iter_mut()
            .find(|entry| entry.effect == Effect::SetStreamBitrateKbps)
            .unwrap()
            .value = Some(SessionControlValue::Range(12345.5));
        malformed.push(state);

        let mut state = moonlight_executor_state("launch");
        state
            .effects
            .iter_mut()
            .find(|entry| entry.effect == Effect::SetStreamBitrateKbps)
            .unwrap()
            .range
            .as_mut()
            .unwrap()
            .min = 500.5;
        malformed.push(state);

        let mut state = moonlight_executor_state("launch");
        state
            .effects
            .iter_mut()
            .find(|entry| entry.effect == Effect::SetStreamFps)
            .unwrap()
            .range
            .as_mut()
            .unwrap()
            .max = 120.5;
        malformed.push(state);

        for odd in [
            (Some(1919.0), None, None),
            (None, Some(3.0), None),
            (None, None, Some(1919.0)),
        ] {
            let mut state = moonlight_executor_state("launch");
            let width = state
                .effects
                .iter_mut()
                .find(|entry| entry.effect == Effect::SetStreamWidth)
                .unwrap();
            if let Some(value) = odd.0 {
                width.value = Some(SessionControlValue::Range(value));
            }
            if let Some(min) = odd.1 {
                width.range.as_mut().unwrap().min = min;
            }
            if let Some(max) = odd.2 {
                width.range.as_mut().unwrap().max = max;
            }
            malformed.push(state);
        }

        let mut state = moonlight_executor_state("launch");
        state
            .effects
            .iter_mut()
            .find(|entry| entry.effect == Effect::SetStreamWidth)
            .unwrap()
            .value = Some(SessionControlValue::Range(1918.5));
        malformed.push(state);

        for state in malformed {
            assert!(!state.is_strict());
        }
    }





    fn assert_invalid_range(interaction: SessionControlInteraction, submitted: f64) {
        assert_eq!(
            validate_session_control_invocation(
                "current",
                &invocation(Some(SessionControlValue::Range(submitted))),
                &control(interaction),
            )
            .expect_err("range invocation must be rejected")
            .reason,
            SessionControlFailureReason::InvalidValue
        );
    }

    #[test]
    fn session_control_values_are_validated_before_effect_resolution() {
        assert!(validate_session_control_invocation(
            "current",
            &invocation(None),
            &control(SessionControlInteraction::Command),
        )
        .is_ok());
        assert!(validate_session_control_invocation(
            "current",
            &invocation(Some(SessionControlValue::Toggle(true))),
            &control(SessionControlInteraction::Toggle {
                value: false,
                true_label: "On".into(),
                false_label: "Off".into(),
            }),
        )
        .is_ok());
        assert!(validate_session_control_invocation(
            "current",
            &invocation(Some(SessionControlValue::Choice("direct".into()))),
            &control(SessionControlInteraction::Choice {
                value: "trackpad".into(),
                options: vec![
                    SessionControlChoice {
                        value: "trackpad".into(),
                        label: "Trackpad".into(),
                    },
                    SessionControlChoice {
                        value: "direct".into(),
                        label: "Direct".into(),
                    },
                ],
            }),
        )
        .is_ok());
        assert!(validate_session_control_invocation(
            "current",
            &invocation(Some(SessionControlValue::Range(55.0))),
            &control(SessionControlInteraction::Range {
                value: 50.0,
                min: 0.0,
                max: 100.0,
                step: 5.0,
            }),
        )
        .is_ok());
    }

    #[test]
    fn range_validation_rejects_large_step_off_grid_values() {
        assert_invalid_range(
            SessionControlInteraction::Range {
                value: 0.0,
                min: 0.0,
                max: 2_000_000_000_000.0,
                step: 1_000_000_000_000.0,
            },
            1.0,
        );
    }

    #[test]
    fn range_validation_bounds_grid_tolerance_for_translated_large_ranges() {
        let min = 1_000_000_000_000_000.0;
        let interaction = SessionControlInteraction::Range {
            value: min,
            min,
            max: min + 10.0,
            step: 1.0,
        };

        assert_invalid_range(interaction.clone(), min + 0.5);
        for submitted in [min, min + 1.0, min + 10.0] {
            assert!(validate_session_control_invocation(
                "current",
                &invocation(Some(SessionControlValue::Range(submitted))),
                &control(interaction.clone()),
            )
            .is_ok());
        }
    }

    #[test]
    fn range_validation_rejects_steps_that_cannot_advance_from_min() {
        let min = 1_000_000_000_000_000.0;
        assert_invalid_range(
            SessionControlInteraction::Range {
                value: min,
                min,
                max: min + 10.0,
                step: 0.01,
            },
            min,
        );
    }

    #[test]
    fn range_validation_rejects_malformed_metadata_and_values() {
        let malformed = [
            SessionControlInteraction::Range {
                value: f64::NAN,
                min: 0.0,
                max: 10.0,
                step: 1.0,
            },
            SessionControlInteraction::Range {
                value: 5.0,
                min: f64::NEG_INFINITY,
                max: 10.0,
                step: 1.0,
            },
            SessionControlInteraction::Range {
                value: 5.0,
                min: 0.0,
                max: f64::INFINITY,
                step: 1.0,
            },
            SessionControlInteraction::Range {
                value: 5.0,
                min: 0.0,
                max: 10.0,
                step: f64::NAN,
            },
            SessionControlInteraction::Range {
                value: 5.0,
                min: 0.0,
                max: 10.0,
                step: 0.0,
            },
            SessionControlInteraction::Range {
                value: 5.0,
                min: 0.0,
                max: 10.0,
                step: -1.0,
            },
            SessionControlInteraction::Range {
                value: 5.0,
                min: 10.0,
                max: 0.0,
                step: 1.0,
            },
            SessionControlInteraction::Range {
                value: 11.0,
                min: 0.0,
                max: 10.0,
                step: 1.0,
            },
            SessionControlInteraction::Range {
                value: 5.5,
                min: 0.0,
                max: 10.0,
                step: 1.0,
            },
        ];
        for interaction in malformed {
            assert_invalid_range(interaction, 5.0);
        }

        let valid_metadata = SessionControlInteraction::Range {
            value: 5.0,
            min: 0.0,
            max: 10.0,
            step: 1.0,
        };
        for submitted in [f64::NAN, f64::NEG_INFINITY, f64::INFINITY, 5.5] {
            assert_invalid_range(valid_metadata.clone(), submitted);
        }
    }

    #[test]
    fn range_validation_preserves_exact_endpoints_and_decimal_steps() {
        let endpoint_range = control(SessionControlInteraction::Range {
            value: 10.0,
            min: 0.0,
            max: 10.0,
            step: 3.0,
        });
        for submitted in [0.0, 10.0] {
            assert!(validate_session_control_invocation(
                "current",
                &invocation(Some(SessionControlValue::Range(submitted))),
                &endpoint_range,
            )
            .is_ok());
        }

        assert!(validate_session_control_invocation(
            "current",
            &invocation(Some(SessionControlValue::Range(0.3))),
            &control(SessionControlInteraction::Range {
                value: 0.2,
                min: 0.0,
                max: 1.0,
                step: 0.1,
            }),
        )
        .is_ok());
    }

    #[test]
    fn malformed_disabled_and_stale_session_invocations_are_rejected() {
        let invalid_cases = [
            (
                control(SessionControlInteraction::Command),
                invocation(Some(SessionControlValue::Toggle(true))),
                SessionControlFailureReason::InvalidValue,
            ),
            (
                control(SessionControlInteraction::Choice {
                    value: "trackpad".into(),
                    options: vec![SessionControlChoice {
                        value: "trackpad".into(),
                        label: "Trackpad".into(),
                    }],
                }),
                invocation(Some(SessionControlValue::Choice("outside".into()))),
                SessionControlFailureReason::InvalidValue,
            ),
            (
                control(SessionControlInteraction::Range {
                    value: 50.0,
                    min: 0.0,
                    max: 100.0,
                    step: 5.0,
                }),
                invocation(Some(SessionControlValue::Range(101.0))),
                SessionControlFailureReason::InvalidValue,
            ),
            (
                control(SessionControlInteraction::Range {
                    value: 50.0,
                    min: 0.0,
                    max: 100.0,
                    step: 5.0,
                }),
                invocation(Some(SessionControlValue::Range(52.0))),
                SessionControlFailureReason::InvalidValue,
            ),
        ];
        for (control, request, reason) in invalid_cases {
            assert_eq!(
                validate_session_control_invocation("current", &request, &control)
                    .expect_err("invocation must be rejected")
                    .reason,
                reason
            );
        }

        let mut disabled = control(SessionControlInteraction::Command);
        disabled.enabled = false;
        disabled.disabled_reason = Some("No live executor".into());
        assert_eq!(
            validate_session_control_invocation("current", &invocation(None), &disabled)
                .unwrap_err()
                .reason,
            SessionControlFailureReason::Disabled
        );

        let mut stale = invocation(None);
        stale.launch_id = "old".into();
        assert_eq!(
            validate_session_control_invocation(
                "current",
                &stale,
                &control(SessionControlInteraction::Command),
            )
            .unwrap_err()
            .reason,
            SessionControlFailureReason::StaleSession
        );
    }

    #[test]
    fn status_with_active_session_maps_to_ok_with_details() {
        let outcome = session_status_outcome(Ok(upstream::UpstreamSessionStatus::SessionStatus {
            active: Some(upstream::UpstreamActiveSession {
                launch_id: "l1".into(),
                host: Some("aka".into()),
                game_id: Some("g1".into()),
                title: Some("Skate 3".into()),
                phase: Some("running".into()),
            }),
        }));
        let SessionStatusOutcome::Ok(status) = outcome else {
            panic!("expected Ok");
        };
        let active = status.active.expect("active session");
        assert_eq!(active.host.as_deref(), Some("aka"));
        assert_eq!(active.game_id.as_deref(), Some("g1"));
        assert_eq!(active.title.as_deref(), Some("Skate 3"));
        assert_eq!(active.phase.as_deref(), Some("running"));
    }

    #[test]
    fn status_without_active_session_maps_to_nothing_playing() {
        let outcome = session_status_outcome(Ok(upstream::UpstreamSessionStatus::SessionStatus {
            active: None,
        }));
        assert!(matches!(
            outcome,
            SessionStatusOutcome::Ok(SessionStatus { active: None })
        ));
    }

    #[test]
    fn absent_optional_fields_are_omitted_from_the_wire() {
        assert_eq!(
            serde_json::to_value(SessionStatus { active: None }).unwrap(),
            serde_json::json!({})
        );
        assert_eq!(
            serde_json::to_value(ActiveSession {
                launch_id: "l1".into(),
                host: None,
                game_id: None,
                title: None,
                phase: None,
            })
            .unwrap(),
            serde_json::json!({ "launchId": "l1" })
        );
        assert_eq!(
            serde_json::to_value(SessionStopRequest {
                force: None,
                expected_launch_id: None,
            })
            .unwrap(),
            serde_json::json!({})
        );
    }

    async fn rpc_body(app: Router, body: &str) -> serde_json::Value {
        rpc_body_authorized(app, body, None).await
    }

    async fn unix_rpc_body(path: PathBuf, body: &str) -> serde_json::Value {
        use std::io::{Read, Write};

        let body = body.to_owned();
        let response = tokio::task::spawn_blocking(move || {
            let mut stream = std::os::unix::net::UnixStream::connect(path).unwrap();
            write!(
                stream,
                "POST /rpc HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
            let mut response = Vec::new();
            stream.read_to_end(&mut response).unwrap();
            response
        })
        .await
        .unwrap();
        let payload = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|position| &response[position + 4..])
            .expect("HTTP response body");
        serde_json::from_slice(payload).unwrap()
    }

    async fn tcp_rpc_body(address: std::net::SocketAddr, body: &str) -> serde_json::Value {
        reqwest::Client::new()
            .post(format!("http://{address}/rpc"))
            .header(header::CONTENT_TYPE.as_str(), "application/json")
            .body(body.to_owned())
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    async fn rpc_body_authorized(
        app: Router,
        body: &str,
        capability: Option<&str>,
    ) -> serde_json::Value {
        let mut request = Request::builder()
            .method("POST")
            .uri("/rpc")
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(capability) = capability {
            request = request.header(header::AUTHORIZATION, format!("Bearer {capability}"));
        }
        let response = app
            .oneshot(request.body(Body::from(body.to_owned())).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    fn discovery_test_router(
        readable: &Path,
        private: &Path,
        grants: discovery::FolderSelectionGrantStore,
    ) -> Router {
        let signing_key = b"test signing key".to_vec();
        router_with_capability_local_root_provision_and_grants(
            "right-token",
            "https://portal.example",
            readable,
            private,
            launcher::FileProvisionMode::Direct,
            signing_key.clone(),
            Arc::new(Mutex::new(launcher::LaunchPublicationReservations::new())),
            Arc::new(Mutex::new(launcher::MoonlightLaunchAuthority::new(
                signing_key,
            ))),
            Arc::new(Mutex::new(None)),
            Arc::new(Mutex::new(None)),
            plugin_policy::RegistrySource::Installed,
            config::snapshot::ConfigSnapshotCoordinator::new(readable),
            grants,
            None,
            None,
            None,
        )
    }

    async fn wait_for_discovery_idle(app: Router) -> serde_json::Value {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let body = rpc_body_authorized(
                app.clone(),
                r#"{"_tag":"app.discovery.snapshot","payload":{}}"#,
                Some("right-token"),
            )
            .await;
            let tag = body["outcome"]["payload"]["state"]["_tag"].as_str();
            if tag == Some("Idle") || tag == Some("Problem") {
                return body;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "discovery did not settle: {body}"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[tokio::test]
    async fn discovery_register_receipt_reaches_idle_with_visible_game() {
        let readable = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("game.gba"), b"rom").unwrap();
        let grants = discovery::FolderSelectionGrantStore::default();
        let receipt = grants.issue_approved_path(folder.path()).unwrap().token;
        let app = discovery_test_router(readable.path(), private.path(), grants);

        let body = rpc_body_authorized(
            app.clone(),
            &serde_json::json!({"_tag":"app.discovery.registerReceipt","payload":{"receipt": receipt}}).to_string(),
            Some("right-token"),
        )
        .await;
        assert_eq!(body["outcome"]["_tag"], "Ok");
        assert_eq!(body["outcome"]["payload"]["state"]["_tag"], "Scanning");

        let idle = wait_for_discovery_idle(app.clone()).await;
        assert_eq!(idle["outcome"]["payload"]["state"]["_tag"], "Idle");
        assert_eq!(
            idle["outcome"]["payload"]["locations"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let games = rpc_body_authorized(
            app,
            r#"{"_tag":"app.local-games.list","payload":{}}"#,
            Some("right-token"),
        )
        .await;
        assert_eq!(games["outcome"]["payload"]["games"][0]["title"], "game");
        assert_eq!(
            games["outcome"]["payload"]["games"][0]["id"]
                .as_str()
                .unwrap()
                .len(),
            26
        );
    }

    #[tokio::test]
    async fn local_games_rpc_projects_current_discovery_cover_asset_identity_only() {
        let readable = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("game.gba"), b"rom").unwrap();
        let grants = discovery::FolderSelectionGrantStore::default();
        let receipt = grants.issue_approved_path(folder.path()).unwrap().token;
        let app = discovery_test_router(readable.path(), private.path(), grants);
        rpc_body_authorized(
            app.clone(),
            &serde_json::json!({"_tag":"app.discovery.registerReceipt","payload":{"receipt": receipt}}).to_string(),
            Some("right-token"),
        )
        .await;
        wait_for_discovery_idle(app.clone()).await;
        let game = discovery::reconcile::owned_discovery_games(readable.path(), private.path())
            .unwrap()
            .pop()
            .unwrap();
        let assignment = game_assets::GameAssetRepository::new(private.path())
            .assign_tile(
                game_assets::AssetOwnerIdentity {
                    playable_id: game.playable_id,
                    release_id: game.release_id,
                    release_fingerprint: game.release_fingerprint,
                    rom_identity: game.rom_identity,
                },
                game_assets::AssetCandidate {
                    bytes: PNG_1X1.to_vec(),
                    declared_width: Some(1),
                    declared_height: Some(1),
                    game_id: 10,
                    grid_id: 20,
                },
            )
            .unwrap();

        let games = rpc_body_authorized(
            app.clone(),
            r#"{"_tag":"app.local-games.list","payload":{}}"#,
            Some("right-token"),
        )
        .await;
        assert_eq!(
            games["outcome"]["payload"]["games"][0]["coverAssetId"],
            assignment.asset_id
        );

        let edited = std::fs::read_to_string(readable.path().join("catalog/games.yaml"))
            .unwrap()
            .replace("title: game", "title: Player Edited");
        crate::config::test_fixtures::write(readable.path().join("catalog/games.yaml"), edited)
            .unwrap();
        let stale = rpc_body_authorized(
            app,
            r#"{"_tag":"app.local-games.list","payload":{}}"#,
            Some("right-token"),
        )
        .await;
        assert!(stale["outcome"]["payload"]["games"][0]
            .as_object()
            .unwrap()
            .get("coverAssetId")
            .is_none());
    }

    #[tokio::test]
    async fn discovery_rejects_unknown_expired_and_replayed_receipts() {
        let readable = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let grants = discovery::FolderSelectionGrantStore::new(std::time::Duration::from_millis(1));
        let replay = grants.issue_approved_path(folder.path()).unwrap().token;
        let expired = grants.issue_approved_path(folder.path()).unwrap().token;
        std::thread::sleep(std::time::Duration::from_millis(5));
        let app = discovery_test_router(readable.path(), private.path(), grants);

        for (receipt, code) in [
            (expired, "FolderSelectionReceiptExpired"),
            ("missing".to_owned(), "FolderSelectionReceiptUnknown"),
        ] {
            let body = rpc_body_authorized(
                app.clone(),
                &serde_json::json!({"_tag":"app.discovery.registerReceipt","payload":{"receipt": receipt}}).to_string(),
                Some("right-token"),
            )
            .await;
            assert_eq!(body["outcome"]["_tag"], "Err");
            assert_eq!(body["outcome"]["payload"]["code"], code);
        }

        let restarted = discovery::FolderSelectionGrantStore::default();
        let restarted_app = discovery_test_router(readable.path(), private.path(), restarted);
        let body = rpc_body_authorized(
            restarted_app,
            &serde_json::json!({"_tag":"app.discovery.registerReceipt","payload":{"receipt": replay}}).to_string(),
            Some("right-token"),
        )
        .await;
        assert_eq!(
            body["outcome"]["payload"]["code"],
            "FolderSelectionReceiptUnknown"
        );
    }

    #[tokio::test]
    async fn discovery_rescan_coalesces_while_catalog_stays_readable() {
        let readable = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("one.gba"), b"one").unwrap();
        let grants = discovery::FolderSelectionGrantStore::default();
        let receipt = grants.issue_approved_path(folder.path()).unwrap().token;
        let app = discovery_test_router(readable.path(), private.path(), grants);
        rpc_body_authorized(
            app.clone(),
            &serde_json::json!({"_tag":"app.discovery.registerReceipt","payload":{"receipt": receipt}}).to_string(),
            Some("right-token"),
        )
        .await;
        let during = rpc_body_authorized(
            app.clone(),
            r#"{"_tag":"app.discovery.rescan","payload":{}}"#,
            Some("right-token"),
        )
        .await;
        assert_eq!(during["outcome"]["payload"]["state"]["_tag"], "Scanning");
        let games = rpc_body_authorized(
            app.clone(),
            r#"{"_tag":"app.local-games.list","payload":{}}"#,
            Some("right-token"),
        )
        .await;
        assert_eq!(games["outcome"]["_tag"], "Ok");
        let idle = wait_for_discovery_idle(app).await;
        assert_eq!(idle["outcome"]["payload"]["state"]["_tag"], "Idle");
    }

    #[tokio::test]
    async fn discovery_invalid_location_reports_problem_without_erasing_catalog() {
        let readable = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let good = tempfile::tempdir().unwrap();
        std::fs::write(good.path().join("good.gba"), b"good").unwrap();
        let bad = tempfile::tempdir().unwrap();
        let grants = discovery::FolderSelectionGrantStore::default();
        let good_receipt = grants.issue_approved_path(good.path()).unwrap().token;
        let bad_receipt = grants.issue_approved_path(bad.path()).unwrap().token;
        let app = discovery_test_router(readable.path(), private.path(), grants);
        rpc_body_authorized(
            app.clone(),
            &serde_json::json!({"_tag":"app.discovery.registerReceipt","payload":{"receipt": good_receipt}}).to_string(),
            Some("right-token"),
        )
        .await;
        wait_for_discovery_idle(app.clone()).await;
        drop(bad);
        rpc_body_authorized(
            app.clone(),
            &serde_json::json!({"_tag":"app.discovery.registerReceipt","payload":{"receipt": bad_receipt}}).to_string(),
            Some("right-token"),
        )
        .await;
        let problem = wait_for_discovery_idle(app.clone()).await;
        assert_eq!(problem["outcome"]["payload"]["state"]["_tag"], "Problem");
        let games = rpc_body_authorized(
            app,
            r#"{"_tag":"app.local-games.list","payload":{}}"#,
            Some("right-token"),
        )
        .await;
        assert_eq!(games["outcome"]["payload"]["games"][0]["title"], "good");
        assert_eq!(
            games["outcome"]["payload"]["games"][0]["id"]
                .as_str()
                .unwrap()
                .len(),
            26
        );
    }

    #[tokio::test]
    async fn host_catalog_serves_configured_games_with_the_host_label() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(
            &config,
            r#"
label = "zao"

[[games]]
id = "neverball"
title = "Neverball"
command = ["neverball"]
"#,
        )
        .unwrap();

        let body = rpc_body(
            host_router(&config),
            r#"{"_tag":"app.catalog.snapshot","payload":{}}"#,
        )
        .await;
        assert_eq!(body["outcome"]["_tag"], "Ok");
        assert_eq!(body["outcome"]["payload"]["games"][0]["id"], "neverball");
        assert_eq!(body["outcome"]["payload"]["games"][0]["host"], "zao");
        let source = &body["outcome"]["payload"]["games"][0]["source"];
        assert_eq!(source["label"], "zao");
        assert_eq!(source["isLocal"], true);
        assert_eq!(source["devicePublicKey"].as_str().unwrap().len(), 64);
    }

    #[tokio::test]
    async fn host_catalog_keeps_serving_a_tagged_error_for_an_invalid_config() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("missing.toml");
        let app = host_router(&config);

        for _ in 0..2 {
            let body = rpc_body(
                app.clone(),
                r#"{"_tag":"app.catalog.snapshot","payload":{}}"#,
            )
            .await;
            assert_eq!(body["outcome"]["_tag"], "Err");
            assert_eq!(body["outcome"]["payload"]["code"], "HostConfigInvalid");
            assert!(body["outcome"]["payload"]["message"]
                .as_str()
                .unwrap()
                .contains("missing.toml"));
        }
    }

    #[tokio::test]
    async fn host_catalog_accepts_an_empty_games_list() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(&config, "label = \"zao\"\n").unwrap();

        let body = rpc_body(
            host_router(&config),
            r#"{"_tag":"app.catalog.snapshot","payload":{}}"#,
        )
        .await;
        assert_eq!(body["outcome"]["_tag"], "Ok");
        assert_eq!(body["outcome"]["payload"]["games"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn brain_router_loads_and_routes_the_storage_root_upstream_config() {
        let root = tempfile::tempdir().unwrap();
        let host_config = root.path().join("host.toml");
        std::fs::write(
            &host_config,
            r#"
label = "zao"
[[games]]
id = "neverball"
title = "Neverball"
command = ["sh", "-c", "sleep 1"]
"#,
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let host = host_router_with_in_memory_units(&host_config);
        tokio::spawn(async move { axum::serve(listener, host).await.unwrap() });
        std::fs::write(
            root.path().join("upstreams.json"),
            format!(
                r#"[{{"label":"zao","kind":"native","baseUrl":"http://{address}","devicePublicKey":"{}"}}]"#,
                "11".repeat(32)
            ),
        )
        .unwrap();
        let brain = router_with_capability_and_local_root(
            "right-token",
            "https://portal.example",
            root.path(),
        );

        let catalog = rpc_body_authorized(
            brain.clone(),
            r#"{"_tag":"app.catalog.snapshot","payload":{}}"#,
            Some("right-token"),
        )
        .await;
        assert_eq!(catalog["outcome"]["payload"]["games"][0]["host"], "zao");
        let prepared = rpc_body_authorized(
            brain.clone(),
            r#"{"_tag":"app.session.prepare","payload":{"gameId":"neverball","host":"zao"}}"#,
            Some("right-token"),
        )
        .await;
        assert_eq!(prepared["outcome"]["_tag"], "Ok");
        let launch_id = prepared["outcome"]["payload"]["launchId"]
            .as_str()
            .unwrap()
            .to_owned();

        let stale = rpc_body_authorized(
            brain.clone(),
            r#"{"_tag":"app.session.stop","payload":{"expectedLaunchId":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}"#,
            Some("right-token"),
        )
        .await;
        assert_eq!(stale["outcome"]["payload"]["code"], "StaleLaunchIdentity");
        let status = rpc_body_authorized(
            brain.clone(),
            r#"{"_tag":"app.session.status","payload":{}}"#,
            Some("right-token"),
        )
        .await;
        assert_eq!(
            status["outcome"]["payload"]["active"]["launchId"],
            launch_id
        );

        let stopped = rpc_body_authorized(
            brain,
            &serde_json::json!({
                "_tag": "app.session.stop",
                "payload": { "expectedLaunchId": launch_id }
            })
            .to_string(),
            Some("right-token"),
        )
        .await;
        assert_eq!(stopped["outcome"]["_tag"], "Ok");
    }


    fn host_device_key(private_state_root: &Path) -> String {
        identity::DeviceIdentity::load_or_create(private_state_root)
            .unwrap()
            .device_public_key()
            .unwrap()
            .to_owned()
    }

    #[tokio::test]
    async fn host_source_status_answers_only_for_its_own_device_key() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(&config, "label = \"zao\"\n").unwrap();
        let private = root.path().join("private");
        let enabled = RecordingMoonlightCertificates::matching("sunshine-host");
        let runtime = host::HostRuntime::from_paths_with_backends(
            &config,
            None,
            private.clone(),
            Arc::new(host::control::InMemoryLaunchUnitBackend::default()),
            enabled.clone(),
        );
        let own_key = host_device_key(&private);
        let (lan, local) = plain_host_routers(runtime);

        for app in [lan.clone(), local] {
            let body = rpc_body(
                app,
                &serde_json::json!({
                    "_tag": "app.source.status",
                    "payload": { "devicePublicKey": own_key }
                })
                .to_string(),
            )
            .await;
            assert_eq!(body["_tag"], "app.source.status");
            assert_eq!(body["outcome"]["_tag"], "Ok");
            assert_eq!(body["outcome"]["payload"]["catalog"], "available");
            assert_eq!(body["outcome"]["payload"]["streamControl"], "enabled");
        }
        // The probe is the only certificate call, and it mutates nothing.
        assert_eq!(enabled.calls(), vec!["available", "available"]);
        assert!(enabled.exact_revocations().is_empty());

        let foreign = rpc_body(
            lan,
            &serde_json::json!({
                "_tag": "app.source.status",
                "payload": { "devicePublicKey": "ab".repeat(32) }
            })
            .to_string(),
        )
        .await;
        assert_eq!(foreign["outcome"]["_tag"], "Err");
        assert_eq!(
            foreign["outcome"]["payload"]["code"],
            "SourceDeviceMismatch"
        );
        // A foreign key is refused before any probe.
        assert_eq!(enabled.calls().len(), 2);
    }

    #[tokio::test]
    async fn host_source_status_reports_unavailable_catalog_and_disabled_stream_control() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(&config, "label = \"zao\"\nthis is not toml\n").unwrap();
        let private = root.path().join("private");
        let disabled = Arc::new(ChangingMoonlightCertificates::default());
        let runtime = host::HostRuntime::from_paths_with_backends(
            &config,
            None,
            private.clone(),
            Arc::new(host::control::InMemoryLaunchUnitBackend::default()),
            disabled.clone(),
        );
        let own_key = host_device_key(&private);
        let (lan, _) = plain_host_routers(runtime);

        let body = rpc_body(
            lan,
            &serde_json::json!({
                "_tag": "app.source.status",
                "payload": { "devicePublicKey": own_key }
            })
            .to_string(),
        )
        .await;
        assert_eq!(body["outcome"]["_tag"], "Ok");
        assert_eq!(body["outcome"]["payload"]["catalog"], "unavailable");
        assert_eq!(body["outcome"]["payload"]["streamControl"], "disabled");
        assert_eq!(
            disabled.calls.lock().unwrap().as_slice(),
            &["available".to_owned()]
        );
    }

    #[tokio::test]
    async fn brain_source_status_fails_closed_for_an_unconfigured_device_key() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("device.yaml"), "{}\n").unwrap();
        crate::config::test_fixtures::write(root.path().join("catalog/games.yaml"), "{}\n")
            .unwrap();
        let brain = router_with_capability_and_local_root(
            "right-token",
            "https://portal.example",
            root.path(),
        );
        let body = rpc_body_authorized(
            brain,
            &serde_json::json!({
                "_tag": "app.source.status",
                "payload": { "devicePublicKey": "ab".repeat(32) }
            })
            .to_string(),
            Some("right-token"),
        )
        .await;
        // The brain routes to the registry; with no configured peer of that
        // key it fails closed with the registry's typed code and no call.
        assert_eq!(body["outcome"]["_tag"], "Err");
        assert_eq!(body["outcome"]["payload"]["code"], "SourcePeerNotFound");
    }

    #[tokio::test]
    async fn host_freeze_and_thaw_require_exact_identity_and_report_state() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(
            &config,
            r#"
label = "zao"
[[games]]
id = "one"
title = "One"
command = ["game-one"]
"#,
        )
        .unwrap();
        let (lan, local) = host_routers_with_in_memory_units(&config);
        let tcp_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let tcp_address = tcp_listener.local_addr().unwrap();
        let socket_path = root.path().join("control.sock");
        let unix_listener = tokio::net::UnixListener::bind(&socket_path).unwrap();
        let lan_server = tokio::spawn(async move { axum::serve(tcp_listener, lan).await.unwrap() });
        let local_server =
            tokio::spawn(async move { axum::serve(unix_listener, local).await.unwrap() });

        let prepared = tcp_rpc_body(
            tcp_address,
            r#"{"_tag":"app.session.prepare","payload":{"gameId":"one"}}"#,
        )
        .await;
        let launch_id = prepared["outcome"]["payload"]["launchId"]
            .as_str()
            .unwrap()
            .to_owned();

        let stale = tcp_rpc_body(
            tcp_address,
            r#"{"_tag":"app.session.freeze","payload":{"expectedLaunchId":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}"#,
        )
        .await;
        assert_eq!(stale["outcome"]["payload"]["code"], "StaleLaunchIdentity");
        assert!(stale.to_string().find(&launch_id).is_none());

        let frozen = tcp_rpc_body(
            tcp_address,
            &serde_json::json!({
                "_tag": "app.session.freeze",
                "payload": { "expectedLaunchId": launch_id }
            })
            .to_string(),
        )
        .await;
        assert_eq!(frozen["_tag"], "app.session.freeze");
        assert_eq!(frozen["outcome"]["_tag"], "Ok");
        assert_eq!(frozen["outcome"]["payload"]["launchId"], launch_id);
        assert_eq!(frozen["outcome"]["payload"]["state"], "frozen");
        assert_eq!(frozen["outcome"]["payload"]["changed"], true);

        let status =
            tcp_rpc_body(tcp_address, r#"{"_tag":"app.session.status","payload":{}}"#).await;
        assert_eq!(status["outcome"]["payload"]["active"]["phase"], "frozen");
        assert_eq!(
            status["outcome"]["payload"]["active"]["launchId"],
            launch_id
        );

        let again = unix_rpc_body(
            socket_path.clone(),
            &serde_json::json!({
                "_tag": "app.session.freeze",
                "payload": { "expectedLaunchId": launch_id }
            })
            .to_string(),
        )
        .await;
        assert_eq!(again["outcome"]["payload"]["changed"], false);
        assert_eq!(again["outcome"]["payload"]["state"], "frozen");

        let thawed = unix_rpc_body(
            socket_path.clone(),
            &serde_json::json!({
                "_tag": "app.session.thaw",
                "payload": { "expectedLaunchId": launch_id }
            })
            .to_string(),
        )
        .await;
        assert_eq!(thawed["_tag"], "app.session.thaw");
        assert_eq!(thawed["outcome"]["payload"]["state"], "running");
        assert_eq!(thawed["outcome"]["payload"]["changed"], true);
        let running =
            tcp_rpc_body(tcp_address, r#"{"_tag":"app.session.status","payload":{}}"#).await;
        assert_eq!(running["outcome"]["payload"]["active"]["phase"], "running");

        // Stop works from the frozen state and does not need a thaw first.
        let refrozen = tcp_rpc_body(
            tcp_address,
            &serde_json::json!({
                "_tag": "app.session.freeze",
                "payload": { "expectedLaunchId": launch_id }
            })
            .to_string(),
        )
        .await;
        assert_eq!(refrozen["outcome"]["payload"]["state"], "frozen");
        let stopped = unix_rpc_body(
            socket_path,
            &serde_json::json!({
                "_tag": "app.session.stop",
                "payload": { "expectedLaunchId": launch_id }
            })
            .to_string(),
        )
        .await;
        assert_eq!(stopped["outcome"]["payload"]["phase"], "stopped");
        let gone = tcp_rpc_body(
            tcp_address,
            &serde_json::json!({
                "_tag": "app.session.thaw",
                "payload": { "expectedLaunchId": launch_id }
            })
            .to_string(),
        )
        .await;
        assert_eq!(gone["outcome"]["payload"]["code"], "NoActiveSession");
        lan_server.abort();
        local_server.abort();
    }

    #[tokio::test]
    async fn private_and_lan_host_control_require_exact_stop_identity() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(
            &config,
            r#"
label = "zao"
[[games]]
id = "one"
title = "One"
command = ["game-one"]
[[games]]
id = "two"
title = "Two"
command = ["game-two"]
"#,
        )
        .unwrap();
        let (lan, local) = host_routers_with_in_memory_units(&config);
        let tcp_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let tcp_address = tcp_listener.local_addr().unwrap();
        let socket_path = root.path().join("control.sock");
        let unix_listener = tokio::net::UnixListener::bind(&socket_path).unwrap();
        let lan_server = tokio::spawn(async move { axum::serve(tcp_listener, lan).await.unwrap() });
        let local_server =
            tokio::spawn(async move { axum::serve(unix_listener, local).await.unwrap() });

        let prepared = tcp_rpc_body(
            tcp_address,
            r#"{"_tag":"app.session.prepare","payload":{"gameId":"one"}}"#,
        )
        .await;
        let first = prepared["outcome"]["payload"]["launchId"]
            .as_str()
            .unwrap()
            .to_owned();

        let lan_status =
            tcp_rpc_body(tcp_address, r#"{"_tag":"app.session.status","payload":{}}"#).await;
        assert_eq!(
            lan_status["outcome"]["payload"]["active"]["launchId"],
            first
        );
        assert_eq!(lan_status["outcome"]["payload"]["active"]["gameId"], "one");
        let status = unix_rpc_body(
            socket_path.clone(),
            r#"{"_tag":"app.session.status","payload":{}}"#,
        )
        .await;
        assert_eq!(status["outcome"]["payload"]["active"]["launchId"], first);
        assert_eq!(status["outcome"]["payload"]["active"]["phase"], "running");

        let lan_stop =
            tcp_rpc_body(tcp_address, r#"{"_tag":"app.session.stop","payload":{}}"#).await;
        assert_eq!(
            lan_stop["outcome"]["payload"]["code"],
            "ExpectedLaunchIdRequired"
        );
        assert!(lan_stop.to_string().find(&first).is_none());

        let still_active = unix_rpc_body(
            socket_path.clone(),
            r#"{"_tag":"app.session.status","payload":{}}"#,
        )
        .await;
        assert_eq!(
            still_active["outcome"]["payload"]["active"]["launchId"],
            first
        );

        let missing_identity = unix_rpc_body(
            socket_path.clone(),
            r#"{"_tag":"app.session.stop","payload":{}}"#,
        )
        .await;
        assert_eq!(
            missing_identity["outcome"]["payload"]["code"],
            "ExpectedLaunchIdRequired"
        );
        let stale = unix_rpc_body(
            socket_path.clone(),
            r#"{"_tag":"app.session.stop","payload":{"expectedLaunchId":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}"#,
        )
        .await;
        assert_eq!(stale["outcome"]["payload"]["code"], "StaleLaunchIdentity");

        let stopped = unix_rpc_body(
            socket_path.clone(),
            &serde_json::json!({
                "_tag": "app.session.stop",
                "payload": { "expectedLaunchId": first }
            })
            .to_string(),
        )
        .await;
        assert_eq!(stopped["outcome"]["payload"]["phase"], "stopped");
        let replacement = tcp_rpc_body(
            tcp_address,
            r#"{"_tag":"app.session.prepare","payload":{"gameId":"two"}}"#,
        )
        .await;
        let second = replacement["outcome"]["payload"]["launchId"]
            .as_str()
            .unwrap();
        assert_ne!(second, first);
        let old_stop = unix_rpc_body(
            socket_path,
            &serde_json::json!({
                "_tag": "app.session.stop",
                "payload": { "expectedLaunchId": first }
            })
            .to_string(),
        )
        .await;
        assert_eq!(
            old_stop["outcome"]["payload"]["code"],
            "StaleLaunchIdentity"
        );
        lan_server.abort();
        local_server.abort();
    }

    #[tokio::test]
    async fn local_games_rpc_lists_wario_land_from_the_device_brain() {
        let root = tempfile::tempdir().unwrap();
        write_wl4_plugin_config(root.path());
        std::fs::create_dir(root.path().join("roms")).unwrap();
        std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
        let app = test_router_with_capability_and_local_root(
            "right-token",
            "https://portal.example",
            root.path(),
        );
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/rpc")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::AUTHORIZATION, "Bearer right-token")
                    .body(Body::from(
                        r#"{"_tag":"app.local-games.list","payload":{}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8_lossy(&body);
        assert!(body.contains("app.local-games.list"));
        assert!(body.contains("Wario Land 4"));
    }

    #[tokio::test]
    async fn settings_rpc_round_trips_a_conflict_safe_device_name_write() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("device.yaml"), "host:\n  title: old\n").unwrap();
        crate::config::test_fixtures::write(root.path().join("catalog/games.yaml"), "{}\n")
            .unwrap();
        let app = test_router_with_capability_and_local_root(
            "right-token",
            "https://portal.example",
            root.path(),
        );

        let before = rpc_body_authorized(
            app.clone(),
            r#"{"_tag":"system.settings.snapshot","payload":{}}"#,
            Some("right-token"),
        )
        .await;
        assert_eq!(before["outcome"]["payload"]["deviceName"], "old");
        assert_eq!(
            before["outcome"]["payload"]["plugins"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
        let revision = before["outcome"]["payload"]["revision"].as_str().unwrap();
        let request = serde_json::json!({
            "_tag": "system.settings.update",
            "payload": {
                "expectedRevision": revision,
                "settingId": "device-name",
                "value": "usu"
            }
        })
        .to_string();
        let updated = rpc_body_authorized(app.clone(), &request, Some("right-token")).await;

        assert_eq!(updated["outcome"]["_tag"], "Ok");
        assert_eq!(updated["outcome"]["payload"]["deviceName"], "usu");
        assert!(std::fs::read_to_string(root.path().join("device.yaml"))
            .unwrap()
            .contains("title: usu"));

        let updated_revision = updated["outcome"]["payload"]["revision"].as_str().unwrap();
        std::fs::write(root.path().join("device.yaml"), "host:\n  title: outside\n").unwrap();
        let stale_request = serde_json::json!({
            "_tag": "system.settings.update",
            "payload": {
                "expectedRevision": updated_revision,
                "settingId": "device-name",
                "value": "overwritten"
            }
        })
        .to_string();
        let conflict = rpc_body_authorized(app, &stale_request, Some("right-token")).await;

        assert_eq!(conflict["outcome"]["_tag"], "Err");
        assert_eq!(conflict["outcome"]["payload"]["code"], "SettingsConflict");
        assert!(std::fs::read_to_string(root.path().join("device.yaml"))
            .unwrap()
            .contains("title: outside"));
    }






    #[cfg(unix)]


    #[cfg(unix)]

    #[tokio::test]
    async fn rpc_rejects_missing_or_wrong_capability() {
        let app = router_with_capability("right-token", "https://portal.example");
        for authorization in [None, Some("Bearer wrong-token")] {
            let mut request = Request::builder()
                .method("POST")
                .uri("/rpc")
                .header(header::CONTENT_TYPE, "application/json");
            if let Some(value) = authorization {
                request = request.header(header::AUTHORIZATION, value);
            }
            let response = app
                .clone()
                .oneshot(
                    request
                        .body(Body::from(r#"{"_tag":"system.health","payload":{}}"#))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
    }

    #[tokio::test]
    async fn rpc_accepts_the_capability_and_exact_portal_origin() {
        let app = router_with_capability("right-token", "https://portal.example");
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/rpc")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::AUTHORIZATION, "Bearer right-token")
                    .header(header::ORIGIN, "https://portal.example")
                    .body(Body::from(r#"{"_tag":"system.health","payload":{}}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .unwrap(),
            "https://portal.example"
        );
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("system.health"));
    }

    #[tokio::test]
    async fn cors_allows_authorized_preflight_only_for_the_exact_origin() {
        let app = router_with_capability("right-token", "https://portal.example");
        let allowed = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/rpc")
                    .header(header::ORIGIN, "https://portal.example")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                    .header(
                        header::ACCESS_CONTROL_REQUEST_HEADERS,
                        "content-type,authorization",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(allowed.status(), StatusCode::OK);
        assert_eq!(
            allowed
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .unwrap(),
            "https://portal.example"
        );
        assert!(allowed
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
            .unwrap()
            .to_str()
            .unwrap()
            .contains("authorization"));

        let foreign = app
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/rpc")
                    .header(header::ORIGIN, "https://evil.example")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                    .header(
                        header::ACCESS_CONTROL_REQUEST_HEADERS,
                        "content-type,authorization",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(
            foreign
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .and_then(|value| value.to_str().ok()),
            Some("https://evil.example")
        );
    }


    #[test]
    fn status_daemon_variants_map_to_distinct_failure_codes() {
        let not_configured = session_status_outcome(Ok(
            upstream::UpstreamSessionStatus::SessiondNotConfigured {},
        ));
        let SessionStatusOutcome::Err(failure) = not_configured else {
            panic!("expected Err");
        };
        assert_eq!(failure.code, "SessiondNotConfigured");

        let unavailable =
            session_status_outcome(Ok(upstream::UpstreamSessionStatus::HostUnavailable {}));
        let SessionStatusOutcome::Err(failure) = unavailable else {
            panic!("expected Err");
        };
        assert_eq!(failure.code, "HostUnavailable");
    }

    #[test]
    fn stop_variants_map_to_stopped_and_pending_phases() {
        let stopped = session_stop_outcome(Ok(upstream::UpstreamSessionStop::Stopped {
            launch_id: Some("l1".into()),
        }));
        assert!(matches!(
            stopped,
            SessionStopOutcome::Ok(SessionStopResult {
                phase: SessionStopPhase::Stopped
            })
        ));

        let pending = session_stop_outcome(Ok(upstream::UpstreamSessionStop::StopPending {
            launch_id: None,
        }));
        assert!(matches!(
            pending,
            SessionStopOutcome::Ok(SessionStopResult {
                phase: SessionStopPhase::Pending
            })
        ));

        let nothing = session_stop_outcome(Ok(upstream::UpstreamSessionStop::NothingToStop {}));
        assert!(matches!(
            nothing,
            SessionStopOutcome::Ok(SessionStopResult {
                phase: SessionStopPhase::Stopped
            })
        ));

        let confirmation =
            session_stop_outcome(Ok(upstream::UpstreamSessionStop::ConfirmationRequired {
                action: Some("stop-session".into()),
            }));
        let SessionStopOutcome::Err(failure) = confirmation else {
            panic!("expected Err");
        };
        assert_eq!(failure.code, "ConfirmationRequired");
    }
    #[derive(Default)]
    pub(super) struct RecordingMoonlightCertificates {
        expected_host_uuid: String,
        calls: Mutex<Vec<String>>,
        exact_revocations: Mutex<Vec<(String, String)>>,
    }

    impl RecordingMoonlightCertificates {
        pub(super) fn matching(expected_host_uuid: &str) -> Arc<Self> {
            Arc::new(Self {
                expected_host_uuid: expected_host_uuid.into(),
                calls: Mutex::new(Vec::new()),
                exact_revocations: Mutex::new(Vec::new()),
            })
        }

        pub(super) fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }

        fn exact_revocations(&self) -> Vec<(String, String)> {
            self.exact_revocations.lock().unwrap().clone()
        }
    }

    impl host::moonlight_certificate::MoonlightCertificateAdapter for RecordingMoonlightCertificates {
        fn available(&self) -> bool {
            self.calls.lock().unwrap().push("available".into());
            true
        }

        fn attest(&self, host_uuid: &str) -> Result<bool, RpcFailure> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("attest:{host_uuid}"));
            Ok(host_uuid == self.expected_host_uuid)
        }

        fn provision(
            &self,
            host_uuid: &str,
            _client_certificate: &str,
        ) -> Result<MoonlightCertificateProvisioned, RpcFailure> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("provision:{host_uuid}"));
            if host_uuid != self.expected_host_uuid {
                return Err(RpcFailure {
                    code: "HostMismatch".into(),
                    message: "Sunshine host UUID does not match".into(),
                });
            }
            Ok(MoonlightCertificateProvisioned {
                server_certificate:
                    "-----BEGIN CERTIFICATE-----\nserver\n-----END CERTIFICATE-----\n".into(),
            })
        }

        fn revoke(&self, host_uuid: &str, client_certificate: &str) -> Result<bool, RpcFailure> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("revoke:{host_uuid}"));
            self.exact_revocations
                .lock()
                .unwrap()
                .push((host_uuid.into(), client_certificate.into()));
            Ok(host_uuid == self.expected_host_uuid)
        }
    }

    #[derive(Default)]
    struct ChangingMoonlightCertificates {
        calls: Mutex<Vec<String>>,
    }

    impl host::moonlight_certificate::MoonlightCertificateAdapter for ChangingMoonlightCertificates {
        fn available(&self) -> bool {
            self.calls.lock().unwrap().push("available".into());
            false
        }

        fn attest(&self, host_uuid: &str) -> Result<bool, RpcFailure> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("attest:{host_uuid}"));
            Ok(true)
        }

        fn provision(
            &self,
            host_uuid: &str,
            _client_certificate: &str,
        ) -> Result<MoonlightCertificateProvisioned, RpcFailure> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("provision:{host_uuid}"));
            Err(RpcFailure {
                code: "HostMismatch".into(),
                message: "peer-controlled host mismatch text".into(),
            })
        }

        fn revoke(&self, host_uuid: &str, _client_certificate: &str) -> Result<bool, RpcFailure> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("revoke:{host_uuid}"));
            Err(RpcFailure {
                code: "HostMismatch".into(),
                message: "peer-controlled host mismatch text".into(),
            })
        }
    }

    fn host_routers_with_certificate_adapter(
        config_path: &Path,
        adapter: Arc<dyn host::moonlight_certificate::MoonlightCertificateAdapter>,
    ) -> (Router, Router) {
        let private = tempfile::tempdir().expect("private host state").keep();
        let runtime = host::HostRuntime::from_paths_with_backends(
            config_path,
            None,
            private,
            Arc::new(host::control::InMemoryLaunchUnitBackend::default()),
            adapter,
        );
        plain_host_routers(runtime)
    }

    fn secure_host_router_with_certificate_adapter_at(
        config_path: &Path,
        private_state_root: &Path,
        adapter: Arc<dyn host::moonlight_certificate::MoonlightCertificateAdapter>,
        now: u64,
    ) -> Router {
        let runtime = host::HostRuntime::from_paths_with_backends(
            config_path,
            None,
            private_state_root.to_owned(),
            Arc::new(host::control::InMemoryLaunchUnitBackend::default()),
            adapter,
        );
        let (lan, _) = app_states(runtime);
        peer_rpc::PeerRpcServer::new_at(lan, private_state_root, now)
            .expect("load or create peer RPC identity")
            .router(None)
    }

    async fn serve_router(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    const TEST_CLIENT_PEM: &str =
        "-----BEGIN CERTIFICATE-----\nclient\n-----END CERTIFICATE-----\n";

    fn owner_revocation(owner: &Keys, device_public_key: &str, created_at: u64) -> String {
        EventBuilder::new(Kind::Custom(30_078), "")
            .tags([
                Tag::parse(["d", &format!("org.korri.device-owner:{device_public_key}")]).unwrap(),
                Tag::parse(["device", device_public_key]).unwrap(),
                Tag::parse(["status", "revoked"]).unwrap(),
            ])
            .custom_created_at(Timestamp::from(created_at))
            .finalize(owner)
            .unwrap()
            .as_json()
    }

    fn stream_pass(
        owner: &Keys,
        device_public_key: &str,
        created_at: u64,
        expires_at: u64,
    ) -> String {
        EventBuilder::new(Kind::Custom(authorization::PERSON_PASS_EVENT_KIND), "")
            .tags([
                Tag::parse(["d", &format!("org.korri.person-pass:{}", "11".repeat(32))]).unwrap(),
                Tag::parse(["device", device_public_key]).unwrap(),
                Tag::parse(["tier", "guest"]).unwrap(),
                Tag::parse(["expires", &expires_at.to_string()]).unwrap(),
                Tag::parse(["scope", authorization::STREAM_LAUNCH_SCOPE]).unwrap(),
            ])
            .custom_created_at(Timestamp::from(created_at))
            .finalize(owner)
            .unwrap()
            .as_json()
    }

    fn pass_revocation(owner: &Keys, pass_event_id: &str, created_at: u64) -> String {
        EventBuilder::new(Kind::Custom(5), "")
            .tags([
                Tag::parse(["e", pass_event_id]).unwrap(),
                Tag::parse(["k", &authorization::PERSON_PASS_EVENT_KIND.to_string()]).unwrap(),
            ])
            .custom_created_at(Timestamp::from(created_at))
            .finalize(owner)
            .unwrap()
            .as_json()
    }

    #[tokio::test]
    async fn secure_peer_catalog_uses_the_requesting_principals_person_key() {
        const HOST: &str = "0000000000000000000000000000000000000000000000000000000000000005";
        const OWNER_CLIENT: &str =
            "0000000000000000000000000000000000000000000000000000000000000006";
        const GUEST_CLIENT: &str =
            "0000000000000000000000000000000000000000000000000000000000000007";
        const NOW: u64 = 1_700_000_000;
        let owner = Keys::generate();
        let guest_owner = Keys::generate();
        let owner_secret = owner.secret_key().to_secret_hex();
        let guest_owner_secret = guest_owner.secret_key().to_secret_hex();
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(
            &config,
            "label = \"zao\"\n[[games]]\nid = \"wario\"\ntitle = \"Wario Land 4\"\ncommand = [\"game\"]\n",
        )
        .unwrap();
        let host_private = tempfile::tempdir().unwrap();
        let owner_private = tempfile::tempdir().unwrap();
        let guest_private = tempfile::tempdir().unwrap();
        let host_identity = peer_rpc::test_owned_identity(host_private.path(), HOST, &owner_secret);
        let host_key = host_identity.device_public_key().unwrap().to_owned();
        let store = host::play_log::PlayLogStore::new(host_private.path());
        for (person, duration) in [(owner.public_key().to_hex(), 20.0), ("11".repeat(32), 45.0)] {
            store
                .record(
                    &host::play_log::PlayHistoryKey {
                        user_id: person,
                        game_id: "wario".into(),
                    },
                    host::play_log::PlayEntry {
                        occurred_at: "2026-09-04T10:00:00.000Z".into(),
                        duration_seconds: duration,
                        release_id: None,
                    },
                )
                .unwrap();
        }
        let server = serve_router(secure_host_router_with_certificate_adapter_at(
            &config,
            host_private.path(),
            RecordingMoonlightCertificates::matching("sunshine-host"),
            NOW,
        ))
        .await;

        let owner_credentials =
            peer_rpc::test_owned_credentials(owner_private.path(), OWNER_CLIENT, &owner_secret);
        let owner_catalog = upstream_native::NativeClient::new_secure_at(
            server.clone(),
            host_key.clone(),
            owner_credentials,
            NOW,
            "11".repeat(32),
            "22".repeat(32),
        )
        .catalog_snapshot()
        .await
        .unwrap();
        assert_eq!(
            owner_catalog.games[0]
                .play_stats
                .as_ref()
                .unwrap()
                .total_playtime_seconds,
            20.0
        );

        let guest_credentials = peer_rpc::test_owned_credentials(
            guest_private.path(),
            GUEST_CLIENT,
            &guest_owner_secret,
        );
        let guest_key = guest_credentials.public_key().unwrap();
        let guest_credentials = guest_credentials.with_person_pass(Some(stream_pass(
            &owner,
            &guest_key,
            NOW,
            NOW + 60,
        )));
        let guest_catalog = upstream_native::NativeClient::new_secure_at(
            server,
            host_key,
            guest_credentials,
            NOW,
            "33".repeat(32),
            "44".repeat(32),
        )
        .catalog_snapshot()
        .await
        .unwrap();
        assert_eq!(
            guest_catalog.games[0]
                .play_stats
                .as_ref()
                .unwrap()
                .total_playtime_seconds,
            45.0
        );
    }

    /// A brain forwards catalog reads to its peers as its own device, so
    /// every peer answers with the brain owner's play history. Only a caller
    /// proven to be that owner may receive it. A guest that holds a pass from
    /// the same owner is still a different person and must never see the
    /// owner's statistics through the brain hop.

    #[tokio::test]
    async fn same_owner_provisions_immediately_and_unauthorized_calls_have_no_effects() {
        const OTHER_OWNER: &str =
            "0000000000000000000000000000000000000000000000000000000000000004";
        const HOST: &str = "0000000000000000000000000000000000000000000000000000000000000005";
        const CLIENT: &str = "0000000000000000000000000000000000000000000000000000000000000006";
        let owner = Keys::generate();
        let owner_secret = owner.secret_key().to_secret_hex();
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(&config, "label = \"zao\"\n").unwrap();
        let host_private = tempfile::tempdir().unwrap();
        let client_private = tempfile::tempdir().unwrap();
        let host_identity = peer_rpc::test_owned_identity(host_private.path(), HOST, &owner_secret);
        let host_key = host_identity.device_public_key().unwrap().to_owned();
        let credentials =
            peer_rpc::test_owned_credentials(client_private.path(), CLIENT, &owner_secret);
        let client_key = credentials.public_key().unwrap();
        let adapter = RecordingMoonlightCertificates::matching("sunshine-host");
        const NOW: u64 = 1_700_000_000;
        let server = serve_router(secure_host_router_with_certificate_adapter_at(
            &config,
            host_private.path(),
            adapter.clone(),
            NOW,
        ))
        .await;
        upstream_native::NativeClient::new_secure_at(
            server.clone(),
            host_key.clone(),
            credentials,
            NOW,
            "11".repeat(32),
            "22".repeat(32),
        )
        .moonlight_certificate_provision("sunshine-host", TEST_CLIENT_PEM)
        .await
        .unwrap();
        assert_eq!(adapter.calls(), vec!["provision:sunshine-host"]);

        let replay_entries_before =
            std::fs::read_dir(host_private.path().join("identity/security-replay"))
                .unwrap()
                .count();
        let grant_before = std::fs::read(
            host_private
                .path()
                .join("identity/peer-certificates")
                .join(&client_key),
        )
        .unwrap();
        std::fs::remove_file(client_private.path().join("identity/owner.event.json")).unwrap();
        let different_owner =
            peer_rpc::test_owned_credentials(client_private.path(), CLIENT, OTHER_OWNER);
        let denied = upstream_native::NativeClient::new_secure_at(
            server,
            host_key,
            different_owner,
            NOW,
            "33".repeat(32),
            "44".repeat(32),
        )
        .moonlight_certificate_provision("sunshine-host", TEST_CLIENT_PEM)
        .await
        .unwrap_err();
        assert!(matches!(
            denied,
            upstreams::UpstreamError::MoonlightCertificatePeerUnavailable
        ));
        assert_eq!(adapter.calls(), vec!["provision:sunshine-host"]);
        assert_eq!(
            std::fs::read_dir(host_private.path().join("identity/security-replay"))
                .unwrap()
                .count(),
            replay_entries_before
        );
        assert_eq!(
            std::fs::read(
                host_private
                    .path()
                    .join("identity/peer-certificates")
                    .join(&client_key),
            )
            .unwrap(),
            grant_before
        );
        assert_eq!(
            std::fs::read_dir(
                host_private
                    .path()
                    .join("identity/authorization-revocations"),
            )
            .unwrap()
            .count(),
            0
        );
    }

    #[tokio::test]
    async fn owner_and_pass_revocations_reconcile_the_exact_sunshine_certificate() {
        const HOST: &str = "0000000000000000000000000000000000000000000000000000000000000005";
        const CLIENT: &str = "0000000000000000000000000000000000000000000000000000000000000006";
        const RELAYER: &str = "0000000000000000000000000000000000000000000000000000000000000007";
        let owner = Keys::generate();
        let owner_secret = owner.secret_key().to_secret_hex();
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(&config, "label = \"zao\"\n").unwrap();
        let host_private = tempfile::tempdir().unwrap();
        let client_private = tempfile::tempdir().unwrap();
        let relayer_private = tempfile::tempdir().unwrap();
        let host_identity = peer_rpc::test_owned_identity(host_private.path(), HOST, &owner_secret);
        let host_key = host_identity.device_public_key().unwrap().to_owned();
        let client_credentials =
            peer_rpc::test_owned_credentials(client_private.path(), CLIENT, &owner_secret);
        let client_key = client_credentials.public_key().unwrap();
        let relayer =
            peer_rpc::test_owned_credentials(relayer_private.path(), RELAYER, &owner_secret);
        let adapter = RecordingMoonlightCertificates::matching("sunshine-host");
        const NOW: u64 = 1_700_000_000;
        let server = serve_router(secure_host_router_with_certificate_adapter_at(
            &config,
            host_private.path(),
            adapter.clone(),
            NOW,
        ))
        .await;
        upstream_native::NativeClient::new_secure_at(
            server.clone(),
            host_key.clone(),
            client_credentials,
            NOW,
            "11".repeat(32),
            "22".repeat(32),
        )
        .moonlight_certificate_provision("sunshine-host", TEST_CLIENT_PEM)
        .await
        .unwrap();
        let relayer =
            relayer.with_revocations(vec![owner_revocation(&owner, &client_key, NOW + 1)]);
        upstream_native::NativeClient::new_secure_at(
            server,
            host_key,
            relayer,
            NOW,
            "33".repeat(32),
            "44".repeat(32),
        )
        .catalog_snapshot()
        .await
        .unwrap();
        assert_eq!(
            adapter.exact_revocations(),
            vec![("sunshine-host".into(), TEST_CLIENT_PEM.into())]
        );
    }

    #[tokio::test]
    async fn guest_stream_pass_provisions_and_expiry_reconciles_before_restart_launch() {
        const HOST: &str = "0000000000000000000000000000000000000000000000000000000000000005";
        const GUEST: &str = "0000000000000000000000000000000000000000000000000000000000000006";
        const RELAYER: &str = "0000000000000000000000000000000000000000000000000000000000000007";
        let owner = Keys::generate();
        let guest_owner = Keys::generate();
        let owner_secret = owner.secret_key().to_secret_hex();
        let guest_owner_secret = guest_owner.secret_key().to_secret_hex();
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(
            &config,
            "label = \"zao\"\n[[games]]\nid = \"neverball\"\ntitle = \"Neverball\"\ncommand = [\"neverball\"]\n",
        )
        .unwrap();
        let host_private = tempfile::tempdir().unwrap();
        let guest_private = tempfile::tempdir().unwrap();
        let relayer_private = tempfile::tempdir().unwrap();
        let host_identity = peer_rpc::test_owned_identity(host_private.path(), HOST, &owner_secret);
        let host_key = host_identity.device_public_key().unwrap().to_owned();
        let guest_credentials =
            peer_rpc::test_owned_credentials(guest_private.path(), GUEST, &guest_owner_secret);
        let guest_key = guest_credentials.public_key().unwrap();
        let pass = stream_pass(&owner, &guest_key, 1_700_000_000, 1_700_000_060);
        let guest_credentials = guest_credentials.with_person_pass(Some(pass));
        let adapter = RecordingMoonlightCertificates::matching("sunshine-host");
        let server = serve_router(secure_host_router_with_certificate_adapter_at(
            &config,
            host_private.path(),
            adapter.clone(),
            1_700_000_000,
        ))
        .await;
        upstream_native::NativeClient::new_secure_at(
            server,
            host_key.clone(),
            guest_credentials,
            1_700_000_000,
            "11".repeat(32),
            "22".repeat(32),
        )
        .moonlight_certificate_provision("sunshine-host", TEST_CLIENT_PEM)
        .await
        .unwrap();

        let restarted = serve_router(secure_host_router_with_certificate_adapter_at(
            &config,
            host_private.path(),
            adapter.clone(),
            1_700_000_060,
        ))
        .await;
        let relayer =
            peer_rpc::test_owned_credentials(relayer_private.path(), RELAYER, &owner_secret);
        let launch = upstream_native::NativeClient::new_secure_at(
            restarted,
            host_key,
            relayer,
            1_700_000_060,
            "33".repeat(32),
            "44".repeat(32),
        )
        .prepare_stream("neverball")
        .await;
        assert!(launch.is_ok());
        assert_eq!(
            adapter.calls(),
            vec!["provision:sunshine-host", "revoke:sunshine-host"]
        );
        assert_eq!(
            adapter.exact_revocations(),
            vec![("sunshine-host".into(), TEST_CLIENT_PEM.into())]
        );
    }

    #[tokio::test]
    async fn signed_pass_revocation_removes_a_guest_certificate() {
        const HOST: &str = "0000000000000000000000000000000000000000000000000000000000000005";
        const GUEST: &str = "0000000000000000000000000000000000000000000000000000000000000006";
        const RELAYER: &str = "0000000000000000000000000000000000000000000000000000000000000007";
        let owner = Keys::generate();
        let guest_owner = Keys::generate();
        let owner_secret = owner.secret_key().to_secret_hex();
        let guest_owner_secret = guest_owner.secret_key().to_secret_hex();
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(&config, "label = \"zao\"\n").unwrap();
        let host_private = tempfile::tempdir().unwrap();
        let guest_private = tempfile::tempdir().unwrap();
        let relayer_private = tempfile::tempdir().unwrap();
        let host_identity = peer_rpc::test_owned_identity(host_private.path(), HOST, &owner_secret);
        let host_key = host_identity.device_public_key().unwrap().to_owned();
        let guest_credentials =
            peer_rpc::test_owned_credentials(guest_private.path(), GUEST, &guest_owner_secret);
        let guest_key = guest_credentials.public_key().unwrap();
        let pass = stream_pass(&owner, &guest_key, 1_700_000_000, 1_700_000_060);
        let pass_id = identity::DeviceIdentity::verify_event(&pass).unwrap().id;
        let guest_with_pass = guest_credentials.with_person_pass(Some(pass));
        let adapter = RecordingMoonlightCertificates::matching("sunshine-host");
        let server = serve_router(secure_host_router_with_certificate_adapter_at(
            &config,
            host_private.path(),
            adapter.clone(),
            1_700_000_000,
        ))
        .await;
        upstream_native::NativeClient::new_secure_at(
            server.clone(),
            host_key.clone(),
            guest_with_pass.clone(),
            1_700_000_000,
            "11".repeat(32),
            "22".repeat(32),
        )
        .moonlight_certificate_provision("sunshine-host", TEST_CLIENT_PEM)
        .await
        .unwrap();
        let relayer =
            peer_rpc::test_owned_credentials(relayer_private.path(), RELAYER, &owner_secret)
                .with_revocations(vec![pass_revocation(&owner, &pass_id, 1_700_000_001)]);
        upstream_native::NativeClient::new_secure_at(
            server,
            host_key.clone(),
            relayer,
            1_700_000_000,
            "33".repeat(32),
            "44".repeat(32),
        )
        .catalog_snapshot()
        .await
        .unwrap();
        assert_eq!(
            adapter.exact_revocations(),
            vec![("sunshine-host".into(), TEST_CLIENT_PEM.into())]
        );

        let restarted = serve_router(secure_host_router_with_certificate_adapter_at(
            &config,
            host_private.path(),
            adapter.clone(),
            1_700_000_000,
        ))
        .await;
        let denied = upstream_native::NativeClient::new_secure_at(
            restarted,
            host_key,
            guest_with_pass,
            1_700_000_000,
            "55".repeat(32),
            "66".repeat(32),
        )
        .moonlight_certificate_provision("sunshine-host", TEST_CLIENT_PEM)
        .await
        .unwrap_err();
        assert!(matches!(
            denied,
            upstreams::UpstreamError::MoonlightCertificatePeerUnavailable
        ));
        assert_eq!(
            adapter.calls(),
            vec!["provision:sunshine-host", "revoke:sunshine-host"]
        );
    }

    #[tokio::test]
    async fn host_certificate_rpc_delegates_only_on_lan_and_rejects_invalid_pem() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(&config, "label = \"zao\"\n").unwrap();
        let adapter = RecordingMoonlightCertificates::matching("sunshine-host");
        let (lan, local) = host_routers_with_certificate_adapter(&config, adapter.clone());

        let attest = rpc_body(
            lan.clone(),
            r#"{"_tag":"app.moonlight.certificate.attest","payload":{"hostUuid":"sunshine-host"}}"#,
        )
        .await;
        assert_eq!(attest["outcome"]["_tag"], "Ok");
        assert_eq!(attest["outcome"]["payload"]["matched"], true);

        let provision = rpc_body(
            lan.clone(),
            &serde_json::json!({
                "_tag": "app.moonlight.certificate.provision",
                "payload": {"hostUuid": "sunshine-host", "clientCertificate": TEST_CLIENT_PEM}
            })
            .to_string(),
        )
        .await;
        assert_eq!(provision["outcome"]["_tag"], "Ok");
        assert!(provision["outcome"]["payload"]["serverCertificate"]
            .as_str()
            .unwrap()
            .contains("BEGIN CERTIFICATE"));

        let revoke = rpc_body(
            lan.clone(),
            &serde_json::json!({
                "_tag": "app.moonlight.certificate.revoke",
                "payload": {"hostUuid": "sunshine-host", "clientCertificate": TEST_CLIENT_PEM}
            })
            .to_string(),
        )
        .await;
        assert_eq!(revoke["outcome"]["payload"]["removed"], true);

        let before_invalid = adapter.calls();
        let invalid = rpc_body(
            lan,
            r#"{"_tag":"app.moonlight.certificate.provision","payload":{"hostUuid":"sunshine-host","clientCertificate":"secret-pem-body"}}"#,
        )
        .await;
        assert_eq!(
            invalid["outcome"]["payload"]["code"],
            "InvalidMoonlightClientCertificate"
        );
        assert_eq!(adapter.calls(), before_invalid);
        assert!(!invalid.to_string().contains("secret-pem-body"));

        for request in [
            r#"{"_tag":"app.moonlight.certificate.attest","payload":{"hostUuid":"sunshine-host"}}"#
                .to_owned(),
            serde_json::json!({
                "_tag": "app.moonlight.certificate.provision",
                "payload": {"hostUuid": "sunshine-host", "clientCertificate": TEST_CLIENT_PEM}
            })
            .to_string(),
            serde_json::json!({
                "_tag": "app.moonlight.certificate.revoke",
                "payload": {"hostUuid": "sunshine-host", "clientCertificate": TEST_CLIENT_PEM}
            })
            .to_string(),
        ] {
            let body = rpc_body(local.clone(), &request).await;
            assert_eq!(body["outcome"]["payload"]["code"], "OperationUnsupported");
        }
    }

    #[tokio::test]
    async fn brain_routes_certificate_mutation_to_one_attested_native_peer_only() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("device.yaml"), "{}\n").unwrap();
        crate::config::test_fixtures::write(root.path().join("catalog/games.yaml"), "{}\n")
            .unwrap();

        let first_config = root.path().join("first.toml");
        let second_config = root.path().join("second.toml");
        std::fs::write(&first_config, "label = \"first\"\n").unwrap();
        std::fs::write(&second_config, "label = \"second\"\n").unwrap();
        let first = RecordingMoonlightCertificates::matching("other-host");
        let second = RecordingMoonlightCertificates::matching("sunshine-host");
        let first_url =
            serve_router(host_routers_with_certificate_adapter(&first_config, first.clone()).0)
                .await;
        let second_url =
            serve_router(host_routers_with_certificate_adapter(&second_config, second.clone()).0)
                .await;
        let legacy_contacts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let contacts = legacy_contacts.clone();
        let legacy = Router::new().fallback(move || {
            let contacts = contacts.clone();
            async move {
                contacts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                StatusCode::NOT_FOUND
            }
        });
        let legacy_url = serve_router(legacy).await;
        std::fs::write(
            root.path().join("upstreams.json"),
            serde_json::json!([
                {"label":"legacy","kind":"legacy","baseUrl":legacy_url},
                {"label":"first","kind":"native","baseUrl":first_url},
                {"label":"second","kind":"native","baseUrl":second_url}
            ])
            .to_string(),
        )
        .unwrap();
        let brain = router_with_capability_and_local_root(
            "right-token",
            "https://portal.example",
            root.path(),
        );
        let provision = rpc_body_authorized(
            brain,
            &serde_json::json!({
                "_tag": "app.moonlight.certificate.provision",
                "payload": {"hostUuid": "sunshine-host", "clientCertificate": TEST_CLIENT_PEM}
            })
            .to_string(),
            Some("right-token"),
        )
        .await;
        assert_eq!(provision["outcome"]["_tag"], "Ok");
        assert_eq!(first.calls(), vec!["attest:sunshine-host"]);
        assert_eq!(
            second.calls(),
            vec!["attest:sunshine-host", "provision:sunshine-host"]
        );
        assert_eq!(legacy_contacts.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn brain_fails_closed_when_an_attestation_peer_is_unavailable() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("device.yaml"), "{}\n").unwrap();
        crate::config::test_fixtures::write(root.path().join("catalog/games.yaml"), "{}\n")
            .unwrap();
        let config = root.path().join("live.toml");
        std::fs::write(&config, "label = \"live\"\n").unwrap();
        let live = RecordingMoonlightCertificates::matching("sunshine-host");
        let live_url =
            serve_router(host_routers_with_certificate_adapter(&config, live.clone()).0).await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let unavailable_url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        std::fs::write(
            root.path().join("upstreams.json"),
            serde_json::json!([
                {"label":"unavailable","kind":"native","baseUrl":unavailable_url},
                {"label":"live","kind":"native","baseUrl":live_url}
            ])
            .to_string(),
        )
        .unwrap();
        let brain = router_with_capability_and_local_root(
            "right-token",
            "https://portal.example",
            root.path(),
        );
        let body = rpc_body_authorized(
            brain,
            &serde_json::json!({
                "_tag": "app.moonlight.certificate.provision",
                "payload": {"hostUuid": "sunshine-host", "clientCertificate": TEST_CLIENT_PEM}
            })
            .to_string(),
            Some("right-token"),
        )
        .await;
        assert_eq!(
            body["outcome"]["payload"]["code"],
            "MoonlightCertificatePeerUnavailable"
        );
        assert_eq!(live.calls(), vec!["attest:sunshine-host"]);
        assert!(!body.to_string().contains(&unavailable_url));
    }

    #[tokio::test]
    async fn brain_rejects_a_host_uuid_change_between_attest_and_mutation() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("device.yaml"), "{}\n").unwrap();
        crate::config::test_fixtures::write(root.path().join("catalog/games.yaml"), "{}\n")
            .unwrap();
        let config = root.path().join("changing.toml");
        std::fs::write(&config, "label = \"changing\"\n").unwrap();
        let changing = Arc::new(ChangingMoonlightCertificates::default());
        let peer_url =
            serve_router(host_routers_with_certificate_adapter(&config, changing.clone()).0).await;
        std::fs::write(
            root.path().join("upstreams.json"),
            serde_json::json!([
                {"label":"changing","kind":"native","baseUrl":peer_url}
            ])
            .to_string(),
        )
        .unwrap();
        let brain = router_with_capability_and_local_root(
            "right-token",
            "https://portal.example",
            root.path(),
        );
        let body = rpc_body_authorized(
            brain,
            &serde_json::json!({
                "_tag": "app.moonlight.certificate.provision",
                "payload": {"hostUuid": "sunshine-host", "clientCertificate": TEST_CLIENT_PEM}
            })
            .to_string(),
            Some("right-token"),
        )
        .await;
        assert_eq!(body["outcome"]["payload"]["code"], "MoonlightHostChanged");
        assert!(!body.to_string().contains("peer-controlled"));
        assert_eq!(
            changing.calls.lock().unwrap().as_slice(),
            ["attest:sunshine-host", "provision:sunshine-host"]
        );
    }

    #[tokio::test]
    async fn brain_rejects_zero_or_ambiguous_certificate_routes_without_mutation() {
        for expected in ["none", "sunshine-host"] {
            let root = tempfile::tempdir().unwrap();
            std::fs::write(root.path().join("device.yaml"), "{}\n").unwrap();
            crate::config::test_fixtures::write(root.path().join("catalog/games.yaml"), "{}\n")
                .unwrap();
            let mut urls = Vec::new();
            let mut adapters = Vec::new();
            for index in 0..2 {
                let config = root.path().join(format!("host-{index}.toml"));
                std::fs::write(&config, format!("label = \"host-{index}\"\n")).unwrap();
                let adapter = RecordingMoonlightCertificates::matching(expected);
                urls.push(
                    serve_router(host_routers_with_certificate_adapter(&config, adapter.clone()).0)
                        .await,
                );
                adapters.push(adapter);
            }
            std::fs::write(
                root.path().join("upstreams.json"),
                serde_json::json!([
                    {"label":"one","kind":"native","baseUrl":urls[0]},
                    {"label":"two","kind":"native","baseUrl":urls[1]}
                ])
                .to_string(),
            )
            .unwrap();
            let brain = router_with_capability_and_local_root(
                "right-token",
                "https://portal.example",
                root.path(),
            );
            let body = rpc_body_authorized(
                brain,
                &serde_json::json!({
                    "_tag": "app.moonlight.certificate.provision",
                    "payload": {"hostUuid": "sunshine-host", "clientCertificate": TEST_CLIENT_PEM}
                })
                .to_string(),
                Some("right-token"),
            )
            .await;
            let expected_code = if expected == "none" {
                "MoonlightHostNotFound"
            } else {
                "MoonlightHostAmbiguous"
            };
            assert_eq!(body["outcome"]["payload"]["code"], expected_code);
            for adapter in adapters {
                assert!(adapter
                    .calls()
                    .iter()
                    .all(|call| call.starts_with("attest:")));
            }
        }
    }
}
