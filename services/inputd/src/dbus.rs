use std::{fmt, future::Future, path::Path, time::Duration};

use futures_util::StreamExt;
use korri_input_core::controls::{Control, ControlTransition, DpadAxis};
use zbus::{message::Type, proxy::CacheProperties, MatchRule, Message, MessageStream};

use crate::source_topology::{SourceTopology, SysfsSourceClassifier};

pub const INPUTPLUMBER_BUS_NAME: &str = "org.shadowblip.InputPlumber";
pub const INPUTPLUMBER_ROOT_PATH: &str = "/org/shadowblip/InputPlumber";
pub const COMPOSITE_DEVICE_INTERFACE: &str = "org.shadowblip.Input.CompositeDevice";
pub const DBUS_TARGET_PATH: &str = "/org/shadowblip/InputPlumber/devices/target/dbus0";
pub const DBUS_TARGET_INTERFACE: &str = "org.shadowblip.Input.DBusDevice";
pub const DBUS_INPUT_MEMBER: &str = "InputEvent";
pub const DBUS_OPERATION_TIMEOUT: Duration = Duration::from_millis(500);
const COMPOSITE_CACHE_PROPERTIES: CacheProperties = CacheProperties::No;

#[zbus::proxy(interface = "org.shadowblip.Input.CompositeDevice")]
trait CompositeDevice {
    #[zbus(property)]
    fn dbus_devices(&self) -> zbus::Result<Vec<String>>;

    #[zbus(property)]
    fn profile_path(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn source_device_paths(&self) -> zbus::Result<Vec<String>>;

    fn load_profile_path(&self, path: String) -> zbus::Result<()>;
    fn stop(&self) -> zbus::Result<()>;
}

#[derive(Debug)]
pub enum DbusRuntimeError {
    TimedOut,
    Rejected(String),
    Zbus(zbus::Error),
}

impl DbusRuntimeError {
    pub fn is_transport_failure(&self) -> bool {
        matches!(self, Self::TimedOut | Self::Zbus(_))
    }
}

impl fmt::Display for DbusRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TimedOut => formatter.write_str("DBus operation timed out"),
            Self::Rejected(error) => formatter.write_str(error),
            Self::Zbus(error) => error.fmt(formatter),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileStatus {
    Pending,
    MissingSource,
    AmbiguousSources,
    Ready,
    Applied,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CompositeDisposition {
    Required,
    MissingRequired,
    EmptyOther,
    Other,
}

impl std::error::Error for DbusRuntimeError {}

impl From<zbus::Error> for DbusRuntimeError {
    fn from(error: zbus::Error) -> Self {
        Self::Zbus(error)
    }
}

async fn bounded<T>(
    operation: impl Future<Output = zbus::Result<T>>,
) -> Result<T, DbusRuntimeError> {
    bounded_with_timeout(DBUS_OPERATION_TIMEOUT, operation).await
}

async fn bounded_with_timeout<T>(
    timeout: Duration,
    operation: impl Future<Output = zbus::Result<T>>,
) -> Result<T, DbusRuntimeError> {
    tokio::time::timeout(timeout, operation)
        .await
        .map_err(|_| DbusRuntimeError::TimedOut)?
        .map_err(DbusRuntimeError::Zbus)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticInput {
    Control(Control, ControlTransition),
    Axis(DpadAxis, i32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Signal<'a> {
    pub sender: &'a str,
    pub path: &'a str,
    pub interface: &'a str,
    pub member: &'a str,
    pub capability: &'a str,
    pub value: f64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DbusAuthenticator {
    owner: Option<String>,
}

impl DbusAuthenticator {
    pub fn owner(&self) -> Option<&str> {
        self.owner.as_deref()
    }

    pub fn set_owner(&mut self, owner: Option<&str>) -> bool {
        let owner = owner
            .filter(|value| zbus::names::UniqueName::try_from(*value).is_ok())
            .map(str::to_owned);
        if self.owner == owner {
            return false;
        }
        self.owner = owner;
        true
    }

    pub fn authenticate(&self, signal: &Signal<'_>) -> Option<SemanticInput> {
        if self.owner.as_deref() != Some(signal.sender)
            || signal.path != DBUS_TARGET_PATH
            || signal.interface != DBUS_TARGET_INTERFACE
            || signal.member != DBUS_INPUT_MEMBER
        {
            return None;
        }
        map_capability(signal.capability, signal.value)
    }
}

pub struct DbusSignalSource {
    connection: zbus::Connection,
    messages: MessageStream,
}

impl DbusSignalSource {
    pub async fn system() -> Result<Self, DbusRuntimeError> {
        bounded(async {
            let connection = zbus::Connection::system().await?;
            Self::for_connection_unbounded(connection).await
        })
        .await
    }

    pub async fn for_connection(connection: zbus::Connection) -> Result<Self, DbusRuntimeError> {
        bounded(Self::for_connection_unbounded(connection)).await
    }

    async fn for_connection_unbounded(connection: zbus::Connection) -> zbus::Result<Self> {
        let rule = MatchRule::builder()
            .msg_type(Type::Signal)
            .path_namespace("/org/shadowblip/InputPlumber/devices/target")?
            .interface(DBUS_TARGET_INTERFACE)?
            .member(DBUS_INPUT_MEMBER)?
            .build();
        let messages = MessageStream::for_match_rule(rule, &connection, Some(32)).await?;
        Ok(Self {
            connection,
            messages,
        })
    }

    pub fn connection(&self) -> &zbus::Connection {
        &self.connection
    }

    pub async fn current_owner(&self) -> Result<Option<String>, DbusRuntimeError> {
        bounded(current_unique_owner(&self.connection)).await
    }

    /// Production multi-source profile reconciliation. DBus paths are mapped
    /// per composite during physical discovery, never assumed to be dbus0.
    pub async fn ensure_capture_profiles(
        &self,
        profile_path: &Path,
    ) -> Result<ProfileStatus, DbusRuntimeError> {
        let path = profile_path
            .to_str()
            .ok_or_else(|| DbusRuntimeError::Rejected("profile path is not UTF-8".into()))?;
        tokio::time::timeout(
            DBUS_OPERATION_TIMEOUT,
            ensure_capture_profiles_unbounded(
                &self.connection,
                path,
                &SysfsSourceClassifier::system(),
            ),
        )
        .await
        .map_err(|_| DbusRuntimeError::TimedOut)?
    }

    pub async fn ensure_profile(
        &self,
        profile_path: &Path,
    ) -> Result<ProfileStatus, DbusRuntimeError> {
        let profile_path = profile_path
            .to_str()
            .ok_or_else(|| DbusRuntimeError::Rejected("profile path is not UTF-8".into()))?;
        tokio::time::timeout(
            DBUS_OPERATION_TIMEOUT,
            ensure_profile_unbounded(
                &self.connection,
                profile_path,
                &SysfsSourceClassifier::system(),
            ),
        )
        .await
        .map_err(|_| DbusRuntimeError::TimedOut)?
    }

    pub async fn next_message(&mut self) -> zbus::Result<Option<Message>> {
        self.messages.next().await.transpose()
    }
}

async fn ensure_capture_profiles_unbounded(
    connection: &zbus::Connection,
    profile_path: &str,
    classifier: &SysfsSourceClassifier,
) -> Result<ProfileStatus, DbusRuntimeError> {
    let Some(owner) = current_unique_owner(connection).await? else {
        return Ok(ProfileStatus::Pending);
    };
    let root = zbus::fdo::IntrospectableProxy::builder(connection)
        .destination(owner.as_str())?
        .path(INPUTPLUMBER_ROOT_PATH)?
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    let xml = root.introspect().await.map_err(zbus::Error::from)?;
    if xml.len() > 1024 * 1024 {
        return Err(DbusRuntimeError::Rejected(
            "composite introspection exceeds bound".into(),
        ));
    }
    let paths = composite_paths_from_introspection(&xml);
    if paths.len() > 256 {
        return Err(DbusRuntimeError::Rejected("too many composites".into()));
    }
    let mut count = 0;
    let mut applied = false;
    for path in paths {
        let proxy = CompositeDeviceProxy::builder(connection)
            .destination(owner.as_str())?
            .path(path.as_str())?
            .cache_properties(CacheProperties::No)
            .build()
            .await?;
        let sources = proxy.source_device_paths().await?;
        if sources.len() > 256 {
            return Err(DbusRuntimeError::Rejected(
                "too many physical sources".into(),
            ));
        }
        match classifier.topology(&sources) {
            SourceTopology::MissingGamepad => continue,
            SourceTopology::AmbiguousGamepads => return Ok(ProfileStatus::AmbiguousSources),
            SourceTopology::OneGamepad => {}
        }
        if !sources
            .iter()
            .any(|path| classifier.classify(path) == crate::source_topology::SourceClass::Gamepad)
        {
            return Ok(ProfileStatus::AmbiguousSources);
        }
        count += 1;
        if proxy.profile_path().await? != profile_path {
            if current_unique_owner(connection).await?.as_deref() != Some(owner.as_str()) {
                return Ok(ProfileStatus::Pending);
            }
            proxy.load_profile_path(profile_path.to_owned()).await?;
            applied = true;
            if proxy.profile_path().await? != profile_path {
                return Err(DbusRuntimeError::Rejected(
                    "InputPlumber did not retain the immutable Korri profile".into(),
                ));
            }
        }
        if proxy.source_device_paths().await? != sources {
            return Ok(ProfileStatus::Pending);
        }
    }
    if current_unique_owner(connection).await?.as_deref() != Some(owner.as_str()) {
        return Ok(ProfileStatus::Pending);
    }
    Ok(if count == 0 {
        ProfileStatus::MissingSource
    } else if applied {
        ProfileStatus::Applied
    } else {
        ProfileStatus::Ready
    })
}

async fn ensure_profile_unbounded(
    connection: &zbus::Connection,
    profile_path: &str,
    classifier: &SysfsSourceClassifier,
) -> Result<ProfileStatus, DbusRuntimeError> {
    let Some(owner) = current_unique_owner(connection).await? else {
        return Ok(ProfileStatus::Pending);
    };
    let root = zbus::fdo::IntrospectableProxy::builder(connection)
        .destination(owner.as_str())?
        .path(INPUTPLUMBER_ROOT_PATH)?
        .build()
        .await?;
    let xml = root
        .introspect()
        .await
        .map_err(|error| DbusRuntimeError::Zbus(error.into()))?;
    let composite_paths = composite_paths_from_introspection(&xml);
    let mut candidates = Vec::new();
    let mut missing_required = false;
    let mut stopped_empty = false;
    for path in composite_paths {
        let proxy = CompositeDeviceProxy::builder(connection)
            .destination(owner.as_str())?
            .path(path.as_str())?
            .cache_properties(COMPOSITE_CACHE_PROPERTIES)
            .build()
            .await?;
        let dbus_devices = proxy.dbus_devices().await?;
        let source_paths = proxy.source_device_paths().await?;
        match composite_disposition(&dbus_devices, &source_paths) {
            CompositeDisposition::Required => candidates.push(path),
            CompositeDisposition::MissingRequired => missing_required = true,
            CompositeDisposition::EmptyOther => {
                proxy.stop().await?;
                stopped_empty = true;
                tracing::info!(
                    event = "inputd_empty_composite_stopped",
                    "stopped an empty non-authoritative InputPlumber composite"
                );
            }
            CompositeDisposition::Other => {}
        }
    }
    if current_unique_owner(connection).await?.as_deref() != Some(owner.as_str()) {
        return Ok(ProfileStatus::Pending);
    }
    if missing_required {
        return if candidates.is_empty() {
            Ok(ProfileStatus::MissingSource)
        } else {
            Err(DbusRuntimeError::Rejected(
                "more than one InputPlumber composite owns the required DBus target".into(),
            ))
        };
    }
    if stopped_empty {
        return Ok(ProfileStatus::Pending);
    }
    let [path] = candidates.as_slice() else {
        return match candidates.len() {
            0 => Ok(ProfileStatus::Pending),
            _ => Err(DbusRuntimeError::Rejected(
                "more than one InputPlumber composite owns the required DBus target".into(),
            )),
        };
    };
    let proxy = CompositeDeviceProxy::builder(connection)
        .destination(owner.as_str())?
        .path(path.as_str())?
        .cache_properties(COMPOSITE_CACHE_PROPERTIES)
        .build()
        .await?;
    let profile_status = if proxy.profile_path().await? == profile_path {
        ProfileStatus::Ready
    } else {
        proxy.load_profile_path(profile_path.to_owned()).await?;
        if current_unique_owner(connection).await?.as_deref() != Some(owner.as_str()) {
            return Ok(ProfileStatus::Pending);
        }
        if proxy.profile_path().await? != profile_path {
            return Err(DbusRuntimeError::Rejected(
                "InputPlumber did not retain the immutable Korri profile".into(),
            ));
        }
        if current_unique_owner(connection).await?.as_deref() != Some(owner.as_str()) {
            return Ok(ProfileStatus::Pending);
        }
        ProfileStatus::Applied
    };
    let source_paths = proxy.source_device_paths().await?;
    if current_unique_owner(connection).await?.as_deref() != Some(owner.as_str()) {
        return Ok(ProfileStatus::Pending);
    }
    Ok(profile_status_with_sources(
        profile_status,
        &source_paths,
        classifier,
    ))
}

fn composite_disposition(dbus_devices: &[String], source_paths: &[String]) -> CompositeDisposition {
    let required = dbus_devices.iter().any(|target| target == DBUS_TARGET_PATH);
    match (required, source_paths.is_empty()) {
        (true, false) => CompositeDisposition::Required,
        (true, true) => CompositeDisposition::MissingRequired,
        (false, true) => CompositeDisposition::EmptyOther,
        (false, false) => CompositeDisposition::Other,
    }
}

/// The composite is usable only with exactly one gamepad-class source. Other
/// sources that the device profile merges on purpose, such as volume keys, do
/// not count. See `source_topology` for the classification rule.
fn profile_status_with_sources(
    status: ProfileStatus,
    source_paths: &[String],
    classifier: &SysfsSourceClassifier,
) -> ProfileStatus {
    match classifier.topology(source_paths) {
        SourceTopology::MissingGamepad => ProfileStatus::MissingSource,
        SourceTopology::OneGamepad => status,
        SourceTopology::AmbiguousGamepads => ProfileStatus::AmbiguousSources,
    }
}

pub fn composite_paths_from_introspection(xml: &str) -> Vec<String> {
    let mut paths = xml
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let name = line
                .strip_prefix("<node name=\"CompositeDevice")?
                .split_once('\"')?
                .0;
            if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            Some(format!("{INPUTPLUMBER_ROOT_PATH}/CompositeDevice{name}"))
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
}

pub async fn current_authenticated_owner(
    connection: &zbus::Connection,
) -> Result<Option<String>, DbusRuntimeError> {
    bounded(current_authenticated_owner_unbounded(connection)).await
}

async fn current_authenticated_owner_unbounded(
    connection: &zbus::Connection,
) -> zbus::Result<Option<String>> {
    let Some(owner) = current_unique_owner(connection).await? else {
        return Ok(None);
    };
    let proxy = zbus::fdo::IntrospectableProxy::builder(connection)
        .destination(owner.as_str())?
        .path(DBUS_TARGET_PATH)?
        .build()
        .await?;
    let Ok(xml) = proxy.introspect().await else {
        return Ok(None);
    };
    if !introspection_has_target_interface(&xml) {
        return Ok(None);
    }
    if current_unique_owner(connection).await?.as_deref() != Some(owner.as_str()) {
        return Ok(None);
    }
    Ok(Some(owner))
}

pub fn introspection_has_target_interface(xml: &str) -> bool {
    xml.contains("<interface name=\"org.shadowblip.Input.DBusDevice\">")
}

pub async fn current_unique_owner(connection: &zbus::Connection) -> zbus::Result<Option<String>> {
    let proxy = zbus::fdo::DBusProxy::new(connection).await?;
    let name = zbus::names::BusName::try_from(INPUTPLUMBER_BUS_NAME)?;
    match proxy.get_name_owner(name).await {
        Ok(owner) => Ok(Some(owner.to_string())),
        Err(zbus::fdo::Error::NameHasNoOwner(_)) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn authenticated_message(
    authenticator: &DbusAuthenticator,
    message: &zbus::Message,
) -> Option<SemanticInput> {
    let header = message.header();
    let signal = Signal {
        sender: header.sender()?.as_str(),
        path: header.path()?.as_str(),
        interface: header.interface()?.as_str(),
        member: header.member()?.as_str(),
        capability: "",
        value: 0.0,
    };
    if authenticator.owner.as_deref() != Some(signal.sender)
        || signal.path != DBUS_TARGET_PATH
        || signal.interface != DBUS_TARGET_INTERFACE
        || signal.member != DBUS_INPUT_MEMBER
    {
        return None;
    }
    let (capability, value) = message.body().deserialize::<(String, f64)>().ok()?;
    map_capability(&capability, value)
}

pub fn map_capability(capability: &str, value: f64) -> Option<SemanticInput> {
    if !value.is_finite() {
        return None;
    }
    let transition = if value >= 0.5 {
        ControlTransition::Pressed
    } else {
        ControlTransition::Released
    };
    let control = match capability {
        "ui_guide" => Control::Home,
        "ui_l1" => Control::L1,
        "ui_r1" => Control::R1,
        "ui_l3" => Control::L3,
        "ui_r3" => Control::R3,
        "ui_option" => Control::Start,
        "ui_select" => Control::Select,
        "ui_back" => Control::Back,
        "ui_osk" => Control::X,
        "ui_volume_up" => Control::VolumeUp,
        "ui_volume_down" => Control::VolumeDown,
        "ui_up" => {
            return Some(SemanticInput::Axis(
                DpadAxis::Vertical,
                if transition == ControlTransition::Pressed {
                    -1
                } else {
                    0
                },
            ));
        }
        "ui_down" => {
            return Some(SemanticInput::Axis(
                DpadAxis::Vertical,
                if transition == ControlTransition::Pressed {
                    1
                } else {
                    0
                },
            ));
        }
        "ui_left" => {
            return Some(SemanticInput::Axis(
                DpadAxis::Horizontal,
                if transition == ControlTransition::Pressed {
                    -1
                } else {
                    0
                },
            ));
        }
        "ui_right" => {
            return Some(SemanticInput::Axis(
                DpadAxis::Horizontal,
                if transition == ControlTransition::Pressed {
                    1
                } else {
                    0
                },
            ));
        }
        _ => return None,
    };
    Some(SemanticInput::Control(control, transition))
}

#[cfg(test)]
mod tests {
    use std::{fs, future, path::Path, time::Duration};

    use super::{
        bounded_with_timeout, composite_disposition, composite_paths_from_introspection,
        profile_status_with_sources, CompositeDisposition, DbusRuntimeError, ProfileStatus,
        COMPOSITE_CACHE_PROPERTIES,
    };
    use crate::source_topology::SysfsSourceClassifier;
    use zbus::proxy::CacheProperties;

    fn write_capabilities(root: &Path, event: &str, key: &str, abs: &str) {
        let capabilities = root
            .join("class/input")
            .join(event)
            .join("device/capabilities");
        fs::create_dir_all(&capabilities).unwrap();
        fs::write(capabilities.join("key"), format!("{key}\n")).unwrap();
        fs::write(capabilities.join("abs"), format!("{abs}\n")).unwrap();
    }

    // Capability bitmaps observed on the Retroid Pocket Mini V2.
    fn mini_v2_sysfs() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        write_capabilities(
            root.path(),
            "event4",
            "f00000000 0 0 0 7cdb000000400000 0 0 0 0",
            "30003f",
        );
        write_capabilities(root.path(), "event3", "8000000000000 0", "0");
        write_capabilities(root.path(), "event2", "4000000000000 0", "0");
        root
    }

    #[derive(Default)]
    struct InMemoryCompositeState {
        sources: Vec<String>,
        profile: String,
    }

    struct InMemoryComposite(std::sync::Arc<std::sync::Mutex<InMemoryCompositeState>>);

    #[zbus::interface(name = "org.shadowblip.Input.CompositeDevice")]
    impl InMemoryComposite {
        #[zbus(property)]
        fn dbus_devices(&self) -> Vec<String> {
            vec![super::DBUS_TARGET_PATH.into()]
        }

        #[zbus(property)]
        fn source_device_paths(&self) -> Vec<String> {
            self.0.lock().unwrap().sources.clone()
        }

        #[zbus(property)]
        fn profile_path(&self) -> String {
            self.0.lock().unwrap().profile.clone()
        }

        fn load_profile_path(&self, path: String) {
            self.0.lock().unwrap().profile = path;
        }

        fn stop(&self) {}
    }

    async fn check_readiness(
        connection: &zbus::Connection,
        classifier: &SysfsSourceClassifier,
        expected: ProfileStatus,
    ) {
        let actual = tokio::time::timeout(
            Duration::from_secs(5),
            super::ensure_profile_unbounded(connection, "/korri/profile.yaml", classifier),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn dbus_readiness_checks_live_composite_sources_and_sysfs() {
        use std::{process::Stdio, sync::Arc, sync::Mutex};
        use tokio::io::{AsyncBufReadExt, BufReader};

        // A private bus: this test never contacts the host's InputPlumber.
        let mut daemon = tokio::process::Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("dbus-daemon is provided by the inputd dev shell");
        let mut output = BufReader::new(daemon.stdout.take().unwrap()).lines();
        let address = tokio::time::timeout(Duration::from_secs(5), output.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let root = mini_v2_sysfs();
        let classifier = SysfsSourceClassifier::new(root.path());
        let mini_sources = vec![
            "/dev/input/event3".into(),
            "/dev/input/event4".into(),
            "/dev/input/event2".into(),
        ];
        let state = Arc::new(Mutex::new(InMemoryCompositeState {
            sources: mini_sources.clone(),
            ..Default::default()
        }));
        let _server = zbus::connection::Builder::address(address.as_str())
            .unwrap()
            .name(super::INPUTPLUMBER_BUS_NAME)
            .unwrap()
            .serve_at(
                format!("{}/CompositeDevice0", super::INPUTPLUMBER_ROOT_PATH),
                InMemoryComposite(state.clone()),
            )
            .unwrap()
            .build()
            .await
            .unwrap();
        let client = zbus::connection::Builder::address(address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        check_readiness(&client, &classifier, ProfileStatus::Applied).await;
        check_readiness(&client, &classifier, ProfileStatus::Ready).await;

        write_capabilities(root.path(), "event7", "1000000000000 0 0 0 0", "0");
        state
            .lock()
            .unwrap()
            .sources
            .push("/dev/input/event7".into());
        check_readiness(&client, &classifier, ProfileStatus::AmbiguousSources).await;
        state.lock().unwrap().sources = mini_sources.clone();

        // A disconnection can remove sysfs before InputPlumber updates DBus.
        fs::remove_dir_all(root.path().join("class/input/event4")).unwrap();
        check_readiness(&client, &classifier, ProfileStatus::AmbiguousSources).await;
        state.lock().unwrap().sources =
            vec!["/dev/input/event3".into(), "/dev/input/event2".into()];
        check_readiness(&client, &classifier, ProfileStatus::MissingSource).await;

        // Malformed capabilities and invalid paths cannot justify merging.
        write_capabilities(root.path(), "event4", "invalid", "0");
        state.lock().unwrap().sources = mini_sources;
        check_readiness(&client, &classifier, ProfileStatus::AmbiguousSources).await;
        state.lock().unwrap().sources = vec!["/dev/input/event3".into(), "/dev/hidraw0".into()];
        check_readiness(&client, &classifier, ProfileStatus::AmbiguousSources).await;
        state.lock().unwrap().sources = vec!["/dev/hidraw0".into()];
        check_readiness(&client, &classifier, ProfileStatus::Ready).await;
        state.lock().unwrap().sources.clear();
        check_readiness(&client, &classifier, ProfileStatus::MissingSource).await;
        daemon.kill().await.unwrap();
        daemon.wait().await.unwrap();
    }

    #[tokio::test]
    async fn bounded_wait_times_out_and_allows_a_fresh_retry() {
        let result =
            bounded_with_timeout(Duration::ZERO, future::pending::<zbus::Result<()>>()).await;
        assert!(matches!(result, Err(DbusRuntimeError::TimedOut)));

        let retry = bounded_with_timeout(Duration::ZERO, future::ready(Ok(42))).await;
        assert_eq!(retry.expect("ready retry"), 42);
    }

    #[test]
    fn composite_properties_are_read_only_by_exact_get_calls() {
        assert_eq!(COMPOSITE_CACHE_PROPERTIES, CacheProperties::No);
    }

    #[test]
    fn composite_discovery_accepts_only_direct_numbered_children() {
        let xml = r#"
            <node>
              <node name="CompositeDevice10"/>
              <node name="devices"/>
              <node name="CompositeDevice2"/>
              <node name="CompositeDevice2"/>
              <node name="CompositeDevicebad"/>
              <node name="CompositeDevice0/child"/>
            </node>
        "#;

        assert_eq!(
            composite_paths_from_introspection(xml),
            vec![
                "/org/shadowblip/InputPlumber/CompositeDevice10",
                "/org/shadowblip/InputPlumber/CompositeDevice2",
            ]
        );
    }

    #[test]
    fn profile_readiness_requires_one_live_gamepad_source() {
        let root = mini_v2_sysfs();
        let classifier = SysfsSourceClassifier::new(root.path());
        assert_eq!(
            profile_status_with_sources(ProfileStatus::Ready, &[], &classifier),
            ProfileStatus::MissingSource
        );
        assert_eq!(
            profile_status_with_sources(
                ProfileStatus::Applied,
                &["/dev/input/event4".into()],
                &classifier
            ),
            ProfileStatus::Applied
        );
        write_capabilities(
            root.path(),
            "event7",
            "f00000000 0 0 0 7cdb000000400000 0 0 0 0",
            "30003f",
        );
        assert_eq!(
            profile_status_with_sources(
                ProfileStatus::Ready,
                &["/dev/input/event4".into(), "/dev/input/event7".into()],
                &classifier
            ),
            ProfileStatus::AmbiguousSources
        );
    }

    #[test]
    fn mini_v2_composite_with_volume_key_sources_keeps_the_profile_status() {
        let root = mini_v2_sysfs();
        let classifier = SysfsSourceClassifier::new(root.path());
        // SourceDevicePaths as busctl reported it on the device.
        let sources = [
            "/dev/input/event3".to_owned(),
            "/dev/input/event4".to_owned(),
            "/dev/input/event2".to_owned(),
        ];
        assert_eq!(
            profile_status_with_sources(ProfileStatus::Ready, &sources, &classifier),
            ProfileStatus::Ready
        );
        assert_eq!(
            profile_status_with_sources(ProfileStatus::Applied, &sources, &classifier),
            ProfileStatus::Applied
        );
    }

    #[test]
    fn volume_key_sources_without_the_gamepad_are_a_missing_source() {
        let root = mini_v2_sysfs();
        let classifier = SysfsSourceClassifier::new(root.path());
        assert_eq!(
            profile_status_with_sources(
                ProfileStatus::Ready,
                &["/dev/input/event3".into(), "/dev/input/event2".into()],
                &classifier
            ),
            ProfileStatus::MissingSource
        );
    }

    #[test]
    fn unclassifiable_sources_beside_the_gamepad_stay_ambiguous() {
        let root = mini_v2_sysfs();
        let classifier = SysfsSourceClassifier::new(root.path());
        assert_eq!(
            profile_status_with_sources(
                ProfileStatus::Ready,
                &["/dev/input/event4".into(), "/dev/input/event9".into()],
                &classifier
            ),
            ProfileStatus::AmbiguousSources
        );
    }

    #[test]
    fn empty_non_authoritative_composites_are_stopped_without_removing_the_primary_target() {
        assert_eq!(
            composite_disposition(&[super::DBUS_TARGET_PATH.into()], &["source".into()]),
            CompositeDisposition::Required
        );
        assert_eq!(
            composite_disposition(&[super::DBUS_TARGET_PATH.into()], &[]),
            CompositeDisposition::MissingRequired
        );
        assert_eq!(
            composite_disposition(&[], &[]),
            CompositeDisposition::EmptyOther
        );
        assert_eq!(
            composite_disposition(&[], &["source".into()]),
            CompositeDisposition::Other
        );
    }
}
