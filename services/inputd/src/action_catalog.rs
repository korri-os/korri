use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fmt,
    os::unix::ffi::OsStrExt,
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
};

use korri_input_core::controls::Control;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ActionId {
    SystemPanel,
    KillCurrentGame,
    VolumeUp,
    VolumeDown,
    BrightnessUp,
    BrightnessDown,
    PowerSuspend,
    LidClosed,
    LidOpened,
    ScreenSwitch,
    ToggleBottomScreen,
    ToggleTopScreen,
    WorkspacePrev,
    WorkspaceNext,
    MoveOutputUp,
    MoveOutputDown,
    ToggleBottomKeyboard,
    ToggleSteamVisibility,
}

impl ActionId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SystemPanel => "system-panel",
            Self::KillCurrentGame => "kill-current-game",
            Self::VolumeUp => "volume-up",
            Self::VolumeDown => "volume-down",
            Self::BrightnessUp => "brightness-up",
            Self::BrightnessDown => "brightness-down",
            Self::PowerSuspend => "power-suspend",
            Self::LidClosed => "lid-closed",
            Self::LidOpened => "lid-opened",
            Self::ScreenSwitch => "screen-switch",
            Self::ToggleBottomScreen => "toggle-bottom-screen",
            Self::ToggleTopScreen => "toggle-top-screen",
            Self::WorkspacePrev => "workspace-prev",
            Self::WorkspaceNext => "workspace-next",
            Self::MoveOutputUp => "move-output-up",
            Self::MoveOutputDown => "move-output-down",
            Self::ToggleBottomKeyboard => "toggle-bottom-keyboard",
            Self::ToggleSteamVisibility => "toggle-steam-visibility",
        }
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<&str> for ActionId {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        ACTION_CATALOG
            .iter()
            .find(|entry| entry.id.as_str() == value)
            .map(|entry| entry.id)
            .ok_or(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchMode {
    Direct,
    ExactPanel,
    ExactStop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Chord {
    pub controls: &'static [Control],
    pub exact: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Trigger {
    Tap(Control),
    Press(Control),
    Chords(&'static [Chord]),
    ChordsAndConfiguredBackTap(&'static [Chord]),
    ConfiguredBackTap,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionCatalogEntry {
    pub id: ActionId,
    pub legacy_environment_name: &'static str,
    pub trigger: Trigger,
    pub dispatch_mode: DispatchMode,
}

const HOME_L1: &[Control] = &[Control::Home, Control::L1];
const HOME_DPAD_LEFT: &[Control] = &[Control::Home, Control::DpadLeft];
const HOME_R1: &[Control] = &[Control::Home, Control::R1];
const HOME_DPAD_RIGHT: &[Control] = &[Control::Home, Control::DpadRight];
const HOME_DPAD_UP: &[Control] = &[Control::Home, Control::DpadUp];
const HOME_DPAD_DOWN: &[Control] = &[Control::Home, Control::DpadDown];
const HOME_VOLUME_UP: &[Control] = &[Control::Home, Control::VolumeUp];
const HOME_VOLUME_DOWN: &[Control] = &[Control::Home, Control::VolumeDown];
const HOME_BACK: &[Control] = &[Control::Home, Control::Back];
const HOME_L3: &[Control] = &[Control::Home, Control::L3];
const HOME_R3: &[Control] = &[Control::Home, Control::R3];
const HOME_X: &[Control] = &[Control::Home, Control::X];
const KILL: &[Control] = &[Control::L1, Control::R1, Control::Start, Control::Select];

const WORKSPACE_PREV: &[Chord] = &[
    Chord {
        controls: HOME_DPAD_LEFT,
        exact: false,
    },
    Chord {
        controls: HOME_L1,
        exact: false,
    },
];
const WORKSPACE_NEXT: &[Chord] = &[
    Chord {
        controls: HOME_DPAD_RIGHT,
        exact: false,
    },
    Chord {
        controls: HOME_R1,
        exact: false,
    },
];
const BRIGHTNESS_UP: &[Chord] = &[Chord {
    controls: HOME_VOLUME_UP,
    exact: false,
}];
const BRIGHTNESS_DOWN: &[Chord] = &[Chord {
    controls: HOME_VOLUME_DOWN,
    exact: false,
}];
const MOVE_OUTPUT_UP: &[Chord] = &[Chord {
    controls: HOME_DPAD_UP,
    exact: false,
}];
const MOVE_OUTPUT_DOWN: &[Chord] = &[Chord {
    controls: HOME_DPAD_DOWN,
    exact: false,
}];
const SCREEN_SWITCH: &[Chord] = &[Chord {
    controls: HOME_BACK,
    exact: false,
}];
const TOGGLE_BOTTOM_SCREEN: &[Chord] = &[Chord {
    controls: HOME_L3,
    exact: true,
}];
const TOGGLE_TOP_SCREEN: &[Chord] = &[Chord {
    controls: HOME_R3,
    exact: true,
}];
const TOGGLE_BOTTOM_KEYBOARD: &[Chord] = &[Chord {
    controls: HOME_X,
    exact: false,
}];
const KILL_CURRENT_GAME: &[Chord] = &[Chord {
    controls: KILL,
    exact: true,
}];

pub const ACTION_CATALOG: &[ActionCatalogEntry] = &[
    entry(
        ActionId::SystemPanel,
        "KORRI_INPUTD_SYSTEM_PANEL",
        Trigger::Tap(Control::Home),
        DispatchMode::ExactPanel,
    ),
    entry(
        ActionId::KillCurrentGame,
        "KORRI_INPUTD_KILL_CURRENT_GAME",
        Trigger::Chords(KILL_CURRENT_GAME),
        DispatchMode::ExactStop,
    ),
    entry(
        ActionId::VolumeUp,
        "KORRI_INPUTD_VOLUME_UP",
        Trigger::Press(Control::VolumeUp),
        DispatchMode::Direct,
    ),
    entry(
        ActionId::VolumeDown,
        "KORRI_INPUTD_VOLUME_DOWN",
        Trigger::Press(Control::VolumeDown),
        DispatchMode::Direct,
    ),
    entry(
        ActionId::BrightnessUp,
        "KORRI_INPUTD_BRIGHTNESS_UP",
        Trigger::Chords(BRIGHTNESS_UP),
        DispatchMode::Direct,
    ),
    entry(
        ActionId::BrightnessDown,
        "KORRI_INPUTD_BRIGHTNESS_DOWN",
        Trigger::Chords(BRIGHTNESS_DOWN),
        DispatchMode::Direct,
    ),
    entry(
        ActionId::PowerSuspend,
        "KORRI_INPUTD_POWER_SUSPEND",
        Trigger::Unsupported,
        DispatchMode::Direct,
    ),
    entry(
        ActionId::LidClosed,
        "KORRI_INPUTD_LID_CLOSED",
        Trigger::Unsupported,
        DispatchMode::Direct,
    ),
    entry(
        ActionId::LidOpened,
        "KORRI_INPUTD_LID_OPENED",
        Trigger::Unsupported,
        DispatchMode::Direct,
    ),
    entry(
        ActionId::ScreenSwitch,
        "KORRI_INPUTD_SCREEN_SWITCH",
        Trigger::Chords(SCREEN_SWITCH),
        DispatchMode::Direct,
    ),
    entry(
        ActionId::ToggleBottomScreen,
        "KORRI_INPUTD_TOGGLE_BOTTOM_SCREEN",
        Trigger::Chords(TOGGLE_BOTTOM_SCREEN),
        DispatchMode::Direct,
    ),
    entry(
        ActionId::ToggleTopScreen,
        "KORRI_INPUTD_TOGGLE_TOP_SCREEN",
        Trigger::Chords(TOGGLE_TOP_SCREEN),
        DispatchMode::Direct,
    ),
    entry(
        ActionId::WorkspacePrev,
        "KORRI_INPUTD_WORKSPACE_PREV",
        Trigger::Chords(WORKSPACE_PREV),
        DispatchMode::Direct,
    ),
    entry(
        ActionId::WorkspaceNext,
        "KORRI_INPUTD_WORKSPACE_NEXT",
        Trigger::Chords(WORKSPACE_NEXT),
        DispatchMode::Direct,
    ),
    entry(
        ActionId::MoveOutputUp,
        "KORRI_INPUTD_MOVE_OUTPUT_UP",
        Trigger::Chords(MOVE_OUTPUT_UP),
        DispatchMode::Direct,
    ),
    entry(
        ActionId::MoveOutputDown,
        "KORRI_INPUTD_MOVE_OUTPUT_DOWN",
        Trigger::Chords(MOVE_OUTPUT_DOWN),
        DispatchMode::Direct,
    ),
    entry(
        ActionId::ToggleBottomKeyboard,
        "KORRI_INPUTD_BOTTOM_KEYBOARD",
        Trigger::ChordsAndConfiguredBackTap(TOGGLE_BOTTOM_KEYBOARD),
        DispatchMode::Direct,
    ),
    entry(
        ActionId::ToggleSteamVisibility,
        "KORRI_INPUTD_TOGGLE_STEAM_VISIBILITY",
        Trigger::ConfiguredBackTap,
        DispatchMode::Direct,
    ),
];

const fn entry(
    id: ActionId,
    legacy_environment_name: &'static str,
    trigger: Trigger,
    dispatch_mode: DispatchMode,
) -> ActionCatalogEntry {
    ActionCatalogEntry {
        id,
        legacy_environment_name,
        trigger,
        dispatch_mode,
    }
}

pub fn action_entry(id: ActionId) -> &'static ActionCatalogEntry {
    ACTION_CATALOG
        .iter()
        .find(|entry| entry.id == id)
        .expect("every ActionId is cataloged")
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ActionRoutes {
    pub back_tap: Option<ActionId>,
}

impl ActionRoutes {
    pub fn from_environment(
        environment: &BTreeMap<OsString, OsString>,
    ) -> Result<Self, ActionConfigError> {
        if environment.contains_key(OsStr::new("KORRI_INPUTD_KEY_F24_ACTION")) {
            return Err(ActionConfigError::UnsupportedTriggerConfiguration(
                "KORRI_INPUTD_KEY_F24_ACTION".into(),
            ));
        }
        let back_tap = match environment
            .get(OsStr::new("KORRI_INPUTD_BACK_TAP_ACTION"))
            .and_then(|value| value.to_str())
        {
            None => None,
            Some("toggle-bottom-keyboard") => Some(ActionId::ToggleBottomKeyboard),
            Some("toggle-steam-visibility") => Some(ActionId::ToggleSteamVisibility),
            Some(_) => {
                return Err(ActionConfigError::UnsupportedTriggerConfiguration(
                    "KORRI_INPUTD_BACK_TAP_ACTION".into(),
                ))
            }
        };
        Ok(Self { back_tap })
    }

    pub fn is_reachable(self, entry: &ActionCatalogEntry) -> bool {
        match entry.trigger {
            Trigger::Unsupported => false,
            Trigger::ConfiguredBackTap => self.back_tap == Some(entry.id),
            _ => true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionCommand {
    executable: PathBuf,
    argv: Vec<OsString>,
    environment: BTreeMap<OsString, OsString>,
}

impl ActionCommand {
    pub fn new(
        executable: impl Into<PathBuf>,
        argv: impl IntoIterator<Item = OsString>,
        environment: BTreeMap<OsString, OsString>,
    ) -> Result<Self, ActionConfigError> {
        let executable = executable.into();
        if !executable.is_absolute() {
            return Err(ActionConfigError::ExecutableNotAbsolute(executable));
        }
        if executable.as_os_str().as_bytes().contains(&0) {
            return Err(ActionConfigError::ExecutableContainsNul);
        }
        for (name, value) in &environment {
            let name = name.as_bytes();
            if name.is_empty() || name.contains(&b'=') || name.contains(&0) {
                return Err(ActionConfigError::InvalidEnvironmentName);
            }
            if value.as_bytes().contains(&0) {
                return Err(ActionConfigError::InvalidEnvironmentValue);
            }
        }
        let argv = argv.into_iter().collect::<Vec<_>>();
        if argv.iter().any(|value| value.as_bytes().contains(&0)) {
            return Err(ActionConfigError::ArgumentContainsNul);
        }
        let executable = canonical_immutable_executable(&executable)?;
        Ok(Self {
            executable,
            argv,
            environment,
        })
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }
    pub fn argv(&self) -> &[OsString] {
        &self.argv
    }
    pub fn environment(&self) -> &BTreeMap<OsString, OsString> {
        &self.environment
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionConfigError {
    ExecutableNotAbsolute(PathBuf),
    ExecutableContainsNul,
    ArgumentContainsNul,
    InvalidEnvironmentName,
    InvalidEnvironmentValue,
    ZeroConcurrency,
    ZeroTimeout,
    ZeroOutputLimit,
    ControlGroupRetained,
    PrivilegedActionIdentity,
    DestructiveCommandOverride,
    CommandIsNotUtf8,
    CommandIsNotExplicitArgv,
    ExecutableNotImmutable(PathBuf),
    ExecutableTraversal(PathBuf),
    ExecutableNotRegular(PathBuf),
    ExecutableNotExecutable(PathBuf),
    UnsupportedConfiguredAction(ActionId),
    UnsupportedTriggerConfiguration(String),
    ContainmentUnavailable(String),
}

impl fmt::Display for ActionConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExecutableNotAbsolute(path) => write!(
                formatter,
                "action executable must be absolute: {}",
                path.display()
            ),
            Self::ExecutableContainsNul => formatter.write_str("action executable contains NUL"),
            Self::ArgumentContainsNul => formatter.write_str("action argument contains NUL"),
            Self::InvalidEnvironmentName => {
                formatter.write_str("action environment name is invalid")
            }
            Self::InvalidEnvironmentValue => {
                formatter.write_str("action environment value contains NUL")
            }
            Self::ZeroConcurrency => formatter.write_str("action concurrency must be positive"),
            Self::ZeroTimeout => formatter.write_str("action timeout must be positive"),
            Self::ZeroOutputLimit => formatter.write_str("action output limit must be positive"),
            Self::ControlGroupRetained => {
                formatter.write_str("action GID must differ from inputd's control GID")
            }
            Self::PrivilegedActionIdentity => {
                formatter.write_str("action UID and GID must be unprivileged")
            }
            Self::DestructiveCommandOverride => {
                formatter.write_str("kill-current-game cannot be configured as a command override")
            }
            Self::CommandIsNotUtf8 => formatter.write_str("action command must be UTF-8 JSON"),
            Self::CommandIsNotExplicitArgv => formatter
                .write_str("action command must contain a JSON executable, argv, and environment"),
            Self::ExecutableNotImmutable(path) => write!(
                formatter,
                "configured action executable must resolve within one /nix/store item: {}",
                path.display()
            ),
            Self::ExecutableTraversal(path) => write!(
                formatter,
                "configured action executable must not contain dot traversal: {}",
                path.display()
            ),
            Self::ExecutableNotRegular(path) => write!(
                formatter,
                "configured action executable must resolve to a regular file: {}",
                path.display()
            ),
            Self::ExecutableNotExecutable(path) => write!(
                formatter,
                "configured action executable is not executable: {}",
                path.display()
            ),
            Self::UnsupportedConfiguredAction(id) => write!(
                formatter,
                "configured action {id} has no current evdev or DBus trigger"
            ),
            Self::UnsupportedTriggerConfiguration(name) => write!(
                formatter,
                "unsupported or unreachable input trigger configuration: {name}"
            ),
            Self::ContainmentUnavailable(message) => write!(
                formatter,
                "action cgroup containment is unavailable: {message}"
            ),
        }
    }
}

impl std::error::Error for ActionConfigError {}

#[derive(Clone, Debug, Default)]
pub struct ActionCommands {
    commands: BTreeMap<ActionId, ActionCommand>,
}

impl ActionCommands {
    pub fn insert(&mut self, id: ActionId, command: ActionCommand) {
        self.commands.insert(id, command);
    }
    pub fn get(&self, id: ActionId) -> Option<&ActionCommand> {
        self.commands.get(&id)
    }
    pub fn configured_ids(&self) -> impl Iterator<Item = ActionId> + '_ {
        self.commands.keys().copied()
    }
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfiguredAction {
    executable: PathBuf,
    argv: Vec<String>,
    #[serde(default)]
    environment: BTreeMap<String, String>,
}

pub fn commands_from_environment(
    environment: &BTreeMap<OsString, OsString>,
) -> Result<(ActionCommands, ActionRoutes), ActionConfigError> {
    if environment.contains_key(OsStr::new(
        action_entry(ActionId::KillCurrentGame).legacy_environment_name,
    )) {
        return Err(ActionConfigError::DestructiveCommandOverride);
    }
    let routes = ActionRoutes::from_environment(environment)?;
    let mut commands = ActionCommands::default();
    for entry in ACTION_CATALOG {
        if entry.dispatch_mode == DispatchMode::ExactStop {
            continue;
        }
        let Some(raw) = environment.get(OsStr::new(entry.legacy_environment_name)) else {
            continue;
        };
        if !routes.is_reachable(entry) {
            return Err(ActionConfigError::UnsupportedConfiguredAction(entry.id));
        }
        let raw = raw.to_str().ok_or(ActionConfigError::CommandIsNotUtf8)?;
        let configured: ConfiguredAction =
            serde_json::from_str(raw).map_err(|_| ActionConfigError::CommandIsNotExplicitArgv)?;
        let executable = canonical_immutable_executable(&configured.executable)?;
        let command = ActionCommand::new(
            executable,
            configured.argv.into_iter().map(OsString::from),
            configured
                .environment
                .into_iter()
                .map(|(name, value)| (OsString::from(name), OsString::from(value)))
                .collect(),
        )?;
        commands.insert(entry.id, command);
    }
    Ok((commands, routes))
}

pub(crate) fn canonical_immutable_executable(path: &Path) -> Result<PathBuf, ActionConfigError> {
    if path
        .as_os_str()
        .as_bytes()
        .split(|byte| *byte == b'/')
        .any(|component| component == b"." || component == b"..")
    {
        return Err(ActionConfigError::ExecutableTraversal(path.to_owned()));
    }
    let raw_item = store_item(path)
        .ok_or_else(|| ActionConfigError::ExecutableNotImmutable(path.to_owned()))?;
    let canonical = std::fs::canonicalize(path)
        .map_err(|_| ActionConfigError::ExecutableNotImmutable(path.to_owned()))?;
    let canonical_item = store_item(&canonical)
        .ok_or_else(|| ActionConfigError::ExecutableNotImmutable(path.to_owned()))?;
    if raw_item != canonical_item {
        return Err(ActionConfigError::ExecutableNotImmutable(path.to_owned()));
    }
    let metadata = std::fs::metadata(&canonical)
        .map_err(|_| ActionConfigError::ExecutableNotRegular(canonical.clone()))?;
    if !metadata.is_file() {
        return Err(ActionConfigError::ExecutableNotRegular(canonical));
    }
    if metadata.permissions().mode() & 0o111 == 0 {
        return Err(ActionConfigError::ExecutableNotExecutable(canonical));
    }
    Ok(canonical)
}

fn store_item(path: &Path) -> Option<&OsStr> {
    let relative = path.strip_prefix("/nix/store").ok()?;
    let mut components = relative.components();
    let Component::Normal(item) = components.next()? else {
        return None;
    };
    if item.is_empty() || components.next().is_none() {
        return None;
    }
    Some(item)
}
