//! Embedded-capable korrid server core: contracts, dispatch, and lifecycle.

use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    routing::post,
    Json, Router,
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
pub mod config;
pub mod discovery;
pub mod enrichment;
mod game_assets;
pub mod identity;
mod peer_rpc;

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
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Game {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<GameIdentity>,
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
    /** Required by the private host control listener. LAN host dispatch remains rejected. */
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
pub enum HealthOutcome {
    Ok(Health),
    Err(RpcFailure),
}

#[typeshare]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum RpcRequest {
    #[serde(rename = "app.catalog.snapshot")]
    CatalogSnapshot(CatalogSnapshotRequest),
    #[serde(rename = "app.moonlight.resolve")]
    MoonlightResolve(MoonlightResolveRequest),
    #[serde(rename = "app.moonlight.launch.prepare")]
    MoonlightLaunchPrepare(MoonlightLaunchPrepareRequest),
    #[serde(rename = "app.moonlight.launch.cancel")]
    MoonlightLaunchCancel(MoonlightLaunchCancelRequest),
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
    #[serde(rename = "app.session.controls")]
    SessionControls(SessionControlsRequest),
    #[serde(rename = "app.session.control.invoke")]
    SessionControlInvoke(SessionControlInvokeRequest),
    #[serde(rename = "app.local-games.list")]
    LocalGamesList(LocalGamesListRequest),
    #[serde(rename = "app.local-games.launch")]
    LocalGameLaunch(LocalGameLaunchRequest),
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
    #[serde(rename = "app.catalog.snapshot")]
    CatalogSnapshot(CatalogSnapshotOutcome),
    #[serde(rename = "app.moonlight.resolve")]
    MoonlightResolve(MoonlightResolveOutcome),
    #[serde(rename = "app.moonlight.launch.prepare")]
    MoonlightLaunchPrepare(MoonlightLaunchPrepareOutcome),
    #[serde(rename = "app.moonlight.launch.cancel")]
    MoonlightLaunchCancel(MoonlightLaunchCancelOutcome),
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
    #[serde(rename = "app.session.controls")]
    SessionControls(SessionControlsOutcome),
    #[serde(rename = "app.session.control.invoke")]
    SessionControlInvoke(SessionControlInvokeOutcome),
    #[serde(rename = "app.local-games.list")]
    LocalGamesList(LocalGamesListOutcome),
    #[serde(rename = "app.local-games.launch")]
    LocalGameLaunch(LocalGameLaunchOutcome),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativePlatform {
    Standalone,
    EmbeddedAndroid,
}

type RetroarchControlAuthority = Arc<launcher::retroarch_control::RetroarchControlAuthority>;
type RetroarchControlSlot = Arc<Mutex<Option<RetroarchControlAuthority>>>;

#[derive(Clone)]
struct BrainRuntime {
    upstream: upstreams::UpstreamRegistry,
    local_storage_root: PathBuf,
    private_state_root: PathBuf,
    local_file_provision: launcher::FileProvisionMode,
    local_launch_signing_key: Vec<u8>,
    local_launch_reservations: Arc<Mutex<launcher::LaunchPublicationReservations>>,
    moonlight_launch_authority: Arc<Mutex<launcher::MoonlightLaunchAuthority>>,
    active_android_launch: Arc<Mutex<Option<launcher::AndroidActiveLaunch>>>,
    retroarch_control_authority: RetroarchControlSlot,
    moonlight_executor_state: Arc<Mutex<Option<MoonlightExecutorState>>>,
    native_platform: NativePlatform,
    config_snapshot: config::snapshot::ConfigSnapshotCoordinator,
    discovery: discovery::DiscoveryLifecycleCoordinator,
    /** Serialises revision-check + replace; external file-manager edits are
     * detected by the revision inside this same critical section. */
    settings_write_lock: Arc<Mutex<()>>,
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
    mode: ServerMode,
    rpc_capability: Option<String>,
    rpc_surface: RpcSurface,
}

fn active_session_conflict() -> RpcFailure {
    RpcFailure {
        code: "ActiveSessionConflict".into(),
        message: "An active RetroArch session must end before another local route can start."
            .into(),
    }
}

fn uses_retroarch_control(active: &launcher::AndroidActiveLaunch) -> bool {
    active
        .executor
        .as_ref()
        .is_some_and(|executor| executor.id == "retroarch-control")
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
        Ok(HostSessionStatus::Running { launch_id }) => SessionStatusOutcome::Ok(SessionStatus {
            active: Some(ActiveSession {
                launch_id,
                host: None,
                game_id: None,
                title: None,
                phase: Some("running".into()),
            }),
        }),
        Ok(HostSessionStatus::Stopping { launch_id }) => SessionStatusOutcome::Ok(SessionStatus {
            active: Some(ActiveSession {
                launch_id,
                host: None,
                game_id: None,
                title: None,
                phase: Some("stopping".into()),
            }),
        }),
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

fn resolve_moonlight_outcome(
    config_state: &config::snapshot::ConfigSnapshotState,
    native_platform: NativePlatform,
) -> MoonlightResolveOutcome {
    if config_state.authorization == config::snapshot::SnapshotAuthorization::Unauthorized {
        return MoonlightResolveOutcome::Unavailable(
            config_state
                .diagnostic
                .as_ref()
                .map(snapshot_diagnostic_failure)
                .unwrap_or_else(|| RpcFailure {
                    code: "LocalConfigUnauthorized".into(),
                    message: "current configuration snapshot is unauthorized".into(),
                }),
        );
    }

    match plugin_policy::registry_for_snapshot(&config_state.snapshot) {
        Ok(registry) => (native_platform == NativePlatform::EmbeddedAndroid)
            .then(|| {
                config::resolver::resolve_moonlight_transport(
                    &registry,
                    config::resolver::RoutePlatform::Android,
                )
            })
            .flatten()
            .map(|resolved| {
                MoonlightResolveOutcome::Available(ResolvedMoonlight {
                    transport_id: resolved.transport_id,
                    implementation: MoonlightImplementation::Artemis,
                    sunshine_app: resolved.sunshine_app,
                })
            })
            .unwrap_or_else(|| {
                MoonlightResolveOutcome::Unavailable(RpcFailure {
                    code: "MoonlightUnavailable".into(),
                    message: "Moonlight is disabled or Artemis is unavailable".into(),
                })
            }),
        Err(error) => MoonlightResolveOutcome::Unavailable(RpcFailure {
            code: "PluginPolicyInvalid".into(),
            message: error.to_string(),
        }),
    }
}

fn authorize_moonlight_launch_against_current_policy(
    authority: &mut launcher::MoonlightLaunchAuthority,
    spec: &launcher::MoonlightLaunchSpec,
    current: MoonlightResolveOutcome,
) -> Result<(), launcher::MoonlightLaunchVerificationFailure> {
    match current {
        MoonlightResolveOutcome::Available(resolved)
            if spec.transport_id == resolved.transport_id
                && spec.implementation == resolved.implementation
                && spec.sunshine_app == resolved.sunshine_app =>
        {
            authority.authorize(spec)
        }
        MoonlightResolveOutcome::Available(_) | MoonlightResolveOutcome::Unavailable(_) => {
            Err(launcher::MoonlightLaunchVerificationFailure::Stale)
        }
    }
}

fn stale_session_control() -> SessionControlFailure {
    SessionControlFailure {
        reason: SessionControlFailureReason::StaleSession,
        message: "The gameplay session changed. Reopen the overlay and try again.".into(),
    }
}

fn session_route_context_unavailable() -> SessionControlFailure {
    SessionControlFailure {
        reason: SessionControlFailureReason::Unavailable,
        message: "Gameplay controls need a current active-session route and live executor state."
            .into(),
    }
}

fn retroarch_probe_unavailable(category: &str) -> SessionControlFailure {
    SessionControlFailure {
        reason: SessionControlFailureReason::Unavailable,
        message: format!("RetroArch controls are unavailable (probe: {category})."),
    }
}

fn retroarch_probe_failure(
    error: launcher::retroarch_control::RetroarchControlError,
) -> SessionControlFailure {
    use launcher::retroarch_control::RetroarchControlError;

    let category = match error {
        RetroarchControlError::InvalidAuthority => "invalid-authority",
        RetroarchControlError::Unavailable => "unavailable",
        RetroarchControlError::Timeout => "timeout",
        RetroarchControlError::WrongSource => "wrong-source",
        RetroarchControlError::WrongResponse => "wrong-response",
    };
    retroarch_probe_unavailable(category)
}

enum MaterializedSessionExecutor {
    Moonlight(MoonlightExecutorState),
    Retroarch(RetroarchControlAuthority),
    OverlayOnly,
}

fn materialize_session_controls_snapshot(
    active_android_launch: &Arc<Mutex<Option<launcher::AndroidActiveLaunch>>>,
    retroarch_control_authority: &RetroarchControlSlot,
    moonlight_executor_state: &Arc<Mutex<Option<MoonlightExecutorState>>>,
    config_snapshot: &config::snapshot::ConfigSnapshotCoordinator,
    launch_id: &str,
) -> Result<
    (
        launcher::AndroidActiveLaunch,
        MaterializedSessionExecutor,
        Vec<(plugin::SessionControlRecord, SessionControl)>,
        Vec<SessionControlGroup>,
    ),
    SessionControlFailure,
> {
    let active = active_android_launch
        .lock()
        .expect("active Android launch mutex poisoned")
        .clone()
        .ok_or_else(stale_session_control)?;
    if active.launch_id != launch_id {
        return Err(stale_session_control());
    }
    let executor = match active
        .executor
        .as_ref()
        .map(|executor| executor.id.as_str())
    {
        Some("android-moonlight") => {
            let state = moonlight_executor_state
                .lock()
                .expect("Moonlight executor state mutex poisoned")
                .clone()
                .filter(|executor| executor.launch_id == active.launch_id)
                .ok_or_else(session_route_context_unavailable)?;
            let live = active.executor.as_ref().is_some_and(|executor| {
                executor.id == state.executor_id
                    && state.generation.len() == 64
                    && state
                        .generation
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit())
                    && state.is_strict()
            });
            if !live {
                return Err(session_route_context_unavailable());
            }
            MaterializedSessionExecutor::Moonlight(state)
        }
        Some("retroarch-control") => {
            let authority = retroarch_control_authority
                .lock()
                .expect("RetroArch control authority mutex poisoned")
                .clone()
                .filter(|authority| authority.is_for(&active.launch_id));
            if !active
                .executor
                .as_ref()
                .is_some_and(|executor| executor.available)
            {
                return Err(session_route_context_unavailable());
            }
            MaterializedSessionExecutor::Retroarch(
                authority.ok_or_else(session_route_context_unavailable)?,
            )
        }
        None => MaterializedSessionExecutor::OverlayOnly,
        _ => return Err(session_route_context_unavailable()),
    };
    let snapshot = config_snapshot.reload();
    if snapshot.authorization == config::snapshot::SnapshotAuthorization::Unauthorized {
        return Err(session_route_context_unavailable());
    }
    let registry = plugin_policy::registry_for_snapshot(&snapshot.snapshot)
        .map_err(|_| session_route_context_unavailable())?;
    let contributors = active
        .contributors
        .iter()
        .map(|contributor| config::resolver::RouteContribution {
            kind: match contributor.kind {
                launcher::LaunchContributorKind::Launcher => {
                    plugin::SessionControlOwnerKind::Launcher
                }
                launcher::LaunchContributorKind::Transport => {
                    plugin::SessionControlOwnerKind::Transport
                }
                launcher::LaunchContributorKind::Runtime => {
                    plugin::SessionControlOwnerKind::Runtime
                }
            },
            id: contributor.id.clone(),
        })
        .collect();
    let available_executors = match &executor {
        MaterializedSessionExecutor::Moonlight(_) => {
            vec![plugin::SessionControlExecutor::AndroidMoonlight]
        }
        MaterializedSessionExecutor::Retroarch(_) => {
            vec![plugin::SessionControlExecutor::RetroarchControl]
        }
        MaterializedSessionExecutor::OverlayOnly => Vec::new(),
    };
    let context = config::resolver::ActiveRouteContext {
        platform: config::resolver::RoutePlatform::Android,
        contributors,
        executor_availability: config::resolver::SessionExecutorAvailability::from_available(
            available_executors,
        ),
    };
    let mut materialized = Vec::new();
    for record in config::resolver::resolve_session_controls(&registry, &context) {
        let interaction = match &executor {
            MaterializedSessionExecutor::Retroarch(_) => {
                if record.effect.retroarch_control_command().is_none()
                    || !matches!(
                        record.interaction,
                        plugin::SessionControlDeclarationInteraction::Command
                    )
                {
                    continue;
                }
                SessionControlInteraction::Command
            }
            MaterializedSessionExecutor::OverlayOnly => continue,
            MaterializedSessionExecutor::Moonlight(executor_state) => {
                let Some(effect) = record.effect.android_moonlight_effect() else {
                    continue;
                };
                let Some(live) = executor_state
                    .effect(effect)
                    .filter(|entry| entry.fulfillable)
                else {
                    continue;
                };
                match (&record.interaction, &live.value) {
                    (plugin::SessionControlDeclarationInteraction::Command, None) => {
                        SessionControlInteraction::Command
                    }
                    (
                        plugin::SessionControlDeclarationInteraction::Toggle {
                            true_label,
                            false_label,
                        },
                        Some(SessionControlValue::Toggle(value)),
                    ) => SessionControlInteraction::Toggle {
                        value: *value,
                        true_label: true_label.clone().unwrap_or_else(|| "On".into()),
                        false_label: false_label.clone().unwrap_or_else(|| "Off".into()),
                    },
                    (
                        plugin::SessionControlDeclarationInteraction::Choice { options },
                        Some(SessionControlValue::Choice(value)),
                    ) if options.iter().any(|option| option.value == *value) => {
                        SessionControlInteraction::Choice {
                            value: value.clone(),
                            options: options
                                .iter()
                                .map(|option| SessionControlChoice {
                                    value: option.value.clone(),
                                    label: option.label.clone(),
                                })
                                .collect(),
                        }
                    }
                    (
                        plugin::SessionControlDeclarationInteraction::Range { min, max, step },
                        Some(SessionControlValue::Range(value)),
                    ) => {
                        let (live_min, live_max, live_step) =
                            live.range.as_ref().map_or((*min, *max, *step), |range| {
                                (range.min, range.max, range.step)
                            });
                        if live_min < *min
                            || live_max > *max
                            || live_step != *step
                            || !valid_range_value(*value, live_min, live_max, live_step)
                        {
                            continue;
                        }
                        SessionControlInteraction::Range {
                            value: *value,
                            min: live_min,
                            max: live_max,
                            step: live_step,
                        }
                    }
                    _ => continue,
                }
            }
        };
        let control = SessionControl {
            id: record.id.clone(),
            label: record.label.clone(),
            description: record.description.clone(),
            enabled: true,
            disabled_reason: None,
            destructive: record.destructive,
            dismiss_on_success: record.dismiss_on_success,
            interaction,
        };
        materialized.push((record, control));
    }
    let mut groups: Vec<SessionControlGroup> = Vec::new();
    for (record, control) in &materialized {
        if let Some(group) = groups.iter_mut().find(|group| group.id == record.plugin_id) {
            group.controls.push(control.clone());
        } else {
            groups.push(SessionControlGroup {
                id: record.plugin_id.clone(),
                label: registry
                    .plugin_title(&record.plugin_id)
                    .unwrap_or(&record.plugin_id)
                    .to_owned(),
                controls: vec![control.clone()],
            });
        }
    }
    Ok((active, executor, materialized, groups))
}

async fn materialize_session_controls(
    active_android_launch: &Arc<Mutex<Option<launcher::AndroidActiveLaunch>>>,
    retroarch_control_authority: &RetroarchControlSlot,
    moonlight_executor_state: &Arc<Mutex<Option<MoonlightExecutorState>>>,
    config_snapshot: &config::snapshot::ConfigSnapshotCoordinator,
    launch_id: &str,
) -> Result<
    (
        launcher::AndroidActiveLaunch,
        MaterializedSessionExecutor,
        Vec<(plugin::SessionControlRecord, SessionControl)>,
        Vec<SessionControlGroup>,
        Option<RetroarchSessionTelemetry>,
    ),
    SessionControlFailure,
> {
    let candidate = materialize_session_controls_snapshot(
        active_android_launch,
        retroarch_control_authority,
        moonlight_executor_state,
        config_snapshot,
        launch_id,
    )?;
    let MaterializedSessionExecutor::Retroarch(authority) = &candidate.1 else {
        return Ok((candidate.0, candidate.1, candidate.2, candidate.3, None));
    };
    let authority = Arc::clone(authority);
    let probe = Arc::clone(&authority);
    let status = tokio::task::spawn_blocking(move || probe.expected_status())
        .await
        .map_err(|_| retroarch_probe_unavailable("unavailable"))?
        .map_err(retroarch_probe_failure)?
        .ok_or_else(|| retroarch_probe_unavailable("identity-mismatch"))?;
    let current = active_android_launch
        .lock()
        .expect("active Android launch mutex poisoned")
        .clone();
    let current_authority = retroarch_control_authority
        .lock()
        .expect("RetroArch control authority mutex poisoned")
        .clone();
    if current.as_ref() != Some(&candidate.0)
        || current_authority
            .as_ref()
            .is_none_or(|current| !Arc::ptr_eq(current, &authority))
    {
        return Err(session_route_context_unavailable());
    }
    let telemetry = RetroarchSessionTelemetry {
        content_basename: status.content,
        crc32: status.crc32,
        menu_alive: status.menu_alive,
        menu_selection: u32::try_from(status.menu_selection)
            .map_err(|_| session_route_context_unavailable())?,
    };
    Ok((
        candidate.0,
        candidate.1,
        candidate.2,
        candidate.3,
        Some(telemetry),
    ))
}

fn retire_exact_retroarch_launch(
    active_android_launch: &Arc<Mutex<Option<launcher::AndroidActiveLaunch>>>,
    retroarch_control_authority: &RetroarchControlSlot,
    expected_launch: &launcher::AndroidActiveLaunch,
    expected_authority: &RetroarchControlAuthority,
) -> bool {
    // Keep the established active-then-authority lock order. Holding both locks
    // makes the terminal transition indivisible to publication and late ACKs.
    let mut active = active_android_launch
        .lock()
        .expect("active Android launch mutex poisoned");
    let mut authority = retroarch_control_authority
        .lock()
        .expect("RetroArch control authority mutex poisoned");
    if active.as_ref() != Some(expected_launch)
        || authority
            .as_ref()
            .is_none_or(|current| !Arc::ptr_eq(current, expected_authority))
    {
        return false;
    }
    *active = None;
    *authority = None;
    true
}

async fn invoke_current_session_control(
    brain: &BrainRuntime,
    request: SessionControlInvokeRequest,
) -> SessionControlInvokeOutcome {
    let (active, _, materialized, _, _) = match materialize_session_controls(
        &brain.active_android_launch,
        &brain.retroarch_control_authority,
        &brain.moonlight_executor_state,
        &brain.config_snapshot,
        &request.launch_id,
    )
    .await
    {
        Ok(resolved) => resolved,
        Err(failure) => return SessionControlInvokeOutcome::Err(failure),
    };
    let Some((_, control)) = materialized
        .iter()
        .find(|(_, control)| control.id == request.control_id)
    else {
        return SessionControlInvokeOutcome::Err(SessionControlFailure {
            reason: SessionControlFailureReason::UnknownControl,
            message: "That gameplay control is no longer available.".into(),
        });
    };
    if let Err(failure) = validate_session_control_invocation(&active.launch_id, &request, control)
    {
        return SessionControlInvokeOutcome::Err(failure);
    }

    // Reconstruct policy, route, live executor state, and the selected control
    // immediately before performing either native or process-local effects.
    let (current, executor, latest, _, _) = match materialize_session_controls(
        &brain.active_android_launch,
        &brain.retroarch_control_authority,
        &brain.moonlight_executor_state,
        &brain.config_snapshot,
        &request.launch_id,
    )
    .await
    {
        Ok(resolved) => resolved,
        Err(failure) => return SessionControlInvokeOutcome::Err(failure),
    };
    let Some((record, control)) = latest
        .iter()
        .find(|(_, control)| control.id == request.control_id)
    else {
        return SessionControlInvokeOutcome::Err(session_route_context_unavailable());
    };
    if let Err(failure) = validate_session_control_invocation(&current.launch_id, &request, control)
    {
        return SessionControlInvokeOutcome::Err(failure);
    }

    match executor {
        MaterializedSessionExecutor::Moonlight(executor) => {
            let Some(effect) = record.effect.android_moonlight_effect() else {
                return SessionControlInvokeOutcome::Err(session_route_context_unavailable());
            };
            let instruction = launcher::PlatformInstruction::protect(
                current.launch_id,
                executor.executor_id,
                executor.generation,
                control.id.clone(),
                control.dismiss_on_success,
                request.value,
                launcher::PlatformEffect::AndroidMoonlight(effect),
                &brain.local_launch_signing_key,
            );
            SessionControlInvokeOutcome::Ok(SessionControlInvokeResult::PlatformInstruction(
                instruction,
            ))
        }
        MaterializedSessionExecutor::OverlayOnly => {
            SessionControlInvokeOutcome::Err(session_route_context_unavailable())
        }
        MaterializedSessionExecutor::Retroarch(authority) => {
            let Some(command) = record.effect.retroarch_control_command() else {
                return SessionControlInvokeOutcome::Err(session_route_context_unavailable());
            };
            let request_authority = Arc::clone(&authority);
            let acknowledged =
                tokio::task::spawn_blocking(move || request_authority.invoke(command))
                    .await
                    .is_ok_and(|result| result.is_ok());
            if !acknowledged {
                return SessionControlInvokeOutcome::Err(SessionControlFailure {
                    reason: SessionControlFailureReason::Unavailable,
                    message: "RetroArch did not acknowledge that gameplay control.".into(),
                });
            }
            if command == launcher::retroarch_control::RetroarchControlCommand::Quit {
                if !retire_exact_retroarch_launch(
                    &brain.active_android_launch,
                    &brain.retroarch_control_authority,
                    &current,
                    &authority,
                ) {
                    return SessionControlInvokeOutcome::Err(session_route_context_unavailable());
                }
            } else {
                let active = brain
                    .active_android_launch
                    .lock()
                    .expect("active Android launch mutex poisoned")
                    .clone();
                let exact_authority = brain
                    .retroarch_control_authority
                    .lock()
                    .expect("RetroArch control authority mutex poisoned")
                    .clone()
                    .is_some_and(|current_authority| Arc::ptr_eq(&current_authority, &authority));
                if active.as_ref() != Some(&current) || !exact_authority {
                    return SessionControlInvokeOutcome::Err(session_route_context_unavailable());
                }
            }
            SessionControlInvokeOutcome::Ok(SessionControlInvokeResult::Completed(
                SessionControlCompleted {
                    launch_id: current.launch_id,
                },
            ))
        }
    }
}

async fn dispatch(state: &AppState, request: RpcRequest) -> RpcResponse {
    match request {
        RpcRequest::CatalogSnapshot(_) => {
            let outcome = match &state.mode {
                ServerMode::Brain(brain) => brain
                    .upstream
                    .catalog_snapshot()
                    .await
                    .map(CatalogSnapshotOutcome::Ok)
                    .unwrap_or_else(|error| CatalogSnapshotOutcome::Err(upstream_failure(error))),
                ServerMode::Host(host) => host
                    .catalog_snapshot()
                    .map(CatalogSnapshotOutcome::Ok)
                    .unwrap_or_else(CatalogSnapshotOutcome::Err),
            };
            RpcResponse::CatalogSnapshot(outcome)
        }
        RpcRequest::MoonlightResolve(_) => {
            let outcome = match &state.mode {
                ServerMode::Brain(brain) => resolve_moonlight_outcome(
                    &brain.config_snapshot.reload(),
                    brain.native_platform,
                ),
                ServerMode::Host(_) => MoonlightResolveOutcome::Unavailable(RpcFailure {
                    code: "MoonlightUnavailable".into(),
                    message: "Artemis is available only at the Android edge".into(),
                }),
            };
            RpcResponse::MoonlightResolve(outcome)
        }
        RpcRequest::MoonlightLaunchPrepare(request) => {
            let outcome = match &state.mode {
                ServerMode::Brain(brain) => {
                    let mut authority = brain
                        .moonlight_launch_authority
                        .lock()
                        .expect("Moonlight launch authority poisoned");
                    match resolve_moonlight_outcome(
                        &brain.config_snapshot.reload(),
                        brain.native_platform,
                    ) {
                        MoonlightResolveOutcome::Available(resolved)
                            if !request.host_uuid.is_empty()
                                && request.app_id > 0
                                && request.app_id <= i32::MAX as u32 =>
                        {
                            let spec = authority.prepare(
                                resolved.transport_id,
                                resolved.implementation,
                                resolved.sunshine_app,
                                request.host_uuid,
                                request.app_id,
                                request.game_id,
                                request.title,
                            );
                            MoonlightLaunchPrepareOutcome::Ok(spec)
                        }
                        MoonlightResolveOutcome::Available(_) => {
                            MoonlightLaunchPrepareOutcome::Err(RpcFailure {
                                code: "InvalidMoonlightLaunchTarget".into(),
                                message:
                                    "Moonlight host UUID and positive Android app ID are required"
                                        .into(),
                            })
                        }
                        MoonlightResolveOutcome::Unavailable(failure) => {
                            MoonlightLaunchPrepareOutcome::Err(failure)
                        }
                    }
                }
                ServerMode::Host(_) => MoonlightLaunchPrepareOutcome::Err(RpcFailure {
                    code: "MoonlightUnavailable".into(),
                    message: "Artemis is available only at the Android edge".into(),
                }),
            };
            RpcResponse::MoonlightLaunchPrepare(outcome)
        }
        RpcRequest::MoonlightLaunchCancel(request) => {
            let outcome = match &state.mode {
                ServerMode::Brain(brain) => {
                    let cancelled = brain
                        .moonlight_launch_authority
                        .lock()
                        .expect("Moonlight launch authority poisoned")
                        .cancel(&request.launch_id);
                    if cancelled {
                        MoonlightLaunchCancelOutcome::Ok(MoonlightLaunchCancelled {
                            launch_id: request.launch_id,
                        })
                    } else {
                        MoonlightLaunchCancelOutcome::Err(RpcFailure {
                            code: "MoonlightLaunchReservationNotCurrent".into(),
                            message: "Moonlight launch reservation is not current and unused"
                                .into(),
                        })
                    }
                }
                ServerMode::Host(_) => MoonlightLaunchCancelOutcome::Err(RpcFailure {
                    code: "MoonlightUnavailable".into(),
                    message: "Artemis is available only at the Android edge".into(),
                }),
            };
            RpcResponse::MoonlightLaunchCancel(outcome)
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
                    .prepare(&request.game_id)
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
            (ServerMode::Host(host), RpcSurface::LocalControl) => {
                RpcResponse::SessionStatus(host_session_status_outcome(host.session_status().await))
            }
            (ServerMode::Host(_), RpcSurface::Lan)
            | (ServerMode::Brain(_), RpcSurface::LocalControl) => {
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
                        .session_stop(request.force.unwrap_or(false))
                        .await,
                ))
            }
            (ServerMode::Host(host), RpcSurface::LocalControl) => {
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
            (ServerMode::Host(_), RpcSurface::Lan)
            | (ServerMode::Brain(_), RpcSurface::LocalControl) => {
                RpcResponse::SessionStop(SessionStopOutcome::Err(RpcFailure {
                    code: "SessionStopUnsupported".into(),
                    message: "session stop is unavailable on this listener".into(),
                }))
            }
        },
        RpcRequest::SessionControls(request) => {
            let outcome = match &state.mode {
                ServerMode::Brain(brain) => materialize_session_controls(
                    &brain.active_android_launch,
                    &brain.retroarch_control_authority,
                    &brain.moonlight_executor_state,
                    &brain.config_snapshot,
                    &request.launch_id,
                )
                .await
                .map(
                    |(active, _, _, groups, retroarch_telemetry)| SessionControls {
                        launch_id: active.launch_id,
                        title: active.title,
                        groups,
                        retroarch_telemetry,
                    },
                )
                .map(SessionControlsOutcome::Ok)
                .unwrap_or_else(SessionControlsOutcome::Err),
                ServerMode::Host(_) => {
                    SessionControlsOutcome::Err(session_route_context_unavailable())
                }
            };
            RpcResponse::SessionControls(outcome)
        }
        RpcRequest::SessionControlInvoke(request) => {
            let outcome = match &state.mode {
                ServerMode::Brain(brain) => invoke_current_session_control(brain, request).await,
                ServerMode::Host(_) => {
                    SessionControlInvokeOutcome::Err(session_route_context_unavailable())
                }
            };
            RpcResponse::SessionControlInvoke(outcome)
        }
        RpcRequest::LocalGamesList(_) => match &state.mode {
            ServerMode::Brain(brain) => {
                let config_state = brain.config_snapshot.reload();
                let registry = match plugin_policy::registry_for_snapshot(&config_state.snapshot) {
                    Ok(registry) => registry,
                    Err(error) => {
                        return RpcResponse::LocalGamesList(LocalGamesListOutcome::Err(
                            RpcFailure {
                                code: "PluginPolicyInvalid".into(),
                                message: error.to_string(),
                            },
                        ));
                    }
                };
                let catalog = launcher::local_games_with_cover_assets(
                    Some(&brain.local_storage_root),
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
        RpcRequest::LocalGameLaunch(request) => match &state.mode {
            ServerMode::Brain(brain) => {
                let config_state = brain.config_snapshot.reload();
                let registry = match plugin_policy::registry_for_snapshot(&config_state.snapshot) {
                    Ok(registry) => registry,
                    Err(error) => {
                        return RpcResponse::LocalGameLaunch(LocalGameLaunchOutcome::Err(
                            RpcFailure {
                                code: "PluginPolicyInvalid".into(),
                                message: error.to_string(),
                            },
                        ));
                    }
                };
                enum ActiveLaunchDecision {
                    Resume(launcher::LaunchSpec),
                    Conflict,
                    Fresh,
                }

                let active = brain
                    .active_android_launch
                    .lock()
                    .expect("active Android launch mutex poisoned")
                    .clone();
                let active_retroarch = active
                    .as_ref()
                    .filter(|active| uses_retroarch_control(active));
                let decision = if let Some(expected_active) = active_retroarch {
                    let authority = brain
                        .retroarch_control_authority
                        .lock()
                        .expect("RetroArch control authority mutex poisoned")
                        .clone();
                    let Some(expected_authority) = authority
                        .as_ref()
                        .filter(|authority| authority.is_for(&expected_active.launch_id))
                    else {
                        return RpcResponse::LocalGameLaunch(LocalGameLaunchOutcome::Err(
                            active_session_conflict(),
                        ));
                    };
                    if expected_active.game_id.as_deref() != Some(request.game_id.as_str()) {
                        ActiveLaunchDecision::Conflict
                    } else {
                        match launcher::launch_game(
                            &brain.local_storage_root,
                            &request.game_id,
                            brain.local_file_provision,
                            &config_state,
                            &registry,
                            expected_authority.port(),
                        ) {
                            Ok(spec)
                                if expected_authority.matches_launch(expected_active, &spec) =>
                            {
                                let request_authority = Arc::clone(expected_authority);
                                let live = tokio::task::spawn_blocking(move || {
                                    request_authority.confirms_expected_content()
                                })
                                .await
                                .is_ok_and(|result| result == Ok(true));
                                let current_active = brain
                                    .active_android_launch
                                    .lock()
                                    .expect("active Android launch mutex poisoned")
                                    .clone();
                                let current_authority = brain
                                    .retroarch_control_authority
                                    .lock()
                                    .expect("RetroArch control authority mutex poisoned")
                                    .clone();
                                let still_exact = current_active.as_ref() == Some(expected_active)
                                    && current_authority.as_ref().is_some_and(|current| {
                                        Arc::ptr_eq(current, expected_authority)
                                    });
                                if live && still_exact {
                                    ActiveLaunchDecision::Resume(
                                        spec.with_launch_id(expected_active.launch_id.clone())
                                            .with_disposition(launcher::LaunchDisposition::Resume)
                                            .sign(&brain.local_launch_signing_key),
                                    )
                                } else if still_exact
                                    || current_active.as_ref().is_some_and(uses_retroarch_control)
                                {
                                    ActiveLaunchDecision::Conflict
                                } else {
                                    ActiveLaunchDecision::Fresh
                                }
                            }
                            Ok(_) | Err(_) => ActiveLaunchDecision::Conflict,
                        }
                    }
                } else {
                    ActiveLaunchDecision::Fresh
                };

                let outcome = match decision {
                    ActiveLaunchDecision::Resume(spec) => LocalGameLaunchOutcome::Ok(spec),
                    ActiveLaunchDecision::Conflict => {
                        LocalGameLaunchOutcome::Err(active_session_conflict())
                    }
                    ActiveLaunchDecision::Fresh => {
                        let fresh_launch_id = generate_launch_id();
                        let fresh_control_port = launcher::derive_retroarch_control_port(
                            &brain.local_launch_signing_key,
                            &fresh_launch_id,
                        );
                        launcher::launch_game(
                            &brain.local_storage_root,
                            &request.game_id,
                            brain.local_file_provision,
                            &config_state,
                            &registry,
                            fresh_control_port,
                        )
                        .map_err(local_launch_failure)
                        .and_then(|spec| {
                            let active = brain
                                .active_android_launch
                                .lock()
                                .expect("active Android launch mutex poisoned")
                                .clone();
                            if active.as_ref().is_some_and(uses_retroarch_control) {
                                return Err(active_session_conflict());
                            }
                            let signed = spec
                                .with_launch_id(fresh_launch_id)
                                .with_disposition(launcher::LaunchDisposition::Fresh)
                                .sign(&brain.local_launch_signing_key);
                            brain
                                .local_launch_reservations
                                .lock()
                                .expect("local launch reservations poisoned")
                                .reserve(signed.launch_id.clone());
                            Ok(signed)
                        })
                        .map(LocalGameLaunchOutcome::Ok)
                        .unwrap_or_else(LocalGameLaunchOutcome::Err)
                    }
                };
                RpcResponse::LocalGameLaunch(outcome)
            }
            ServerMode::Host(_) => {
                RpcResponse::LocalGameLaunch(LocalGameLaunchOutcome::Err(RpcFailure {
                    code: "OperationUnsupported".into(),
                    message: "local game launch is available only from the Android brain".into(),
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
                config::settings::read(&brain.local_storage_root)
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
                let _write = brain
                    .settings_write_lock
                    .lock()
                    .expect("settings write lock poisoned");
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
                    config::settings::update(
                        &brain.local_storage_root,
                        &request.expected_revision,
                        change,
                    )
                });
                if outcome.is_ok() {
                    brain.config_snapshot.reload();
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
    }
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
    if let Some(capability) = &state.rpc_capability {
        let expected = format!("Bearer {capability}");
        if headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            != Some(expected.as_str())
        {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    Ok(Json(dispatch(&state, request).await))
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
        private_state_root,
        launcher::FileProvisionMode::Direct,
        signing_key,
        local_launch_reservations,
        moonlight_launch_authority,
        Arc::new(Mutex::new(None)),
        Arc::new(Mutex::new(None)),
        Arc::new(Mutex::new(None)),
        NativePlatform::Standalone,
        config_snapshot,
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
        Arc::new(Mutex::new(None)),
        NativePlatform::Standalone,
        config_snapshot,
    )
}

/** Test-only Android router. Production selects this platform only through
 * the Android-gated JNI server entrypoint. */
#[cfg(test)]
fn android_router_with_capability_and_local_root(
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
        local_storage_root.clone(),
        local_storage_root.join(".private-test"),
        launcher::FileProvisionMode::Deferred,
        signing_key,
        local_launch_reservations,
        moonlight_launch_authority,
        Arc::new(Mutex::new(None)),
        Arc::new(Mutex::new(None)),
        Arc::new(Mutex::new(None)),
        NativePlatform::EmbeddedAndroid,
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
    active_android_launch: Arc<Mutex<Option<launcher::AndroidActiveLaunch>>>,
    retroarch_control_authority: RetroarchControlSlot,
    moonlight_executor_state: Arc<Mutex<Option<MoonlightExecutorState>>>,
    native_platform: NativePlatform,
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
        retroarch_control_authority,
        moonlight_executor_state,
        native_platform,
        config_snapshot,
        discovery::FolderSelectionGrantStore::default(),
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
    active_android_launch: Arc<Mutex<Option<launcher::AndroidActiveLaunch>>>,
    retroarch_control_authority: RetroarchControlSlot,
    moonlight_executor_state: Arc<Mutex<Option<MoonlightExecutorState>>>,
    native_platform: NativePlatform,
    config_snapshot: config::snapshot::ConfigSnapshotCoordinator,
    folder_selection_grants: discovery::FolderSelectionGrantStore,
    configured_upstream: Option<upstreams::UpstreamRegistry>,
) -> Router {
    let local_storage_root = local_storage_root.as_ref().to_owned();
    let private_state_root = private_state_root.as_ref().to_owned();
    let settings_write_lock = Arc::new(Mutex::new(()));
    let discovery = discovery::DiscoveryLifecycleCoordinator::new(
        &local_storage_root,
        &private_state_root,
        settings_write_lock.clone(),
        folder_selection_grants.clone(),
    );
    let upstream = configured_upstream.unwrap_or_else(|| {
        #[cfg(not(test))]
        {
            let peer_credentials = peer_rpc::PeerCredentials::load(&private_state_root, None)
                .expect("load or create the local device identity");
            upstreams::UpstreamRegistry::from_env_or_file(
                &local_storage_root.join("upstreams.json"),
                peer_credentials,
            )
        }
        #[cfg(test)]
        {
            upstreams::UpstreamRegistry::from_env_or_file_for_tests(
                &local_storage_root.join("upstreams.json"),
            )
        }
    });
    let state = AppState {
        mode: ServerMode::Brain(BrainRuntime {
            upstream,
            local_storage_root,
            private_state_root,
            local_file_provision,
            local_launch_signing_key,
            local_launch_reservations,
            moonlight_launch_authority,
            active_android_launch,
            retroarch_control_authority,
            moonlight_executor_state,
            native_platform,
            config_snapshot,
            discovery,
            settings_write_lock,
        }),
        rpc_capability: Some(rpc_capability.into()),
        rpc_surface: RpcSurface::Lan,
    };
    let configured_origin: HeaderValue = allowed_origin
        .parse()
        .expect("allowed portal origin must be a valid header value");
    let allowed_origins = if native_platform == NativePlatform::EmbeddedAndroid {
        let bundled_origin = HeaderValue::from_static(ANDROID_BUNDLED_PORTAL_ORIGIN);
        let mut origins = vec![configured_origin];
        if origins[0] != bundled_origin {
            origins.push(bundled_origin);
        }
        tower_http::cors::AllowOrigin::list(origins)
    } else {
        tower_http::cors::AllowOrigin::exact(configured_origin)
    };
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
        )
        .0
    }
}

/// Build LAN and private-control routers over one singular host runtime.
pub fn host_routers_with_storage_and_private(
    config_path: impl AsRef<Path>,
    storage_root: Option<impl Into<PathBuf>>,
    private_state_root: impl Into<PathBuf>,
) -> (Router, Router) {
    let private_state_root = private_state_root.into();
    let runtime = host::HostRuntime::from_paths_with_private_state(
        config_path.as_ref(),
        storage_root.map(Into::into),
        private_state_root.clone(),
    );
    secure_host_routers(runtime, &private_state_root)
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
fn secure_host_router_with_in_memory_units(
    config_path: impl AsRef<Path>,
    private_state_root: &Path,
) -> Router {
    let runtime = host::HostRuntime::from_paths_with_backend(
        config_path.as_ref(),
        None,
        private_state_root.to_owned(),
        Arc::new(host::control::InMemoryLaunchUnitBackend::default()),
    );
    secure_host_routers(runtime, private_state_root).0
}

fn app_states(runtime: host::HostRuntime) -> (AppState, AppState) {
    let lan = AppState {
        mode: ServerMode::Host(runtime.clone()),
        rpc_capability: None,
        rpc_surface: RpcSurface::Lan,
    };
    let local = AppState {
        mode: ServerMode::Host(runtime),
        rpc_capability: None,
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

fn secure_host_routers(runtime: host::HostRuntime, private_state_root: &Path) -> (Router, Router) {
    let (lan, local) = app_states(runtime);
    let peer = peer_rpc::PeerRpcServer::new(lan, private_state_root)
        .expect("load or create peer RPC identity");
    (
        peer.router(),
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
    launch_signing_key: Vec<u8>,
    local_launch_reservations: Arc<Mutex<launcher::LaunchPublicationReservations>>,
    active_android_launch: Arc<Mutex<Option<launcher::AndroidActiveLaunch>>>,
    retroarch_control_authority: RetroarchControlSlot,
    moonlight_executor_state: Arc<Mutex<Option<MoonlightExecutorState>>>,
    platform_instruction_verifier: Option<launcher::PlatformInstructionVerifier>,
    moonlight_launch_authority: Arc<Mutex<launcher::MoonlightLaunchAuthority>>,
    moonlight_config_snapshot: config::snapshot::ConfigSnapshotCoordinator,
    native_platform: NativePlatform,
    folder_selection_grants: discovery::FolderSelectionGrantStore,
    upstream: upstreams::UpstreamRegistry,
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
        NativePlatform::Standalone,
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
        NativePlatform::EmbeddedAndroid,
    )
}

fn start_local_server_for_platform(
    allowed_origin: &str,
    local_storage_root: &str,
    private_state_root: &str,
    native_platform: NativePlatform,
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
    let retroarch_control_authority = Arc::new(Mutex::new(None));
    let router_retroarch_control_authority = Arc::clone(&retroarch_control_authority);
    let moonlight_executor_state = Arc::new(Mutex::new(None));
    let router_moonlight_executor_state = Arc::clone(&moonlight_executor_state);
    let folder_selection_grants = discovery::FolderSelectionGrantStore::default();
    let server_folder_selection_grants = folder_selection_grants.clone();
    let allowed_origin = allowed_origin.to_owned();
    let local_storage_root = local_storage_root.to_owned();
    let private_state_root = private_state_root.to_owned();
    let moonlight_config_snapshot =
        config::snapshot::ConfigSnapshotCoordinator::new(&local_storage_root);
    let server_config_snapshot = moonlight_config_snapshot.clone();
    #[cfg(not(test))]
    let upstream = {
        let peer_credentials = peer_rpc::PeerCredentials::load(Path::new(&private_state_root), None)
            .map_err(|error| ServerError::StartFailed {
                details: error.to_string(),
            })?;
        upstreams::UpstreamRegistry::from_env_or_file(
            Path::new(&local_storage_root)
                .join("upstreams.json")
                .as_path(),
            peer_credentials,
        )
    };
    #[cfg(test)]
    let upstream = upstreams::UpstreamRegistry::from_env_or_file_for_tests(
        Path::new(&local_storage_root)
            .join("upstreams.json")
            .as_path(),
    );
    let server_upstream = upstream.clone();
    let (stop, stopped) = oneshot::channel();
    let thread = std::thread::Builder::new()
        .name("korrid".into())
        .spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("create Tokio runtime");
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener)
                    .expect("convert localhost listener");
                axum::serve(
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
                        router_retroarch_control_authority,
                        router_moonlight_executor_state,
                        native_platform,
                        server_config_snapshot,
                        server_folder_selection_grants,
                        Some(server_upstream),
                    ),
                )
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
                .expect("serve korrid");
            });
        })
        .map_err(|error| ServerError::StartFailed {
            details: error.to_string(),
        })?;

    *slot = Some(ServerHandle {
        port,
        rpc_capability,
        launch_signing_key,
        local_launch_reservations,
        active_android_launch,
        retroarch_control_authority,
        moonlight_executor_state,
        platform_instruction_verifier: None,
        moonlight_launch_authority,
        moonlight_config_snapshot,
        native_platform,
        folder_selection_grants,
        upstream,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActiveAndroidLaunchFailure {
    InvalidSpec,
    Integrity,
    Stale,
    NotStarted,
    AlreadyPublished,
    ServerUnavailable,
}

fn active_launch_reservation_failure(
    failure: launcher::LaunchPublicationReservationFailure,
) -> ActiveAndroidLaunchFailure {
    match failure {
        launcher::LaunchPublicationReservationFailure::Stale => ActiveAndroidLaunchFailure::Stale,
        launcher::LaunchPublicationReservationFailure::NotStarted => {
            ActiveAndroidLaunchFailure::NotStarted
        }
        launcher::LaunchPublicationReservationFailure::AlreadyPublished
        | launcher::LaunchPublicationReservationFailure::Replay => {
            ActiveAndroidLaunchFailure::AlreadyPublished
        }
    }
}

fn publish_verified_android_launch(
    server: &mut ServerHandle,
    launch: launcher::AndroidActiveLaunch,
) -> launcher::AndroidActiveLaunch {
    let same_launch = server
        .active_android_launch
        .lock()
        .expect("active Android launch mutex poisoned")
        .as_ref()
        .is_some_and(|active| active.launch_id == launch.launch_id);
    if !same_launch {
        server.platform_instruction_verifier = Some(launcher::PlatformInstructionVerifier::new(
            launch.launch_id.clone(),
        ));
    }
    *server
        .active_android_launch
        .lock()
        .expect("active Android launch mutex poisoned") = Some(launch.clone());
    launch
}

/** Publish signed local context only after Java reports Android start success. */
pub fn publish_local_active_launch(
    spec_json: &str,
) -> Result<launcher::AndroidActiveLaunch, ActiveAndroidLaunchFailure> {
    let spec = serde_json::from_str::<launcher::LaunchSpec>(spec_json)
        .map_err(|_| ActiveAndroidLaunchFailure::InvalidSpec)?;
    let mut slot = server_slot().lock().expect("server mutex poisoned");
    let server = slot
        .as_mut()
        .ok_or(ActiveAndroidLaunchFailure::ServerUnavailable)?;
    if !spec.verify(&server.launch_signing_key) {
        return Err(ActiveAndroidLaunchFailure::Integrity);
    }
    if spec.context.foreground.kind == launcher::LaunchForegroundKind::ArtemisGame {
        return Err(ActiveAndroidLaunchFailure::InvalidSpec);
    }
    let retroarch_authority = spec
        .retroarch_control_token(&server.launch_signing_key)
        .zip(spec.retroarch_control_port(&server.launch_signing_key))
        .map(|(token, port)| {
            launcher::retroarch_control::RetroarchControlAuthority::retain_from_verified_launch(
                &spec, &token, port,
            )
        })
        .transpose()
        .map(|authority| authority.map(Arc::new))
        .map_err(|_| ActiveAndroidLaunchFailure::InvalidSpec)?;
    server
        .local_launch_reservations
        .lock()
        .expect("local launch reservations poisoned")
        .publish(&spec.launch_id)
        .map_err(active_launch_reservation_failure)?;
    let launch = publish_verified_android_launch(
        server,
        launcher::AndroidActiveLaunch::from_context(spec.launch_id, spec.context),
    );
    *server
        .retroarch_control_authority
        .lock()
        .expect("RetroArch control authority mutex poisoned") = retroarch_authority;
    Ok(launch)
}

/** Publish a signed Moonlight context. Rust signs an Artemis marker; Java is
 * the authority for its own application package and Game component names. */
pub fn publish_moonlight_active_launch(
    spec_json: &str,
    application_package: &str,
    game_class_name: &str,
) -> Result<launcher::AndroidActiveLaunch, ActiveAndroidLaunchFailure> {
    if application_package.is_empty() || game_class_name.is_empty() {
        return Err(ActiveAndroidLaunchFailure::InvalidSpec);
    }
    let spec = serde_json::from_str::<launcher::MoonlightLaunchSpec>(spec_json)
        .map_err(|_| ActiveAndroidLaunchFailure::InvalidSpec)?;
    let mut slot = server_slot().lock().expect("server mutex poisoned");
    let server = slot
        .as_mut()
        .ok_or(ActiveAndroidLaunchFailure::ServerUnavailable)?;
    if !spec.verify(&server.launch_signing_key)
        || spec.context.foreground.kind != launcher::LaunchForegroundKind::ArtemisGame
    {
        return Err(ActiveAndroidLaunchFailure::Integrity);
    }
    server
        .moonlight_launch_authority
        .lock()
        .expect("Moonlight launch authority poisoned")
        .publish(&spec)
        .map_err(active_launch_reservation_failure)?;
    let mut context = spec.context;
    context.foreground = launcher::LaunchForegroundRule {
        kind: launcher::LaunchForegroundKind::Component,
        package_name: Some(application_package.to_owned()),
        class_name: Some(game_class_name.to_owned()),
    };
    let launch = publish_verified_android_launch(
        server,
        launcher::AndroidActiveLaunch::from_context(spec.launch_id, context),
    );
    *server
        .retroarch_control_authority
        .lock()
        .expect("RetroArch control authority mutex poisoned") = None;
    Ok(launch)
}

pub fn active_android_launch() -> Option<launcher::AndroidActiveLaunch> {
    server_slot()
        .lock()
        .expect("server mutex poisoned")
        .as_ref()
        .and_then(|server| {
            server
                .active_android_launch
                .lock()
                .expect("active Android launch mutex poisoned")
                .clone()
        })
}

/** Late end evidence for A cannot clear replacement B. */
pub fn clear_active_android_launch(launch_id: &str) -> bool {
    let mut slot = server_slot().lock().expect("server mutex poisoned");
    let Some(server) = slot.as_mut() else {
        return false;
    };
    let mut active = server
        .active_android_launch
        .lock()
        .expect("active Android launch mutex poisoned");
    if active
        .as_ref()
        .is_none_or(|current| current.launch_id != launch_id)
    {
        return false;
    }
    *active = None;
    drop(active);
    *server
        .retroarch_control_authority
        .lock()
        .expect("RetroArch control authority mutex poisoned") = None;
    *server
        .moonlight_executor_state
        .lock()
        .expect("Moonlight executor state mutex poisoned") = None;
    server.platform_instruction_verifier = None;
    true
}

pub fn publish_moonlight_executor_state(state_json: &str) -> bool {
    let Ok(state) = serde_json::from_str::<MoonlightExecutorState>(state_json) else {
        return false;
    };
    if state.executor_id != "android-moonlight"
        || state.generation.len() != 64
        || !state
            .generation
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || !state.is_strict()
    {
        return false;
    }
    let mut slot = server_slot().lock().expect("server mutex poisoned");
    let Some(server) = slot.as_mut() else {
        return false;
    };
    let active = server
        .active_android_launch
        .lock()
        .expect("active Android launch mutex poisoned");
    let current = active.as_ref().is_some_and(|launch| {
        launch.launch_id == state.launch_id
            && launch
                .executor
                .as_ref()
                .is_some_and(|executor| executor.id == "android-moonlight")
    });
    drop(active);
    if !current {
        return false;
    }
    *server
        .moonlight_executor_state
        .lock()
        .expect("Moonlight executor state mutex poisoned") = Some(state);
    true
}

pub fn clear_moonlight_executor_state(launch_id: &str, generation: &str) -> bool {
    let mut slot = server_slot().lock().expect("server mutex poisoned");
    let Some(server) = slot.as_mut() else {
        return false;
    };
    let mut state = server
        .moonlight_executor_state
        .lock()
        .expect("Moonlight executor state mutex poisoned");
    if state
        .as_ref()
        .is_none_or(|current| current.launch_id != launch_id || current.generation != generation)
    {
        return false;
    }
    *state = None;
    true
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizedPlatformInstruction {
    pub launch_id: String,
    pub executor_id: String,
    pub generation: String,
    pub effect: launcher::AndroidMoonlightEffect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<SessionControlValue>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum PlatformInstructionAuthorization {
    Authorized(AuthorizedPlatformInstruction),
    InvalidSpec,
    Integrity,
    Stale,
    Replay,
    NoActiveLaunch,
    ExecutorUnavailable,
    ServerUnavailable,
}

fn platform_instruction_verification_failure(
    failure: launcher::PlatformInstructionVerificationFailure,
) -> PlatformInstructionAuthorization {
    match failure {
        launcher::PlatformInstructionVerificationFailure::Integrity => {
            PlatformInstructionAuthorization::Integrity
        }
        launcher::PlatformInstructionVerificationFailure::StaleSession => {
            PlatformInstructionAuthorization::Stale
        }
        launcher::PlatformInstructionVerificationFailure::Replay => {
            PlatformInstructionAuthorization::Replay
        }
    }
}

/** Verify, consume once, and return only the closed authorized effect/value. */
pub fn authorize_platform_instruction(instruction_json: &str) -> PlatformInstructionAuthorization {
    let Ok(instruction) = serde_json::from_str::<launcher::PlatformInstruction>(instruction_json)
    else {
        return PlatformInstructionAuthorization::InvalidSpec;
    };
    let mut slot = server_slot().lock().expect("server mutex poisoned");
    let Some(server) = slot.as_mut() else {
        return PlatformInstructionAuthorization::ServerUnavailable;
    };
    let Some(verifier) = server.platform_instruction_verifier.as_ref() else {
        return PlatformInstructionAuthorization::NoActiveLaunch;
    };
    if let Err(failure) = verifier.verify_current(&instruction, &server.launch_signing_key) {
        return platform_instruction_verification_failure(failure);
    }

    // Reload policy and rematerialize from the exact live executor before the
    // nonce is consumed. Disabled, changed, revoked, or replaced authority gets
    // no first-use replay oracle and cannot execute.
    let Ok((current, executor, materialized, _)) = materialize_session_controls_snapshot(
        &server.active_android_launch,
        &server.retroarch_control_authority,
        &server.moonlight_executor_state,
        &server.moonlight_config_snapshot,
        &instruction.launch_id,
    ) else {
        return PlatformInstructionAuthorization::ExecutorUnavailable;
    };
    let MaterializedSessionExecutor::Moonlight(executor) = executor else {
        return PlatformInstructionAuthorization::ExecutorUnavailable;
    };
    if instruction.executor_id != executor.executor_id
        || instruction.generation != executor.generation
    {
        return PlatformInstructionAuthorization::Stale;
    }
    let Some((record, control)) = materialized
        .iter()
        .find(|(_, control)| control.id == instruction.action_id)
    else {
        return PlatformInstructionAuthorization::ExecutorUnavailable;
    };
    let effect = match instruction.effect {
        launcher::PlatformEffect::AndroidMoonlight(effect) => effect,
    };
    if record.effect.android_moonlight_effect() != Some(effect)
        || control.dismiss_on_success != instruction.dismiss_on_success
    {
        return PlatformInstructionAuthorization::ExecutorUnavailable;
    }
    let request = SessionControlInvokeRequest {
        launch_id: instruction.launch_id.clone(),
        control_id: instruction.action_id.clone(),
        value: instruction.value.clone(),
    };
    if validate_session_control_invocation(&current.launch_id, &request, control).is_err() {
        return PlatformInstructionAuthorization::ExecutorUnavailable;
    }
    let Some(verifier) = server.platform_instruction_verifier.as_mut() else {
        return PlatformInstructionAuthorization::NoActiveLaunch;
    };
    if let Err(failure) = verifier.consume_nonce(&instruction) {
        return platform_instruction_verification_failure(failure);
    }
    PlatformInstructionAuthorization::Authorized(AuthorizedPlatformInstruction {
        launch_id: instruction.launch_id,
        executor_id: instruction.executor_id,
        generation: instruction.generation,
        effect,
        value: instruction.value,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MoonlightLaunchAuthorization {
    Authorized,
    InvalidSpec,
    Integrity,
    Stale,
    Replay,
    ServerUnavailable,
}

/** Verify and atomically consume a signed Moonlight startup instruction. */
pub fn authorize_moonlight_launch_spec(spec_json: &str) -> MoonlightLaunchAuthorization {
    let Ok(spec) = serde_json::from_str::<launcher::MoonlightLaunchSpec>(spec_json) else {
        return MoonlightLaunchAuthorization::InvalidSpec;
    };
    let Some((authority, config_snapshot, native_platform)) = server_slot()
        .lock()
        .expect("server mutex poisoned")
        .as_ref()
        .map(|server| {
            (
                Arc::clone(&server.moonlight_launch_authority),
                server.moonlight_config_snapshot.clone(),
                server.native_platform,
            )
        })
    else {
        return MoonlightLaunchAuthorization::ServerUnavailable;
    };
    let mut authority = authority
        .lock()
        .expect("Moonlight launch authority poisoned");
    let verification = authorize_moonlight_launch_against_current_policy(
        &mut authority,
        &spec,
        resolve_moonlight_outcome(&config_snapshot.reload(), native_platform),
    );
    match verification {
        Ok(()) => MoonlightLaunchAuthorization::Authorized,
        Err(launcher::MoonlightLaunchVerificationFailure::Integrity) => {
            MoonlightLaunchAuthorization::Integrity
        }
        Err(launcher::MoonlightLaunchVerificationFailure::Stale) => {
            MoonlightLaunchAuthorization::Stale
        }
        Err(launcher::MoonlightLaunchVerificationFailure::Replay) => {
            MoonlightLaunchAuthorization::Replay
        }
    }
}

pub(crate) struct AuthorizedLocalLaunch {
    pub(crate) publication_required: bool,
    pub(crate) retroarch_authority: Option<RetroarchControlAuthority>,
}

/// Atomically authorize one signed local start and return the exact optional
/// launch authority. Fresh starts consume their reservation; a same-live-game
/// resume consumes nothing and must not be republished.
pub(crate) fn authorize_local_launch_spec(spec_json: &str) -> Option<AuthorizedLocalLaunch> {
    let spec = serde_json::from_str::<launcher::LaunchSpec>(spec_json).ok()?;
    let slot = server_slot().lock().expect("server mutex poisoned");
    let server = slot.as_ref()?;
    if !spec.verify(&server.launch_signing_key) {
        return None;
    }
    match spec.disposition {
        launcher::LaunchDisposition::Fresh => {
            server
                .local_launch_reservations
                .lock()
                .expect("local launch reservations poisoned")
                .authorize(&spec.launch_id)
                .ok()?;
            let retroarch_authority = match (
                spec.retroarch_control_token(&server.launch_signing_key),
                spec.retroarch_control_port(&server.launch_signing_key),
            ) {
                (Some(token), Some(port)) => Some(Arc::new(
                    launcher::retroarch_control::RetroarchControlAuthority::retain_from_verified_launch(
                        &spec, &token, port,
                    )
                    .ok()?,
                )),
                (None, None) => None,
                _ => return None,
            };
            Some(AuthorizedLocalLaunch {
                publication_required: true,
                retroarch_authority,
            })
        }
        launcher::LaunchDisposition::Resume => {
            let active = server
                .active_android_launch
                .lock()
                .expect("active Android launch mutex poisoned")
                .clone()?;
            let authority = server
                .retroarch_control_authority
                .lock()
                .expect("RetroArch control authority mutex poisoned")
                .clone()?;
            if active.launch_id != spec.launch_id || !authority.matches_launch(&active, &spec) {
                return None;
            }
            Some(AuthorizedLocalLaunch {
                publication_required: false,
                retroarch_authority: Some(authority),
            })
        }
    }
}

/// Compatibility test helper around the atomic authorization seam.
pub fn verify_local_launch_spec(spec_json: &str) -> bool {
    authorize_local_launch_spec(spec_json).is_some_and(|authorized| {
        let _publication_required = authorized.publication_required;
        let _retroarch_authority = authorized.retroarch_authority;
        true
    })
}

pub mod host;
pub mod launcher;
pub mod plugin;
pub mod plugin_policy;
pub mod script;
pub mod upstream;
pub mod upstream_native;
pub mod upstreams;

#[cfg(target_os = "android")]
mod android;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{header, Request, StatusCode},
    };
    use tower::ServiceExt;

    const WL4_PLUGIN_LIBRARY: &str =
        include_str!("../../../docs/research/retroarch-plugin-route/library-wl4.yaml");
    const CHECKPOINT_ANDROID_CONFIG: &str =
        include_str!("../../../docs/research/android-app-plugin-schema-checkpoint/config.yaml");
    const CHECKPOINT_ANDROID_LIBRARY: &str =
        include_str!("../../../docs/research/android-app-plugin-schema-checkpoint/library.yaml");
    const PNG_1X1: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 4,
        0, 0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 156, 99, 250, 207, 0, 0, 2, 7,
        1, 2, 154, 28, 49, 113, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];

    /// The embedded server is a process singleton, so the tests that start it
    /// must not overlap. Poisoning is irrelevant here: the guard only orders
    /// them, so a panicking test still hands the next one a usable lock.
    static EMBEDDED_SERVER_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn embedded_server_guard() -> std::sync::MutexGuard<'static, ()> {
        EMBEDDED_SERVER_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write_wl4_plugin_config(root: &Path) {
        std::fs::write(root.join("config.yaml"), "{}\n").unwrap();
        std::fs::write(root.join("library.yaml"), WL4_PLUGIN_LIBRARY).unwrap();
    }

    fn write_checkpoint_android_config(root: &Path) {
        std::fs::write(root.join("config.yaml"), CHECKPOINT_ANDROID_CONFIG).unwrap();
        std::fs::write(root.join("library.yaml"), CHECKPOINT_ANDROID_LIBRARY).unwrap();
    }

    #[test]
    fn retroarch_probe_failures_keep_unavailable_reason_and_safe_categories() {
        use launcher::retroarch_control::RetroarchControlError;

        for (error, category) in [
            (RetroarchControlError::InvalidAuthority, "invalid-authority"),
            (RetroarchControlError::Unavailable, "unavailable"),
            (RetroarchControlError::Timeout, "timeout"),
            (RetroarchControlError::WrongSource, "wrong-source"),
            (RetroarchControlError::WrongResponse, "wrong-response"),
        ] {
            let failure = retroarch_probe_failure(error);
            assert_eq!(failure.reason, SessionControlFailureReason::Unavailable);
            assert_eq!(
                failure.message,
                format!("RetroArch controls are unavailable (probe: {category}).")
            );
            assert!(!failure.message.contains("token"));
            assert!(!failure.message.contains("nonce"));
            assert!(!failure.message.contains("capability"));
        }
        let mismatch = retroarch_probe_unavailable("identity-mismatch");
        assert_eq!(mismatch.reason, SessionControlFailureReason::Unavailable);
        assert_eq!(
            mismatch.message,
            "RetroArch controls are unavailable (probe: identity-mismatch)."
        );
    }

    #[tokio::test]
    async fn generic_local_launch_materializes_overlay_owned_resume_only() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("config.yaml"), "{}\n").unwrap();
        std::fs::write(root.path().join("library.yaml"), "{}\n").unwrap();
        let active = launcher::AndroidActiveLaunch {
            launch_id: "generic-launch".into(),
            game_id: Some("tmnt".into()),
            title: Some("TMNT".into()),
            content_crc32: None,
            contributors: vec![launcher::LaunchRouteContributor {
                kind: launcher::LaunchContributorKind::Launcher,
                id: "@korri:android-app/android-app".into(),
            }],
            executor: None,
            foreground: launcher::LaunchForegroundRule {
                kind: launcher::LaunchForegroundKind::Package,
                package_name: Some("org.example.game".into()),
                class_name: None,
            },
        };
        let active_slot = Arc::new(Mutex::new(Some(active)));
        let authority_slot: RetroarchControlSlot = Arc::new(Mutex::new(None));
        let moonlight_slot = Arc::new(Mutex::new(None));
        let config = config::snapshot::ConfigSnapshotCoordinator::new(root.path());

        let (_, executor, materialized, groups, telemetry) = materialize_session_controls(
            &active_slot,
            &authority_slot,
            &moonlight_slot,
            &config,
            "generic-launch",
        )
        .await
        .unwrap();

        assert!(matches!(executor, MaterializedSessionExecutor::OverlayOnly));
        assert!(materialized.is_empty());
        assert!(groups.is_empty());
        assert!(telemetry.is_none());
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

    #[test]
    fn malformed_dynamic_ranges_cannot_materialize_moonlight_controls() {
        use launcher::AndroidMoonlightEffect as Effect;
        let root = tempfile::tempdir().unwrap();
        let config = config::snapshot::ConfigSnapshotCoordinator::new(root.path());
        let active = launcher::AndroidActiveLaunch {
            launch_id: "launch".into(),
            game_id: None,
            title: Some("Stream".into()),
            content_crc32: None,
            contributors: vec![launcher::LaunchRouteContributor {
                kind: launcher::LaunchContributorKind::Transport,
                id: "@korri:moonlight/moonlight".into(),
            }],
            executor: Some(launcher::LaunchExecutor {
                id: "android-moonlight".into(),
                available: true,
            }),
            foreground: launcher::LaunchForegroundRule {
                kind: launcher::LaunchForegroundKind::ArtemisGame,
                package_name: None,
                class_name: None,
            },
        };
        let active_slot = Arc::new(Mutex::new(Some(active)));
        let authority_slot: RetroarchControlSlot = Arc::new(Mutex::new(None));

        let mut states = Vec::new();
        let mut odd_width = moonlight_executor_state("launch");
        odd_width
            .effects
            .iter_mut()
            .find(|entry| entry.effect == Effect::SetStreamWidth)
            .unwrap()
            .value = Some(SessionControlValue::Range(1919.0));
        states.push(odd_width);

        let mut fractional_min = moonlight_executor_state("launch");
        fractional_min
            .effects
            .iter_mut()
            .find(|entry| entry.effect == Effect::SetStreamBitrateKbps)
            .unwrap()
            .range
            .as_mut()
            .unwrap()
            .min = 500.5;
        states.push(fractional_min);

        let mut fractional_max = moonlight_executor_state("launch");
        fractional_max
            .effects
            .iter_mut()
            .find(|entry| entry.effect == Effect::SetStreamFps)
            .unwrap()
            .range
            .as_mut()
            .unwrap()
            .max = 120.5;
        states.push(fractional_max);

        let mut fractional_value = moonlight_executor_state("launch");
        fractional_value
            .effects
            .iter_mut()
            .find(|entry| entry.effect == Effect::SetStreamWidth)
            .unwrap()
            .value = Some(SessionControlValue::Range(1918.5));
        states.push(fractional_value);

        for state in states {
            let moonlight_slot = Arc::new(Mutex::new(Some(state)));
            let failure = materialize_session_controls_snapshot(
                &active_slot,
                &authority_slot,
                &moonlight_slot,
                &config,
                "launch",
            )
            .err()
            .expect("malformed executor state must not materialize");
            assert_eq!(failure.reason, SessionControlFailureReason::Unavailable);
        }
    }

    #[test]
    fn native_moonlight_authority_rejects_a_changed_plugin_owned_app_without_consuming() {
        let mut authority = launcher::MoonlightLaunchAuthority::new(b"test signing key".to_vec());
        let spec = authority.prepare(
            "@korri:moonlight/moonlight",
            MoonlightImplementation::Artemis,
            "Korri Stream",
            "host-uuid",
            7,
            None,
            None,
        );
        let changed = MoonlightResolveOutcome::Available(ResolvedMoonlight {
            transport_id: "@korri:moonlight/moonlight".into(),
            implementation: MoonlightImplementation::Artemis,
            sunshine_app: "Renamed Stream App".into(),
        });
        assert_eq!(
            authorize_moonlight_launch_against_current_policy(&mut authority, &spec, changed),
            Err(launcher::MoonlightLaunchVerificationFailure::Stale)
        );

        let original = MoonlightResolveOutcome::Available(ResolvedMoonlight {
            transport_id: "@korri:moonlight/moonlight".into(),
            implementation: MoonlightImplementation::Artemis,
            sunshine_app: "Korri Stream".into(),
        });
        assert!(
            authorize_moonlight_launch_against_current_policy(&mut authority, &spec, original)
                .is_ok()
        );
    }

    #[tokio::test]
    async fn embedded_android_test_router_resolves_artemis_and_honors_current_user_policy() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("config.yaml"), "{}\n").unwrap();
        std::fs::write(root.path().join("library.yaml"), "{}\n").unwrap();
        let app = android_router_with_capability_and_local_root(
            "right-token",
            "https://portal.example",
            root.path(),
        );
        let request = || {
            Request::builder()
                .method("POST")
                .uri("/rpc")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, "Bearer right-token")
                .body(Body::from(
                    r#"{"_tag":"app.moonlight.resolve","payload":{}}"#,
                ))
                .unwrap()
        };
        let outcome =
            |body: axum::body::Bytes| serde_json::from_slice::<serde_json::Value>(&body).unwrap();

        let response = app.clone().oneshot(request()).await.unwrap();
        let body = outcome(to_bytes(response.into_body(), usize::MAX).await.unwrap());
        assert_eq!(body["outcome"]["_tag"], "Available");
        assert_eq!(body["outcome"]["payload"]["implementation"], "artemis");

        for payload in [
            r#"{"hostUuid":"","appId":7}"#,
            r#"{"hostUuid":"host-uuid","appId":0}"#,
            r#"{"hostUuid":"host-uuid","appId":2147483648}"#,
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/rpc")
                        .header(header::CONTENT_TYPE, "application/json")
                        .header(header::AUTHORIZATION, "Bearer right-token")
                        .body(Body::from(format!(
                            r#"{{"_tag":"app.moonlight.launch.prepare","payload":{payload}}}"#
                        )))
                        .unwrap(),
                )
                .await
                .unwrap();
            let body = outcome(to_bytes(response.into_body(), usize::MAX).await.unwrap());
            assert_eq!(body["outcome"]["_tag"], "Err");
            assert_eq!(
                body["outcome"]["payload"]["code"],
                "InvalidMoonlightLaunchTarget"
            );
        }

        std::fs::write(
            root.path().join("config.yaml"),
            "host:\n  plugin:\n    '@korri:moonlight': false\n",
        )
        .unwrap();
        let response = app.oneshot(request()).await.unwrap();
        let body = outcome(to_bytes(response.into_body(), usize::MAX).await.unwrap());
        assert_eq!(body["outcome"]["_tag"], "Unavailable");
    }

    #[test]
    fn moonlight_resolution_withholds_retained_snapshot_while_unauthorized() {
        let state = config::snapshot::ConfigSnapshotState {
            snapshot: Arc::new(config::ConfigSnapshot::default()),
            generation: 1,
            diagnostic: Some(config::snapshot::SnapshotDiagnostic {
                code: config::snapshot::SnapshotDiagnosticCode::LocalConfigUnauthorized,
                message: "configured storage denial".into(),
            }),
            authorization: config::snapshot::SnapshotAuthorization::Unauthorized,
        };

        let outcome = resolve_moonlight_outcome(&state, NativePlatform::EmbeddedAndroid);
        let MoonlightResolveOutcome::Unavailable(failure) = outcome else {
            panic!("retained Moonlight declaration must be withheld");
        };
        assert_eq!(failure.code, "LocalConfigUnauthorized");
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
            Arc::new(Mutex::new(None)),
            NativePlatform::Standalone,
            config::snapshot::ConfigSnapshotCoordinator::new(readable),
            grants,
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
        assert_eq!(games["outcome"]["payload"]["games"][0]["id"], "game");
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

        let edited = std::fs::read_to_string(readable.path().join("library.yaml"))
            .unwrap()
            .replace("title: game", "title: Player Edited");
        std::fs::write(readable.path().join("library.yaml"), edited).unwrap();
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
        assert_eq!(games["outcome"]["payload"]["games"][0]["id"], "good");
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
            format!(r#"[{{"label":"zao","kind":"native","baseUrl":"http://{address}"}}]"#),
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
            brain,
            r#"{"_tag":"app.session.prepare","payload":{"gameId":"neverball","host":"zao"}}"#,
            Some("right-token"),
        )
        .await;
        assert_eq!(prepared["outcome"]["_tag"], "Ok");
    }

    #[tokio::test]
    async fn host_router_rejects_brain_and_unmanaged_session_operations() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(&config, "label = \"zao\"\n").unwrap();
        let app = host_router(&config);

        for (request, code) in [
            (
                r#"{"_tag":"app.local-games.launch","payload":{"gameId":"wl4"}}"#,
                "OperationUnsupported",
            ),
            (
                r#"{"_tag":"app.session.status","payload":{}}"#,
                "SessionStatusUnsupported",
            ),
            (
                r#"{"_tag":"app.session.stop","payload":{}}"#,
                "SessionStopUnsupported",
            ),
        ] {
            let body = rpc_body(app.clone(), request).await;
            assert_eq!(body["outcome"]["_tag"], "Err");
            assert_eq!(body["outcome"]["payload"]["code"], code);
        }
    }

    #[tokio::test]
    async fn private_host_control_is_exact_while_lan_control_stays_rejected() {
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
            lan_status["outcome"]["payload"]["code"],
            "SessionStatusUnsupported"
        );
        assert!(lan_status.to_string().find(&first).is_none());
        let status = unix_rpc_body(
            socket_path.clone(),
            r#"{"_tag":"app.session.status","payload":{}}"#,
        )
        .await;
        assert_eq!(status["outcome"]["payload"]["active"]["launchId"], first);
        assert_eq!(status["outcome"]["payload"]["active"]["phase"], "running");

        let lan_stop = tcp_rpc_body(
            tcp_address,
            &serde_json::json!({
                "_tag": "app.session.stop",
                "payload": { "expectedLaunchId": first }
            })
            .to_string(),
        )
        .await;
        assert_eq!(
            lan_stop["outcome"]["payload"]["code"],
            "SessionStopUnsupported"
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
        let app = router_with_capability_and_local_root(
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
        std::fs::write(root.path().join("config.yaml"), "host:\n  title: old\n").unwrap();
        std::fs::write(root.path().join("library.yaml"), "{}\n").unwrap();
        let app = router_with_capability_and_local_root(
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
        assert!(std::fs::read_to_string(root.path().join("config.yaml"))
            .unwrap()
            .contains("title: usu"));

        let updated_revision = updated["outcome"]["payload"]["revision"].as_str().unwrap();
        std::fs::write(root.path().join("config.yaml"), "host:\n  title: outside\n").unwrap();
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
        assert!(std::fs::read_to_string(root.path().join("config.yaml"))
            .unwrap()
            .contains("title: outside"));
    }

    #[tokio::test]
    async fn local_launch_rpc_uses_the_configured_root_and_returns_a_spec() {
        let root = tempfile::tempdir().unwrap();
        write_wl4_plugin_config(root.path());
        std::fs::create_dir_all(root.path().join("roms")).unwrap();
        std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
        let app = router_with_capability_and_local_root(
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
                        r#"{"_tag":"app.local-games.launch","payload":{"gameId":"wl4"}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8_lossy(&body);
        assert!(body.contains("app.local-games.launch"));
        assert!(body.contains("com.korri.retroarch"));
        assert!(root.path().join("retroarch.cfg").is_file());
    }

    #[tokio::test]
    async fn embedded_local_launch_rpc_defers_the_config_and_save_tree() {
        let root = tempfile::tempdir().unwrap();
        write_wl4_plugin_config(root.path());
        std::fs::create_dir_all(root.path().join("roms")).unwrap();
        std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
        let private = tempfile::tempdir().unwrap();
        let app = router_with_capability_local_root_and_provision(
            "right-token",
            "https://portal.example",
            root.path(),
            private.path(),
            launcher::FileProvisionMode::Deferred,
            b"test signing key".to_vec(),
            Arc::new(Mutex::new(launcher::LaunchPublicationReservations::new())),
            Arc::new(Mutex::new(launcher::MoonlightLaunchAuthority::new(
                b"test signing key".to_vec(),
            ))),
            Arc::new(Mutex::new(None)),
            Arc::new(Mutex::new(None)),
            Arc::new(Mutex::new(None)),
            NativePlatform::EmbeddedAndroid,
            config::snapshot::ConfigSnapshotCoordinator::new(root.path()),
        );
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/rpc")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::AUTHORIZATION, "Bearer right-token")
                    .body(Body::from(
                        r#"{"_tag":"app.local-games.launch","payload":{"gameId":"wl4"}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let spec = &body["outcome"]["payload"];
        assert_eq!(spec["launchId"].as_str().unwrap().len(), 32);
        assert_eq!(spec["files"].as_array().unwrap().len(), 1);
        assert_eq!(spec["directories"].as_array().unwrap().len(), 4);
        assert!(!spec["integrity"].as_str().unwrap().is_empty());
        assert!(spec["extras"].get("KORRI_CONTROL_TOKEN").is_none());
        assert!(!root.path().join("retroarch.cfg").exists());
        assert!(!root.path().join("saves").exists());
    }

    #[tokio::test]
    async fn running_server_verifies_only_its_current_launch_specs_and_capability() {
        let _serialized = embedded_server_guard();
        struct StopServer;
        impl Drop for StopServer {
            fn drop(&mut self) {
                let _ = stop_local_server();
            }
        }

        let root = tempfile::tempdir().unwrap();
        write_wl4_plugin_config(root.path());
        std::fs::create_dir_all(root.path().join("roms")).unwrap();
        std::fs::write(root.path().join("roms/wl4.gba"), b"123456789").unwrap();
        let private = tempfile::tempdir().unwrap();
        let port = start_local_server(
            "https://portal.example",
            root.path().to_str().expect("UTF-8 temp path"),
            private.path().to_str().expect("UTF-8 temp path"),
        )
        .unwrap();
        let _stop = StopServer;
        let capability = local_server_capability().unwrap();
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{port}/rpc");
        let moonlight = client
            .post(&url)
            .bearer_auth(&capability)
            .json(&serde_json::json!({
                "_tag": "app.moonlight.resolve",
                "payload": {}
            }))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(moonlight["outcome"]["_tag"], "Unavailable");

        stop_local_server().unwrap();
        let port = start_local_server_for_platform(
            "https://portal.example",
            root.path().to_str().expect("UTF-8 temp path"),
            private.path().to_str().expect("UTF-8 temp path"),
            NativePlatform::EmbeddedAndroid,
        )
        .unwrap();
        let capability = local_server_capability().unwrap();
        let url = format!("http://127.0.0.1:{port}/rpc");

        let mut response = None;
        for _ in 0..20 {
            match client
                .post(&url)
                .bearer_auth(&capability)
                .json(&serde_json::json!({
                    "_tag": "app.local-games.launch",
                    "payload": { "gameId": "wl4" }
                }))
                .send()
                .await
            {
                Ok(value) => {
                    response = Some(value.json::<serde_json::Value>().await.unwrap());
                    break;
                }
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(10)),
            }
        }
        let response = response.expect("embedded server response");
        let mut spec = response["outcome"]["payload"].clone();
        let spec_json = serde_json::to_string(&spec).unwrap();
        assert!(verify_local_launch_spec(&spec_json));
        let local = publish_local_active_launch(&spec_json).unwrap();
        assert_eq!(
            publish_local_active_launch(&spec_json),
            Err(ActiveAndroidLaunchFailure::AlreadyPublished)
        );
        assert_eq!(local.game_id.as_deref(), Some("wl4"));
        assert_eq!(local.title.as_deref(), Some("Wario Land 4"));
        assert_eq!(
            local.contributors,
            vec![
                launcher::LaunchRouteContributor {
                    kind: launcher::LaunchContributorKind::Launcher,
                    id: "@korri:retroarch/retroarch".into(),
                },
                launcher::LaunchRouteContributor {
                    kind: launcher::LaunchContributorKind::Runtime,
                    id: "@korri:mgba/mgba".into(),
                },
            ]
        );
        assert!(!clear_active_android_launch("late-older-launch"));
        assert_eq!(active_android_launch().unwrap().launch_id, local.launch_id);
        let controls = client
            .post(&url)
            .bearer_auth(&capability)
            .json(&serde_json::json!({
                "_tag": "app.session.controls",
                "payload": { "launchId": local.launch_id.clone() }
            }))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(controls["outcome"]["_tag"], "Err");
        assert_eq!(controls["outcome"]["payload"]["reason"], "Unavailable");

        assert!(response["outcome"]["payload"]["extras"]
            .get("KORRI_CONTROL_TOKEN")
            .is_none());
        let control_token = {
            let prepared: launcher::LaunchSpec = serde_json::from_str(&spec_json).unwrap();
            let slot = server_slot().lock().unwrap();
            prepared
                .retroarch_control_token(&slot.as_ref().unwrap().launch_signing_key)
                .unwrap()
        };
        let control_port = {
            let prepared: launcher::LaunchSpec = serde_json::from_str(&spec_json).unwrap();
            let slot = server_slot().lock().unwrap();
            prepared
                .retroarch_control_port(&slot.as_ref().unwrap().launch_signing_key)
                .unwrap()
        };
        let control_server = std::net::UdpSocket::bind(("127.0.0.1", control_port)).unwrap();
        control_server
            .set_read_timeout(Some(std::time::Duration::from_millis(500)))
            .unwrap();
        let control_token: [u8; 64] = control_token.as_bytes().try_into().unwrap();
        let delay_next_status = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let responder_delay = Arc::clone(&delay_next_status);
        let status_mode = Arc::new(std::sync::atomic::AtomicU8::new(0));
        let responder_status_mode = Arc::clone(&status_mode);
        let responder = std::thread::spawn(move || loop {
            let mut request = [0_u8; 66];
            let Ok((length, source)) = control_server.recv_from(&mut request) else {
                break;
            };
            assert_eq!(length, request.len());
            assert_eq!(request[0], 1);
            let mut nonce = [0_u8; 32];
            nonce.copy_from_slice(&request[1..33]);
            let (command, result) = match request[33] {
                1 => {
                    if responder_delay.swap(false, std::sync::atomic::Ordering::SeqCst) {
                        std::thread::sleep(std::time::Duration::from_millis(200));
                    }
                    match responder_status_mode.swap(0, std::sync::atomic::Ordering::SeqCst) {
                        1 => (
                            launcher::retroarch_control::RetroarchControlCommand::GetStatus,
                            b"GET_STATUS CONTENTLESS menu=0,selection=0\n".as_slice(),
                        ),
                        2 => continue,
                        _ => (
                            launcher::retroarch_control::RetroarchControlCommand::GetStatus,
                            b"GET_STATUS PLAYING mGBA,wl4.gba,crc32=cbf43926,menu=0,selection=0\n"
                                .as_slice(),
                        ),
                    }
                }
                2 => (
                    launcher::retroarch_control::RetroarchControlCommand::ShowMenu,
                    b"SHOW_MENU OK".as_slice(),
                ),
                3 => (
                    launcher::retroarch_control::RetroarchControlCommand::Quit,
                    b"QUIT OK".as_slice(),
                ),
                _ => panic!("unexpected RetroArch command tag"),
            };
            let reply = launcher::retroarch_control::authenticated_response(
                &control_token,
                nonce,
                command,
                result,
            );
            control_server.send_to(&reply, source).unwrap();
        });

        delay_next_status.store(true, std::sync::atomic::Ordering::SeqCst);
        let controls_client = client.clone();
        let controls_url = url.clone();
        let controls_capability = capability.clone();
        let controls_launch_id = local.launch_id.clone();
        let delayed_controls = tokio::spawn(async move {
            controls_client
                .post(&controls_url)
                .bearer_auth(&controls_capability)
                .json(&serde_json::json!({
                    "_tag": "app.session.controls",
                    "payload": { "launchId": controls_launch_id }
                }))
                .send()
                .await
                .unwrap()
                .json::<serde_json::Value>()
                .await
                .unwrap()
        });
        tokio::task::yield_now().await;
        std::thread::sleep(std::time::Duration::from_millis(20));
        let health_started = std::time::Instant::now();
        let health = client
            .post(&url)
            .bearer_auth(&capability)
            .json(&serde_json::json!({ "_tag": "system.health", "payload": {} }))
            .send()
            .await
            .unwrap();
        assert!(health.status().is_success());
        assert!(health_started.elapsed() < std::time::Duration::from_millis(150));
        assert_eq!(delayed_controls.await.unwrap()["outcome"]["_tag"], "Ok");

        let controls = client
            .post(&url)
            .bearer_auth(&capability)
            .json(&serde_json::json!({
                "_tag": "app.session.controls",
                "payload": { "launchId": local.launch_id.clone() }
            }))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(controls["outcome"]["_tag"], "Ok");
        assert_eq!(
            controls["outcome"]["payload"]["retroarchTelemetry"],
            serde_json::json!({
                "contentBasename": "wl4.gba",
                "crc32": "cbf43926",
                "menuAlive": false,
                "menuSelection": 0
            })
        );
        assert_eq!(
            controls["outcome"]["payload"]["groups"][0]["id"],
            "@korri:retroarch"
        );
        assert_eq!(
            controls["outcome"]["payload"]["groups"][0]["controls"]
                .as_array()
                .unwrap()
                .iter()
                .map(|control| (
                    control["id"].as_str().unwrap(),
                    control["label"].as_str().unwrap(),
                    control["destructive"].as_bool().unwrap(),
                ))
                .collect::<Vec<_>>(),
            [
                ("@korri:retroarch/open-menu", "Open RetroArch menu", false),
                ("@korri:retroarch/quit", "Quit game", true),
            ]
        );

        let repeated = client
            .post(&url)
            .bearer_auth(&capability)
            .json(&serde_json::json!({
                "_tag": "app.local-games.launch",
                "payload": { "gameId": "wl4" }
            }))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(repeated["outcome"]["_tag"], "Ok");
        assert_eq!(repeated["outcome"]["payload"]["launchId"], local.launch_id);
        assert_eq!(repeated["outcome"]["payload"]["disposition"], "resume");
        // Acceptance discovers a published local launch through this repeated
        // request. Resume may change only disposition/integrity: route,
        // package, core, content, and provisioning must remain exact.
        for field in [
            "launchId",
            "launcherId",
            "context",
            "component",
            "extras",
            "directories",
            "files",
        ] {
            assert_eq!(
                repeated["outcome"]["payload"][field], spec[field],
                "resume changed launch field {field}"
            );
        }
        assert_eq!(
            repeated["outcome"]["payload"]["component"]["packageName"],
            "com.korri.retroarch"
        );
        assert_eq!(
            repeated["outcome"]["payload"]["extras"]["ROM"],
            root.path().join("roms/wl4.gba").display().to_string()
        );
        assert_eq!(
            repeated["outcome"]["payload"]["extras"]["LIBRETRO"],
            "/data/data/com.korri.retroarch/cores/mgba_libretro_android.so"
        );
        assert_eq!(repeated["outcome"]["payload"]["context"]["gameId"], "wl4");
        assert_eq!(
            repeated["outcome"]["payload"]["context"]["contentCrc32"],
            "cbf43926"
        );
        assert!(repeated["outcome"]["payload"]["files"][0]["content"]
            .as_str()
            .unwrap()
            .contains(&format!("network_cmd_port = \"{control_port}\"")));
        let repeated_json = serde_json::to_string(&repeated["outcome"]["payload"]).unwrap();
        let authorized = authorize_local_launch_spec(&repeated_json).unwrap();
        assert!(!authorized.publication_required);
        assert!(authorized.retroarch_authority.is_some());
        assert_eq!(active_android_launch().unwrap().launch_id, local.launch_id);

        for mode in [1, 2] {
            status_mode.store(mode, std::sync::atomic::Ordering::SeqCst);
            let conflict = client
                .post(&url)
                .bearer_auth(&capability)
                .json(&serde_json::json!({
                    "_tag": "app.local-games.launch",
                    "payload": { "gameId": "wl4" }
                }))
                .send()
                .await
                .unwrap()
                .json::<serde_json::Value>()
                .await
                .unwrap();
            assert_eq!(conflict["outcome"]["_tag"], "Err");
            assert_eq!(
                conflict["outcome"]["payload"]["code"],
                "ActiveSessionConflict"
            );
            assert_eq!(active_android_launch().unwrap().launch_id, local.launch_id);
        }

        std::fs::write(root.path().join("roms/wl4.gba"), b"replaced bytes").unwrap();
        let replaced_content = client
            .post(&url)
            .bearer_auth(&capability)
            .json(&serde_json::json!({
                "_tag": "app.local-games.launch",
                "payload": { "gameId": "wl4" }
            }))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(replaced_content["outcome"]["_tag"], "Err");
        assert_eq!(
            replaced_content["outcome"]["payload"]["code"],
            "ActiveSessionConflict"
        );
        assert_eq!(active_android_launch().unwrap().launch_id, local.launch_id);
        std::fs::write(root.path().join("roms/wl4.gba"), b"123456789").unwrap();

        std::fs::write(root.path().join("roms/other.gba"), b"123456789").unwrap();
        std::fs::write(
            root.path().join("library.yaml"),
            WL4_PLUGIN_LIBRARY.replace("wl4.gba", "other.gba"),
        )
        .unwrap();
        let different_route = client
            .post(&url)
            .bearer_auth(&capability)
            .json(&serde_json::json!({
                "_tag": "app.local-games.launch",
                "payload": { "gameId": "wl4" }
            }))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(different_route["outcome"]["_tag"], "Err");
        assert_eq!(
            different_route["outcome"]["payload"]["code"],
            "ActiveSessionConflict"
        );
        assert_eq!(active_android_launch().unwrap().launch_id, local.launch_id);
        write_wl4_plugin_config(root.path());

        let different_game = client
            .post(&url)
            .bearer_auth(&capability)
            .json(&serde_json::json!({
                "_tag": "app.local-games.launch",
                "payload": { "gameId": "another-game" }
            }))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(different_game["outcome"]["_tag"], "Err");
        assert_eq!(
            different_game["outcome"]["payload"]["code"],
            "ActiveSessionConflict"
        );
        assert_eq!(active_android_launch().unwrap().launch_id, local.launch_id);

        let original_authority = {
            let slot = server_slot().lock().unwrap();
            let authority = slot
                .as_ref()
                .unwrap()
                .retroarch_control_authority
                .lock()
                .unwrap()
                .clone()
                .unwrap();
            authority
        };
        let open_menu = client
            .post(&url)
            .bearer_auth(&capability)
            .json(&serde_json::json!({
                "_tag": "app.session.control.invoke",
                "payload": {
                    "launchId": local.launch_id.clone(),
                    "controlId": "@korri:retroarch/open-menu",
                }
            }))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(open_menu["outcome"]["_tag"], "Ok");
        assert_eq!(open_menu["outcome"]["payload"]["_tag"], "Completed");
        assert_eq!(
            open_menu["outcome"]["payload"]["payload"]["launchId"],
            local.launch_id
        );
        assert_eq!(active_android_launch().as_ref(), Some(&local));
        {
            let slot = server_slot().lock().unwrap();
            let authority = slot
                .as_ref()
                .unwrap()
                .retroarch_control_authority
                .lock()
                .unwrap()
                .clone()
                .unwrap();
            assert!(Arc::ptr_eq(&authority, &original_authority));
        }
        std::fs::write(
            root.path().join("config.yaml"),
            "host:\n  plugin:\n    \"@korri:retroarch\": false\n",
        )
        .unwrap();
        let disabled = client
            .post(&url)
            .bearer_auth(&capability)
            .json(&serde_json::json!({
                "_tag": "app.session.controls",
                "payload": { "launchId": local.launch_id.clone() }
            }))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(disabled["outcome"]["_tag"], "Ok");
        assert!(disabled["outcome"]["payload"]["groups"]
            .as_array()
            .unwrap()
            .is_empty());

        write_wl4_plugin_config(root.path());
        let quit = client
            .post(&url)
            .bearer_auth(&capability)
            .json(&serde_json::json!({
                "_tag": "app.session.control.invoke",
                "payload": {
                    "launchId": local.launch_id.clone(),
                    "controlId": "@korri:retroarch/quit",
                }
            }))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(quit["outcome"]["_tag"], "Ok");
        assert_eq!(quit["outcome"]["payload"]["_tag"], "Completed");
        assert_eq!(
            quit["outcome"]["payload"]["payload"]["launchId"],
            local.launch_id
        );
        assert!(active_android_launch().is_none());
        {
            let slot = server_slot().lock().unwrap();
            assert!(slot
                .as_ref()
                .unwrap()
                .retroarch_control_authority
                .lock()
                .unwrap()
                .is_none());
        }

        for tag in ["app.session.controls", "app.session.control.invoke"] {
            let payload = if tag == "app.session.controls" {
                serde_json::json!({ "launchId": local.launch_id.clone() })
            } else {
                serde_json::json!({
                    "launchId": local.launch_id.clone(),
                    "controlId": "@korri:retroarch/quit",
                })
            };
            let stale = client
                .post(&url)
                .bearer_auth(&capability)
                .json(&serde_json::json!({ "_tag": tag, "payload": payload }))
                .send()
                .await
                .unwrap()
                .json::<serde_json::Value>()
                .await
                .unwrap();
            assert_eq!(stale["outcome"]["_tag"], "Err");
            assert_eq!(stale["outcome"]["payload"]["reason"], "StaleSession");
        }

        responder.join().unwrap();
        assert_eq!(
            publish_local_active_launch(&spec_json),
            Err(ActiveAndroidLaunchFailure::AlreadyPublished)
        );
        let fresh_after_end = client
            .post(&url)
            .bearer_auth(&capability)
            .json(&serde_json::json!({
                "_tag": "app.local-games.launch",
                "payload": { "gameId": "wl4" }
            }))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(
            fresh_after_end["outcome"]["payload"]["disposition"],
            "fresh"
        );
        assert_ne!(
            fresh_after_end["outcome"]["payload"]["launchId"],
            local.launch_id
        );
        let fresh_json = serde_json::to_string(&fresh_after_end["outcome"]["payload"]).unwrap();
        assert!(authorize_local_launch_spec(&fresh_json)
            .is_some_and(|authorized| authorized.publication_required));
        let replacement = publish_local_active_launch(&fresh_json).unwrap();
        let replacement_authority = {
            let slot = server_slot().lock().unwrap();
            let authority = slot
                .as_ref()
                .unwrap()
                .retroarch_control_authority
                .lock()
                .unwrap()
                .clone()
                .unwrap();
            authority
        };
        // The exact retirement step of a late old QUIT ACK must not clear the
        // fresh replacement or its distinct authority.
        let (active_slot, authority_slot) = {
            let slot = server_slot().lock().unwrap();
            let server = slot.as_ref().unwrap();
            (
                Arc::clone(&server.active_android_launch),
                Arc::clone(&server.retroarch_control_authority),
            )
        };
        assert!(!retire_exact_retroarch_launch(
            &active_slot,
            &authority_slot,
            &local,
            &original_authority,
        ));
        assert_eq!(active_android_launch().as_ref(), Some(&replacement));
        {
            let slot = server_slot().lock().unwrap();
            let authority = slot
                .as_ref()
                .unwrap()
                .retroarch_control_authority
                .lock()
                .unwrap()
                .clone()
                .unwrap();
            assert!(Arc::ptr_eq(&authority, &replacement_authority));
        }
        assert!(clear_active_android_launch(&replacement.launch_id));

        spec["files"][0]["content"] = serde_json::Value::String("tampered".into());
        assert!(!verify_local_launch_spec(
            &serde_json::to_string(&spec).unwrap()
        ));

        let prepare = || {
            client
                .post(&url)
                .bearer_auth(&capability)
                .json(&serde_json::json!({
                    "_tag": "app.moonlight.launch.prepare",
                    "payload": { "hostUuid": "host-uuid", "appId": 7 }
                }))
                .send()
        };
        let first = prepare()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap()["outcome"]["payload"]
            .clone();
        let first_json = serde_json::to_string(&first).unwrap();
        assert_eq!(
            authorize_moonlight_launch_spec(&first_json),
            MoonlightLaunchAuthorization::Authorized
        );
        assert_eq!(
            authorize_moonlight_launch_spec(&first_json),
            MoonlightLaunchAuthorization::Replay
        );
        let stream = publish_moonlight_active_launch(
            &first_json,
            "com.simonwjackson.korri",
            "com.limelight.Game",
        )
        .unwrap();
        assert_eq!(stream.launch_id, first["launchId"].as_str().unwrap());
        assert_eq!(
            stream.foreground.kind,
            launcher::LaunchForegroundKind::Component
        );
        assert_eq!(
            stream.foreground.package_name.as_deref(),
            Some("com.simonwjackson.korri")
        );
        assert_eq!(
            stream.foreground.class_name.as_deref(),
            Some("com.limelight.Game")
        );
        let stale_retroarch = client
            .post(&url)
            .bearer_auth(&capability)
            .json(&serde_json::json!({
                "_tag": "app.session.control.invoke",
                "payload": {
                    "launchId": local.launch_id.clone(),
                    "controlId": "@korri:retroarch/quit",
                }
            }))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(stale_retroarch["outcome"]["_tag"], "Err");
        assert_eq!(
            stale_retroarch["outcome"]["payload"]["reason"],
            "StaleSession"
        );
        assert!(publish_moonlight_executor_state(
            &serde_json::to_string(&moonlight_executor_state(&stream.launch_id)).unwrap()
        ));
        let controls = client
            .post(&url)
            .bearer_auth(&capability)
            .json(&serde_json::json!({
                "_tag": "app.session.controls",
                "payload": { "launchId": stream.launch_id.clone() }
            }))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(controls["outcome"]["_tag"], "Ok");
        assert_eq!(
            controls["outcome"]["payload"]["groups"][0]["id"],
            "@korri:moonlight"
        );
        assert_eq!(
            controls["outcome"]["payload"]["groups"][0]["label"],
            "Moonlight"
        );
        assert_eq!(
            controls["outcome"]["payload"]["groups"][0]["controls"]
                .as_array()
                .unwrap()
                .len(),
            24
        );
        let fill = controls["outcome"]["payload"]["groups"][0]["controls"]
            .as_array()
            .unwrap()
            .iter()
            .find(|control| control["id"] == "@korri:moonlight/fill")
            .unwrap();
        assert_eq!(
            fill["interaction"],
            serde_json::json!({
                "kind": "toggle",
                "payload": {
                    "value": false,
                    "trueLabel": "crop to fill",
                    "falseLabel": "fit (letterbox)"
                }
            })
        );
        let bitrate = controls["outcome"]["payload"]["groups"][0]["controls"]
            .as_array()
            .unwrap()
            .iter()
            .find(|control| control["id"] == "@korri:moonlight/stream-bitrate")
            .unwrap();
        assert_eq!(
            bitrate["interaction"],
            serde_json::json!({
                "kind": "range",
                "payload": { "value": 12345.0, "min": 500.0, "max": 150000.0, "step": 1.0 }
            })
        );
        let invoke = client
            .post(&url)
            .bearer_auth(&capability)
            .json(&serde_json::json!({
                "_tag": "app.session.control.invoke",
                "payload": {
                    "launchId": stream.launch_id.clone(),
                    "controlId": "@korri:moonlight/fill",
                    "value": { "kind": "toggle", "value": true }
                }
            }))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(invoke["outcome"]["_tag"], "Ok");
        let protected_fill = &invoke["outcome"]["payload"]["payload"];
        assert_eq!(
            protected_fill["effect"],
            serde_json::json!({
                "kind": "android-moonlight", "payload": "set-fill-mode"
            })
        );
        assert_eq!(
            authorize_platform_instruction(&serde_json::to_string(protected_fill).unwrap()),
            PlatformInstructionAuthorization::Authorized(AuthorizedPlatformInstruction {
                launch_id: stream.launch_id.clone(),
                executor_id: "android-moonlight".into(),
                generation: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .into(),
                effect: launcher::AndroidMoonlightEffect::SetFillMode,
                value: Some(SessionControlValue::Toggle(true)),
            })
        );
        let mut refreshed = moonlight_executor_state(&stream.launch_id);
        refreshed
            .effects
            .iter_mut()
            .find(|entry| entry.effect == launcher::AndroidMoonlightEffect::SetFillMode)
            .unwrap()
            .value = Some(SessionControlValue::Toggle(true));
        assert!(publish_moonlight_executor_state(
            &serde_json::to_string(&refreshed).unwrap()
        ));
        let refreshed_controls = client
            .post(&url)
            .bearer_auth(&capability)
            .json(&serde_json::json!({
                "_tag": "app.session.controls", "payload": { "launchId": stream.launch_id.clone() }
            }))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        let refreshed_fill = refreshed_controls["outcome"]["payload"]["groups"][0]["controls"]
            .as_array()
            .unwrap()
            .iter()
            .find(|control| control["id"] == "@korri:moonlight/fill")
            .unwrap();
        assert_eq!(refreshed_fill["interaction"]["payload"]["value"], true);
        let signing_key = server_slot()
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .launch_signing_key
            .clone();
        let instruction = launcher::PlatformInstruction::protect(
            stream.launch_id.clone(),
            "android-moonlight",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "@korri:moonlight/disconnect",
            true,
            None,
            launcher::PlatformEffect::AndroidMoonlight(
                launcher::AndroidMoonlightEffect::Disconnect,
            ),
            &signing_key,
        );
        let instruction_json = serde_json::to_string(&instruction).unwrap();
        assert_eq!(
            authorize_platform_instruction(&instruction_json),
            PlatformInstructionAuthorization::Authorized(AuthorizedPlatformInstruction {
                launch_id: stream.launch_id.clone(),
                executor_id: "android-moonlight".into(),
                generation: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .into(),
                effect: launcher::AndroidMoonlightEffect::Disconnect,
                value: None,
            })
        );
        assert_eq!(
            publish_moonlight_active_launch(
                &first_json,
                "com.simonwjackson.korri",
                "com.limelight.Game",
            ),
            Err(ActiveAndroidLaunchFailure::AlreadyPublished)
        );
        assert_eq!(
            authorize_platform_instruction(&instruction_json),
            PlatformInstructionAuthorization::Replay
        );

        let second = prepare()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap()["outcome"]["payload"]
            .clone();
        let mut tampered = second.clone();
        tampered["appId"] = serde_json::json!(8);
        assert_eq!(
            authorize_moonlight_launch_spec(&serde_json::to_string(&tampered).unwrap()),
            MoonlightLaunchAuthorization::Integrity
        );

        let third = prepare()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap()["outcome"]["payload"]
            .clone();
        assert_eq!(
            authorize_moonlight_launch_spec(&serde_json::to_string(&second).unwrap()),
            MoonlightLaunchAuthorization::Stale
        );
        let mut unknown = third;
        unknown["signingKey"] = serde_json::json!("must not cross");
        assert_eq!(
            authorize_moonlight_launch_spec(&serde_json::to_string(&unknown).unwrap()),
            MoonlightLaunchAuthorization::InvalidSpec
        );

        let old_quit = launcher::PlatformInstruction::protect(
            stream.launch_id.clone(),
            "android-moonlight",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "@korri:moonlight/quit-host",
            true,
            None,
            launcher::PlatformEffect::AndroidMoonlight(launcher::AndroidMoonlightEffect::QuitHost),
            &signing_key,
        );
        let mut replacement_executor = moonlight_executor_state(&stream.launch_id);
        replacement_executor.generation =
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into();
        let unavailable_mouse = replacement_executor
            .effects
            .iter_mut()
            .find(|entry| entry.effect == launcher::AndroidMoonlightEffect::SetMouseMode)
            .unwrap();
        unavailable_mouse.fulfillable = false;
        unavailable_mouse.value = None;
        for effect in [
            launcher::AndroidMoonlightEffect::SetStreamBitrateKbps,
            launcher::AndroidMoonlightEffect::RestoreStreamBitrate,
        ] {
            let entry = replacement_executor
                .effects
                .iter_mut()
                .find(|entry| entry.effect == effect)
                .unwrap();
            entry.fulfillable = false;
            entry.value = None;
            entry.range = None;
        }
        assert!(publish_moonlight_executor_state(
            &serde_json::to_string(&replacement_executor).unwrap()
        ));
        // A late failed-publication cleanup for generation A cannot erase the
        // already-published replacement generation B.
        assert!(!clear_moonlight_executor_state(
            &stream.launch_id,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ));
        assert_eq!(
            authorize_platform_instruction(&serde_json::to_string(&old_quit).unwrap()),
            PlatformInstructionAuthorization::Stale
        );
        let partially_available = client
            .post(&url)
            .bearer_auth(&capability)
            .json(&serde_json::json!({
                "_tag": "app.session.controls",
                "payload": { "launchId": stream.launch_id.clone() }
            }))
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        let available_controls = partially_available["outcome"]["payload"]["groups"][0]["controls"]
            .as_array()
            .unwrap();
        assert_eq!(available_controls.len(), 21);
        assert!(!available_controls
            .iter()
            .any(|control| control["id"] == "@korri:moonlight/mouse-mode"));
        assert!(!available_controls
            .iter()
            .any(|control| control["id"] == "@korri:moonlight/stream-bitrate"));
        assert!(!available_controls
            .iter()
            .any(|control| control["id"] == "@korri:moonlight/restore-stream-bitrate"));
        assert!(available_controls
            .iter()
            .any(|control| control["id"] == "@korri:moonlight/keyboard"));

        let policy_instruction = launcher::PlatformInstruction::protect(
            stream.launch_id.clone(),
            "android-moonlight",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "@korri:moonlight/keyboard",
            true,
            None,
            launcher::PlatformEffect::AndroidMoonlight(
                launcher::AndroidMoonlightEffect::ToggleKeyboard,
            ),
            &signing_key,
        );
        let policy_instruction_json = serde_json::to_string(&policy_instruction).unwrap();
        std::fs::write(
            root.path().join("config.yaml"),
            "host:\n  plugin:\n    '@korri:moonlight': false\n",
        )
        .unwrap();
        assert_eq!(
            authorize_platform_instruction(&policy_instruction_json),
            PlatformInstructionAuthorization::ExecutorUnavailable
        );
        assert_eq!(
            authorize_platform_instruction(&policy_instruction_json),
            PlatformInstructionAuthorization::ExecutorUnavailable
        );
        write_wl4_plugin_config(root.path());
        assert_eq!(
            authorize_platform_instruction(&policy_instruction_json),
            PlatformInstructionAuthorization::Authorized(AuthorizedPlatformInstruction {
                launch_id: stream.launch_id.clone(),
                executor_id: "android-moonlight".into(),
                generation: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    .into(),
                effect: launcher::AndroidMoonlightEffect::ToggleKeyboard,
                value: None,
            })
        );

        let prepared_before_disable = prepare()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap()["outcome"]["payload"]
            .clone();
        std::fs::write(
            root.path().join("config.yaml"),
            "host:\n  plugin:\n    '@korri:moonlight': false\n",
        )
        .unwrap();
        assert_eq!(
            authorize_moonlight_launch_spec(
                &serde_json::to_string(&prepared_before_disable).unwrap()
            ),
            MoonlightLaunchAuthorization::Stale
        );
    }

    #[tokio::test]
    async fn restarting_the_server_rotates_capability_and_invalidates_old_specs() {
        let _serialized = embedded_server_guard();
        struct StopServer;
        impl Drop for StopServer {
            fn drop(&mut self) {
                let _ = stop_local_server();
            }
        }

        async fn launch_wl4(
            client: &reqwest::Client,
            url: &str,
            capability: &str,
        ) -> serde_json::Value {
            for _ in 0..20 {
                match client
                    .post(url)
                    .bearer_auth(capability)
                    .json(&serde_json::json!({
                        "_tag": "app.local-games.launch",
                        "payload": { "gameId": "wl4" }
                    }))
                    .send()
                    .await
                {
                    Ok(value) => return value.json::<serde_json::Value>().await.unwrap(),
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(10)),
                }
            }
            panic!("embedded server response");
        }

        let root = tempfile::tempdir().unwrap();
        let private_root = tempfile::tempdir().unwrap();
        write_wl4_plugin_config(root.path());
        std::fs::create_dir_all(root.path().join("roms")).unwrap();
        std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
        let first_port = start_local_server(
            "https://portal.example",
            root.path().to_str().expect("UTF-8 temp path"),
            private_root.path().to_str().expect("UTF-8 temp path"),
        )
        .unwrap();
        let _stop = StopServer;
        let first_capability = local_server_capability().unwrap();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .unwrap();
        let first_url = format!("http://127.0.0.1:{first_port}/rpc");
        let response = launch_wl4(&client, &first_url, &first_capability).await;
        let mut spec = response["outcome"]["payload"].clone();
        let first_spec_json = serde_json::to_string(&spec).unwrap();
        assert!(verify_local_launch_spec(&first_spec_json));

        spec["files"][0]["content"] = serde_json::Value::String("tampered".into());
        assert!(!verify_local_launch_spec(
            &serde_json::to_string(&spec).unwrap()
        ));

        stop_local_server().unwrap();
        assert!(local_server_port().is_none());
        assert!(local_server_capability().is_none());
        assert!(!verify_local_launch_spec(&first_spec_json));

        let second_port = start_local_server(
            "https://portal.example",
            root.path().to_str().expect("UTF-8 temp path"),
            private_root.path().to_str().expect("UTF-8 temp path"),
        )
        .unwrap();
        let second_capability = local_server_capability().unwrap();
        assert!(
            first_capability != second_capability,
            "server restart must rotate the capability"
        );
        let second_url = format!("http://127.0.0.1:{second_port}/rpc");
        let second_response = launch_wl4(&client, &second_url, &second_capability).await;
        let second_spec_json =
            serde_json::to_string(&second_response["outcome"]["payload"]).unwrap();
        assert!(verify_local_launch_spec(&second_spec_json));
        assert!(!verify_local_launch_spec(&first_spec_json));

        let old_capability_response = client
            .post(&second_url)
            .bearer_auth(&first_capability)
            .json(&serde_json::json!({
                "_tag": "system.health",
                "payload": {}
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(old_capability_response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn settings_update_changes_public_android_route_availability() {
        fn listed_game_ids(body: &serde_json::Value) -> Vec<String> {
            body["outcome"]["payload"]["games"]
                .as_array()
                .unwrap()
                .iter()
                .map(|game| game["id"].as_str().unwrap().to_owned())
                .collect()
        }

        fn plugin_enabled(body: &serde_json::Value, plugin_id: &str) -> bool {
            body["outcome"]["payload"]["plugins"]
                .as_array()
                .unwrap()
                .iter()
                .find(|plugin| plugin["id"] == plugin_id)
                .and_then(|plugin| plugin["enabled"].as_bool())
                .unwrap()
        }

        let root = tempfile::tempdir().unwrap();
        write_checkpoint_android_config(root.path());
        let app = router_with_capability_and_local_root(
            "right-token",
            "https://portal.example",
            root.path(),
        );

        let listed = rpc_body_authorized(
            app.clone(),
            r#"{"_tag":"app.local-games.list","payload":{}}"#,
            Some("right-token"),
        )
        .await;
        assert_eq!(listed["outcome"]["_tag"], "Ok");
        assert_eq!(listed_game_ids(&listed), ["tmnt-shredders-revenge"]);

        let launched = rpc_body_authorized(
            app.clone(),
            r#"{"_tag":"app.local-games.launch","payload":{"gameId":"tmnt-shredders-revenge"}}"#,
            Some("right-token"),
        )
        .await;
        assert_eq!(launched["outcome"]["_tag"], "Ok");
        assert_eq!(launched["outcome"]["payload"]["launcherId"], "android-app");

        let before = rpc_body_authorized(
            app.clone(),
            r#"{"_tag":"system.settings.snapshot","payload":{}}"#,
            Some("right-token"),
        )
        .await;
        assert!(plugin_enabled(
            &before,
            plugin_policy::ANDROID_APP_PLUGIN_ID
        ));
        let revision = before["outcome"]["payload"]["revision"].as_str().unwrap();
        let disable_request = serde_json::json!({
            "_tag": "system.settings.update",
            "payload": {
                "expectedRevision": revision,
                "settingId": plugin_policy::ANDROID_APP_PLUGIN_ID,
                "value": "false"
            }
        })
        .to_string();
        let disabled =
            rpc_body_authorized(app.clone(), &disable_request, Some("right-token")).await;
        assert_eq!(disabled["outcome"]["_tag"], "Ok");
        assert!(!plugin_enabled(
            &disabled,
            plugin_policy::ANDROID_APP_PLUGIN_ID
        ));
        let disabled_revision = disabled["outcome"]["payload"]["revision"].as_str().unwrap();

        let disabled_list = rpc_body_authorized(
            app.clone(),
            r#"{"_tag":"app.local-games.list","payload":{}}"#,
            Some("right-token"),
        )
        .await;
        assert_eq!(disabled_list["outcome"]["_tag"], "Ok");
        assert!(listed_game_ids(&disabled_list).is_empty());

        let disabled_launch = rpc_body_authorized(
            app.clone(),
            r#"{"_tag":"app.local-games.launch","payload":{"gameId":"tmnt-shredders-revenge"}}"#,
            Some("right-token"),
        )
        .await;
        assert_eq!(disabled_launch["outcome"]["_tag"], "Err");
        assert_eq!(
            disabled_launch["outcome"]["payload"]["code"],
            "LocalRouteUnavailable"
        );

        let stale_enable_request = serde_json::json!({
            "_tag": "system.settings.update",
            "payload": {
                "expectedRevision": revision,
                "settingId": plugin_policy::ANDROID_APP_PLUGIN_ID,
                "value": "true"
            }
        })
        .to_string();
        let stale_enable =
            rpc_body_authorized(app.clone(), &stale_enable_request, Some("right-token")).await;
        assert_eq!(stale_enable["outcome"]["_tag"], "Err");
        assert_eq!(
            stale_enable["outcome"]["payload"]["code"],
            "SettingsConflict"
        );
        let still_disabled = rpc_body_authorized(
            app.clone(),
            r#"{"_tag":"app.local-games.list","payload":{}}"#,
            Some("right-token"),
        )
        .await;
        assert!(listed_game_ids(&still_disabled).is_empty());

        let enable_request = serde_json::json!({
            "_tag": "system.settings.update",
            "payload": {
                "expectedRevision": disabled_revision,
                "settingId": plugin_policy::ANDROID_APP_PLUGIN_ID,
                "value": "true"
            }
        })
        .to_string();
        let enabled = rpc_body_authorized(app.clone(), &enable_request, Some("right-token")).await;
        assert_eq!(enabled["outcome"]["_tag"], "Ok");
        assert!(plugin_enabled(
            &enabled,
            plugin_policy::ANDROID_APP_PLUGIN_ID
        ));

        let relisted = rpc_body_authorized(
            app.clone(),
            r#"{"_tag":"app.local-games.list","payload":{}}"#,
            Some("right-token"),
        )
        .await;
        assert_eq!(listed_game_ids(&relisted), ["tmnt-shredders-revenge"]);

        let relaunched = rpc_body_authorized(
            app,
            r#"{"_tag":"app.local-games.launch","payload":{"gameId":"tmnt-shredders-revenge"}}"#,
            Some("right-token"),
        )
        .await;
        assert_eq!(relaunched["outcome"]["_tag"], "Ok");
        assert_eq!(
            relaunched["outcome"]["payload"]["launcherId"],
            "android-app"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn local_launch_rpc_reports_initial_empty_unauthorized_config_for_non_static_ids() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let app = router_with_capability_and_local_root(
            "right-token",
            "https://portal.example",
            root.path(),
        );
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o000)).unwrap();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/rpc")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::AUTHORIZATION, "Bearer right-token")
                    .body(Body::from(
                        r#"{"_tag":"app.local-games.launch","payload":{"gameId":"tmnt-shredders-revenge"}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("LocalConfigUnauthorized"));
    }

    #[tokio::test]
    async fn local_launch_rpc_maps_missing_and_unknown_games_to_distinct_codes() {
        for (game_id, code) in [("wl4", "LocalRomMissing"), ("unknown", "LocalGameNotFound")] {
            let root = tempfile::tempdir().unwrap();
            write_wl4_plugin_config(root.path());
            let app = router_with_capability_and_local_root(
                "right-token",
                "https://portal.example",
                root.path(),
            );
            let body = format!(
                r#"{{"_tag":"app.local-games.launch","payload":{{"gameId":"{game_id}"}}}}"#
            );
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/rpc")
                        .header(header::CONTENT_TYPE, "application/json")
                        .header(header::AUTHORIZATION, "Bearer right-token")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            assert!(String::from_utf8_lossy(&body).contains(code));
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn local_launch_rpc_distinguishes_config_authorization_and_write_failures() {
        use std::os::unix::fs::PermissionsExt;

        for (mode, code) in [
            (0o000, "LocalConfigUnauthorized"),
            (0o500, "LocalConfigWriteFailed"),
        ] {
            let root = tempfile::tempdir().unwrap();
            write_wl4_plugin_config(root.path());
            std::fs::create_dir_all(root.path().join("roms")).unwrap();
            std::fs::write(root.path().join("roms/wl4.gba"), b"rom").unwrap();
            let app = router_with_capability_and_local_root(
                "right-token",
                "https://portal.example",
                root.path(),
            );
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(mode)).unwrap();
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/rpc")
                        .header(header::CONTENT_TYPE, "application/json")
                        .header(header::AUTHORIZATION, "Bearer right-token")
                        .body(Body::from(
                            r#"{"_tag":"app.local-games.launch","payload":{"gameId":"wl4"}}"#,
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            assert!(
                String::from_utf8_lossy(&body).contains(code),
                "mode {mode:o} expected {code}, got {}",
                String::from_utf8_lossy(&body)
            );
        }
    }

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
                .unwrap(),
            "https://evil.example"
        );
    }

    #[tokio::test]
    async fn android_cors_allows_bundled_overlay_and_configured_shell_origins_only() {
        let root = tempfile::tempdir().unwrap();
        let app = android_router_with_capability_and_local_root(
            "right-token",
            "http://10.0.2.2:5173",
            root.path(),
        );
        for origin in [
            "https://appassets.androidplatform.net",
            "http://10.0.2.2:5173",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("OPTIONS")
                        .uri("/rpc")
                        .header(header::ORIGIN, origin)
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
            assert_eq!(
                response
                    .headers()
                    .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                    .unwrap(),
                origin
            );
        }

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
        assert!(foreign
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none());
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
    struct RecordingMoonlightCertificates {
        expected_host_uuid: String,
        calls: Mutex<Vec<String>>,
    }

    impl RecordingMoonlightCertificates {
        fn matching(expected_host_uuid: &str) -> Arc<Self> {
            Arc::new(Self {
                expected_host_uuid: expected_host_uuid.into(),
                calls: Mutex::new(Vec::new()),
            })
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl host::moonlight_certificate::MoonlightCertificateAdapter for RecordingMoonlightCertificates {
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

        fn revoke(&self, host_uuid: &str, _client_certificate: &str) -> Result<bool, RpcFailure> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("revoke:{host_uuid}"));
            Ok(host_uuid == self.expected_host_uuid)
        }
    }

    #[derive(Default)]
    struct ChangingMoonlightCertificates {
        calls: Mutex<Vec<String>>,
    }

    impl host::moonlight_certificate::MoonlightCertificateAdapter for ChangingMoonlightCertificates {
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

    fn secure_host_router_with_certificate_adapter(
        config_path: &Path,
        private_state_root: &Path,
        adapter: Arc<dyn host::moonlight_certificate::MoonlightCertificateAdapter>,
    ) -> Router {
        let runtime = host::HostRuntime::from_paths_with_backends(
            config_path,
            None,
            private_state_root.to_owned(),
            Arc::new(host::control::InMemoryLaunchUnitBackend::default()),
            adapter,
        );
        secure_host_routers(runtime, private_state_root).0
    }

    async fn serve_router(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    const TEST_CLIENT_PEM: &str =
        "-----BEGIN CERTIFICATE-----\nclient\n-----END CERTIFICATE-----\n";

    #[tokio::test]
    async fn same_owner_automatically_provisions_and_owner_change_revokes_certificate() {
        const OWNER: &str = "0000000000000000000000000000000000000000000000000000000000000003";
        const OTHER_OWNER: &str =
            "0000000000000000000000000000000000000000000000000000000000000004";
        const HOST: &str = "0000000000000000000000000000000000000000000000000000000000000005";
        const CLIENT: &str = "0000000000000000000000000000000000000000000000000000000000000006";
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("host.toml");
        std::fs::write(&config, "label = \"zao\"\n").unwrap();
        let host_private = tempfile::tempdir().unwrap();
        let client_private = tempfile::tempdir().unwrap();
        let host_identity = peer_rpc::test_owned_identity(host_private.path(), HOST, OWNER);
        let host_key = host_identity.device_public_key().unwrap().to_owned();
        let credentials = peer_rpc::test_owned_credentials(client_private.path(), CLIENT, OWNER);
        let adapter = RecordingMoonlightCertificates::matching("sunshine-host");
        let server = serve_router(secure_host_router_with_certificate_adapter(
            &config,
            host_private.path(),
            adapter.clone(),
        ))
        .await;
        let client = upstream_native::NativeClient::new_secure(
            server.clone(),
            host_key.clone(),
            credentials,
        );
        client
            .moonlight_certificate_provision("sunshine-host", TEST_CLIENT_PEM)
            .await
            .unwrap();
        assert_eq!(adapter.calls(), vec!["provision:sunshine-host"]);

        std::fs::remove_file(client_private.path().join("identity/owner.event.json")).unwrap();
        let changed = peer_rpc::test_owned_credentials(client_private.path(), CLIENT, OTHER_OWNER);
        let denied = upstream_native::NativeClient::new_secure(server, host_key, changed)
            .catalog_snapshot()
            .await
            .unwrap_err();
        assert!(matches!(denied, upstreams::UpstreamError::Http(403)));
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
        std::fs::write(root.path().join("config.yaml"), "{}\n").unwrap();
        std::fs::write(root.path().join("library.yaml"), "{}\n").unwrap();

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
        std::fs::write(root.path().join("config.yaml"), "{}\n").unwrap();
        std::fs::write(root.path().join("library.yaml"), "{}\n").unwrap();
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
        std::fs::write(root.path().join("config.yaml"), "{}\n").unwrap();
        std::fs::write(root.path().join("library.yaml"), "{}\n").unwrap();
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
            std::fs::write(root.path().join("config.yaml"), "{}\n").unwrap();
            std::fs::write(root.path().join("library.yaml"), "{}\n").unwrap();
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
