//! Read-only, owner-bound discovery of normalized physical controller outputs.
//!
//! The returned paths are live provenance, not reconnect identities. Callers must still open through their provider and
//! call `validate_opened_descriptor` on the actual opened descriptor. Neither
//! DBus reads nor an enumeration snapshot can close that device-open race.

use std::{collections::HashSet, path::Path};

use zbus::proxy::CacheProperties;

use crate::{
    dbus::{
        composite_paths_from_introspection, current_unique_owner, DbusRuntimeError,
        DBUS_OPERATION_TIMEOUT, INPUTPLUMBER_ROOT_PATH,
    },
    devices::DeviceDescriptor,
    source_topology::{SourceTopology, SysfsSourceClassifier},
};

// Resource limits, not controller identity or persisted configuration.
const MAX_ENTRIES: usize = 256;
const MAX_INTROSPECTION_BYTES: usize = 1024 * 1024;
const MAX_PATH_BYTES: usize = 4096;

#[zbus::proxy(interface = "org.shadowblip.Input.CompositeDevice")]
trait Composite {
    #[zbus(property)]
    fn source_device_paths(&self) -> zbus::Result<Vec<String>>;
    #[zbus(property)]
    fn target_devices(&self) -> zbus::Result<Vec<String>>;
    #[zbus(property)]
    fn dbus_devices(&self) -> zbus::Result<Vec<String>>;
}

#[zbus::proxy(interface = "org.shadowblip.Input.Target")]
trait Target {
    #[zbus(property)]
    fn device_type(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn device_paths(&self) -> zbus::Result<Vec<String>>;
}

/// Internal inputd data only: no serialization or wire contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhysicalSources {
    pub owner: String,
    pub controllers: Vec<PhysicalSource>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhysicalSource {
    pub composite_path: String,
    pub source_device_paths: Vec<String>,
    pub target_path: String,
    pub dbus_paths: Vec<String>,
    pub descriptor: DeviceDescriptor,
}

/// Legacy discover-devices.ts stableDeviceId uses uniq, then phys. Its eventN
/// fallback is deliberately rejected: node numbers cannot identify reconnects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentifiedSource {
    pub source: PhysicalSource,
    pub device_id: String,
    pub raw: DeviceDescriptor,
}

pub fn identify_sources(
    sources: &PhysicalSources,
    devices: &[DeviceDescriptor],
    classifier: &SysfsSourceClassifier,
) -> Result<Vec<IdentifiedSource>, DbusRuntimeError> {
    if sources.controllers.len() > korri_input_contract::MAX_PHYSICAL_SOURCES {
        return Err(rejected("too many physical controllers"));
    }
    let mut identities = HashSet::new();
    let mut result = Vec::new();
    for source in &sources.controllers {
        if source
            .source_device_paths
            .iter()
            .any(|path| classifier.classify(path) == crate::source_topology::SourceClass::Unknown)
        {
            return Err(rejected("unknown physical source class"));
        }
        let gamepads = source
            .source_device_paths
            .iter()
            .filter(|path| {
                classifier.classify(path) == crate::source_topology::SourceClass::Gamepad
            })
            .collect::<Vec<_>>();
        let [path] = gamepads.as_slice() else {
            return Err(rejected(
                "physical identity requires one positive gamepad source",
            ));
        };
        let raw = devices
            .iter()
            .filter(|device| device.path == Path::new(path.as_str()))
            .collect::<Vec<_>>();
        let [raw] = raw.as_slice() else {
            return Err(rejected("missing or ambiguous raw source descriptor"));
        };
        if raw.is_validated_inputplumber_xb360()
            || raw.class != crate::devices::DeviceClass::Gamepad
            || raw.name.is_empty()
            || raw.name.len() > 512
            || raw.name.chars().any(char::is_control)
        {
            return Err(rejected("raw source is not a physical gamepad"));
        }
        let device_id = raw
            .unique_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .or(raw.physical_path.as_deref().filter(|id| !id.is_empty()))
            .ok_or_else(|| {
                rejected("physical source has neither uniq nor phys reconnect identity")
            })?;
        if device_id.len() > 256
            || device_id.chars().any(char::is_control)
            || !identities.insert(device_id.to_owned())
        {
            return Err(rejected("invalid or duplicate physical reconnect identity"));
        }
        result.push(IdentifiedSource {
            source: source.clone(),
            device_id: device_id.to_owned(),
            raw: (*raw).clone(),
        });
    }
    Ok(result)
}

struct CompositeSnapshot {
    path: String,
    sources: Vec<String>,
    topology: SourceTopology,
    dbus_paths: Vec<String>,
    // Persisted composites without a gamepad do not need target inspection.
    targets: Option<Vec<TargetSnapshot>>,
}

struct TargetSnapshot {
    path: String,
    device_type: String,
    nodes: Option<Vec<String>>,
}

fn rejected(message: &str) -> DbusRuntimeError {
    DbusRuntimeError::Rejected(message.into())
}

fn check_paths(paths: &[String]) -> Result<(), DbusRuntimeError> {
    if paths.len() > MAX_ENTRIES
        || paths
            .iter()
            .any(|path| path.is_empty() || path.len() > MAX_PATH_BYTES)
    {
        return Err(rejected("InputPlumber path list exceeds discovery bounds"));
    }
    Ok(())
}

fn path_set(mut paths: Vec<String>) -> Result<Vec<String>, DbusRuntimeError> {
    check_paths(&paths)?;
    paths.sort();
    if paths.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(rejected("duplicate InputPlumber path"));
    }
    Ok(paths)
}

fn is_normalized_type(device_type: &str) -> bool {
    // InputPlumber 0.75.2 TargetDeviceTypeId and XBoxController support these
    // two type IDs for the XB360 output. The descriptor must still validate.
    matches!(device_type, "xb360" | "gamepad")
}

fn is_event_path(path: &str) -> bool {
    path.strip_prefix("/dev/input/event").is_some_and(|number| {
        !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
    })
}

pub fn is_dbus_target_path(path: &str) -> bool {
    path.strip_prefix("/org/shadowblip/InputPlumber/devices/target/dbus")
        .is_some_and(|number| {
            !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn is_target_path(path: &str) -> bool {
    path.strip_prefix("/org/shadowblip/InputPlumber/devices/target/")
        .is_some_and(|name| !name.is_empty() && !name.contains('/'))
        && zbus::zvariant::ObjectPath::try_from(path).is_ok()
}

/// Discover against a current provider enumeration, without opening raw input.
///
/// Composites with one source according to `SourceTopology` must have one
/// validated XB360 output. Empty and known non-gamepad-only composites are
/// skipped, with their sources rechecked. DeviceType selects xb360/gamepad
/// targets; auxiliary DevicePaths are never queried. Missing DevicePaths on a
/// selected target is an error, with no compatibility path for older providers.
/// The complete pass, including the final owner check, has a fixed deadline.
/// No `dbus0` assumption, profile write, Stop, GetAll, or stable key is involved.
pub async fn discover_physical_sources(
    connection: &zbus::Connection,
    devices: &[DeviceDescriptor],
    classifier: &SysfsSourceClassifier,
) -> Result<PhysicalSources, DbusRuntimeError> {
    tokio::time::timeout(
        DBUS_OPERATION_TIMEOUT,
        discover(connection, devices, classifier),
    )
    .await
    .map_err(|_| DbusRuntimeError::TimedOut)?
}

async fn discover(
    connection: &zbus::Connection,
    devices: &[DeviceDescriptor],
    classifier: &SysfsSourceClassifier,
) -> Result<PhysicalSources, DbusRuntimeError> {
    let owner = current_unique_owner(connection)
        .await?
        .ok_or_else(|| rejected("InputPlumber has no current owner"))?;
    let root = zbus::fdo::IntrospectableProxy::builder(connection)
        .destination(owner.as_str())?
        .path(INPUTPLUMBER_ROOT_PATH)?
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    let xml = root.introspect().await.map_err(zbus::Error::from)?;
    if xml.len() > MAX_INTROSPECTION_BYTES {
        return Err(rejected(
            "InputPlumber introspection exceeds discovery bounds",
        ));
    }
    let paths = composite_paths_from_introspection(&xml);
    check_paths(&paths)?;
    let mut controllers = Vec::new();
    let mut snapshots = Vec::new();
    let mut assigned_sources = HashSet::new();
    let mut assigned_targets = HashSet::new();
    let mut assigned_nodes = HashSet::new();
    let mut assigned_dbus = HashSet::new();
    for path in paths {
        let composite = CompositeProxy::builder(connection)
            .destination(owner.as_str())?
            .path(path.as_str())?
            .cache_properties(CacheProperties::No)
            .build()
            .await?;
        let sources = path_set(composite.source_device_paths().await?)?;
        if sources
            .iter()
            .any(|source| source.starts_with("/dev/input/") && !is_event_path(source))
        {
            return Err(rejected("malformed composite event source path"));
        }
        if sources
            .iter()
            .any(|source| !assigned_sources.insert(source.clone()))
        {
            return Err(rejected("duplicate or cross-composite physical source"));
        }
        let topology = classifier.topology(&sources);
        let dbus_paths = path_set(composite.dbus_devices().await?)?;
        match topology {
            SourceTopology::MissingGamepad => {
                snapshots.push(CompositeSnapshot {
                    path,
                    sources,
                    topology,
                    dbus_paths,
                    targets: None,
                });
                continue;
            }
            SourceTopology::AmbiguousGamepads => {
                return Err(rejected("composite requires one gamepad-class source"));
            }
            SourceTopology::OneGamepad => {}
        }
        // OneGamepad historically also accepts a single Unknown source. That
        // is not positive physical identity evidence.
        if sources
            .iter()
            .filter(|path| {
                classifier.classify(path) == crate::source_topology::SourceClass::Gamepad
            })
            .count()
            != 1
            || sources.iter().any(|path| {
                classifier.classify(path) == crate::source_topology::SourceClass::Unknown
            })
        {
            return Err(rejected(
                "physical controller lacks positive gamepad provenance",
            ));
        }
        if dbus_paths.len() != 1
            || !dbus_paths
                .iter()
                .all(|path| is_dbus_target_path(path) && assigned_dbus.insert(path.clone()))
        {
            return Err(rejected(
                "controller requires one authenticated DBus target",
            ));
        }
        let targets = path_set(composite.target_devices().await?)?;
        let mut candidates = Vec::new();
        let mut target_snapshots = Vec::new();
        for target_path in targets {
            if !is_target_path(&target_path) || !assigned_targets.insert(target_path.clone()) {
                return Err(rejected("invalid or multiply assigned InputPlumber target"));
            }
            let target = TargetProxy::builder(connection)
                .destination(owner.as_str())?
                .path(target_path.as_str())?
                .cache_properties(CacheProperties::No)
                .build()
                .await?;
            let device_type = target.device_type().await?;
            if device_type.is_empty() || device_type.len() > MAX_PATH_BYTES {
                return Err(rejected("invalid InputPlumber target type"));
            }
            if !is_normalized_type(&device_type) {
                target_snapshots.push(TargetSnapshot {
                    path: target_path,
                    device_type,
                    nodes: None,
                });
                continue;
            }
            let nodes = path_set(target.device_paths().await?)?;
            for node in &nodes {
                if !is_event_path(node) || !assigned_nodes.insert(node.clone()) {
                    return Err(rejected("invalid or multiply assigned target event node"));
                }
                // Never associate by name, enumeration order, or stable_identity:
                // multiple normalized controllers have identical fingerprints.
                let matching = devices
                    .iter()
                    .filter(|device| device.path == Path::new(node));
                let mut matching = matching.peekable();
                let descriptor = matching.next();
                if matching.peek().is_some() {
                    return Err(rejected("ambiguous descriptor for target event node"));
                }
                if let Some(descriptor) = descriptor.filter(|d| d.is_validated_inputplumber_xb360())
                {
                    candidates.push((target_path.clone(), descriptor.clone()));
                }
            }
            target_snapshots.push(TargetSnapshot {
                path: target_path,
                device_type,
                nodes: Some(nodes),
            });
        }
        let [(target_path, descriptor)] = candidates.as_slice() else {
            return Err(rejected("missing or ambiguous normalized composite output"));
        };
        controllers.push(PhysicalSource {
            composite_path: path.clone(),
            source_device_paths: sources.clone(),
            target_path: target_path.clone(),
            dbus_paths: dbus_paths.clone(),
            descriptor: descriptor.clone(),
        });
        snapshots.push(CompositeSnapshot {
            path,
            sources,
            topology,
            dbus_paths,
            targets: Some(target_snapshots),
        });
    }
    // Reject observed topology changes under the same owner as well. This is
    // still not an atomic snapshot; provider-open revalidation remains required.
    for snapshot in snapshots {
        let composite = CompositeProxy::builder(connection)
            .destination(owner.as_str())?
            .path(snapshot.path.as_str())?
            .cache_properties(CacheProperties::No)
            .build()
            .await?;
        if path_set(composite.source_device_paths().await?)? != snapshot.sources
            || classifier.topology(&snapshot.sources) != snapshot.topology
            || path_set(composite.dbus_devices().await?)? != snapshot.dbus_paths
        {
            return Err(rejected("InputPlumber composite changed during discovery"));
        }
        let Some(targets) = snapshot.targets else {
            continue;
        };
        if path_set(composite.target_devices().await?)?
            != targets
                .iter()
                .map(|target| target.path.clone())
                .collect::<Vec<_>>()
        {
            return Err(rejected("InputPlumber composite changed during discovery"));
        }
        for snapshot in targets {
            let target = TargetProxy::builder(connection)
                .destination(owner.as_str())?
                .path(snapshot.path.as_str())?
                .cache_properties(CacheProperties::No)
                .build()
                .await?;
            if target.device_type().await? != snapshot.device_type {
                return Err(rejected(
                    "InputPlumber target type changed during discovery",
                ));
            }
            if let Some(nodes) = snapshot.nodes {
                if path_set(target.device_paths().await?)? != nodes {
                    return Err(rejected("InputPlumber target changed during discovery"));
                }
            }
        }
    }
    if current_unique_owner(connection).await?.as_deref() != Some(owner.as_str()) {
        return Err(rejected("InputPlumber owner changed during discovery"));
    }
    Ok(PhysicalSources { owner, controllers })
}
