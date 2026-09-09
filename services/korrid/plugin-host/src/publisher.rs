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
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

pub struct PublishArgs {
    pub nix: PathBuf,
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

pub fn catalog_from_records(paths: &[PathBuf]) -> Result<Vec<u8>, String> {
    if paths.is_empty() || paths.len() > 10_000 {
        return Err("supply 1–10,000 catalog record files".into());
    }
    let records = paths
        .iter()
        .map(|path| {
            let bytes = read_metadata(path)?;
            serde_json::from_slice::<CatalogRecord>(&bytes).map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;
    let bytes = Catalog { records }.to_json_pretty()?;
    // Enforce the reader's serialized-size limit as well as record validation.
    Catalog::from_json(&bytes)?;
    Ok(bytes)
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

/// Validate a complete single-plugin release before any external write. The
/// expected platforms are the workflow's explicit build matrix, not metadata.
/// Return upload names only after every asset passes the reader's contract.
pub fn release_assets(
    catalog_path: &Path,
    asset_dir: &Path,
    base_url: &str,
    id: &str,
    release: &str,
    platforms: &[String],
) -> Result<Vec<String>, String> {
    validate_archive_url(base_url)?;
    let base = url::Url::parse(base_url).map_err(|e| e.to_string())?;
    if !base_url.ends_with('/') || base.query().is_some() {
        return Err("asset base URL must end in / and have no query".into());
    }
    let catalog = Catalog::from_json(&read_metadata(catalog_path)?)?;
    let expected: HashSet<_> = platforms.iter().collect();
    if expected.is_empty()
        || expected.len() != platforms.len()
        || catalog.records.len() != expected.len()
    {
        return Err("release must contain exactly one record per expected platform".into());
    }
    let mut assets = Vec::new();
    for platform in platforms {
        let record = catalog.record(id, release, platform)?;
        let name = archive_name(id, release, platform)?;
        if record.archive_url != format!("{base_url}{name}") {
            return Err(format!("archive URL does not match uploaded asset {name}"));
        }
        let path = asset_dir.join(&name);
        let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if !metadata.is_file()
            || metadata.len() == 0
            || metadata.len() >= archive::MAX_ARCHIVE_BYTES
        {
            return Err(format!(
                "asset {name} must be a nonempty regular file smaller than 2 GiB"
            ));
        }
        let mut file = fs::File::open(&path).map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        if hex::encode(hash.finalize()) != record.archive_sha256 {
            return Err(format!("archive hash mismatch for {name}"));
        }
        assets.push(name);
    }
    Ok(assets)
}

pub fn publish(args: &PublishArgs) -> Result<PublishResult, String> {
    package::validate_store_path(&args.input_store_path)?;
    validate_release_version(&args.release_version)?;
    validate_platform(&args.platform)?;
    validate_archive_url(&args.archive_url)?;
    let nix = package::tools(&args.nix)?;
    let output_dir =
        fs::canonicalize(&args.output_dir).map_err(|e| format!("invalid output directory: {e}"))?;
    if !output_dir.is_dir() {
        return Err("output must be an existing directory".into());
    }
    let declaration = package::load_declaration(&args.input_store_path)?;
    // Reuse the host's collision-free identity name, not a lossy punctuation
    // replacement that conflates different plugin namespaces.
    let archive_path = output_dir.join(archive_name(
        &declaration.id(),
        &args.release_version,
        &args.platform,
    )?);
    if fs::symlink_metadata(&archive_path).is_ok() {
        return Err("publication output already exists".into());
    }
    let input = args
        .input_store_path
        .to_str()
        .ok_or("non-UTF-8 store path")?;
    let json = process::checked(
        &nix,
        [
            "--extra-experimental-features",
            "nix-command",
            "store",
            "make-content-addressed",
            "--json",
            input,
        ],
        Duration::from_secs(300),
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
    // A private export is disposable on every failure, so a retry cannot pick
    // up half an earlier cache or leave a persistent unpacked copy behind.
    let export = tempfile::tempdir_in(&output_dir).map_err(|e| e.to_string())?;
    let cache_dir = export.path().join("cache");
    fs::create_dir(&cache_dir).map_err(|e| e.to_string())?;
    let cache_uri = format!("{}?compression=xz", archive::path_to_file_uri(&cache_dir)?);
    process::checked(
        &nix,
        [
            "--extra-experimental-features",
            "nix-command",
            "copy",
            "--to",
            &cache_uri,
            &ca_store_path,
        ],
        Duration::from_secs(600),
    )?;
    archive::preflight_cache(&nix, &ca_store_path, &cache_dir, archive::MAX_NAR_BYTES)?;
    let converted = package::load_declaration(Path::new(&ca_store_path))?;
    if converted.id() != declaration.id() {
        return Err("conversion changed plugin identity".into());
    }
    let archive_sha256 = archive::pack(&cache_dir, &archive_path)?;
    let record = CatalogRecord {
        plugin_id: converted.id(),
        title: converted.title,
        description: converted.description,
        release_version: args.release_version.clone(),
        platform: args.platform.clone(),
        store_path: ca_store_path.clone(),
        archive_url: args.archive_url.clone(),
        archive_sha256: archive_sha256.clone(),
    };
    record.validate()?;
    Ok(PublishResult {
        ca_store_path,
        archive_path,
        archive_sha256,
        record,
    })
}
