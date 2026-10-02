use crate::{
    declaration::Declaration, process, provenance::Provenance, script::source::SourceSnapshot,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    os::{
        fd::AsRawFd,
        unix::fs::{OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    time::Duration,
};

pub const BASE_POLICY: &str = "policy-v4: exact named native service and socket sets; validated systemd directives; collision-safe runtime ownership; approved capabilities and device access; host hardening drop-ins; dynamic unprivileged services by default; host-owned IPv4/IPv6 ports; no install scripts; no host module loading";

pub const ROOT_POLICY: &str = "policy-root-v3: exact named native service and socket sets; explicit native User=root; collision-safe runtime ownership; device-wide root authority including account switching, host files, devices and network; host-owned lifecycle, private state and declared IPv4/IPv6 ports; no host module loading";

#[derive(Clone, Serialize)]
pub struct DependencyReport {
    #[serde(flatten)]
    pub report: Report,
    pub already_approved: bool,
}

#[derive(Clone, Serialize)]
pub struct Report {
    pub id: String,
    pub package: PathBuf,
    pub provenance: Provenance,
    pub approval: String,
    pub policy: &'static str,
    pub warning: String,
    pub unit: Option<String>,
    pub state_directory: String,
    pub runtime_directory: String,
    pub unit_configuration: String,
    pub declaration: Declaration,
    pub native_units: BTreeMap<String, crate::native_unit::NativeUnit>,
    pub ports: crate::firewall::Ports,
    pub packages: BTreeMap<String, PathBuf>,
    pub files: BTreeMap<String, PathBuf>,
    pub entry: String,
    pub sources: Vec<String>,
    pub requires: Vec<PathBuf>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub brings: Vec<DependencyReport>,
}

pub fn validate_store_path(path: &Path) -> Result<(), String> {
    let parent = path.parent().ok_or("package has no parent")?;
    let name = path
        .file_name()
        .and_then(|p| p.to_str())
        .ok_or("invalid store item")?;
    let alphabet = b"0123456789abcdfghijklmnpqrsvwxyz";
    if parent != Path::new("/nix/store")
        || name.len() < 34
        || !name.as_bytes()[..32].iter().all(|c| alphabet.contains(c))
        || name.as_bytes()[32] != b'-'
        || name.ends_with(".drv")
        || name
            .bytes()
            .any(|c| !c.is_ascii_alphanumeric() && !b"+-._?=".contains(&c))
    {
        return Err("package must be an exact /nix/store output directory, never a derivation or installable".into());
    }
    Ok(())
}

pub fn tools(path: &Path) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(path).map_err(|e| format!("helper is unavailable: {e}"))?;
    let relative = canonical
        .strip_prefix("/nix/store")
        .map_err(|_| "helper must be an immutable store executable")?;
    if relative.components().count() < 2
        || !canonical.is_file()
        || fs::metadata(&canonical)
            .map_err(|e| e.to_string())?
            .permissions()
            .mode()
            & 0o111
            == 0
    {
        return Err("helper must be an immutable store executable".into());
    }
    // Preserve the selected immutable argv[0] for multicall helpers such as
    // iptables/ip6tables (both resolve to xtables-nft-multi).
    crate::native_unit::immutable_path(path.to_str().ok_or("invalid helper path")?)?;
    Ok(path.to_path_buf())
}

pub fn validate_cache_source(source: &str) -> Result<(), String> {
    if !(source.starts_with("https://")
        || source.starts_with("http://")
        || source.starts_with("file:///"))
        || source.len() > 4096
        || source.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err("source must be an HTTP(S) or local file binary cache".into());
    }
    Ok(())
}

pub fn import(nix: &Path, source: &str, package: &Path) -> Result<(), String> {
    validate_store_path(package)?;
    validate_cache_source(source)?;
    let package_text = package.to_str().ok_or("invalid package path")?;
    // Resolve across the plugin cache and configured upstream substituters.
    // `copy --from` requires one source to contain the entire closure.
    process::checked(
        nix,
        [
            "--extra-experimental-features",
            "nix-command",
            "--option",
            "max-jobs",
            "0",
            "--option",
            "builders",
            "",
            "--option",
            "fallback",
            "false",
            "--option",
            "require-sigs",
            "true",
            "build",
            "--no-link",
            "--extra-substituters",
            source,
            package_text,
        ],
        Duration::from_secs(180),
    )?;
    // Realization can reuse paths without locally registered signatures (for
    // example, in a NixOS store image). Unlike build, store verify consults only
    // explicit --substituter arguments, not the configured substituters.
    // `nix config show substituters` prints Nix's effective whitespace-separated
    // list, including extra-substituters; keep that policy owned by Nix.
    let substituters = process::checked(
        nix,
        [
            "--extra-experimental-features",
            "nix-command",
            "config",
            "show",
            "substituters",
        ],
        Duration::from_secs(10),
    )?;
    let mut verify_args = vec![
        "--extra-experimental-features",
        "nix-command",
        "store",
        "verify",
        "--recursive",
        "--sigs-needed",
        "1",
        "--substituter",
        source,
    ];
    for substituter in substituters.split_whitespace() {
        verify_args.extend(["--substituter", substituter]);
    }
    verify_args.push(package_text);
    process::checked(nix, verify_args, Duration::from_secs(180))?;
    // The receipt must not become durable before downloaded store contents.
    // Nix may register substituted outputs while their data still lives in the
    // filesystem's writeback cache. Sync the whole store filesystem, including
    // the transitive closure, before permission approval or activation.
    let store = fs::File::open("/nix/store").map_err(|e| e.to_string())?;
    if unsafe { libc::syncfs(store.as_raw_fd()) } != 0 {
        return Err(format!(
            "cannot persist imported package: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

// The package's manifest is generated by trusted publisher composition. It
// claims a namespace, not a key. Only the device's binding grants authority.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    publisher: Publisher,
    entry: String,
    sources: Vec<String>,
    #[serde(default)]
    packages: BTreeMap<String, PathBuf>,
    #[serde(default)]
    files: BTreeMap<String, PathBuf>,
    #[serde(default)]
    services: BTreeMap<String, PathBuf>,
    #[serde(default)]
    ports: crate::firewall::Ports,
    #[serde(default)]
    requires: Vec<PathBuf>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Publisher {
    namespace: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublisherBinding {
    pub public_key: String,
    pub cache_url: String,
}

pub type PublisherBindings = BTreeMap<String, PublisherBinding>;

pub fn publisher_bindings(path: &Path) -> Result<PublisherBindings, String> {
    let bindings: PublisherBindings = serde_json::from_slice(&read_regular(path, 64 * 1024)?)
        .map_err(|error| format!("invalid publisher bindings: {error}"))?;
    for (namespace, binding) in &bindings {
        crate::declaration::validate_id(&format!("{namespace}:binding"))?;
        validate_cache_source(&binding.cache_url)?;
        public_key_bytes(&binding.public_key)?;
    }
    Ok(bindings)
}

fn read_regular(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err(format!("{} must be a regular file", path.display()));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > limit {
        return Err(format!("{} exceeds {limit} bytes", path.display()));
    }
    Ok(bytes)
}

const MANIFEST_BYTES: u64 = 512 * 1024;

fn manifest(package: &Path) -> Result<Manifest, String> {
    let manifest: Manifest = serde_json::from_slice(&read_regular(
        &package.join("manifest.json"),
        MANIFEST_BYTES,
    )?)
    .map_err(|error| format!("invalid plugin manifest: {error}"))?;
    crate::declaration::validate_id(&format!("{}:manifest", manifest.publisher.namespace))?;
    for names in [&manifest.packages, &manifest.files, &manifest.services] {
        crate::native_unit::validate_names(&names.keys().cloned().collect::<Vec<_>>())?;
    }
    manifest.ports.validate()?;
    if manifest.requires.len() > crate::dependencies::MAX_PLUGINS {
        return Err("too many required plugins".into());
    }
    let mut requires = BTreeSet::new();
    for path in &manifest.requires {
        validate_store_path(path)?;
        if !requires.insert(path) {
            return Err("duplicate required plugin output".into());
        }
    }
    if manifest.entry != "plugin.ts" {
        return Err("plugin manifest entry must be plugin.ts".into());
    }
    if manifest.sources.is_empty() || !manifest.sources.iter().any(|name| name == &manifest.entry) {
        return Err("plugin source inventory must contain plugin.ts".into());
    }
    if manifest.sources.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err("plugin source inventory must be sorted and unique".into());
    }
    Ok(manifest)
}

pub fn manifest_namespace(package: &Path) -> Result<String, String> {
    Ok(manifest(package)?.publisher.namespace)
}

pub fn manifest_requires(package: &Path) -> Result<Vec<PathBuf>, String> {
    Ok(manifest(package)?.requires)
}

fn public_key_bytes(key: &str) -> Result<Vec<u8>, String> {
    let (label, encoded) = key
        .split_once(':')
        .ok_or("publisher needs a full Nix public key")?;
    if label.is_empty() || label.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("invalid Nix public key label".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "invalid Nix public key encoding")?;
    if bytes.len() != 32 {
        return Err("publisher needs a 32-byte Ed25519 public key".into());
    }
    Ok(bytes)
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NarInfo {
    nar_hash: String,
    nar_size: u64,
    references: Vec<String>,
    #[serde(default)]
    signatures: Vec<String>,
}

fn path_info(nix: &Path, package: &str, cache: Option<&str>) -> Result<NarInfo, String> {
    let mut args = vec![
        "--extra-experimental-features",
        "nix-command",
        "path-info",
        "--json",
    ];
    if let Some(cache) = cache {
        args.extend(["--store", cache]);
    }
    args.push(package);
    let json = process::checked(nix, args, Duration::from_secs(30))?;
    let mut paths: BTreeMap<String, NarInfo> = serde_json::from_str(&json)
        .map_err(|error| format!("invalid Nix path metadata: {error}"))?;
    if paths.len() != 1 {
        return Err("Nix must describe exactly the selected package".into());
    }
    paths
        .remove(package)
        .ok_or_else(|| "Nix metadata names another output".into())
}

/// Nix's base-32 form of an SRI `sha256-` hash, as `nix hash convert --to
/// nix32` prints it. Computed here because each `nix` call costs about 0.3 s
/// on an RK3566.
pub fn nix32_sha256(sri: &str) -> Result<String, String> {
    const ALPHABET: &[u8; 32] = b"0123456789abcdfghijklmnpqrsvwxyz";
    let encoded = sri
        .strip_prefix("sha256-")
        .ok_or("unsupported NAR fingerprint")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "invalid NAR hash encoding")?;
    if bytes.len() != 32 {
        return Err("invalid NAR hash length".into());
    }
    let length = (bytes.len() * 8 - 1) / 5 + 1;
    let mut out = String::with_capacity(length);
    for n in (0..length).rev() {
        let bit = n * 5;
        let (i, j) = (bit / 8, bit % 8);
        let low = u16::from(bytes[i]) >> j;
        let high = bytes
            .get(i + 1)
            .map_or(0, |byte| u16::from(*byte) << (8 - j));
        out.push(char::from(ALPHABET[usize::from((low | high) & 0x1f)]));
    }
    Ok(out)
}

fn fingerprint(package: &str, info: &NarInfo) -> Result<String, String> {
    // This is Nix's ValidPathInfo::fingerprint(), not a signature-name check.
    // Nix normalizes its SRI hash to the nix32 form used in that fingerprint.
    if !info.nar_hash.starts_with("sha256-") || info.nar_size == 0 {
        return Err("unsupported NAR fingerprint".into());
    }
    let hash = nix32_sha256(&info.nar_hash)?;
    let mut references = info.references.clone();
    for reference in &references {
        validate_store_path(Path::new(reference))?;
    }
    references.sort();
    references.dedup();
    Ok(format!(
        "1;{package};sha256:{};{};{}",
        hash.trim(),
        info.nar_size,
        references.join(",")
    ))
}

fn signed_by(info: &NarInfo, fingerprint: &str, key: &[u8]) -> bool {
    info.signatures.iter().any(|signature| {
        let Some((_, encoded)) = signature.split_once(':') else {
            return false;
        };
        let Ok(signature) = base64::engine::general_purpose::STANDARD.decode(encoded) else {
            return false;
        };
        ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key)
            .verify(fingerprint.as_bytes(), &signature)
            .is_ok()
    })
}

/// Verify the exact package output, even when Nix reused a cached or
/// content-addressed path. Dependencies may use other trusted Nix signers;
/// they cannot claim the publisher's namespace for this output.
/// Whether a publisher check rehashes the package's store contents.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoreContents {
    /// Rehash the contents against Nix's registered hash. Install, enable,
    /// update and rollback use this.
    Rehash,
    /// Trust contents the image build or an earlier install already
    /// verified. Boot restore uses this (owner decision 2026-09-30): the
    /// rehash cost about a minute of CPU per boot on an RK3566. The signature
    /// over the registered hash is still checked.
    TrustRegistered,
}

/// Store metadata for several packages and their closures, read with one
/// `nix path-info --json --recursive` call. Boot restore uses it so that 20
/// plugins cost one Nix call instead of three each.
pub struct StoreSnapshot {
    infos: BTreeMap<String, NarInfo>,
}

impl StoreSnapshot {
    pub fn query(nix: &Path, packages: &[&Path]) -> Result<Self, String> {
        let mut args = vec![
            "--extra-experimental-features",
            "nix-command",
            "path-info",
            "--json",
            "--recursive",
        ];
        for package in packages {
            validate_store_path(package)?;
            args.push(package.to_str().ok_or("invalid package path")?);
        }
        let json = process::checked(nix, args, Duration::from_secs(60))?;
        let infos: BTreeMap<String, NarInfo> = serde_json::from_str(&json)
            .map_err(|error| format!("invalid Nix path metadata: {error}"))?;
        for path in infos.keys() {
            validate_store_path(Path::new(path))?;
        }
        Ok(Self { infos })
    }

    pub fn contains(&self, package: &Path) -> bool {
        package
            .to_str()
            .is_some_and(|package| self.infos.contains_key(package))
    }

    fn info(&self, package: &Path) -> Result<&NarInfo, String> {
        package
            .to_str()
            .and_then(|package| self.infos.get(package))
            .ok_or_else(|| format!("no store metadata for {}", package.display()))
    }

    /// The same set `nix path-info --recursive <package>` prints.
    pub fn closure(&self, package: &Path) -> Result<BTreeSet<PathBuf>, String> {
        let mut closure = BTreeSet::new();
        let mut queue = vec![package.to_path_buf()];
        while let Some(path) = queue.pop() {
            if !closure.insert(path.clone()) {
                continue;
            }
            for reference in &self.info(&path)?.references {
                validate_store_path(Path::new(reference))?;
                queue.push(PathBuf::from(reference));
            }
        }
        Ok(closure)
    }

    pub fn fingerprint(&self, package: &Path) -> Result<String, String> {
        fingerprint(
            package.to_str().ok_or("invalid package path")?,
            self.info(package)?,
        )
    }
}

pub fn verify_publisher(
    nix: &Path,
    package: &Path,
    source: Option<&str>,
    bindings: &PublisherBindings,
    contents: StoreContents,
) -> Result<String, String> {
    verify_publisher_in(nix, None, package, source, bindings, contents)
}

/// `verify_publisher` that reads the package's local metadata from a
/// snapshot when one covers it. The cache fallback still queries Nix.
pub fn verify_publisher_in(
    nix: &Path,
    snapshot: Option<&StoreSnapshot>,
    package: &Path,
    source: Option<&str>,
    bindings: &PublisherBindings,
    contents: StoreContents,
) -> Result<String, String> {
    validate_store_path(package)?;
    let package_text = package.to_str().ok_or("invalid package path")?;
    if contents == StoreContents::Rehash {
        process::checked(
            nix,
            [
                "--extra-experimental-features",
                "nix-command",
                "store",
                "verify",
                "--no-trust",
                package_text,
            ],
            Duration::from_secs(180),
        )?;
    }
    let namespace = manifest_namespace(package)?;
    let binding = bindings
        .get(&namespace)
        .ok_or_else(|| format!("publisher {namespace} is not bound on this device"))?;
    validate_cache_source(&binding.cache_url)?;
    if source.is_some_and(|source| source != binding.cache_url) {
        return Err(format!(
            "publisher {namespace} is bound to cache {}",
            binding.cache_url
        ));
    }
    let key = public_key_bytes(&binding.public_key)?;
    let local = match snapshot.filter(|snapshot| snapshot.contains(package)) {
        Some(snapshot) => snapshot.info(package)?.clone(),
        None => path_info(nix, package_text, None)?,
    };
    let local_fingerprint = fingerprint(package_text, &local)?;
    if signed_by(&local, &local_fingerprint, &key) {
        return Ok(namespace);
    }
    // Store images may omit locally registered signatures. Fetch only metadata
    // from the bound cache, and require it to sign the actual local NAR.
    let remote = path_info(nix, package_text, Some(&binding.cache_url))?;
    if fingerprint(package_text, &remote)? != local_fingerprint
        || !signed_by(&remote, &local_fingerprint, &key)
    {
        return Err(format!(
            "package is not signed by the full key bound to publisher {namespace}"
        ));
    }
    // Preserve the verified signature in Nix's own metadata, not a Korri
    // receipt. A later offline enable/restore must check the same full key.
    process::checked(
        nix,
        [
            "--extra-experimental-features",
            "nix-command",
            "--option",
            "trusted-public-keys",
            &binding.public_key,
            "store",
            "copy-sigs",
            "--substituter",
            &binding.cache_url,
            package_text,
        ],
        Duration::from_secs(30),
    )?;
    let registered = path_info(nix, package_text, None)?;
    if !signed_by(&registered, &local_fingerprint, &key) {
        return Err(format!(
            "cannot retain verified signature for publisher {namespace}"
        ));
    }
    Ok(namespace)
}

pub fn load_declaration(package: &Path) -> Result<Declaration, String> {
    Ok(load_declaration_snapshot(package)?.0)
}

fn load_declaration_snapshot(package: &Path) -> Result<(Declaration, SourceSnapshot), String> {
    validate_store_path(package)?;
    if fs::canonicalize(package).map_err(|e| e.to_string())? != package || !package.is_dir() {
        return Err("package must be an exact immutable directory".into());
    }
    let manifest = manifest(package)?;
    let source = SourceSnapshot::package_plugin(package, &manifest.entry, &manifest.sources)?;
    let namespace = manifest.publisher.namespace.clone();
    let declaration = Declaration::evaluate_snapshot(&namespace, &source)?;
    crate::plugin_references::validate_files(
        serde_json::to_value(&declaration).map_err(|e| e.to_string())?,
        &manifest.files,
    )?;
    crate::native_unit::validate_unit_selection(
        &declaration.services,
        &manifest.services.keys().cloned().collect::<Vec<_>>(),
    )?;
    Ok((declaration, source))
}

pub fn load(nix: &Path, package: &Path, provenance: Provenance) -> Result<Report, String> {
    let closure = closure_of(nix, package)?;
    leaf_report(build_report(package, provenance, Some(&closure))?)
}

fn leaf_report(report: Report) -> Result<Report, String> {
    if !report.requires.is_empty() {
        return Err(
            "required plugins need independent publisher bindings and closure inspection".into(),
        );
    }
    Ok(report)
}

/// `load` with the closure taken from a snapshot instead of a Nix call.
pub fn load_from(
    snapshot: &StoreSnapshot,
    package: &Path,
    provenance: Provenance,
) -> Result<Report, String> {
    let closure = snapshot.closure(package)?;
    leaf_report(build_report(package, provenance, Some(&closure))?)
}

/// Load an exact graph using each dependency's own provenance. The caller
/// resolves a selected receipt or the dependency publisher's bound raw cache.
/// No parent repository identity or cache is inherited by another plugin.
pub fn load_graph(
    nix: &Path,
    snapshot: Option<&StoreSnapshot>,
    package: &Path,
    provenance: Provenance,
    mut dependency_provenance: impl FnMut(&Path) -> Result<Provenance, String>,
) -> Result<Report, String> {
    let mut reports = BTreeMap::new();
    let order = crate::dependencies::dependency_order([package.to_path_buf()], |path| {
        let origin = if path == package {
            provenance.clone()
        } else {
            dependency_provenance(path)?
        };
        let closure = match snapshot {
            Some(snapshot) if snapshot.contains(path) => snapshot.closure(path)?,
            _ => closure_of(nix, path)?,
        };
        let report = build_report(path, origin, Some(&closure))?;
        let requires = report.requires.clone();
        reports.insert(path.clone(), report);
        Ok(requires)
    })?;
    let mut identities = BTreeMap::new();
    for report in reports.values() {
        if identities
            .insert(report.id.clone(), report.package.clone())
            .is_some()
        {
            return Err(format!(
                "conflicting exact versions for plugin {}",
                report.id
            ));
        }
    }
    let mut closures: BTreeMap<PathBuf, BTreeSet<PathBuf>> = BTreeMap::new();
    for path in &order {
        let mut report = reports.remove(path).unwrap();
        let mut reachable = BTreeSet::new();
        for required in &report.requires {
            reachable.insert(required.clone());
            reachable.extend(closures[required].iter().cloned());
        }
        let dependencies: Vec<_> = order.iter().filter(|p| reachable.contains(*p)).collect();
        // Keep one full report per node. Intermediate closures store only
        // paths, not quadratic copies of every source/native authority report.
        // Leaf approvals remain unchanged; non-leaf summaries preserve the
        // f3aa66d91 ID/output/approval binding and dependency-first order.
        if !dependencies.is_empty() {
            let summaries: Vec<_> = dependencies
                .iter()
                .map(|path| {
                    let dep = &reports[*path];
                    (&dep.id, &dep.package, &dep.approval)
                })
                .collect();
            let bytes =
                serde_json::to_vec(&(&report.approval, summaries)).map_err(|e| e.to_string())?;
            report.approval = hex::encode(Sha256::digest(bytes));
        }
        if path == package {
            report.brings = dependencies
                .iter()
                .map(|path| DependencyReport {
                    report: reports[*path].clone(),
                    already_approved: false,
                })
                .collect();
        }
        closures.insert(path.clone(), reachable);
        reports.insert(path.clone(), report);
    }
    Ok(reports.remove(package).unwrap())
}

/// Digest-only report for image-time seeding.
///
/// The runtime `load` re-derives this report with the full closure check
/// before any receipt is trusted, so a seeded receipt can never authorize a
/// package the device would refuse to load. An image build has no nix, which
/// is the only reason this entry point exists.
pub fn load_for_seed(package: &Path, provenance: Provenance) -> Result<Report, String> {
    leaf_report(build_report(package, provenance, None)?)
}

fn closure_of(nix: &Path, package: &Path) -> Result<BTreeSet<PathBuf>, String> {
    let closure = process::checked(
        nix,
        [
            "--extra-experimental-features",
            "nix-command",
            "path-info",
            "--recursive",
            package.to_str().ok_or("invalid package path")?,
        ],
        Duration::from_secs(30),
    )?;
    let closure: BTreeSet<PathBuf> = closure.lines().map(PathBuf::from).collect();
    for path in &closure {
        validate_store_path(path)?;
    }
    Ok(closure)
}

pub fn authority_warning(
    native_units: &BTreeMap<String, crate::native_unit::NativeUnit>,
) -> String {
    if native_units.is_empty() {
        return "No native service is activated by this package. Approval covers the exact declaration and launch callback source. Game launch effects can access the runtime user's files and display; they must run in the existing runtime-user systemd sandbox, never as the administrator.".into();
    }

    let mut report =
        String::from("NATIVE AUTHORITY REQUEST: approval is limited to this exact plugin build.");
    if native_units
        .values()
        .any(|unit| unit.user.as_deref() == Some("root"))
    {
        report.push_str(" DEVICE-WIDE ROOT AUTHORITY: User=root can change device files, accounts, credentials, devices and network policy.");
    }
    for (name, unit) in native_units {
        report.push_str(&format!(" unit {name}:"));
        if unit.privileged_directives.is_empty() {
            report.push_str(" privileged directives [none];");
        } else {
            report.push_str(&format!(
                " privileged directives [{}];",
                unit.privileged_directives.join(", ")
            ));
            if unit.privileged_directives.iter().any(|directive| {
                directive.starts_with("ExecStartPre=+") || directive.starts_with("ExecStopPost=+")
            }) {
                report.push_str(" PRIVILEGED HELPER: + commands bypass the service sandbox and run with root authority.");
            }
        }
        if unit.capabilities.is_empty() {
            report.push_str(" capabilities [none];");
        } else {
            report.push_str(&format!(
                " capabilities [{}];",
                unit.capabilities.join(", ")
            ));
        }
        if unit.devices.is_empty()
            && unit.effective_device_policy() == crate::native_unit::DevicePolicy::Auto
        {
            report.push_str(" devices [all host devices (PrivateDevices=false)].");
        } else if unit.devices.is_empty() {
            report.push_str(" devices [none].");
        } else {
            report.push_str(&format!(" devices [{}].", unit.devices.join(", ")));
        }
    }
    report
}

fn native_rule_warning(files: &BTreeMap<String, PathBuf>) -> Result<String, String> {
    let mut warning = String::new();
    for (name, path) in files {
        if path
            .file_name()
            .is_some_and(|file| file.to_string_lossy().ends_with(".rules"))
        {
            let bytes = read_regular(path, 64 * 1024)?;
            let rules = std::str::from_utf8(&bytes)
                .map_err(|_| format!("native udev rules {name} are not UTF-8"))?;
            warning.push_str(&format!(" native udev rules {name}: {rules:?};"));
        }
    }
    Ok(warning)
}

fn build_report(
    package: &Path,
    provenance: Provenance,
    closure: Option<&BTreeSet<PathBuf>>,
) -> Result<Report, String> {
    let (declaration, source) = load_declaration_snapshot(package)?;
    let id = declaration.id();
    provenance.validate(&id)?;
    let unit = unit_name(&id);
    let manifest = manifest(package)?;
    if let Some(closure) = closure {
        for path in manifest.packages.values().chain(&manifest.requires) {
            validate_store_path(path)?;
            if !closure.contains(path)
                || fs::canonicalize(path).map_err(|e| e.to_string())? != *path
            {
                return Err("package is outside the immutable closure".into());
            }
        }
        for path in manifest.files.values().chain(manifest.services.values()) {
            validate_artifact(path, closure, false)?;
        }
    }
    let mut native_units = BTreeMap::new();
    for name in &declaration.services {
        let bytes = read_regular(
            &fs::canonicalize(&manifest.services[name]).map_err(|e| e.to_string())?,
            128 * 1024,
        )?;
        let unit = crate::native_unit::NativeUnit::parse(
            std::str::from_utf8(&bytes).map_err(|_| "native unit is not UTF-8")?,
        )?;
        crate::native_unit::managed_unit_name(name, unit.kind)?;
        if let Some(closure) = closure {
            for executable in &unit.executables {
                if !crate::native_unit::is_host_wrapper(executable) {
                    validate_artifact(Path::new(executable), closure, true)?;
                }
            }
        }
        native_units.insert(name.clone(), unit);
    }
    if native_units.is_empty() && !manifest.ports.is_empty() {
        return Err("ports require an active service contribution".into());
    }
    let mut managed_names = BTreeSet::new();
    for (name, unit) in &native_units {
        let managed = crate::native_unit::managed_unit_name(name, unit.kind)?;
        if !managed_names.insert(managed) {
            return Err("native declarations resolve to a duplicate systemd unit name".into());
        }
    }
    let managed_unit = if managed_names.len() == 1 {
        managed_names.iter().next().cloned()
    } else {
        None
    };
    let has_root = native_units
        .values()
        .any(|unit| unit.user.as_deref() == Some("root"));
    let mut warning = authority_warning(&native_units);
    warning.push_str(&native_rule_warning(&manifest.files)?);
    let mut report = Report {
        id,
        package: package.into(),
        provenance,
        approval: String::new(),
        policy: if has_root { ROOT_POLICY } else { BASE_POLICY },
        warning: if native_units
            .values()
            .any(|unit| !unit.credentials.is_empty())
        {
            format!("{warning} NATIVE CREDENTIAL LOOKUP: systemd searches its inherited credentials and credstore for tailscale-authkey. A missing named credential is non-fatal in systemd. Korri supplies no secret and performs no login; Tailscale still needs an explicit operator tailscale up.")
        } else {
            warning
        },
        state_directory: format!("/var/lib/{unit}"),
        runtime_directory: format!("/run/{unit}"),
        unit: managed_unit,
        unit_configuration: String::new(),
        declaration,
        native_units,
        ports: manifest.ports,
        packages: manifest.packages,
        files: manifest.files,
        entry: manifest.entry,
        sources: manifest.sources,
        requires: manifest.requires,
        brings: Vec::new(),
    };
    report.unit_configuration = crate::unit::render(&report)?;
    report.approval = approval_digest(
        &report.package,
        &report.provenance,
        &report.declaration,
        report.policy,
        &report.unit_configuration,
        &source,
    )?;
    Ok(report)
}

fn approval_digest(
    package: &Path,
    provenance: &Provenance,
    declaration: &Declaration,
    policy: &str,
    unit: &str,
    source: &SourceSnapshot,
) -> Result<String, String> {
    let sources = source.canonical_entries();
    let manifest = read_regular(&package.join("manifest.json"), MANIFEST_BYTES)?;
    let bytes = serde_json::to_vec(&(
        policy,
        package,
        provenance,
        declaration,
        unit,
        sources,
        manifest,
    ))
    .map_err(|e| e.to_string())?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub fn unit_name(id: &str) -> String {
    format!(
        "korri-plugin-{}",
        hex::encode(Sha256::digest(id.as_bytes()))
    )
}

fn validate_artifact(
    path: &Path,
    closure: &std::collections::BTreeSet<PathBuf>,
    executable: bool,
) -> Result<(), String> {
    let canonical = fs::canonicalize(path)
        .map_err(|e| format!("artifact {} is unavailable: {e}", path.display()))?;
    for path in [path, canonical.as_path()] {
        let text = path.to_str().ok_or("invalid artifact path")?;
        crate::native_unit::immutable_path(text)?;
        let root = path.components().take(4).collect::<PathBuf>();
        if !closure.contains(&root) {
            return Err(format!(
                "artifact {} is outside the selected closure",
                path.display()
            ));
        }
    }
    let metadata = fs::metadata(&canonical).map_err(|e| e.to_string())?;
    if executable && (!metadata.is_file() || metadata.permissions().mode() & 0o111 == 0) {
        return Err("payload must resolve to an immutable regular executable".into());
    }
    Ok(())
}

#[cfg(test)]
mod approval_tests {
    use super::*;

    #[test]
    fn native_udev_rule_requests_are_named_in_the_approval_warning() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("99-z-korri-sunshine-input.rules");
        fs::write(
            &path,
            "KERNEL==\"uinput\", GROUP=\"korri-sunshine-input-seat\"\n",
        )
        .unwrap();
        let warning = native_rule_warning(&BTreeMap::from([("input-rules".into(), path)])).unwrap();
        assert!(warning.contains("native udev rules input-rules"));
        assert!(warning.contains("uinput"));
        assert!(warning.contains("korri-sunshine-input-seat"));
    }

    fn snapshot(package: &Path) -> Result<SourceSnapshot, String> {
        SourceSnapshot::package_plugin(package, "plugin.ts", &["plugin.ts".into()])
    }

    fn approval_digest(
        package: &Path,
        provenance: &Provenance,
        declaration: &Declaration,
        unit: &str,
    ) -> Result<String, String> {
        super::approval_digest(
            package,
            provenance,
            declaration,
            BASE_POLICY,
            unit,
            &snapshot(package)?,
        )
    }

    #[test]
    fn source_approval_consumes_the_evaluated_snapshot_without_reopening_source() {
        let directory = tempfile::tempdir().unwrap();
        let package = directory.path();
        fs::write(package.join("plugin.ts"), "export const name = 'snapshot';").unwrap();
        fs::write(package.join("manifest.json"), "approved manifest").unwrap();
        let source = snapshot(package).unwrap();
        let declaration = Declaration::evaluate_snapshot("@test", &source).unwrap();
        let origin = Provenance::RawCache {
            cache_url: "file:///cache".into(),
        };
        let digest =
            super::approval_digest(package, &origin, &declaration, BASE_POLICY, "unit", &source)
                .unwrap();
        // Snapshot ownership does not change the existing approval tuple or
        // the source's JSON byte-array representation.
        let approved = serde_json::to_vec(&(
            BASE_POLICY,
            package,
            &origin,
            &declaration,
            "unit",
            source.canonical_entries(),
            fs::read(package.join("manifest.json")).unwrap(),
        ))
        .unwrap();
        assert_eq!(digest, hex::encode(Sha256::digest(approved)));
        fs::remove_file(package.join("plugin.ts")).unwrap();
        assert_eq!(
            digest,
            super::approval_digest(package, &origin, &declaration, BASE_POLICY, "unit", &source,)
                .unwrap()
        );
        assert_eq!(
            Declaration::evaluate_snapshot("@test", &source)
                .unwrap()
                .id(),
            "@test:snapshot"
        );
    }

    #[test]
    fn approval_binds_every_selected_source_module() {
        let directory = tempfile::tempdir().unwrap();
        let package = directory.path();
        fs::write(
            package.join("plugin.ts"),
            "import { title } from './helper.ts'; export const name = title;",
        )
        .unwrap();
        fs::write(package.join("helper.ts"), "export const title = 'first';").unwrap();
        fs::write(package.join("manifest.json"), "approved manifest").unwrap();
        let declaration = Declaration::evaluate_snapshot(
            "@test",
            &SourceSnapshot::package_plugin(
                package,
                "plugin.ts",
                &["helper.ts".into(), "plugin.ts".into()],
            )
            .unwrap(),
        )
        .unwrap();
        let origin = Provenance::RawCache {
            cache_url: "file:///cache".into(),
        };
        let digest = super::approval_digest(
            package,
            &origin,
            &declaration,
            BASE_POLICY,
            "unit",
            &SourceSnapshot::package_plugin(
                package,
                "plugin.ts",
                &["helper.ts".into(), "plugin.ts".into()],
            )
            .unwrap(),
        )
        .unwrap();
        fs::write(package.join("helper.ts"), "export const title = 'second';").unwrap();
        let changed = super::approval_digest(
            package,
            &origin,
            &declaration,
            BASE_POLICY,
            "unit",
            &SourceSnapshot::package_plugin(
                package,
                "plugin.ts",
                &["helper.ts".into(), "plugin.ts".into()],
            )
            .unwrap(),
        )
        .unwrap();
        assert_ne!(digest, changed);
    }

    #[test]
    fn approval_binds_source_release_archive_plugin_identity_authority_and_policy_version_not_staging(
    ) {
        let declaration = Declaration::evaluate(
            "@test",
            "export const name = 'plugin'; export const services = [];",
        )
        .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let package_path = directory.path().join("package");
        let other_package = directory.path().join("other-package");
        for path in [&package_path, &other_package] {
            fs::create_dir(path).unwrap();
            fs::write(path.join("plugin.ts"), "approved source").unwrap();
            fs::write(path.join("manifest.json"), "approved manifest").unwrap();
        }
        let package = package_path.as_path();
        let origin = Provenance::Repository {
            source_url: "https://a.example/catalog".into(),
            plugin_id: declaration.id(),
            release_version: "1".into(),
            platform: crate::provenance::current_platform().into(),
            archive_sha256: "a".repeat(64),
        };
        let digest = approval_digest(package, &origin, &declaration, "effective unit").unwrap();
        assert_eq!(
            digest,
            approval_digest(package, &origin.clone(), &declaration, "effective unit").unwrap()
        );
        fs::write(package.join("plugin.ts"), "different launch callback").unwrap();
        assert_ne!(
            digest,
            approval_digest(package, &origin, &declaration, "effective unit").unwrap()
        );
        fs::write(package.join("plugin.ts"), "approved source").unwrap();
        fs::write(package.join("manifest.json"), "different publisher").unwrap();
        assert_ne!(
            digest,
            approval_digest(package, &origin, &declaration, "effective unit").unwrap()
        );
        fs::write(package.join("manifest.json"), "approved manifest").unwrap();
        for field in ["source_url", "release_version", "archive_sha256"] {
            let mut changed = serde_json::to_value(&origin).unwrap();
            changed[field] = serde_json::Value::String("different".into());
            let changed: Provenance = serde_json::from_value(changed).unwrap();
            assert_ne!(
                digest,
                approval_digest(package, &changed, &declaration, "effective unit").unwrap()
            );
        }
        assert_ne!(
            digest,
            approval_digest(&other_package, &origin, &declaration, "effective unit").unwrap()
        );
        for changed_authority in [
            "# native unit: other.service\neffective unit",
            "effective unit\nEnvironmentFile=/etc/environment",
            "effective unit\nDeviceAllow=/dev/sda rw",
            "effective unit\nDevicePolicy=closed",
            "effective unit\nDevicePolicy=auto",
            "effective unit\nCapabilityBoundingSet=CAP_SYS_BOOT",
        ] {
            assert_ne!(
                digest,
                approval_digest(package, &origin, &declaration, changed_authority).unwrap(),
                "{changed_authority}"
            );
        }
        let closed_devices = approval_digest(
            package,
            &origin,
            &declaration,
            "PrivateDevices=false\nDevicePolicy=closed",
        )
        .unwrap();
        let automatic_devices = approval_digest(
            package,
            &origin,
            &declaration,
            "PrivateDevices=false\nDevicePolicy=auto",
        )
        .unwrap();
        assert_ne!(closed_devices, automatic_devices);
        assert_ne!(
            digest,
            super::approval_digest(
                package,
                &origin,
                &declaration,
                "policy-v999",
                "effective unit",
                &snapshot(package).unwrap(),
            )
            .unwrap()
        );
        let other_plugin = Declaration::evaluate(
            "@other",
            "export const name = 'plugin'; export const services = [];",
        )
        .unwrap();
        assert_ne!(
            digest,
            approval_digest(package, &origin, &other_plugin, "effective unit").unwrap()
        );
        assert_ne!(
            digest,
            approval_digest(
                package,
                &Provenance::RawCache {
                    cache_url: "file:///staging/cache".into()
                },
                &declaration,
                "effective unit"
            )
            .unwrap()
        );
    }
}
