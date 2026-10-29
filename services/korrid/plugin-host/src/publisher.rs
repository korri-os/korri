//! Build-side publication of a real declaration and complete content-addressed
//! closure. The caller supplies a release label and intended HTTPS asset URL.

use crate::{
    archive,
    catalog::{
        validate_archive_url, validate_platform, validate_release_version, Catalog, CatalogRecord,
    },
    package, process,
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub struct PublishArgs {
    pub nix: PathBuf,
    pub secret_key: Option<PathBuf>,
    pub self_contained: bool,
    pub input_store_path: PathBuf,
    pub release_version: String,
    pub platform: String,
    pub archive_url: String,
    pub output_dir: PathBuf,
}

#[derive(Debug)]
pub struct PublishResult {
    pub ca_store_path: String,
    pub archive_path: PathBuf,
    pub archive_sha256: String,
    pub record: CatalogRecord,
}

/// The upload workflow and archive producer share this collision-free name.
pub fn archive_name(id: &str, release: &str, platform: &str) -> Result<String, String> {
    crate::declaration::validate_id(id)?;
    validate_release_version(release)?;
    validate_platform(platform)?;
    Ok(format!(
        "{}-{release}-{platform}.tar",
        package::unit_name(id)
    ))
}

fn read_metadata(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("publication metadata exceeds 4 MiB".into());
    }
    Ok(bytes)
}

pub struct ArchiveResult {
    pub ca_store_path: String,
    pub archive_path: PathBuf,
    pub archive_sha256: String,
    pub platform: String,
    pub declaration: crate::declaration::Declaration,
    pub public_key: Option<String>,
}

pub fn publish(args: &PublishArgs) -> Result<PublishResult, String> {
    validate_archive_url(&args.archive_url)?;
    let result = publish_archive(args)?;
    let record = CatalogRecord {
        plugin_id: result.declaration.id(),
        title: result.declaration.title,
        description: result.declaration.description,
        release_version: args.release_version.clone(),
        platform: result.platform.clone(),
        store_path: result.ca_store_path.clone(),
        archive_url: format!(
            "{}{}",
            args.archive_url,
            result
                .archive_path
                .file_name()
                .ok_or("invalid archive filename")?
                .to_str()
                .ok_or("invalid archive name")?
        ),
        archive_sha256: result.archive_sha256.clone(),
        public_key: result.public_key,
    };
    record.validate()?;
    Ok(PublishResult {
        ca_store_path: result.ca_store_path,
        archive_path: result.archive_path,
        archive_sha256: result.archive_sha256,
        record,
    })
}

pub fn publish_archive(args: &PublishArgs) -> Result<ArchiveResult, String> {
    publish_archive_with_upstream(args, None)
}

pub(crate) fn publish_archive_with_upstream(
    args: &PublishArgs,
    upstream: Option<&mut crate::slim::Upstream>,
) -> Result<ArchiveResult, String> {
    package::validate_store_path(&args.input_store_path)?;
    validate_release_version(&args.release_version)?;
    validate_platform(&args.platform)?;
    let nix = package::tools(&args.nix)?;
    let output_dir =
        fs::canonicalize(&args.output_dir).map_err(|e| format!("invalid output directory: {e}"))?;
    if !output_dir.is_dir() {
        return Err("output must be an existing directory".into());
    }
    let public_key = args
        .secret_key
        .as_deref()
        .map(|key| {
            let secret = read_metadata(key)?;
            public_half(
                std::str::from_utf8(&secret)
                    .map_err(|_| "invalid secret key")?
                    .trim(),
            )
        })
        .transpose()?;
    let declaration = package::load_declaration(&args.input_store_path)?;
    // Reuse the host's collision-free identity name, not a lossy punctuation
    // replacement that conflates different plugin namespaces.
    let input_info = package::path_info(
        &nix,
        args.input_store_path
            .to_str()
            .ok_or("invalid package path")?,
        None,
    )?;
    let self_contained = args.self_contained
        || input_info.references.is_empty()
        || input_info.ca.as_deref().is_some_and(|ca| !ca.is_empty())
        || public_key.is_none();
    let ca_store_path = if self_contained {
        content_addressed(&nix, &args.input_store_path)?
    } else {
        args.input_store_path
            .to_str()
            .ok_or("invalid package path")?
            .to_owned()
    };
    let platform = if package::architecture_independent(Path::new(&ca_store_path))?
        && package::no_references(&nix, Path::new(&ca_store_path))?
    {
        "any"
    } else {
        if args.platform == "any" {
            return Err("any-architecture package has references or manifest paths".into());
        }
        &args.platform
    };
    let archive_path = output_dir.join(archive_name(
        &declaration.id(),
        &args.release_version,
        platform,
    )?);
    if fs::symlink_metadata(&archive_path).is_ok() {
        return Err("publication output already exists".into());
    }
    if let Some(key) = &args.secret_key {
        process::checked_uncapped(
            &nix,
            [
                "--extra-experimental-features",
                "nix-command",
                "store",
                "sign",
                "--recursive",
                "--key-file",
                key.to_str().ok_or("invalid secret key path")?,
                &ca_store_path,
            ],
            None,
        )?;
    }
    // A private export is disposable on every failure, so a retry cannot pick
    // up half an earlier cache or leave a persistent unpacked copy behind.
    let export = tempfile::tempdir_in(&output_dir).map_err(|e| e.to_string())?;
    let cache_dir = export.path().join("cache");
    fs::create_dir(&cache_dir).map_err(|e| e.to_string())?;
    let cache_uri = format!("{}?compression=xz", archive::path_to_file_uri(&cache_dir)?);
    let mut outputs = package::StoreSnapshot::query(&nix, &[Path::new(&ca_store_path)])?
        .closure(Path::new(&ca_store_path))?
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let mut upstream_notes = Vec::new();
    if !self_contained {
        let mut default_upstream;
        let upstream = match upstream {
            Some(upstream) => upstream,
            None => {
                let curl = std::env::var_os("KORRI_PLUGIN_CURL")
                    .ok_or("publisher must supply KORRI_PLUGIN_CURL")?;
                default_upstream = crate::slim::Upstream::new(
                    &nix,
                    &crate::repository::Configuration {
                        curl: package::tools(Path::new(&curl))?,
                        ca_bundle: None,
                        official: None,
                    },
                )?;
                &mut default_upstream
            }
        };
        let keys = crate::slim::normal_keys(
            &nix,
            public_key
                .as_deref()
                .ok_or("slim archive requires a signing key")?,
        )?;
        let mut retained = Vec::new();
        for path in outputs {
            match upstream.info(&path)? {
                Some(remote) => {
                    let local = package::path_info(&nix, &path, None)?;
                    if package::fingerprint(&path, &local)? != package::fingerprint(&path, &remote)?
                    {
                        return Err(format!("upstream NAR identity differs for {path}"));
                    }
                    if !crate::slim::signed(&path, &remote, &keys)? {
                        return Err(format!(
                            "upstream path {path} lacks a normal trusted cache signature"
                        ));
                    }
                }
                None => retained.push(path),
            }
        }
        outputs = retained;
        if !outputs.iter().any(|p| p == &ca_store_path) {
            return Err("plugin root must not be served by cache.nixos.org".into());
        }
        upstream_notes = upstream.stage_metadata(&cache_dir)?;
    }
    process::checked_uncapped(
        &nix,
        [
            "--extra-experimental-features",
            "nix-command",
            "copy",
            "--no-recursive",
            "--to",
            &cache_uri,
        ]
        .into_iter()
        .chain(outputs.iter().map(String::as_str)),
        None,
    )?;
    for note in upstream_notes {
        fs::remove_file(note).map_err(|e| e.to_string())?;
    }
    if public_key.is_none() {
        // Nix retains signatures already registered for the same CA bytes.
        // They are not this unsigned publication's publisher evidence.
        for entry in fs::read_dir(&cache_dir).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().is_some_and(|ext| ext == "narinfo") {
                let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
                fs::write(
                    &path,
                    text.lines()
                        .filter(|line| !line.starts_with("Sig:"))
                        .collect::<Vec<_>>()
                        .join("\n")
                        + "\n",
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    if self_contained {
        archive::preflight_cache(&nix, &ca_store_path, &cache_dir)?;
    } else {
        archive::preflight_slim(&nix, &outputs, &cache_dir)?;
    }
    let converted = package::load_declaration(Path::new(&ca_store_path))?;
    if converted.id() != declaration.id() {
        return Err("conversion changed plugin identity".into());
    }
    let archive_sha256 = archive::pack(&cache_dir, &archive_path)?;
    Ok(ArchiveResult {
        ca_store_path,
        archive_path,
        archive_sha256,
        platform: platform.into(),
        declaration: converted,
        public_key,
    })
}

fn content_addressed(nix: &Path, package: &Path) -> Result<String, String> {
    package::validate_store_path(package)?;
    let input = package.to_str().ok_or("non-UTF-8 store path")?;
    let json = process::checked_uncapped(
        nix,
        [
            "--extra-experimental-features",
            "nix-command",
            "store",
            "make-content-addressed",
            "--json",
            input,
        ],
        None,
    )?;
    #[derive(Deserialize)]
    struct Rewrites {
        rewrites: HashMap<String, String>,
    }
    let rewrites: Rewrites =
        serde_json::from_str(&json).map_err(|e| format!("invalid Nix rewrite response: {e}"))?;
    let ca_store_path = rewrites
        .rewrites
        .get(input)
        .ok_or("Nix did not return the converted package")?
        .clone();
    package::validate_store_path(Path::new(&ca_store_path))?;
    Ok(ca_store_path)
}

/// Nix secret keys contain an Ed25519 seed followed by its public half.
pub fn public_half(secret: &str) -> Result<String, String> {
    use base64::Engine;
    let (label, encoded) = secret.split_once(':').ok_or("invalid Nix secret key")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "invalid Nix secret key encoding")?;
    if bytes.len() != 64 {
        return Err("Nix secret key must contain 64 bytes".into());
    }
    ring::signature::Ed25519KeyPair::from_seed_and_public_key(&bytes[..32], &bytes[32..])
        .map_err(|_| "Nix secret key public half does not match its seed")?;
    let key = format!(
        "{label}:{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes[32..])
    );
    package::public_key_bytes(&key)?;
    Ok(key)
}

pub fn publication_platform(
    nix: &Path,
    package: &Path,
    requested: Option<&str>,
) -> Result<String, String> {
    if let Some(platform) = requested {
        validate_platform(platform)?;
    }
    if package::architecture_independent(package)? && package::no_references(nix, package)? {
        return select_platform(package, true, None, requested);
    }
    // Nix can import a valid output without its build history. Ask for valid
    // derivers explicitly; do not classify failures by their diagnostic text.
    let derivers = process::checked_uncapped(
        &nix.with_file_name("nix-store"),
        [
            "--query",
            "--valid-derivers",
            package.to_str().ok_or("invalid path")?,
        ],
        None,
    )?;
    if derivers.trim().is_empty() {
        return select_platform(package, false, None, requested);
    }
    let derivation = process::checked_uncapped(
        nix,
        [
            "--extra-experimental-features",
            "nix-command",
            "derivation",
            "show",
        ]
        .into_iter()
        .chain(derivers.split_whitespace()),
        None,
    )?;
    // Observed Nix derivation-show JSON: the versioned response contains a
    // derivations map; each record's system is the builder's Nix platform.
    #[derive(Deserialize)]
    struct Derivations {
        derivations: BTreeMap<String, DerivationPlatform>,
    }
    #[derive(Deserialize)]
    struct DerivationPlatform {
        system: String,
    }
    let response: Derivations = serde_json::from_str(&derivation).map_err(|e| e.to_string())?;
    let systems: HashSet<_> = response
        .derivations
        .values()
        .map(|d| d.system.as_str())
        .collect();
    if systems.len() != 1 {
        return Err("cannot detect the native package platform from its derivation".into());
    }
    let system = systems.into_iter().next().unwrap();
    select_platform(package, false, Some(system), requested)
}

fn select_platform(
    package: &Path,
    architecture_independent: bool,
    known: Option<&str>,
    requested: Option<&str>,
) -> Result<String, String> {
    if let Some(platform) = requested {
        validate_platform(platform)?;
    }
    if architecture_independent {
        return Ok("any".into());
    }
    if let Some(known) = known {
        validate_platform(known)?;
        if let Some(requested) = requested {
            if known != requested {
                return Err(format!(
                    "platform of {} is {known}, but --platform is {requested}",
                    package.display()
                ));
            }
        }
        return Ok(known.into());
    }
    match requested {
        Some("any") => Err(format!(
            "{} is not architecture-independent; --platform any is not valid",
            package.display()
        )),
        Some(platform) => Ok(platform.into()),
        None => Err(format!(
            "cannot tell the platform of {}; pass --platform",
            package.display()
        )),
    }
}

pub struct RepositoryArgs<'a> {
    pub nix: &'a Path,
    pub secret_key: Option<&'a Path>,
    pub release: &'a str,
    pub base_url: &'a str,
    pub output: &'a Path,
    pub packages: &'a [PathBuf],
    pub platform: Option<&'a str>,
    pub self_contained: bool,
}

pub fn publish_repository(args: &RepositoryArgs<'_>) -> Result<Catalog, String> {
    let RepositoryArgs {
        nix,
        secret_key,
        release,
        base_url,
        output,
        packages,
        platform,
        self_contained,
    } = *args;
    validate_archive_url(base_url)?;
    let base = url::Url::parse(base_url).map_err(|e| e.to_string())?;
    if !base_url.ends_with('/') || base.query().is_some() {
        return Err("base URL must end in / and have no query".into());
    }
    if packages.is_empty() || packages.len() > 10_000 {
        return Err("supply 1–10,000 plugin outputs".into());
    }
    fs::create_dir_all(output).map_err(|e| e.to_string())?;
    let mut records = Vec::new();
    let mut seen_inputs = HashSet::new();
    for package in packages {
        if !seen_inputs.insert(package.clone()) {
            continue;
        }
        let platform = publication_platform(nix, package, platform)?;
        let converted =
            if self_contained || secret_key.is_none() || package::no_references(nix, package)? {
                content_addressed(nix, package)?
            } else {
                package.to_string_lossy().into_owned()
            };
        if records
            .iter()
            .any(|r: &CatalogRecord| r.store_path == converted)
        {
            continue;
        }
        let result = publish(&PublishArgs {
            nix: nix.into(),
            secret_key: secret_key.map(PathBuf::from),
            self_contained,
            input_store_path: converted.into(),
            release_version: release.into(),
            platform,
            archive_url: base_url.into(),
            output_dir: output.into(),
        })?;
        if records.iter().any(|r: &CatalogRecord| {
            r.plugin_id == result.record.plugin_id && r.platform == result.record.platform
        }) {
            return Err("duplicate release output".into());
        }
        records.push(result.record);
    }
    let latest = records
        .iter()
        .map(|r| (r.plugin_id.clone(), release.to_owned()))
        .collect();
    let catalog = Catalog { records, latest };
    crate::storage::write_atomic_mode(
        &output.join("catalog.json"),
        &catalog.to_json_pretty()?,
        0o644,
    )?;
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn output() -> &'static Path {
        Path::new("/nix/store/00000000000000000000000000000000-plugin")
    }

    #[test]
    fn eligible_source_only_outputs_always_get_any() {
        for explicit in [None, Some("aarch64-linux"), Some("any")] {
            assert_eq!(
                select_platform(output(), true, Some("x86_64-linux"), explicit).unwrap(),
                "any"
            );
        }
    }

    #[test]
    fn known_derivation_system_wins_and_mismatches_name_both_values() {
        for explicit in [None, Some("x86_64-linux")] {
            assert_eq!(
                select_platform(output(), false, Some("x86_64-linux"), explicit).unwrap(),
                "x86_64-linux"
            );
        }
        let error = select_platform(output(), false, Some("x86_64-linux"), Some("aarch64-linux"))
            .unwrap_err();
        assert!(
            error.contains(output().to_str().unwrap())
                && error.contains("x86_64-linux")
                && error.contains("aarch64-linux")
        );
    }

    #[test]
    fn imported_native_output_without_a_deriver_needs_an_explicit_platform() {
        assert_eq!(
            select_platform(output(), false, None, Some("aarch64-linux")).unwrap(),
            "aarch64-linux"
        );
        assert_eq!(
            select_platform(output(), false, None, None).unwrap_err(),
            format!(
                "cannot tell the platform of {}; pass --platform",
                output().display()
            )
        );
    }

    #[test]
    fn requested_platform_is_validated_and_any_cannot_describe_native_outputs() {
        assert!(select_platform(output(), false, None, Some("invalid")).is_err());
        assert!(select_platform(output(), false, None, Some("any")).is_err());
        assert!(select_platform(output(), true, None, Some("invalid")).is_err());
    }

    #[test]
    #[ignore = "requires builder fixtures and writable build-machine Nix; no target builds"]
    fn direction_imported_native_outputs_publish_without_derivers() {
        let nix = PathBuf::from(std::env::var_os("KORRI_PUBLISH_NIX").expect("immutable Nix"));
        let fixtures = PathBuf::from(
            std::env::var_os("KORRI_TEST_DEPENDENCY_FIXTURES").expect("builder fixtures"),
        );
        let original = fs::canonicalize(fixtures.join("native")).unwrap();
        let imported = content_addressed(&nix, &original).unwrap();
        assert!(process::checked_uncapped(
            &nix.with_file_name("nix-store"),
            ["--query", "--valid-derivers", imported.as_str()],
            None
        )
        .unwrap()
        .trim()
        .is_empty());
        let platform = crate::provenance::current_platform();
        assert_eq!(
            publication_platform(&nix, Path::new(&imported), None).unwrap_err(),
            format!("cannot tell the platform of {imported}; pass --platform")
        );
        assert_eq!(
            publication_platform(&nix, Path::new(&imported), Some(platform)).unwrap(),
            platform
        );
        let root = tempfile::tempdir().unwrap();
        let secret = process::checked_uncapped(
            &nix,
            [
                "--extra-experimental-features",
                "nix-command",
                "key",
                "generate-secret",
                "--key-name",
                "imported-native-output",
            ],
            None,
        )
        .unwrap();
        let key = root.path().join("secret");
        fs::write(&key, secret).unwrap();
        let repository = root.path().join("repository");
        let catalog = publish_repository(&RepositoryArgs {
            nix: &nix,
            secret_key: Some(&key),
            release: "v1",
            base_url: "https://example.invalid/releases/",
            output: &repository,
            packages: &[PathBuf::from(&imported)],
            platform: Some(platform),
            self_contained: true,
        })
        .unwrap();
        assert_eq!(catalog.records[0].store_path, imported);
        assert_eq!(catalog.records[0].platform, platform);
        let archive_path =
            repository.join(catalog.records[0].archive_url.rsplit('/').next().unwrap());
        let cache = root.path().join("cache");
        fs::create_dir(&cache).unwrap();
        crate::archive::extract(&archive_path, &cache).unwrap();
        assert!(package::archive_signature(
            &nix,
            Path::new(&imported),
            &crate::archive::path_to_file_uri(&cache).unwrap(),
            catalog.records[0].public_key.as_deref()
        )
        .unwrap());
    }
}
