use crate::{
    declaration::Declaration, process, provenance::Provenance, script::source::SourceSnapshot,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    os::{
        fd::AsRawFd,
        unix::fs::{OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    time::Duration,
};

pub const BASE_POLICY: &str = "policy-v2: validated native systemd unit; host hardening drop-in; dynamic unprivileged user; read-only system; private state; private temporary files; no privilege escalation; at most one managed service; host-owned IPv4/IPv6 ports; no install scripts; no host module loading";

pub const ROOT_POLICY: &str = "policy-root-v1: explicit native User=root; device-wide root authority including account switching, host files, devices and network; host-owned service lifecycle, private state and declared IPv4/IPv6 ports; no host module loading";

#[derive(Serialize)]
pub struct Report {
    pub id: String,
    pub package: PathBuf,
    pub provenance: Provenance,
    pub approval: String,
    pub policy: &'static str,
    pub warning: String,
    pub unit: String,
    pub state_directory: String,
    pub runtime_directory: String,
    pub unit_configuration: String,
    pub declaration: Declaration,
    pub native_unit: Option<crate::native_unit::NativeUnit>,
    pub ports: crate::firewall::Ports,
    pub packages: BTreeMap<String, PathBuf>,
    pub files: BTreeMap<String, PathBuf>,
    pub entry: String,
    pub sources: Vec<String>,
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

fn manifest_namespace(package: &Path) -> Result<String, String> {
    Ok(manifest(package)?.publisher.namespace)
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

#[derive(Deserialize)]
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

fn fingerprint(nix: &Path, package: &str, info: &NarInfo) -> Result<String, String> {
    // This is Nix's ValidPathInfo::fingerprint(), not a signature-name check.
    // Nix normalizes its SRI hash to the nix32 form used in that fingerprint.
    if !info.nar_hash.starts_with("sha256-") || info.nar_size == 0 {
        return Err("unsupported NAR fingerprint".into());
    }
    let hash = process::checked(
        nix,
        [
            "--extra-experimental-features",
            "nix-command",
            "hash",
            "convert",
            "--hash-algo",
            "sha256",
            "--to",
            "nix32",
            &info.nar_hash,
        ],
        Duration::from_secs(10),
    )?;
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
pub fn verify_publisher(
    nix: &Path,
    package: &Path,
    source: Option<&str>,
    bindings: &PublisherBindings,
) -> Result<String, String> {
    validate_store_path(package)?;
    let package_text = package.to_str().ok_or("invalid package path")?;
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
    let local = path_info(nix, package_text, None)?;
    let local_fingerprint = fingerprint(nix, package_text, &local)?;
    if signed_by(&local, &local_fingerprint, &key) {
        return Ok(namespace);
    }
    // Store images may omit locally registered signatures. Fetch only metadata
    // from the bound cache, and require it to sign the actual local NAR.
    let remote = path_info(nix, package_text, Some(&binding.cache_url))?;
    if fingerprint(nix, package_text, &remote)? != local_fingerprint
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
    for name in &declaration.services {
        if !manifest.services.contains_key(name) {
            return Err(format!("service {name} has no packaged native unit"));
        }
    }
    Ok((declaration, source))
}

pub fn load(nix: &Path, package: &Path, provenance: Provenance) -> Result<Report, String> {
    let (declaration, source) = load_declaration_snapshot(package)?;
    let id = declaration.id();
    provenance.validate(&id)?;
    let unit = unit_name(&id);
    let manifest = manifest(package)?;
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
    let closure: std::collections::BTreeSet<PathBuf> = closure.lines().map(PathBuf::from).collect();
    for path in &closure {
        validate_store_path(path)?;
    }
    for path in manifest.packages.values() {
        validate_store_path(path)?;
        if !closure.contains(path) || fs::canonicalize(path).map_err(|e| e.to_string())? != *path {
            return Err("package is outside the immutable closure".into());
        }
    }
    for path in manifest.files.values().chain(manifest.services.values()) {
        validate_artifact(path, &closure, false)?;
    }
    let native_unit = declaration
        .services
        .first()
        .map(|name| {
            let bytes = read_regular(
                &fs::canonicalize(&manifest.services[name]).map_err(|e| e.to_string())?,
                128 * 1024,
            )?;
            let unit = crate::native_unit::NativeUnit::parse(
                std::str::from_utf8(&bytes).map_err(|_| "native unit is not UTF-8")?,
            )?;
            for executable in &unit.executables {
                validate_artifact(Path::new(executable), &closure, true)?;
            }
            Ok::<_, String>(unit)
        })
        .transpose()?;
    if native_unit.is_none() && !manifest.ports.is_empty() {
        return Err("ports require an active service contribution".into());
    }
    let capabilities = native_unit
        .as_ref()
        .map(|u| u.capabilities.as_slice())
        .unwrap_or_default();
    let warning = if native_unit.is_none() {
        "No native service is activated by this package. Approval covers the exact declaration and launch callback source. Game launch effects can access the runtime user's files and display; they must run in the existing runtime-user systemd sandbox, never as the administrator."
    } else if native_unit
        .as_ref()
        .is_some_and(|u| u.user.as_deref() == Some("root"))
    {
        "DEVICE-WIDE ROOT AUTHORITY: User=root runs every service command as root. This plugin can read or change all device files, accounts, credentials, devices and network policy, and start administrative sessions. It is NOT confined by the default dynamic-user sandbox. Approval grants this authority only to this exact plugin build."
    } else if capabilities.iter().any(|c| c == "CAP_NET_ADMIN") {
        "HOST NETWORK ADMINISTRATION: this daemon can change host routes, interfaces and firewall rules. It can interrupt connectivity or redirect traffic. This access is not confined to its own interface."
    } else if capabilities.iter().any(|c| c == "CAP_NET_RAW") {
        "RAW HOST NETWORK: this daemon can create raw sockets and observe or forge IP traffic."
    } else {
        "This daemon has ordinary host-network access. It has no Linux capabilities."
    };
    let mut report = Report {
        id,
        package: package.into(),
        provenance,
        approval: String::new(),
        policy: if native_unit
            .as_ref()
            .is_some_and(|u| u.user.as_deref() == Some("root"))
        {
            ROOT_POLICY
        } else {
            BASE_POLICY
        },
        warning: if native_unit
            .as_ref()
            .is_some_and(|u| !u.credentials.is_empty())
        {
            format!("{warning} NATIVE CREDENTIAL LOOKUP: systemd searches its inherited credentials and credstore for tailscale-authkey. A missing named credential is non-fatal in systemd. Korri supplies no secret and performs no login; Tailscale still needs an explicit operator tailscale up.")
        } else {
            warning.into()
        },
        state_directory: format!("/var/lib/{unit}"),
        runtime_directory: format!("/run/{unit}"),
        unit: format!("{unit}.service"),
        unit_configuration: String::new(),
        declaration,
        native_unit,
        ports: manifest.ports,
        packages: manifest.packages,
        files: manifest.files,
        entry: manifest.entry,
        sources: manifest.sources,
    };
    report.unit_configuration = crate::unit::render(&report)?;
    report.approval = approval_digest(
        &report.package,
        &report.provenance,
        &report.declaration,
        &report.unit_configuration,
        &source,
    )?;
    Ok(report)
}

fn approval_digest(
    package: &Path,
    provenance: &Provenance,
    declaration: &Declaration,
    unit: &str,
    source: &SourceSnapshot,
) -> Result<String, String> {
    let sources = source.canonical_entries();
    let manifest = read_regular(&package.join("manifest.json"), MANIFEST_BYTES)?;
    let bytes = serde_json::to_vec(&(
        BASE_POLICY,
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

    fn snapshot(package: &Path) -> Result<SourceSnapshot, String> {
        SourceSnapshot::package_plugin(package, "plugin.ts", &["plugin.ts".into()])
    }

    fn approval_digest(
        package: &Path,
        provenance: &Provenance,
        declaration: &Declaration,
        unit: &str,
    ) -> Result<String, String> {
        super::approval_digest(package, provenance, declaration, unit, &snapshot(package)?)
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
            super::approval_digest(package, &origin, &declaration, "unit", &source).unwrap();
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
            super::approval_digest(package, &origin, &declaration, "unit", &source).unwrap()
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
    fn approval_binds_source_release_archive_package_and_effective_policy_not_staging() {
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
        assert_ne!(
            digest,
            approval_digest(package, &origin, &declaration, "new policy").unwrap()
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
