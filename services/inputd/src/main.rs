use std::{collections::BTreeMap, ffi::OsString, path::PathBuf, process::ExitCode, time::Duration};

use korri_inputd::{
    actions::{
        commands_from_environment, set_parent_non_dumpable, ActionCommands, ActionDispatcher,
        ActionId, ActionIdentity, ActionLimits, ActionOutcome, ActionRoutes, DispatchMode,
    },
    bundle::is_inside_store_item,
    capture::CaptureRuntime as Runtime,
    dbus::{DbusSignalSource, ProfileStatus},
    devices::{CaptureProvider, EvdevProvider},
    health::{systemd::SystemdHealthPublisher, HealthPublisher, RuntimeHealth},
    korrid_client::{ExactPanelOutcome, ExactStopOutcome, KorridClient},
    producer::PrivateInputChannel,
    runtime::{RuntimeAction, RECONCILE_INTERVAL},
};
use tokio::time::MissedTickBehavior;
use tracing_subscriber::EnvFilter;

const DBUS_RETRY_INTERVAL: Duration = RECONCILE_INTERVAL;
const HOLD_POLL_INTERVAL: Duration = Duration::from_millis(50);
const STORE_ROOT: &str = "/nix/store";
const SUPPORTED_PROFILE_NAME: &str = "korri-60-xbox_one_gamepad.yaml";

#[tokio::main]
async fn main() -> ExitCode {
    if let Err(error) = set_parent_non_dumpable() {
        eprintln!("korri-inputd could not disable process dumps: {error}");
        return ExitCode::FAILURE;
    }
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    if let Err(error) = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .try_init()
    {
        eprintln!("korri-inputd could not initialize structured logging: {error}");
        return ExitCode::FAILURE;
    }

    let services = match configured_services() {
        Ok(configured) => configured,
        Err(error) => {
            tracing::error!(
                event = "inputd_configuration_rejected",
                error,
                "configuration is unsafe"
            );
            return ExitCode::FAILURE;
        }
    };

    let mut health = SystemdHealthPublisher::default();
    if let Err(error) = initialize_health(&mut health) {
        tracing::error!(
            event = "inputd_initial_ready_failed",
            error = %error,
            "systemd did not accept initial readiness"
        );
        return ExitCode::FAILURE;
    }
    run(services, &mut health).await;
    ExitCode::SUCCESS
}

fn initialize_health(health: &mut impl HealthPublisher) -> std::io::Result<()> {
    health.initialized(RuntimeHealth::Recovering)
}

async fn run(services: ConfiguredServices, health: &mut impl HealthPublisher) {
    let mut runtime = Runtime::with_action_routes(services.routes);
    if services.activity.is_some() {
        runtime.enable_activity();
    }
    let mut provider = EvdevProvider::default();
    let mut producer: Option<PrivateInputChannel> = None;
    let mut remote_baseline = true;
    let mut heartbeat = tokio::time::interval(Duration::from_millis(200));
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut dbus = None;
    let mut dbus_failure_logged = false;
    let mut profile_wait_logged = false;
    let mut reconcile = tokio::time::interval(RECONCILE_INTERVAL);
    reconcile.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut hold_poll = tokio::time::interval(HOLD_POLL_INTERVAL);
    hold_poll.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        let has_evdev = runtime.has_open_target();
        let has_dbus = dbus.is_some();
        let has_producer = producer.is_some();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!(event = "inputd_shutdown", "shutdown requested");
                return;
            }
            _ = heartbeat.tick(), if has_producer => runtime.heartbeat(),
            reply = next_producer_reply(&mut producer), if has_producer => {
                match reply {
                    Some(reply) => {
                        if remote_baseline {
                            for source in reply.remote_sources {
                                runtime.handle_remote_event(korri_input_contract::RemoteEvent::Connected { source });
                            }
                            remote_baseline = false;
                        }
                        for event in reply.remote_events {
                            dispatch_actions(runtime.handle_remote_event(event), &services);
                        }
                    }
                    None => {
                        producer = None;
                        runtime.transport_lost();
                    }
                }
            }
            _ = hold_poll.tick() => {
                dispatch_actions(
                    runtime.advance_actions(),
                    &services,
                );
            }
            _ = reconcile.tick(), if services.physical_input => {
                if producer.is_none() {
                    if let Some(client) = &services.korrid {
                        match client.input_channel().await {
                            Ok(channel) => { producer = Some(channel); remote_baseline = true; }
                            Err(_) => { runtime.transport_lost(); continue; }
                        }
                    }
                }
                if dbus.is_none() {
                    match DbusSignalSource::system().await {
                        Ok(source) => dbus = Some(source),
                        Err(error) => {
                            if !dbus_failure_logged {
                                tracing::warn!(
                                    event = "inputd_dbus_connect_failed",
                                    retry_ms = DBUS_RETRY_INTERVAL.as_millis() as u64,
                                    error = %error,
                                    "system bus is unavailable"
                                );
                            }
                        }
                    }
                }
                // A full pass holds this loop for its DBus round trips (about
                // 26 calls), and no controller event is read meanwhile. Skip it
                // while the captured topology is unchanged; see topology_settled.
                if dbus.is_some()
                    && provider
                        .enumerate_capture()
                        .is_ok_and(|devices| runtime.topology_settled(&devices))
                {
                    continue;
                }
                let profile_status = ensure_runtime_profile(
                    &mut runtime,
                    &mut dbus,
                    services.profile_path.as_deref(),
                    &mut profile_wait_logged,
                )
                .await;
                let dbus_available = match profile_status {
                    ProfileStatus::Ready
                    | ProfileStatus::Applied
                    | ProfileStatus::MissingSource
                    | ProfileStatus::AmbiguousSources => {
                        refresh_owner(&mut runtime, &mut dbus).await
                    }
                    ProfileStatus::Pending => {
                        runtime.set_dbus_owner(None);
                        dbus.is_some()
                    }
                };
                if dbus_available && dbus_failure_logged {
                    tracing::info!(
                        event = "inputd_dbus_recovered",
                        "system bus connection recovered"
                    );
                }
                dbus_failure_logged = !dbus_available;
                if runtime.dbus_owner().is_some() {
                    match profile_status {
                        ProfileStatus::Ready | ProfileStatus::Applied => {
                            runtime.reconcile(&mut provider, dbus.as_ref().expect("authenticated DBus connection")).await
                        }
                        ProfileStatus::MissingSource => runtime.source_missing(),
                        ProfileStatus::AmbiguousSources => runtime.source_ambiguous(),
                        ProfileStatus::Pending => {}
                    }
                }
            }
            message = next_dbus_message(&mut dbus), if has_dbus => {
                match message {
                    Ok(Some(message)) => {
                        if refresh_owner(&mut runtime, &mut dbus).await {
                            dbus_failure_logged = false;
                            dispatch_actions(
                                runtime.handle_dbus_message(&message),
                                &services,
                            );
                        } else {
                            dbus_failure_logged = true;
                        }
                    }
                    Ok(None) => {
                        tracing::warn!(event = "inputd_dbus_stream_ended", "DBus signal stream ended");
                        dbus = None;
                        dbus_failure_logged = true;
                        runtime.set_dbus_owner(None);
                    }
                    Err(error) => {
                        tracing::warn!(
                            event = "inputd_dbus_stream_failed",
                            error = %error,
                            "DBus signal stream failed"
                        );
                        dbus = None;
                        dbus_failure_logged = true;
                        runtime.set_dbus_owner(None);
                    }
                }
            }
            result = runtime.next_evdev_actions(), if has_evdev => {
                match result {
                    Ok(Some(matched)) => {
                        dispatch_actions(
                            matched,
                            &services,
                        )
                    }
                    Ok(None) => tracing::warn!(
                        event = "inputd_evdev_stream_ended",
                        "normalized target event stream ended"
                    ),
                    Err(error) => tracing::warn!(
                        event = "inputd_evdev_stream_failed",
                        error_kind = ?error.kind(),
                        "normalized target event stream failed"
                    ),
                }
            }
        }
        let updates = runtime.take_updates();
        let delivered = match updates {
            Ok(updates) => match &producer {
                Some(channel) => updates
                    .into_iter()
                    .all(|update| channel.send(update).is_ok()),
                // Explicit development capture has no effects or destination.
                None => services.korrid.is_none(),
            },
            Err(_) => false,
        };
        if !delivered {
            producer = None;
            runtime.transport_lost();
        }
        if let Err(error) = health.publish(RuntimeHealth::from(runtime.state())) {
            tracing::warn!(
                event = "inputd_health_publish_failed",
                error = %error,
                "systemd health publication failed"
            );
        }
    }
}

async fn next_producer_reply(
    source: &mut Option<PrivateInputChannel>,
) -> Option<korri_input_contract::SeatReply> {
    source
        .as_mut()
        .expect("producer branch only while connected")
        .next()
        .await
}

async fn next_dbus_message(
    source: &mut Option<DbusSignalSource>,
) -> zbus::Result<Option<zbus::Message>> {
    source
        .as_mut()
        .expect("DBus branch is enabled only while connected")
        .next_message()
        .await
}

async fn ensure_runtime_profile(
    runtime: &mut Runtime,
    source: &mut Option<DbusSignalSource>,
    profile_path: Option<&std::path::Path>,
    wait_logged: &mut bool,
) -> ProfileStatus {
    let Some(profile_path) = profile_path else {
        return ProfileStatus::Ready;
    };
    let Some(connected) = source.as_ref() else {
        runtime.set_dbus_owner(None);
        return ProfileStatus::Pending;
    };
    match connected.ensure_capture_profiles(profile_path).await {
        Ok(ProfileStatus::Ready) => {
            if *wait_logged {
                tracing::info!(
                    event = "inputd_profile_ready",
                    profile = %profile_path.display(),
                    "immutable InputPlumber profile is ready"
                );
            }
            *wait_logged = false;
            ProfileStatus::Ready
        }
        Ok(ProfileStatus::Applied) => {
            tracing::info!(
                event = "inputd_profile_applied",
                profile = %profile_path.display(),
                "loaded immutable Korri profile into the supported composite"
            );
            *wait_logged = false;
            ProfileStatus::Applied
        }
        Ok(ProfileStatus::MissingSource) => {
            *wait_logged = false;
            ProfileStatus::MissingSource
        }
        Ok(ProfileStatus::AmbiguousSources) => {
            *wait_logged = false;
            ProfileStatus::AmbiguousSources
        }
        Ok(ProfileStatus::Pending) => {
            if !*wait_logged {
                tracing::info!(
                    event = "inputd_profile_pending",
                    profile = %profile_path.display(),
                    "waiting for validated InputPlumber composites"
                );
            }
            *wait_logged = true;
            runtime.set_dbus_owner(None);
            ProfileStatus::Pending
        }
        Err(error) => {
            if !*wait_logged {
                tracing::warn!(
                    event = "inputd_profile_rejected",
                    profile = %profile_path.display(),
                    error = %error,
                    "InputPlumber profile selection failed closed"
                );
            }
            *wait_logged = true;
            runtime.set_dbus_owner(None);
            if error.is_transport_failure() {
                *source = None;
            }
            ProfileStatus::Pending
        }
    }
}

async fn refresh_owner(runtime: &mut Runtime, source: &mut Option<DbusSignalSource>) -> bool {
    let Some(connected) = source.as_ref() else {
        runtime.set_dbus_owner(None);
        return false;
    };
    match connected.current_owner().await {
        Ok(owner) => {
            runtime.set_dbus_owner(owner.as_deref());
            true
        }
        Err(error) => {
            tracing::warn!(
                event = "inputd_dbus_owner_query_failed",
                error = %error,
                "could not authenticate the InputPlumber DBus owner"
            );
            *source = None;
            runtime.set_dbus_owner(None);
            false
        }
    }
}

fn dispatch_actions(matched: Vec<RuntimeAction>, services: &ConfiguredServices) {
    for action in matched {
        if action.id == ActionId::ControllerActivity {
            // Separate permit and short timeout: a stalled compositor must not
            // occupy a volume/Home action slot. Runtime coalesces to <= 1 Hz.
            if let Some(dispatcher) = services.activity.clone() {
                tokio::spawn(async move {
                    let outcome = dispatcher.dispatch(ActionId::ControllerActivity).await;
                    if !matches!(outcome, ActionOutcome::Completed(_)) {
                        log_action_outcome(ActionId::ControllerActivity, outcome);
                    }
                });
            }
            continue;
        }
        tracing::info!(
            event = "inputd_policy_match",
            action = %action.id,
            dispatch_mode = ?action.dispatch_mode,
            "input policy matched"
        );
        let Some(dispatcher) = services.actions.as_ref() else {
            tracing::info!(
                event = "inputd_development_action_suppressed",
                action = %action.id,
                "development profile does not perform actions"
            );
            continue;
        };
        if action.dispatch_mode == DispatchMode::ExactStop {
            let client = services
                .korrid
                .as_ref()
                .expect("hardened actions always configure exact local control")
                .clone();
            tokio::spawn(async move {
                match client.stop_active_exact().await {
                    Ok(outcome) => log_stop_outcome(outcome),
                    Err(error) => tracing::warn!(
                        event = "inputd_exact_stop_failed",
                        error = %error,
                        "exact stop failed without fallback"
                    ),
                }
            });
            continue;
        }
        if action.dispatch_mode == DispatchMode::ExactPanel {
            let client = services
                .korrid
                .as_ref()
                .expect("hardened actions always configure exact local control")
                .clone();
            let dispatcher = dispatcher.clone();
            tokio::spawn(async move {
                let outcome = client
                    .toggle_panel_exact_with(|| async {
                        // HostSessionControl::set_freezer acknowledges receiver
                        // route(None) before this exact-freeze RPC succeeds.
                        // Its shared seats remain neutral while Portal owns input;
                        // inputd must not inject a second routing decision.
                        let focus = dispatcher.dispatch(action.id).await;
                        let succeeded = portal_focus_succeeded(&focus);
                        log_action_outcome(action.id, focus);
                        succeeded
                    })
                    .await;
                match outcome {
                    Ok(outcome) => {
                        log_panel_outcome(outcome);
                    }
                    Err(error) => {
                        // No focus effect follows an uncertain transition.
                        // Korrid retains the sole receiver routing authority.
                        tracing::warn!(
                            event = "inputd_exact_panel_failed",
                            error = %error,
                            "exact gameplay overlay toggle failed without fallback"
                        )
                    }
                }
            });
            continue;
        }

        let dispatcher = dispatcher.clone();
        tokio::spawn(async move {
            let action_id = action.id;
            log_action_outcome(action_id, dispatcher.dispatch(action_id).await);
        });
    }
}

fn log_action_outcome(action_id: korri_inputd::actions::ActionId, outcome: ActionOutcome) {
    match outcome {
        ActionOutcome::Unconfigured => tracing::warn!(
            event = "inputd_action_unconfigured", action = %action_id,
            "input action has no configured command"
        ),
        ActionOutcome::ConcurrencyLimited => tracing::warn!(
            event = "inputd_action_concurrency_limited", action = %action_id,
            "input action was rejected at the concurrency limit"
        ),
        ActionOutcome::Completed(output) => tracing::info!(
            event = "inputd_action_completed", action = %action_id,
            stdout_bytes = output.stdout.len(), stderr_bytes = output.stderr.len(),
            stdout_truncated = output.stdout_truncated,
            stderr_truncated = output.stderr_truncated,
            "input action completed"
        ),
        ActionOutcome::Failed(output) => tracing::warn!(
            event = "inputd_action_failed", action = %action_id,
            status = ?output.status, stdout_bytes = output.stdout.len(),
            stderr_bytes = output.stderr.len(), "input action failed without retry"
        ),
        ActionOutcome::TimedOut(output) => tracing::warn!(
            event = "inputd_action_timed_out", action = %action_id,
            stdout_bytes = output.stdout.len(), stderr_bytes = output.stderr.len(),
            "input action exceeded its runtime limit"
        ),
        ActionOutcome::SpawnFailed(error) => tracing::warn!(
            event = "inputd_action_spawn_failed", action = %action_id, error,
            "input action child was rejected"
        ),
        ActionOutcome::ContainmentFailed(error) => tracing::error!(
            event = "inputd_action_containment_failed", action = %action_id, error,
            "input action containment failed closed"
        ),
    }
}

fn portal_focus_succeeded(outcome: &ActionOutcome) -> bool {
    matches!(outcome, ActionOutcome::Completed(_))
}

fn log_panel_outcome(outcome: ExactPanelOutcome) {
    let outcome = match outcome {
        ExactPanelOutcome::Opened => "opened",
        ExactPanelOutcome::Returned => "returned",
        ExactPanelOutcome::FocusFailed => "focus-failed",
        ExactPanelOutcome::LeaveRefused => "leave-refused",
        ExactPanelOutcome::NoActive => "no-active",
        ExactPanelOutcome::AlreadyStopping => "already-stopping",
        ExactPanelOutcome::RecoveryBlocked => "recovery-blocked",
    };
    tracing::info!(
        event = "inputd_exact_panel_outcome",
        outcome,
        "exact gameplay overlay toggle completed"
    );
}

fn log_stop_outcome(outcome: ExactStopOutcome) {
    let outcome = match outcome {
        ExactStopOutcome::NoActive => "no-active",
        ExactStopOutcome::StaleIdentity => "stale",
        ExactStopOutcome::AlreadyStopping => "already-stopping",
        ExactStopOutcome::Completed => "completed",
        ExactStopOutcome::RecoveryBlocked => "recovery-blocked",
    };
    tracing::info!(
        event = "inputd_exact_stop_outcome",
        outcome,
        "exact stop request completed"
    );
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RuntimeProfile {
    Hardened,
    Development,
}

impl RuntimeProfile {
    fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.unwrap_or("hardened") {
            "hardened" => Ok(Self::Hardened),
            "development" => Ok(Self::Development),
            other => Err(format!(
                "KORRI_INPUTD_PROFILE must be hardened or development, got {other:?}"
            )),
        }
    }
}

struct ConfiguredServices {
    actions: Option<ActionDispatcher>,
    activity: Option<ActionDispatcher>,
    routes: ActionRoutes,
    korrid: Option<KorridClient>,
    physical_input: bool,
    profile_path: Option<PathBuf>,
}

fn configured_services() -> Result<ConfiguredServices, String> {
    let environment = std::env::vars_os().collect::<BTreeMap<OsString, OsString>>();
    configured_services_from_environment(&environment)
}

fn configured_services_from_environment(
    environment: &BTreeMap<OsString, OsString>,
) -> Result<ConfiguredServices, String> {
    let profile = RuntimeProfile::parse(
        environment
            .get(std::ffi::OsStr::new("KORRI_INPUTD_PROFILE"))
            .and_then(|value| value.to_str()),
    )?;
    let (commands, routes) =
        commands_from_environment(environment).map_err(|error| error.to_string())?;
    if profile == RuntimeProfile::Development {
        if !commands.is_empty() {
            return Err("development profile cannot configure action commands".into());
        }
        let physical_input = match environment
            .get(std::ffi::OsStr::new("KORRI_INPUTD_SOURCE"))
            .and_then(|value| value.to_str())
            .unwrap_or("disabled")
        {
            "disabled" => false,
            "physical" => true,
            other => {
                return Err(format!(
                    "KORRI_INPUTD_SOURCE must be disabled or physical, got {other:?}"
                ))
            }
        };
        if environment.contains_key(std::ffi::OsStr::new("KORRI_INPUTD_PROFILE_PATH")) {
            return Err("development profile cannot load an InputPlumber profile".into());
        }
        return Ok(ConfiguredServices {
            actions: None,
            activity: None,
            routes,
            korrid: None,
            physical_input,
            profile_path: None,
        });
    }
    if let Some(source) = environment
        .get(std::ffi::OsStr::new("KORRI_INPUTD_SOURCE"))
        .and_then(|value| value.to_str())
    {
        if source != "physical" {
            return Err("hardened profile requires physical input".into());
        }
    }
    let identity = ActionIdentity {
        uid: required_unprivileged_id("KORRI_INPUTD_ACTION_UID", environment)?,
        gid: required_unprivileged_id("KORRI_INPUTD_ACTION_GID", environment)?,
        control_gid: required_unprivileged_id("KORRI_INPUTD_CONTROL_GID", environment)?,
    };
    if unsafe { libc::getegid() } != identity.control_gid {
        return Err("inputd primary GID does not match KORRI_INPUTD_CONTROL_GID".into());
    }
    let activity = commands
        .get(ActionId::ControllerActivity)
        .cloned()
        .map(|command| {
            let mut activity_commands = ActionCommands::default();
            activity_commands.insert(ActionId::ControllerActivity, command);
            ActionDispatcher::new(
                activity_commands,
                identity,
                ActionLimits {
                    max_concurrency: 1,
                    timeout: Duration::from_millis(500),
                    max_output_bytes: 1024,
                },
            )
        })
        .transpose()
        .map_err(|error| error.to_string())?;
    let dispatcher = ActionDispatcher::new(commands, identity, ActionLimits::default())
        .map_err(|error| error.to_string())?;
    let socket = required_absolute_path("KORRI_INPUTD_CONTROL_SOCKET", environment)?;
    let profile_path = required_immutable_profile_path(environment)?;
    Ok(ConfiguredServices {
        actions: Some(dispatcher),
        activity,
        routes,
        korrid: Some(KorridClient::new(socket)),
        physical_input: true,
        profile_path: Some(profile_path),
    })
}

fn required_unprivileged_id(
    name: &str,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<u32, String> {
    let value = environment
        .get(std::ffi::OsStr::new(name))
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("{name} must be configured"))?;
    let id = value
        .parse::<u32>()
        .map_err(|_| format!("{name} must be a numeric ID"))?;
    if id == 0 {
        return Err(format!("{name} must identify an unprivileged account"));
    }
    Ok(id)
}

fn required_absolute_path(
    name: &str,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<PathBuf, String> {
    let path = environment
        .get(std::ffi::OsStr::new(name))
        .map(PathBuf::from)
        .ok_or_else(|| format!("{name} must be configured"))?;
    if !path.is_absolute() {
        return Err(format!("{name} must be absolute"));
    }
    Ok(path)
}

fn required_immutable_profile_path(
    environment: &BTreeMap<OsString, OsString>,
) -> Result<PathBuf, String> {
    required_immutable_profile_path_at(environment, std::path::Path::new(STORE_ROOT))
}

fn required_immutable_profile_path_at(
    environment: &BTreeMap<OsString, OsString>,
    store_root: &std::path::Path,
) -> Result<PathBuf, String> {
    let configured = required_absolute_path("KORRI_INPUTD_PROFILE_PATH", environment)?;
    let profile = std::fs::canonicalize(&configured)
        .map_err(|error| format!("KORRI_INPUTD_PROFILE_PATH is unavailable: {error}"))?;
    if configured != profile {
        return Err("KORRI_INPUTD_PROFILE_PATH must be a canonical immutable path".into());
    }
    if !is_inside_store_item(&profile, store_root) {
        return Err("KORRI_INPUTD_PROFILE_PATH must resolve inside the Nix store".into());
    }
    let metadata = std::fs::metadata(&profile)
        .map_err(|error| format!("KORRI_INPUTD_PROFILE_PATH is unavailable: {error}"))?;
    if !metadata.is_file()
        || profile.file_name().and_then(|name| name.to_str()) != Some(SUPPORTED_PROFILE_NAME)
    {
        return Err(format!(
            "KORRI_INPUTD_PROFILE_PATH must select {SUPPORTED_PROFILE_NAME}"
        ));
    }
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ConfigurableHealthPublisher {
        initialization: std::io::Result<()>,
    }

    impl HealthPublisher for ConfigurableHealthPublisher {
        fn initialized(&mut self, _health: RuntimeHealth) -> std::io::Result<()> {
            std::mem::replace(&mut self.initialization, Ok(()))
        }

        fn publish(&mut self, _health: RuntimeHealth) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn only_a_completed_portal_action_allows_a_frozen_leave_to_remain_in_portal() {
        use korri_inputd::actions::ActionOutput;
        use std::os::unix::process::ExitStatusExt;

        let output = |raw| ActionOutput {
            status: Some(std::process::ExitStatus::from_raw(raw)),
            stdout: Vec::new(),
            stderr: Vec::new(),
            stdout_truncated: false,
            stderr_truncated: false,
        };
        assert!(portal_focus_succeeded(&ActionOutcome::Completed(output(0))));
        for outcome in [
            ActionOutcome::Unconfigured,
            ActionOutcome::Failed(output(256)),
            ActionOutcome::TimedOut(output(256)),
            ActionOutcome::SpawnFailed("spawn refused".into()),
        ] {
            assert!(!portal_focus_succeeded(&outcome));
        }
    }

    #[test]
    fn runtime_profile_defaults_to_hardened_and_rejects_unknown_values() {
        assert_eq!(RuntimeProfile::parse(None), Ok(RuntimeProfile::Hardened));
        assert_eq!(
            RuntimeProfile::parse(Some("development")),
            Ok(RuntimeProfile::Development)
        );
        assert!(RuntimeProfile::parse(Some("unsafe")).is_err());
    }

    #[test]
    fn development_profile_needs_no_service_credentials_or_cgroup() {
        let environment = BTreeMap::from([(
            OsString::from("KORRI_INPUTD_PROFILE"),
            OsString::from("development"),
        )]);

        let services = configured_services_from_environment(&environment).unwrap();

        assert!(services.actions.is_none());
        assert!(services.korrid.is_none());
        assert!(!services.physical_input);
        assert!(services.profile_path.is_none());
    }

    #[test]
    fn development_profile_requires_an_explicit_physical_input_opt_in() {
        let environment = BTreeMap::from([
            (
                OsString::from("KORRI_INPUTD_PROFILE"),
                OsString::from("development"),
            ),
            (
                OsString::from("KORRI_INPUTD_SOURCE"),
                OsString::from("physical"),
            ),
        ]);

        let services = configured_services_from_environment(&environment).unwrap();

        assert!(services.physical_input);
    }

    #[test]
    fn development_profile_rejects_an_inputplumber_profile_path() {
        let environment = BTreeMap::from([
            (
                OsString::from("KORRI_INPUTD_PROFILE"),
                OsString::from("development"),
            ),
            (
                OsString::from("KORRI_INPUTD_PROFILE_PATH"),
                OsString::from("/nix/store/not-used-in-development"),
            ),
        ]);

        let error = configured_services_from_environment(&environment)
            .err()
            .expect("development profile mutation must be rejected");

        assert_eq!(
            error,
            "development profile cannot load an InputPlumber profile"
        );
    }

    #[test]
    fn hardened_profile_path_must_be_canonical_and_immutable() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let store = root.path().join("store");
        let package = store.join("package");
        let profile = package
            .join("share/inputplumber/profiles")
            .join(SUPPORTED_PROFILE_NAME);
        std::fs::create_dir_all(profile.parent().unwrap()).unwrap();
        std::fs::write(&profile, b"profile").unwrap();
        let environment = BTreeMap::from([(
            OsString::from("KORRI_INPUTD_PROFILE_PATH"),
            profile.clone().into_os_string(),
        )]);

        assert_eq!(
            required_immutable_profile_path_at(&environment, &store).unwrap(),
            profile
        );

        let selector = root.path().join("profile-link");
        symlink(&profile, &selector).unwrap();
        let linked = BTreeMap::from([(
            OsString::from("KORRI_INPUTD_PROFILE_PATH"),
            selector.into_os_string(),
        )]);
        assert_eq!(
            required_immutable_profile_path_at(&linked, &store).unwrap_err(),
            "KORRI_INPUTD_PROFILE_PATH must be a canonical immutable path"
        );
    }

    #[test]
    fn development_profile_rejects_configured_action_commands() {
        use korri_inputd::actions::{action_entry, ActionId};

        let executable = std::fs::canonicalize("/run/current-system/sw/bin/true").unwrap();
        let command = serde_json::json!({
            "executable": executable,
            "argv": [],
            "environment": {}
        });
        let environment = BTreeMap::from([
            (
                OsString::from("KORRI_INPUTD_PROFILE"),
                OsString::from("development"),
            ),
            (
                OsString::from(action_entry(ActionId::WorkspaceNext).legacy_environment_name),
                OsString::from(command.to_string()),
            ),
        ]);

        let error = configured_services_from_environment(&environment)
            .err()
            .expect("development command must be rejected");

        assert_eq!(
            error,
            "development profile cannot configure action commands"
        );
    }

    #[test]
    fn initial_ready_publication_failure_is_fatal_to_startup() {
        let mut health = ConfigurableHealthPublisher {
            initialization: Err(std::io::Error::other("notify socket rejected READY")),
        };

        let error = initialize_health(&mut health).unwrap_err();

        assert_eq!(error.to_string(), "notify socket rejected READY");
    }
}
