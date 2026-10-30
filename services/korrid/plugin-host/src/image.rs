//! Build-host acquisition and offline proof registration for catalog images.
//! These commands never install a receipt or activate a plugin. Nix alone
//! imports and registers store paths; the runtime still checks every approval.
use crate::{
    archive,
    catalog::{Catalog, CatalogRecord},
    declaration, package, process,
    repository::{self, Configuration, SourceUrl},
    slim,
};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

pub fn record<'a>(
    catalog: &'a Catalog,
    id: &str,
    platform: &str,
    bindings: &package::PublisherBindings,
) -> Result<&'a CatalogRecord, String> {
    declaration::validate_id(id)?;
    if !id.starts_with("@korri:") {
        return Err("product images select only @korri plugins".into());
    }
    let release = catalog
        .latest
        .get(id)
        .ok_or_else(|| format!("missing latest mark for {id}"))?;
    let record = catalog.record(id, release, platform)?;
    let bound = bindings
        .get("@korri")
        .ok_or("image publisher @korri is not bound")?;
    if record.public_key.as_deref() != Some(&bound.public_key) {
        return Err("image catalog publicKey differs from the bound @korri key".into());
    }
    Ok(record)
}

/// Same container confinement and digest checks as repository installation.
pub fn extract(input: &Path, sha256: &str, output: &Path) -> Result<(), String> {
    repository::verify_archive_hash(input, sha256)?;
    fs::create_dir(output).map_err(|e| e.to_string())?;
    archive::extract(input, output)
}

pub fn run(nix: &Path, config: &Configuration, args: &[String]) -> Result<(), String> {
    let [mode, bindings, platform, catalog, source, rest @ ..] = args else {
        return Err(
            "image prepare|proofs BINDINGS PLATFORM CATALOG SOURCE_URL ID ARCHIVE_OR_CACHE..."
                .into(),
        );
    };
    if !matches!(mode.as_str(), "prepare" | "proofs") || rest.is_empty() || rest.len() % 2 != 0 {
        return Err(
            "image requires prepare|proofs and one ID/input pair per selected plugin".into(),
        );
    }
    if !matches!(platform.as_str(), "aarch64-linux" | "x86_64-linux") {
        return Err("image requires a supported Linux platform".into());
    }
    SourceUrl::parse(source)?;
    let bindings = package::publisher_bindings(Path::new(bindings))?;
    let catalog = Catalog::from_json(&fs::read(catalog).map_err(|e| e.to_string())?)?;
    let mut upstream = slim::Upstream::new(nix, config)?;
    upstream.exclude_publishers(bindings.values().map(|b| b.public_key.as_str()))?;
    let mut seen = BTreeSet::new();
    let mut roots = Vec::new();
    // Preserve the image unit's first-registration content check and later
    // immutable-store fast path. Nix's bound signatures are the only cache.
    let mut rehash = mode == "prepare";
    for pair in rest.chunks_exact(2) {
        if !seen.insert(&pair[0]) {
            return Err("duplicate selected image plugin".into());
        }
        let record = record(&catalog, &pair[0], platform, &bindings)?;
        let root = Path::new(&record.store_path);
        let key = record
            .public_key
            .as_deref()
            .ok_or("image release must be signed")?;
        let temporary = tempfile::tempdir().map_err(|e| e.to_string())?;
        let cache = if mode == "prepare" {
            let cache = temporary.path().join("cache");
            extract(Path::new(&pair[1]), &record.archive_sha256, &cache)?;
            cache
        } else {
            PathBuf::from(&pair[1])
        };
        let uri = archive::path_to_file_uri(&cache)?;
        if !package::archive_signature(nix, root, &uri, Some(key)).map_err(|e| e.to_string())? {
            return Err("image archive requires the bound publisher signature".into());
        }
        let info = package::path_info(nix, &record.store_path, Some(&uri))?;
        let paths = archive::cache_paths(&cache)?;
        let ca = info.ca.as_deref().is_some_and(|ca| !ca.is_empty());
        if mode == "prepare" {
            if ca {
                archive::preflight_cache(nix, &record.store_path, &cache)?;
            } else {
                archive::preflight_slim(nix, &paths, &cache)?;
            }
            if ca {
                package::import_archive(nix, &uri, root)?;
            } else {
                slim::import(nix, &cache, root, key, &mut upstream, &BTreeSet::new())?;
            }
        }
        // An image starts with Nix's unsigned closureInfo registration. Check
        // each archive path against those exact bytes before retaining its Sig.
        // Omitted upstream paths remain root-registered, not publisher-signed.
        let key_bytes = package::public_key_bytes(key)?;
        for path in &paths {
            let supplied = package::path_info(nix, path, Some(&uri))?;
            let actual = package::path_info(nix, path, None)?;
            let fingerprint = package::fingerprint(path, &actual)?;
            rehash |= !package::signed_by(&actual, &fingerprint, &key_bytes);
            if package::fingerprint(path, &supplied)? != fingerprint
                || !package::signed_by(&supplied, &fingerprint, &key_bytes)
            {
                return Err(format!(
                    "image proof for {path} does not sign the actual store bytes"
                ));
            }
        }
        copy_proofs(nix, &uri, key, &paths)?;
        let mut verify = vec![
            "--extra-experimental-features".into(),
            "nix-command".into(),
            "--offline".into(),
            "--option".into(),
            "substituters".into(),
            "".into(),
            "--option".into(),
            "extra-substituters".into(),
            "".into(),
            "--option".into(),
            "trusted-public-keys".into(),
            key.into(),
            "--option".into(),
            "extra-trusted-public-keys".into(),
            "".into(),
            "store".into(),
            "verify".into(),
            "--sigs-needed".into(),
            "1".into(),
            "--no-contents".into(),
        ];
        roots.extend(paths.iter().cloned());
        verify.extend(paths);
        process::checked_uncapped(nix, verify, None)?;
        let declaration = package::load_declaration(root)?;
        if declaration.id() != record.plugin_id {
            return Err("image catalog identity differs from declaration".into());
        }
        if record.platform == "any"
            && (!package::architecture_independent(root)? || !package::no_references(nix, root)?)
        {
            return Err("any-architecture image package has references or manifest paths".into());
        }
        roots.push(record.store_path.clone());
    }
    let mut verify = vec![
        "--extra-experimental-features".into(),
        "nix-command".into(),
        "--offline".into(),
        "--option".into(),
        "substituters".into(),
        "".into(),
        "--option".into(),
        "extra-substituters".into(),
        "".into(),
        "store".into(),
        "verify".into(),
        "--recursive".into(),
        "--no-trust".into(),
    ];
    verify.extend(roots);
    if rehash {
        process::checked_uncapped(nix, verify, None)?;
    }
    // Make local proof registration durable before recovery can use it.
    package::sync_store()?;
    Ok(())
}

fn copy_proofs(nix: &Path, uri: &str, key: &str, paths: &[String]) -> Result<(), String> {
    let mut args = vec![
        "--extra-experimental-features".into(),
        "nix-command".into(),
        "--offline".into(),
        "--option".into(),
        "substituters".into(),
        "".into(),
        "--option".into(),
        "extra-substituters".into(),
        "".into(),
        "--option".into(),
        "trusted-public-keys".into(),
        key.into(),
        "--option".into(),
        "extra-trusted-public-keys".into(),
        "".into(),
        "store".into(),
        "copy-sigs".into(),
        "--substituter".into(),
        uri.into(),
    ];
    args.extend(paths.iter().cloned());
    process::checked_uncapped(nix, args, None)?;
    Ok(())
}

#[cfg(test)]
#[path = "image_tests.rs"]
mod tests;
