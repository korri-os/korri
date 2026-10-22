use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::{self, BufRead, BufReader, Read, Write},
    os::{fd::AsRawFd, unix::net::UnixStream},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

type Result<T> = std::result::Result<T, String>;
const PARENT_SLICE: &str = "korri.slice";
const GAME_SLICE: &str = "korri-games.slice";

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub min: u8,
    pub max: u8,
}
impl Limits {
    fn validate(self) -> Result<()> {
        if self.min > 100 || self.max == 0 || self.max > 100 || self.min > self.max {
            return Err("percents require 0 <= min <= max <= 100 and max >= 1".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    pub cpu: Limits,
    pub gpu: Limits,
    pub governor: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "_tag", content = "payload", deny_unknown_fields)]
pub enum Request {
    Describe {},
    ApplyDevice(Defaults),
    ApplySession {
        unit: String,
        cpu: Limits,
        gpu: Limits,
    },
    EndSession {
        unit: String,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "_tag", content = "payload")]
pub enum Response {
    Ok(Value),
    Err { message: String },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CpuPolicy {
    name: String,
    cpus: Vec<u32>,
    cpuinfo_max_freq: u64,
    available_frequencies: Vec<u64>,
    available_governors: Vec<String>,
    #[serde(skip)]
    min_freq: u64,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GpuDevfreq {
    name: String,
    available_frequencies: Vec<u64>,
}

pub struct Roots {
    pub sysfs: PathBuf,
    pub cgroup: PathBuf,
}
impl Roots {
    fn cpu(&self, name: &str) -> PathBuf {
        self.sysfs.join("devices/system/cpu/cpufreq").join(name)
    }
    fn gpu(&self, name: &str) -> PathBuf {
        self.sysfs.join("class/devfreq").join(name)
    }
    fn parent_slice(&self) -> PathBuf {
        self.cgroup.join(PARENT_SLICE)
    }
    fn slice(&self) -> PathBuf {
        self.parent_slice().join(GAME_SLICE)
    }
}

enum Session {
    // Freeze the device CPU baseline as well as session settings until end.
    Active {
        unit: String,
        device: Defaults,
        cpu: Limits,
        gpu: Limits,
    },
    Reverting {
        unit: String,
    },
}
impl Session {
    fn unit(&self) -> &str {
        match self {
            Self::Active { unit, .. } | Self::Reverting { unit } => unit,
        }
    }
}

pub struct Limitsd {
    roots: Roots,
    defaults: Defaults,
    device: Defaults,
    policies: Vec<CpuPolicy>,
    gpus: Vec<GpuDevfreq>,
    session: Option<Session>,
}

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path)
        .map(|s| s.trim().to_owned())
        .map_err(|e| format!("read {}: {e}", path.display()))
}
fn number(path: &Path) -> Result<u64> {
    read(path)?
        .parse()
        .map_err(|_| format!("invalid frequency in {}", path.display()))
}
fn frequencies(path: &Path) -> Result<Vec<u64>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(format!("read {}: {e}", path.display())),
    };
    let mut values = text
        .split_whitespace()
        .map(|v| {
            v.parse::<u64>()
                .map_err(|_| format!("invalid frequency in {}", path.display()))
        })
        .collect::<Result<Vec<_>>>()?;
    if values.contains(&0) {
        return Err(format!("zero frequency in {}", path.display()));
    }
    values.sort_unstable();
    values.dedup();
    Ok(values)
}

// Select by the same rounded percent labels the portal offers. Raw frequency
// rounding is only the fallback for policies without an available step list.
fn mapped(limits: Limits, top: u64, bottom: u64, steps: &[u64]) -> (u64, u64) {
    let raw_min = (u128::from(top) * u128::from(limits.min)).div_ceil(100) as u64;
    let raw_max = (u128::from(top) * u128::from(limits.max) / 100) as u64;
    let step_percent = |step: u64| (u128::from(step) * 100 + u128::from(top) / 2) / u128::from(top);
    let max = steps
        .iter()
        .copied()
        .rev()
        .find(|v| step_percent(*v) <= u128::from(limits.max))
        .unwrap_or_else(|| steps.first().copied().unwrap_or(raw_max.clamp(bottom, top)));
    let min = steps
        .iter()
        .copied()
        .find(|v| step_percent(*v) >= u128::from(limits.min))
        .unwrap_or_else(|| steps.last().copied().unwrap_or(raw_min.clamp(bottom, top)));
    if min > max {
        log::debug!("rounded interval {min}..{max} is empty; the max cap wins");
    }
    (min.min(max), max)
}

pub fn validate_unit(unit: &str) -> Result<()> {
    if unit.len() != "korri-game-".len() + 32 + ".service".len()
        || !unit.starts_with("korri-game-")
        || !unit.ends_with(".service")
        || !unit.as_bytes()[11..43]
            .iter()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c))
    {
        return Err("unit must match ^korri-game-[0-9a-f]{32}\\.service$".into());
    }
    Ok(())
}

fn cpu_policies(roots: &Roots) -> Result<Vec<CpuPolicy>> {
    let mut policies = vec![];
    let cpu_root = roots.sysfs.join("devices/system/cpu/cpufreq");
    match fs::read_dir(&cpu_root) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|e| e.to_string())?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| "non-UTF8 CPU policy")?;
                if !name.strip_prefix("policy").is_some_and(|suffix| {
                    !suffix.is_empty() && suffix.bytes().all(|c| c.is_ascii_digit())
                }) {
                    continue;
                }
                let path = roots.cpu(&name);
                let top = number(&path.join("cpuinfo_max_freq"))?;
                let bottom = number(&path.join("cpuinfo_min_freq"))?;
                if bottom == 0 || top < bottom {
                    return Err(format!("invalid hardware frequency range for {name}"));
                }
                policies.push(CpuPolicy {
                    name,
                    cpuinfo_max_freq: top,
                    min_freq: bottom,
                    cpus: read(&path.join("related_cpus"))?
                        .split_whitespace()
                        .map(|s| s.parse().map_err(|_| "invalid related_cpus".into()))
                        .collect::<Result<_>>()?,
                    available_frequencies: frequencies(
                        &path.join("scaling_available_frequencies"),
                    )?,
                    available_governors: read(&path.join("scaling_available_governors"))?
                        .split_whitespace()
                        .map(str::to_owned)
                        .collect(),
                });
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("read {}: {e}", cpu_root.display())),
    }
    policies.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(policies)
}

impl Limitsd {
    pub fn start(roots: Roots, defaults: Defaults, gpu_nodes: &[String]) -> Result<Self> {
        let policies = cpu_policies(&roots)?;
        let mut gpus = vec![];
        for name in gpu_nodes {
            if name.is_empty()
                || name == "."
                || name == ".."
                || !name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c))
            {
                return Err("invalid configured GPU node".into());
            }
            let available_frequencies =
                frequencies(&roots.gpu(name).join("available_frequencies"))?;
            if available_frequencies.is_empty() {
                return Err(format!("GPU {name} has no available_frequencies"));
            }
            gpus.push(GpuDevfreq {
                name: name.clone(),
                available_frequencies,
            });
        }
        let service = Self {
            roots,
            device: defaults.clone(),
            defaults,
            policies,
            gpus,
            session: None,
        };
        service.validate_device(&service.defaults)?;
        for slice in [service.roots.parent_slice(), service.roots.slice()] {
            service.write(&slice.join("cgroup.subtree_control"), "+cpu")?;
            service.write(&slice.join("cpu.uclamp.min"), "max")?;
        }
        service.apply_device(&service.device)?;
        Ok(service)
    }

    fn write(&self, path: &Path, value: &str) -> Result<()> {
        // sysfs and cgroup files must already exist; never create a supplied leaf.
        let write = || {
            fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(path)?
                .write_all(format!("{value}\n").as_bytes())
        };
        if let Err(first) = write() {
            // Match clock-governor's read-only sysfs recovery. Never remount the
            // fixture root, or cgroups; these failures are returned to the caller.
            if self.roots.sysfs == Path::new("/sys")
                && path.starts_with(&self.roots.sysfs)
                && !path.starts_with(&self.roots.cgroup)
            {
                let _ = Command::new("mount")
                    .args(["-o", "remount,rw", "/sys"])
                    .status();
                write().map_err(|e| {
                    format!("write {}: {e} (before retry: {first})", path.display())
                })?;
            } else {
                return Err(format!("write {}: {first}", path.display()));
            }
        }
        Ok(())
    }

    fn validate_device(&self, device: &Defaults) -> Result<()> {
        device.cpu.validate()?;
        device.gpu.validate()?;
        for policy in &self.policies {
            if !policy.available_governors.contains(&device.governor) {
                return Err(format!(
                    "governor '{}' is unavailable on {}",
                    device.governor, policy.name
                ));
            }
        }
        Ok(())
    }

    fn write_interval(
        &self,
        root: &Path,
        min_name: &str,
        max_name: &str,
        min: u64,
        max: u64,
    ) -> Result<()> {
        // Cross the old interval safely. Raising min above the old max, or
        // lowering max below the old min, is rejected by cpufreq/devfreq.
        let old_min = number(&root.join(min_name))?;
        if max < old_min {
            self.write(&root.join(min_name), &min.to_string())?;
            self.write(&root.join(max_name), &max.to_string())?;
        } else {
            self.write(&root.join(max_name), &max.to_string())?;
            self.write(&root.join(min_name), &min.to_string())?;
        }
        Ok(())
    }

    fn apply_hardware(&self, device: &Defaults, cpu_max: u8, gpu: Limits) -> Result<()> {
        for policy in &self.policies {
            let path = self.roots.cpu(&policy.name);
            // Select the governor first: some governors reset cpufreq limits.
            self.write(&path.join("scaling_governor"), &device.governor)?;
            let (min, _) = mapped(
                device.cpu,
                policy.cpuinfo_max_freq,
                policy.min_freq,
                &policy.available_frequencies,
            );
            let (_, max) = mapped(
                Limits {
                    min: 0,
                    max: cpu_max,
                },
                policy.cpuinfo_max_freq,
                policy.min_freq,
                &policy.available_frequencies,
            );
            self.write_interval(&path, "scaling_min_freq", "scaling_max_freq", min, max)?;
        }
        for node in &self.gpus {
            let bottom = node.available_frequencies[0];
            let top = *node
                .available_frequencies
                .last()
                .expect("non-empty GPU steps");
            let (min, max) = mapped(gpu, top, bottom, &node.available_frequencies);
            self.write_interval(
                &self.roots.gpu(&node.name),
                "min_freq",
                "max_freq",
                min,
                max,
            )?;
        }
        Ok(())
    }
    fn apply_device(&self, device: &Defaults) -> Result<()> {
        self.apply_hardware(device, device.cpu.max, device.gpu)
    }

    fn cgroup(&self, unit: &str) -> Result<PathBuf> {
        validate_unit(unit)?;
        let root = self
            .roots
            .cgroup
            .canonicalize()
            .map_err(|e| format!("resolve cgroup root: {e}"))?;
        let expected = root.join(PARENT_SLICE).join(GAME_SLICE).join(unit);
        let path = self
            .roots
            .slice()
            .join(unit)
            .canonicalize()
            .map_err(|e| format!("resolve game cgroup: {e}"))?;
        if path != expected {
            return Err("game cgroup must resolve at korri.slice/korri-games.slice/<unit>".into());
        }
        Ok(path)
    }
    fn populated(&self, unit: &str) -> Result<bool> {
        let path = self.roots.slice().join(unit);
        if !path.try_exists().map_err(|e| e.to_string())? {
            return Ok(false);
        }
        let events = read(&self.cgroup(unit)?.join("cgroup.events"))?;
        match events
            .lines()
            .find_map(|line| line.strip_prefix("populated "))
        {
            Some("0") => Ok(false),
            Some("1") => Ok(true),
            _ => Err("invalid cgroup.events populated value".into()),
        }
    }
    fn write_uclamp(&self, unit: &str, cpu: Limits) -> Result<()> {
        let path = self.cgroup(unit)?;
        self.write(&path.join("cpu.uclamp.min"), &cpu.min.to_string())?;
        self.write(&path.join("cpu.uclamp.max"), &cpu.max.to_string())
    }
    fn restore(&mut self) -> Result<()> {
        // Keep recovery state until every write succeeds. A failed rollback
        // must retry even when the game's cgroup is still populated.
        if let Some(session) = &self.session {
            self.session = Some(Session::Reverting {
                unit: session.unit().to_owned(),
            });
        }
        self.apply_device(&self.device)?;
        if let Some(session) = &self.session {
            if self
                .roots
                .slice()
                .join(session.unit())
                .try_exists()
                .map_err(|e| e.to_string())?
            {
                self.write_uclamp(session.unit(), Limits { min: 0, max: 100 })?;
            }
        }
        self.session = None;
        Ok(())
    }

    pub fn poll_session(&mut self) -> Result<()> {
        match &self.session {
            Some(Session::Reverting { .. }) => self.restore()?,
            Some(session) if !self.populated(session.unit())? => self.restore()?,
            _ => {}
        }
        Ok(())
    }
    pub fn reassert(&mut self) -> Result<()> {
        self.policies = cpu_policies(&self.roots)?;
        self.poll_session()?;
        for slice in [self.roots.parent_slice(), self.roots.slice()] {
            self.write(&slice.join("cpu.uclamp.min"), "max")?;
        }
        if let Some(Session::Active {
            unit,
            device,
            cpu,
            gpu,
        }) = &self.session
        {
            self.apply_hardware(device, device.cpu.max.max(cpu.max), *gpu)?;
            self.write_uclamp(unit, *cpu)
        } else {
            self.apply_device(&self.device)
        }
    }

    fn execute(&mut self, request: Request) -> Result<Value> {
        // CPU policies can register after userspace starts or change on wake.
        self.policies = cpu_policies(&self.roots)?;
        match request {
            Request::Describe {} => {
                return Ok(
                    json!({ "defaults": self.defaults, "cpuPolicies": self.policies, "gpuDevfreq": self.gpus }),
                )
            }
            Request::ApplyDevice(device) => {
                self.validate_device(&device)?;
                if self.session.is_none() {
                    if let Err(error) = self.apply_device(&device) {
                        if let Err(restore) = self.apply_device(&self.device) {
                            log::error!("device rollback failed: {restore}");
                        }
                        return Err(error);
                    }
                }
                self.device = device;
            }
            Request::ApplySession { unit, cpu, gpu } => {
                validate_unit(&unit)?;
                cpu.validate()?;
                gpu.validate()?;
                self.validate_device(&self.device)?;
                if self.session.is_some() {
                    return Err("a session is already active".into());
                }
                if !self.populated(&unit)? {
                    return Err("game cgroup must exist and be populated".into());
                }
                // Check both leaves before any hardware writes.
                let path = self.cgroup(&unit)?;
                read(&path.join("cpu.uclamp.min"))?;
                read(&path.join("cpu.uclamp.max"))?;
                self.session = Some(Session::Active {
                    unit: unit.clone(),
                    device: self.device.clone(),
                    cpu,
                    gpu,
                });
                if let Err(error) = self
                    .apply_hardware(&self.device, self.device.cpu.max.max(cpu.max), gpu)
                    .and_then(|_| self.write_uclamp(&unit, cpu))
                {
                    if let Err(restore) = self.restore() {
                        log::error!("session rollback failed: {restore}");
                    }
                    return Err(error);
                }
            }
            Request::EndSession { unit } => {
                validate_unit(&unit)?;
                if !self
                    .session
                    .as_ref()
                    .is_some_and(|session| session.unit() == unit)
                {
                    return Err("unit does not name the active session".into());
                }
                self.restore()?;
            }
        }
        Ok(json!({}))
    }
    pub fn handle(&mut self, request: Request) -> Response {
        match self.execute(request) {
            Ok(value) => Response::Ok(value),
            Err(message) => Response::Err { message },
        }
    }
}

pub fn peer_uid(stream: &UnixStream) -> Result<u32> {
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    if rc != 0 || len as usize != std::mem::size_of::<libc::ucred>() {
        return Err(format!("SO_PEERCRED: {}", io::Error::last_os_error()));
    }
    Ok(cred.uid)
}

pub fn receive(stream: &UnixStream, allowed_uid: u32) -> Result<Request> {
    let authorized = peer_uid(stream)? == allowed_uid;
    // Check credentials before reading, but drain the bounded frame before
    // replying even when denied. Otherwise closing can race the caller's send.
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    let mut bytes = vec![];
    BufReader::new(stream)
        .take(4097)
        .read_until(b'\n', &mut bytes)
        .map_err(|e| format!("read request: {e}"))?;
    if !authorized {
        return Err("peer uid is not korrid".into());
    }
    if bytes.len() > 4096 || bytes.last() != Some(&b'\n') {
        return Err("request must be one JSON line of at most 4096 bytes".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("invalid request: {e}"))
}

pub fn reply(mut stream: &UnixStream, response: &Response) -> Result<()> {
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut stream, response).map_err(|e| e.to_string())?;
    stream.write_all(b"\n").map_err(|e| e.to_string())
}

pub fn serve_connection(stream: UnixStream, allowed_uid: u32, service: &mut Limitsd) -> Result<()> {
    let response = match receive(&stream, allowed_uid) {
        Ok(request) => service.handle(request),
        Err(message) => Response::Err { message },
    };
    reply(&stream, &response)
}
