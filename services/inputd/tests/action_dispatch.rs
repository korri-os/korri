use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    io::Read,
    os::fd::AsRawFd,
    os::unix::net::UnixStream,
    path::PathBuf,
    time::Duration,
};

use korri_inputd::{
    actions::{
        action_entry, commands_from_environment, ActionCommand, ActionConfigError,
        ActionDispatcher, ActionId, ActionIdentity, ActionLimits, ActionOutcome, ACTION_CATALOG,
    },
    cgroup_sandbox::{ActionCgroup, ActionCgroupBackend},
};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct ProcessCgroups {
    processes: Mutex<BTreeMap<String, Vec<u32>>>,
}

impl ActionCgroupBackend for ProcessCgroups {
    fn create(&self, action: ActionId) -> std::io::Result<ActionCgroup> {
        let name = format!(
            "{}-{}",
            action.as_str(),
            self.processes.lock().unwrap().len()
        );
        self.processes
            .lock()
            .unwrap()
            .insert(name.clone(), Vec::new());
        ActionCgroup::for_backend(name)
    }

    fn attach(&self, cgroup: &ActionCgroup, pid: u32) -> std::io::Result<()> {
        self.processes
            .lock()
            .unwrap()
            .get_mut(cgroup.backend_name())
            .unwrap()
            .push(pid);
        Ok(())
    }

    fn contains(&self, cgroup: &ActionCgroup, pid: u32) -> std::io::Result<bool> {
        Ok(self.processes.lock().unwrap()[cgroup.backend_name()].contains(&pid))
    }

    fn kill(&self, cgroup: &ActionCgroup) -> std::io::Result<()> {
        for pid in &self.processes.lock().unwrap()[cgroup.backend_name()] {
            unsafe {
                libc::kill(*pid as i32, libc::SIGKILL);
            }
        }
        Ok(())
    }

    fn populated(&self, cgroup: &ActionCgroup) -> std::io::Result<bool> {
        Ok(self.processes.lock().unwrap()[cgroup.backend_name()]
            .iter()
            .any(|pid| std::path::Path::new(&format!("/proc/{pid}")).exists()))
    }

    fn remove(&self, cgroup: &ActionCgroup) -> std::io::Result<()> {
        self.processes.lock().unwrap().remove(cgroup.backend_name());
        Ok(())
    }
}

struct RejectingCgroups;

impl ActionCgroupBackend for RejectingCgroups {
    fn create(&self, _action: ActionId) -> std::io::Result<ActionCgroup> {
        ActionCgroup::for_backend("rejected-1")
    }
    fn attach(&self, _cgroup: &ActionCgroup, _pid: u32) -> std::io::Result<()> {
        Ok(())
    }
    fn contains(&self, _cgroup: &ActionCgroup, _pid: u32) -> std::io::Result<bool> {
        Ok(false)
    }
    fn kill(&self, _cgroup: &ActionCgroup) -> std::io::Result<()> {
        Ok(())
    }
    fn populated(&self, _cgroup: &ActionCgroup) -> std::io::Result<bool> {
        Ok(false)
    }
    fn remove(&self, _cgroup: &ActionCgroup) -> std::io::Result<()> {
        Ok(())
    }
}

fn dispatcher(
    commands: korri_inputd::actions::ActionCommands,
    identity: ActionIdentity,
    limits: ActionLimits,
) -> Result<ActionDispatcher, ActionConfigError> {
    ActionDispatcher::with_cgroup_backend(
        commands,
        identity,
        limits,
        Arc::new(ProcessCgroups::default()),
    )
}

fn identity_requiring_privilege_drop() -> ActionIdentity {
    let uid = unsafe { libc::geteuid() };
    let control_gid = unsafe { libc::getegid() };
    if uid == 0 {
        ActionIdentity {
            uid: std::env::var("SUDO_UID")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(65_534),
            gid: std::env::var("SUDO_GID")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(65_534),
            control_gid,
        }
    } else {
        ActionIdentity {
            uid,
            gid: control_gid.saturating_add(10_000),
            control_gid,
        }
    }
}

#[test]
fn legacy_action_vocabulary_has_stable_configuration_names() {
    assert_eq!(ACTION_CATALOG.len(), 19);
    assert_eq!(
        action_entry(ActionId::SystemPanel).legacy_environment_name,
        "KORRI_INPUTD_SYSTEM_PANEL"
    );
    assert_eq!(
        action_entry(ActionId::ToggleBottomKeyboard).legacy_environment_name,
        "KORRI_INPUTD_BOTTOM_KEYBOARD"
    );
    assert!(ActionId::try_from("unknown").is_err());
}

#[test]
fn commands_require_absolute_executables_and_explicit_argv() {
    assert!(matches!(
        ActionCommand::new("swaymsg", [], BTreeMap::new()),
        Err(ActionConfigError::ExecutableNotAbsolute(_))
    ));

    let mut environment = BTreeMap::new();
    environment.insert(
        OsString::from("KORRI_INPUTD_SYSTEM_PANEL"),
        OsString::from("/bin/tool --flag"),
    );
    assert!(matches!(
        commands_from_environment(&environment),
        Err(ActionConfigError::CommandIsNotExplicitArgv)
    ));

    let executable = std::fs::canonicalize("/run/current-system/sw/bin/true").unwrap();
    environment.insert(
        OsString::from("KORRI_INPUTD_SYSTEM_PANEL"),
        OsString::from(
            serde_json::json!({
                "executable": executable,
                "argv": ["--flag", "value with spaces"],
                "environment": {"SWAYSOCK": "/run/user/100/sway.sock"}
            })
            .to_string(),
        ),
    );
    let (commands, _) = commands_from_environment(&environment).unwrap();
    let command = commands.get(ActionId::SystemPanel).unwrap();
    assert_eq!(command.executable(), executable);
    assert_eq!(
        command.argv(),
        [
            OsString::from("--flag"),
            OsString::from("value with spaces")
        ]
    );
    assert_eq!(
        command.environment().get(OsStr::new("SWAYSOCK")),
        Some(&OsString::from("/run/user/100/sway.sock"))
    );
}

#[test]
fn immutable_executable_validation_rejects_dot_and_parent_traversal() {
    let executable = std::fs::canonicalize("/run/current-system/sw/bin/true").unwrap();
    let item = executable.ancestors().nth(2).unwrap();
    for traversed in [
        PathBuf::from(format!("{}/bin/./true", item.display())),
        PathBuf::from(format!("{}/bin/../bin/true", item.display())),
    ] {
        let environment = BTreeMap::from([(
            OsString::from("KORRI_INPUTD_SYSTEM_PANEL"),
            OsString::from(
                serde_json::json!({"executable": traversed, "argv": [], "environment": {}})
                    .to_string(),
            ),
        )]);
        assert!(matches!(
            commands_from_environment(&environment),
            Err(ActionConfigError::ExecutableTraversal(_))
        ));
    }
}

#[test]
fn immutable_executable_must_resolve_to_a_regular_executable_in_the_same_store_item() {
    let executable = std::fs::canonicalize("/run/current-system/sw/bin/true").unwrap();
    let item = executable.ancestors().nth(2).unwrap();
    let environment = BTreeMap::from([(
        OsString::from("KORRI_INPUTD_SYSTEM_PANEL"),
        OsString::from(
            serde_json::json!({"executable": item.join("bin"), "argv": [], "environment": {}})
                .to_string(),
        ),
    )]);
    assert!(matches!(
        commands_from_environment(&environment),
        Err(ActionConfigError::ExecutableNotRegular(_))
    ));
}

#[test]
fn configured_action_matrix_accepts_only_currently_reachable_routes() {
    let executable = std::fs::canonicalize("/run/current-system/sw/bin/true").unwrap();
    let command =
        serde_json::json!({"executable": executable, "argv": [], "environment": {}}).to_string();
    let mut environment = BTreeMap::from([(
        OsString::from("KORRI_INPUTD_BACK_TAP_ACTION"),
        OsString::from("toggle-steam-visibility"),
    )]);
    for entry in ACTION_CATALOG {
        if entry.dispatch_mode == korri_inputd::actions::DispatchMode::Direct
            && !matches!(
                entry.trigger,
                korri_inputd::action_catalog::Trigger::Unsupported
            )
        {
            environment.insert(
                OsString::from(entry.legacy_environment_name),
                OsString::from(&command),
            );
        }
    }
    let (commands, routes) = commands_from_environment(&environment).unwrap();
    let accepted = commands
        .configured_ids()
        .collect::<std::collections::BTreeSet<_>>();
    let reachable = ACTION_CATALOG
        .iter()
        .filter(|entry| entry.dispatch_mode == korri_inputd::actions::DispatchMode::Direct)
        .filter(|entry| routes.is_reachable(entry))
        .map(|entry| entry.id)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(accepted, reachable);

    for id in [
        ActionId::PowerSuspend,
        ActionId::LidClosed,
        ActionId::LidOpened,
    ] {
        let entry = action_entry(id);
        let rejected = BTreeMap::from([(
            OsString::from(entry.legacy_environment_name),
            OsString::from(&command),
        )]);
        assert!(matches!(
            commands_from_environment(&rejected),
            Err(ActionConfigError::UnsupportedConfiguredAction(rejected_id)) if rejected_id == id
        ));
    }
}

#[test]
fn destructive_command_override_is_rejected() {
    let mut environment = BTreeMap::new();
    environment.insert(
        OsString::from("KORRI_INPUTD_KILL_CURRENT_GAME"),
        OsString::from(r#"["/bin/killall","game"]"#),
    );
    assert!(matches!(
        commands_from_environment(&environment),
        Err(ActionConfigError::DestructiveCommandOverride)
    ));
}

#[test]
fn environment_is_an_explicit_allowlist() {
    let mut allowed = BTreeMap::new();
    allowed.insert(
        OsString::from("SWAYSOCK"),
        OsString::from("/run/user/100/sway.sock"),
    );
    let executable = std::fs::canonicalize("/run/current-system/sw/bin/true").unwrap();
    let command = ActionCommand::new(&executable, [], allowed.clone()).unwrap();

    assert_eq!(command.environment(), &allowed);
    assert!(!command.environment().contains_key(OsStr::new("HOME")));
    assert!(matches!(
        ActionCommand::new(
            executable,
            [],
            BTreeMap::from([(OsString::from("BAD=NAME"), OsString::from("value"))]),
        ),
        Err(ActionConfigError::InvalidEnvironmentName)
    ));
}

#[tokio::test]
async fn untrusted_exec_never_begins_when_cgroup_membership_cannot_be_proven() {
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("executed");
    let executable = std::fs::canonicalize("/run/current-system/sw/bin/sh").unwrap();
    let command = ActionCommand::new(
        executable,
        [
            OsString::from("-c"),
            OsString::from(format!("printf executed > {}", marker.display())),
        ],
        BTreeMap::new(),
    )
    .unwrap();
    let mut commands = korri_inputd::actions::ActionCommands::default();
    commands.insert(ActionId::WorkspaceNext, command);
    let outcome = ActionDispatcher::with_cgroup_backend(
        commands,
        identity_requiring_privilege_drop(),
        ActionLimits::default(),
        Arc::new(RejectingCgroups),
    )
    .unwrap()
    .dispatch(ActionId::WorkspaceNext)
    .await;

    assert!(matches!(outcome, ActionOutcome::ContainmentFailed(_)));
    assert!(!marker.exists());
}

#[tokio::test]
async fn unconfigured_action_is_a_bounded_no_op() {
    let dispatcher = dispatcher(
        Default::default(),
        identity_requiring_privilege_drop(),
        ActionLimits::default(),
    )
    .unwrap();

    assert_eq!(
        dispatcher.dispatch(ActionId::WorkspaceNext).await,
        ActionOutcome::Unconfigured
    );
}

#[test]
fn action_child_process_contract_probe() {
    let Some(path) = std::env::var_os("KORRI_TEST_CONTROL_SOCKET") else {
        return;
    };
    let inherited_fd = std::env::var("KORRI_TEST_INHERITED_FD")
        .unwrap()
        .parse::<i32>()
        .unwrap();
    assert!(std::fs::metadata(format!("/proc/self/fd/{inherited_fd}")).is_err());

    let mut stream = UnixStream::connect(path).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(250)))
        .unwrap();
    let mut response = [0_u8; 1];
    assert_eq!(stream.read(&mut response).unwrap(), 0);
}

#[tokio::test]
async fn action_child_cannot_retain_or_reopen_local_control_authority() {
    let root = tempfile::tempdir().unwrap();
    let socket_path = root.path().join("control.sock");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let listener = tokio::net::UnixListener::bind(&socket_path).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o777)).unwrap();
    }
    let expected_uid = unsafe { libc::geteuid() };
    let expected_gid = unsafe { libc::getegid() };
    let server = tokio::spawn(async move {
        let accepted = tokio::time::timeout(Duration::from_millis(500), listener.accept()).await;
        let Ok(Ok((stream, _))) = accepted else {
            return None;
        };
        let credentials = stream.peer_cred().unwrap();
        if credentials.uid() == expected_uid && credentials.gid() == expected_gid {
            use tokio::io::AsyncWriteExt;
            let mut stream = stream;
            stream.write_all(b"A").await.unwrap();
        }
        Some((credentials.uid(), credentials.gid()))
    });

    let (retained, _peer) = UnixStream::pair().unwrap();
    let fd = retained.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    assert!(flags >= 0);
    assert_eq!(
        unsafe { libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) },
        0
    );

    let test_executable = std::env::current_exe().unwrap();
    let executable = std::fs::canonicalize("/run/current-system/sw/bin/sh").unwrap();
    let environment = BTreeMap::from([
        (
            OsString::from("KORRI_TEST_CONTROL_SOCKET"),
            socket_path.as_os_str().to_owned(),
        ),
        (
            OsString::from("KORRI_TEST_INHERITED_FD"),
            OsString::from(fd.to_string()),
        ),
    ]);
    let command = ActionCommand::new(
        executable,
        [
            OsString::from("-c"),
            OsString::from("exec \"$1\" --exact action_child_process_contract_probe --nocapture"),
            OsString::from("action-probe"),
            test_executable.into_os_string(),
        ],
        environment,
    )
    .unwrap();
    let mut commands = korri_inputd::actions::ActionCommands::default();
    commands.insert(ActionId::WorkspaceNext, command);
    let identity = identity_requiring_privilege_drop();
    let dispatcher = dispatcher(
        commands,
        identity,
        ActionLimits {
            max_concurrency: 1,
            timeout: Duration::from_secs(2),
            max_output_bytes: 4096,
        },
    )
    .unwrap();

    let outcome = dispatcher.dispatch(ActionId::WorkspaceNext).await;
    let observed = server.await.unwrap();
    if unsafe { libc::geteuid() } == 0 {
        assert!(
            matches!(outcome, ActionOutcome::Completed(_)),
            "unexpected privileged action outcome: {outcome:?}"
        );
        assert_eq!(observed, Some((identity.uid, identity.gid)));
    } else {
        assert!(matches!(outcome, ActionOutcome::SpawnFailed(_)));
        assert_eq!(observed, None);
    }
}

#[tokio::test]
async fn child_fails_closed_when_group_drop_cannot_be_proven() {
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let executable = std::fs::canonicalize("/run/current-system/sw/bin/true").unwrap();
    let command = ActionCommand::new(executable, [], BTreeMap::new()).unwrap();
    let mut commands = korri_inputd::actions::ActionCommands::default();
    commands.insert(ActionId::WorkspaceNext, command);
    let dispatcher = dispatcher(
        commands,
        identity_requiring_privilege_drop(),
        ActionLimits {
            max_concurrency: 1,
            timeout: Duration::from_secs(1),
            max_output_bytes: 128,
        },
    )
    .unwrap();

    assert!(matches!(
        dispatcher.dispatch(ActionId::WorkspaceNext).await,
        ActionOutcome::SpawnFailed(_)
    ));
}
