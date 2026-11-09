//! The surface host's one road into korrid: the same typed `RpcRequest`s the
//! portal's `clients/portal/src/korrid/client.ts` sent over loopback, handed
//! to korrid's own dispatcher in process. Each method converts a refusal into
//! the outcome the portal client produced for it, so the ported logic sees
//! exactly what the portal saw. Plan-time runner labels are an internal host
//! fact reader; they do not add an RPC or give a surface package registry access.

use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

use crate::{
    acquisition::{self, AcquisitionOutcome},
    game_routes::SelectedGameLaunchOutcome,
    network, onboarding, plugin_admin, CatalogSnapshotOutcome, HealthOutcome,
    IdentityBackupOutcome, IdentityRetiredDeleteOutcome, IdentityStatusOutcome,
    IdentitySwitchOutcome, LibraryFoldersSnapshotOutcome, LocalGamesListOutcome,
    OwnerBindingOutcome, PeerListOutcome, PluginChangeOutcome, PluginRepositoriesOutcome,
    RpcFailure, RpcRequest, RpcResponse, SensitiveSettingOutcome, SessionEndOutcome,
    SessionReserveOutcome, SessionStatusOutcome, SessionTransitionOutcome, SettingsSnapshotOutcome,
    SettingsUpdateOutcome,
};

pub type Reply = Pin<Box<dyn Future<Output = Result<RpcResponse, Refusal>> + Send>>;

/// Why korrid did not answer a request.
#[derive(Clone, Debug)]
pub enum Refusal {
    /// The caller's authority does not cover the request (the portal saw HTTP
    /// 403).
    Denied,
    /// korrid could not be reached or answered with something else.
    Unreachable(String),
}

/// Sends one request to korrid. Production wraps `dispatch`; tests answer
/// with configured outcomes.
pub type Send_ = Arc<dyn Fn(RpcRequest) -> Reply + Send + Sync>;

pub(crate) type LaunchSteps = Arc<
    dyn Fn(
            String,
            Option<String>,
        ) -> Pin<Box<dyn Future<Output = Option<Vec<crate::plugin::RunnerStep>>> + Send>>
        + Send
        + Sync,
>;

#[derive(Clone)]
pub struct Korrid {
    send: Send_,
    launch_steps: Option<LaunchSteps>,
}

/// How a refusal reads for one method, as the portal client mapped it.
#[derive(Clone, Copy)]
pub enum Mapping {
    /// `unreachable`: always `BrainUnreachable`.
    Unreachable,
    /// `routeUnavailable`: HTTP 403 is `PermissionDenied`.
    Route,
    /// `settingWriteUnavailable`: HTTP 403 is `PermissionDenied`, worded for
    /// a setting.
    SettingWrite,
}

fn failure(mapping: Mapping, refusal: Refusal) -> RpcFailure {
    match (mapping, refusal) {
        (Mapping::Route, Refusal::Denied) => RpcFailure {
            code: "PermissionDenied".into(),
            message: "This device does not have permission for this action. Runtime reads remain available.".into(),
        },
        (Mapping::SettingWrite, Refusal::Denied) => RpcFailure {
            code: "PermissionDenied".into(),
            message: "This device cannot change this setting.".into(),
        },
        (_, Refusal::Denied) => RpcFailure {
            code: "BrainUnreachable".into(),
            message: "korrid returned HTTP 403".into(),
        },
        (_, Refusal::Unreachable(message)) => RpcFailure {
            code: "BrainUnreachable".into(),
            message,
        },
    }
}

fn unexpected() -> Refusal {
    Refusal::Unreachable("korrid answered a different request".into())
}

macro_rules! call {
    ($self:expr, $mapping:expr, $variant:ident, $payload:expr, $outcome:ident) => {{
        match ($self.send)(RpcRequest::$variant($payload)).await {
            Ok(RpcResponse::$variant(outcome)) => outcome,
            Ok(_) => $outcome::Err(failure($mapping, unexpected())),
            Err(refusal) => $outcome::Err(failure($mapping, refusal)),
        }
    }};
}

/// One typed request, its response variant, and the refusal mapping the
/// portal client used for it.
macro_rules! ask {
    ($korrid:expr, $mapping:ident, $variant:ident, $payload:expr, $outcome:path) => {{
        use $outcome as Outcome;
        match $korrid
            .rpc(
                $crate::surface::korrid::Mapping::$mapping,
                $crate::RpcRequest::$variant($payload),
            )
            .await
        {
            Ok($crate::RpcResponse::$variant(outcome)) => outcome,
            Ok(_) => Outcome::Err($crate::RpcFailure {
                code: "BrainUnreachable".into(),
                message: "korrid answered a different request".into(),
            }),
            Err(failure) => Outcome::Err(failure),
        }
    }};
}
pub(crate) use ask;

impl Korrid {
    pub fn new(send: Send_) -> Self {
        Self {
            send,
            launch_steps: None,
        }
    }

    pub(crate) fn with_launch_steps(mut self, resolve: LaunchSteps) -> Self {
        self.launch_steps = Some(resolve);
        self
    }

    pub(crate) async fn launch_steps(
        &self,
        game_id: String,
        runner_id: Option<String>,
    ) -> Option<Vec<crate::plugin::RunnerStep>> {
        match &self.launch_steps {
            Some(resolve) => resolve(game_id, runner_id).await,
            None => None,
        }
    }

    /// Any request, its refusal read the way `mapping` says.
    pub async fn rpc(
        &self,
        mapping: Mapping,
        request: RpcRequest,
    ) -> Result<RpcResponse, RpcFailure> {
        (self.send)(request)
            .await
            .map_err(|refusal| failure(mapping, refusal))
    }

    pub async fn peer_list(&self) -> PeerListOutcome {
        call!(
            self,
            Mapping::Unreachable,
            PeerList,
            crate::PeerListRequest {},
            PeerListOutcome
        )
    }
    pub async fn catalog_snapshot(&self) -> CatalogSnapshotOutcome {
        call!(
            self,
            Mapping::Unreachable,
            CatalogSnapshot,
            crate::CatalogSnapshotRequest {},
            CatalogSnapshotOutcome
        )
    }
    pub async fn local_games(&self) -> LocalGamesListOutcome {
        call!(
            self,
            Mapping::Unreachable,
            LocalGamesList,
            crate::LocalGamesListRequest {},
            LocalGamesListOutcome
        )
    }
    /// The portal bounded this read; past the deadline it is a
    /// `StatusTimeout` observation failure.
    pub async fn session_status(&self, timeout: Option<Duration>) -> SessionStatusOutcome {
        let read = async {
            call!(
                self,
                Mapping::Unreachable,
                SessionStatus,
                crate::SessionStatusRequest {},
                SessionStatusOutcome
            )
        };
        match timeout {
            None => read.await,
            Some(limit) => tokio::time::timeout(limit, read).await.unwrap_or_else(|_| {
                SessionStatusOutcome::Err(RpcFailure {
                    code: "StatusTimeout".into(),
                    message: "session status timed out".into(),
                })
            }),
        }
    }
    pub async fn health(&self) -> HealthOutcome {
        call!(
            self,
            Mapping::Unreachable,
            Health,
            crate::HealthRequest {},
            HealthOutcome
        )
    }
    pub async fn settings_snapshot(&self) -> SettingsSnapshotOutcome {
        call!(
            self,
            Mapping::Route,
            SettingsSnapshot,
            crate::SettingsSnapshotRequest {},
            SettingsSnapshotOutcome
        )
    }
    pub async fn update_setting(
        &self,
        expected_revision: String,
        setting_id: String,
        value: String,
    ) -> SettingsUpdateOutcome {
        call!(
            self,
            Mapping::SettingWrite,
            SettingsUpdate,
            crate::SettingsUpdateRequest {
                expected_revision,
                setting_id,
                value
            },
            SettingsUpdateOutcome
        )
    }
    pub async fn set_steam_grid_db_credential(&self, token: String) -> SensitiveSettingOutcome {
        call!(
            self,
            Mapping::SettingWrite,
            SteamGridDbCredentialSet,
            crate::SteamGridDbCredentialSetRequest { token },
            SensitiveSettingOutcome
        )
    }
    pub async fn clear_steam_grid_db_credential(&self) -> SensitiveSettingOutcome {
        call!(
            self,
            Mapping::SettingWrite,
            SteamGridDbCredentialClear,
            crate::SteamGridDbCredentialClearRequest {},
            SensitiveSettingOutcome
        )
    }
    pub async fn library_folders_snapshot(&self) -> LibraryFoldersSnapshotOutcome {
        call!(
            self,
            Mapping::Unreachable,
            LibraryFoldersSnapshot,
            crate::LibraryFoldersSnapshotRequest {},
            LibraryFoldersSnapshotOutcome
        )
    }
    pub async fn remove_library_folder(&self, folder: String) -> LibraryFoldersSnapshotOutcome {
        call!(
            self,
            Mapping::Unreachable,
            LibraryFolderRemove,
            crate::LibraryFolderRemoveRequest { folder },
            LibraryFoldersSnapshotOutcome
        )
    }
    pub async fn rescan_library_folders(&self) -> LibraryFoldersSnapshotOutcome {
        call!(
            self,
            Mapping::Unreachable,
            LibraryFoldersRescan,
            crate::LibraryFoldersRescanRequest {},
            LibraryFoldersSnapshotOutcome
        )
    }
    pub async fn identity_status(&self) -> IdentityStatusOutcome {
        call!(
            self,
            Mapping::Route,
            IdentityStatus,
            crate::IdentityStatusRequest {},
            IdentityStatusOutcome
        )
    }
    pub async fn export_identity_backup(
        &self,
        password: String,
        retired_public_key: Option<String>,
    ) -> IdentityBackupOutcome {
        match retired_public_key {
            None => call!(
                self,
                Mapping::Route,
                IdentityBackupExport,
                crate::IdentityBackupExportRequest { password },
                IdentityBackupOutcome
            ),
            Some(public_key) => call!(
                self,
                Mapping::Route,
                IdentityRetiredExport,
                crate::IdentityRetiredExportRequest {
                    public_key,
                    password
                },
                IdentityBackupOutcome
            ),
        }
    }
    pub async fn switch_identity_from_backup(
        &self,
        encrypted_secret: String,
        password: String,
        data_disposition: crate::IdentityDataDisposition,
        trust_loss_confirmed: bool,
    ) -> IdentitySwitchOutcome {
        call!(
            self,
            Mapping::Route,
            IdentityLocalSwitch,
            crate::IdentityLocalSwitchRequest {
                encrypted_secret,
                password,
                data_disposition,
                trust_loss_confirmed
            },
            IdentitySwitchOutcome
        )
    }
    pub async fn switch_identity_to_nip46(
        &self,
        bunker_uri: String,
        data_disposition: crate::IdentityDataDisposition,
        trust_loss_confirmed: bool,
    ) -> IdentitySwitchOutcome {
        call!(
            self,
            Mapping::Route,
            IdentityNip46Switch,
            crate::IdentityNip46SwitchRequest {
                bunker_uri,
                data_disposition,
                trust_loss_confirmed
            },
            IdentitySwitchOutcome
        )
    }
    pub async fn delete_retired_identity(
        &self,
        public_key: String,
        backup_confirmed: bool,
    ) -> IdentityRetiredDeleteOutcome {
        call!(
            self,
            Mapping::Route,
            IdentityRetiredDelete,
            crate::IdentityRetiredDeleteRequest {
                public_key,
                backup_confirmed
            },
            IdentityRetiredDeleteOutcome
        )
    }
    pub async fn owner_binding_snapshot(&self) -> OwnerBindingOutcome {
        call!(
            self,
            Mapping::Route,
            OwnerBindingSnapshot,
            crate::OwnerBindingRequest {},
            OwnerBindingOutcome
        )
    }
    pub async fn start_owner_binding(&self) -> OwnerBindingOutcome {
        call!(
            self,
            Mapping::Route,
            OwnerBindingStart,
            crate::OwnerBindingRequest {},
            OwnerBindingOutcome
        )
    }
    pub async fn session_reserve(&self, game_id: String) -> SessionReserveOutcome {
        call!(
            self,
            Mapping::Route,
            SessionReserve,
            crate::SessionReserveRequest { game_id },
            SessionReserveOutcome
        )
    }
    pub async fn session_start(
        &self,
        game_id: String,
        expected_launch_id: String,
        runner_id: Option<String>,
    ) -> SelectedGameLaunchOutcome {
        call!(
            self,
            Mapping::Route,
            SessionStart,
            crate::SessionStartRequest {
                game_id,
                expected_launch_id,
                runner_id,
                launch: None
            },
            SelectedGameLaunchOutcome
        )
    }
    pub async fn session_cancel(&self, expected_launch_id: String) -> SessionEndOutcome {
        call!(
            self,
            Mapping::Route,
            SessionCancel,
            crate::SessionCancelRequest { expected_launch_id },
            SessionEndOutcome
        )
    }
    pub async fn session_prepare(
        &self,
        game_id: String,
        host: Option<String>,
    ) -> SelectedGameLaunchOutcome {
        call!(
            self,
            Mapping::Unreachable,
            SessionPrepare,
            crate::SessionPrepareRequest { game_id, host },
            SelectedGameLaunchOutcome
        )
    }
    pub async fn session_end(&self, expected_launch_id: String) -> SessionEndOutcome {
        call!(
            self,
            Mapping::Unreachable,
            SessionEnd,
            crate::SessionEndRequest {
                force: None,
                expected_launch_id: Some(expected_launch_id)
            },
            SessionEndOutcome
        )
    }
    pub async fn session_resume(&self, expected_launch_id: String) -> SessionTransitionOutcome {
        call!(
            self,
            Mapping::Unreachable,
            SessionResume,
            crate::SessionResumeRequest {
                expected_launch_id: Some(expected_launch_id)
            },
            SessionTransitionOutcome
        )
    }
    pub async fn network_status(&self) -> network::NetworkStatusOutcome {
        use network::NetworkStatusOutcome;
        call!(
            self,
            Mapping::Unreachable,
            NetworkStatus,
            network::NetworkStatusRequest {},
            NetworkStatusOutcome
        )
    }
    pub async fn scan_wifi(&self) -> network::NetworkStatusOutcome {
        use network::NetworkStatusOutcome;
        call!(
            self,
            Mapping::Unreachable,
            WifiScan,
            network::WifiScanRequest {},
            NetworkStatusOutcome
        )
    }
    pub async fn connect_wifi(
        &self,
        network_id: String,
        password: Option<String>,
    ) -> network::NetworkStatusOutcome {
        use network::NetworkStatusOutcome;
        call!(
            self,
            Mapping::Unreachable,
            WifiConnect,
            network::WifiConnectRequest {
                network_id,
                password
            },
            NetworkStatusOutcome
        )
    }
    pub async fn connect_hidden_wifi(
        &self,
        ssid: String,
        password: Option<String>,
    ) -> network::NetworkStatusOutcome {
        use network::NetworkStatusOutcome;
        call!(
            self,
            Mapping::Unreachable,
            WifiConnectHidden,
            network::WifiConnectHiddenRequest { ssid, password },
            NetworkStatusOutcome
        )
    }
    pub async fn onboarding_status(&self) -> onboarding::OnboardingStatusOutcome {
        use onboarding::OnboardingStatusOutcome;
        call!(
            self,
            Mapping::Unreachable,
            OnboardingStatus,
            onboarding::OnboardingStatusRequest {},
            OnboardingStatusOutcome
        )
    }
    pub async fn finish_onboarding(&self) -> onboarding::OnboardingStatusOutcome {
        use onboarding::OnboardingStatusOutcome;
        call!(
            self,
            Mapping::Unreachable,
            OnboardingFinish,
            onboarding::OnboardingFinishRequest {},
            OnboardingStatusOutcome
        )
    }
    pub async fn restart_onboarding(&self) -> onboarding::OnboardingStatusOutcome {
        use onboarding::OnboardingStatusOutcome;
        call!(
            self,
            Mapping::Unreachable,
            OnboardingRestart,
            onboarding::OnboardingRestartRequest {},
            OnboardingStatusOutcome
        )
    }
    pub async fn plugin_repositories(&self) -> PluginRepositoriesOutcome {
        call!(
            self,
            Mapping::Route,
            PluginRepositories,
            plugin_admin::PluginRepositoriesRequest {},
            PluginRepositoriesOutcome
        )
    }
    pub async fn set_plugin_enabled(&self, id: String, enabled: bool) -> PluginChangeOutcome {
        call!(
            self,
            Mapping::Route,
            PluginSetEnabled,
            plugin_admin::PluginSetEnabledRequest { id, enabled },
            PluginChangeOutcome
        )
    }

    pub async fn acquisition_status(
        &self,
        game_id: Option<String>,
        runner_id: Option<String>,
    ) -> AcquisitionOutcome {
        call!(
            self,
            Mapping::Unreachable,
            AcquisitionStatus,
            acquisition::AcquisitionStatusRequest { game_id, runner_id },
            AcquisitionOutcome
        )
    }
    pub async fn acquisition_start(&self, artifact_ids: Vec<String>) -> AcquisitionOutcome {
        call!(
            self,
            Mapping::Unreachable,
            AcquisitionStart,
            acquisition::AcquisitionStartRequest { artifact_ids },
            AcquisitionOutcome
        )
    }
    pub async fn acquisition_cancel(&self, artifact_id: String) -> AcquisitionOutcome {
        call!(
            self,
            Mapping::Unreachable,
            AcquisitionCancel,
            acquisition::AcquisitionCancelRequest { artifact_id },
            AcquisitionOutcome
        )
    }
    pub async fn acquisition_download(&self, game_id: String) -> AcquisitionOutcome {
        call!(
            self,
            Mapping::Unreachable,
            AcquisitionDownload,
            acquisition::AcquisitionDownloadRequest { game_id },
            AcquisitionOutcome
        )
    }
    pub async fn acquisition_remove(&self, game_id: String) -> AcquisitionOutcome {
        call!(
            self,
            Mapping::Unreachable,
            AcquisitionRemove,
            acquisition::AcquisitionRemoveRequest { game_id },
            AcquisitionOutcome
        )
    }

    /// The portal mapped any refusal of these to its own Unavailable failure.
    pub async fn session_controls(&self, launch_id: String) -> crate::SessionControlsOutcome {
        use crate::{SessionControlFailure, SessionControlFailureReason, SessionControlsOutcome};
        match (self.send)(RpcRequest::SessionControls(crate::SessionControlsRequest {
            launch_id,
        }))
        .await
        {
            Ok(RpcResponse::SessionControls(outcome)) => outcome,
            _ => SessionControlsOutcome::Err(SessionControlFailure {
                reason: SessionControlFailureReason::Unavailable,
                message: "Gameplay controls are unavailable right now.".into(),
            }),
        }
    }
    pub async fn invoke_session_control(
        &self,
        launch_id: String,
        control_id: String,
        value: Option<crate::SessionControlValue>,
    ) -> crate::SessionControlInvokeOutcome {
        use crate::{
            SessionControlFailure, SessionControlFailureReason, SessionControlInvokeOutcome,
        };
        match (self.send)(RpcRequest::SessionControlInvoke(
            crate::SessionControlInvokeRequest {
                launch_id,
                control_id,
                value,
            },
        ))
        .await
        {
            Ok(RpcResponse::SessionControlInvoke(outcome)) => outcome,
            _ => SessionControlInvokeOutcome::Err(SessionControlFailure {
                reason: SessionControlFailureReason::Unavailable,
                message: "That gameplay action did not answer.".into(),
            }),
        }
    }

    pub async fn surface_displays(
        &self,
        displays: crate::host::display_placement::SurfaceDisplayScope,
        overlay: bool,
        launch_id: Option<String>,
    ) -> crate::host::display_placement::SurfaceDisplaysOutcome {
        use crate::host::display_placement::{SurfaceDisplaysOutcome, SurfaceDisplaysRequest};
        call!(
            self,
            Mapping::Unreachable,
            SurfaceDisplays,
            SurfaceDisplaysRequest {
                displays,
                overlay,
                launch_id
            },
            SurfaceDisplaysOutcome
        )
    }

    pub async fn session_leave(&self, expected_launch_id: String) -> SessionTransitionOutcome {
        call!(
            self,
            Mapping::Unreachable,
            SessionLeave,
            crate::SessionLeaveRequest {
                expected_launch_id: Some(expected_launch_id)
            },
            SessionTransitionOutcome
        )
    }
}
