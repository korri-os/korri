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
    collections::{BTreeMap, BTreeSet},
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
    let mut expected = BTreeMap::new();
    let mut proofs = Vec::new();
    // Preserve the image unit's first-registration content check and later
    // immutable-store fast path. Nix's bound signatures are the only cache.
    let mut rehash = mode == "prepare";
    let pairs: Vec<&[String]> = rest.chunks_exact(2).collect();
    let (mut supplied_metadata, local_metadata) = if mode == "proofs" {
        proof_metadata(nix, &catalog, platform, &bindings, &pairs)
    } else {
        (pairs.iter().map(|_| None).collect(), None)
    };
    for (index, pair) in pairs.iter().enumerate() {
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
        let paths = archive::cache_paths(&cache)?;
        // One Nix call reads the archive metadata for the root and every path.
        let mut query = paths.clone();
        if !query.contains(&record.store_path) {
            query.push(record.store_path.clone());
        }
        let proof = |error: String| format!("image archive proof for {}: {error}", root.display());
        let supplied = supplied_metadata[index]
            .take()
            .unwrap_or_else(|| package::path_infos(nix, &query, Some(&uri)))
            .map_err(proof)?;
        let info = &supplied[&record.store_path];
        if !package::archive_signature_of(&record.store_path, info, Some(key))
            .map_err(|error| proof(error.to_string()))?
        {
            return Err("image archive requires the bound publisher signature".into());
        }
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
        let actual = match &local_metadata {
            Some(all) => paths
                .iter()
                .map(|path| {
                    Ok((
                        path.clone(),
                        all.get(path).ok_or("missing metadata")?.clone(),
                    ))
                })
                .collect::<Result<BTreeMap<_, _>, String>>()?,
            None => package::path_infos(nix, &paths, None)?,
        };
        for path in &paths {
            let supplied = &supplied[path];
            let actual = &actual[path];
            let fingerprint = package::fingerprint(path, actual)?;
            rehash |= !package::signed_by(actual, &fingerprint, &key_bytes);
            expected.insert(path.clone(), (fingerprint.clone(), key.to_owned()));
            if package::fingerprint(path, supplied)? != fingerprint
                || !package::signed_by(supplied, &fingerprint, &key_bytes)
            {
                return Err(format!(
                    "image proof for {path} does not sign the actual store bytes"
                ));
            }
        }
        roots.extend(paths.iter().cloned());
        proofs.push((uri, key.to_owned(), paths, temporary));
        let declaration = package::load_declaration(root)?;
        if declaration.id() != record.plugin_id {
            return Err("image catalog identity differs from declaration".into());
        }
        if record.platform == "any"
            && (!package::architecture_independent(root)?
                || !match actual.get(&record.store_path) {
                    Some(info) => info.references.is_empty(),
                    None => package::no_references(nix, root)?,
                })
        {
            return Err("any-architecture image package has references or manifest paths".into());
        }
        roots.push(record.store_path.clone());
    }
    roots.sort();
    roots.dedup();
    // Immutable-store fast path: every archive path already carries the bound
    // publisher's signature over its actual local bytes, checked above. A
    // previous boot registered these proofs; there is nothing to register.
    if !rehash {
        return Ok(());
    }
    // Retained signatures are the successful first-registration marker. Never
    // publish that marker before checking the complete physical image closure:
    // otherwise a failed first boot can make an incomplete image pass on retry.
    if rehash {
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
        verify.extend(roots.iter().cloned());
        process::checked_uncapped(nix, verify, None)?;
    }
    for (uri, key, paths, _cache) in proofs {
        copy_proofs(nix, &uri, &key, &paths)?;
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
            key,
            "--option".into(),
            "extra-trusted-public-keys".into(),
            "".into(),
            "store".into(),
            "verify".into(),
            "--sigs-needed".into(),
            "1".into(),
            "--no-contents".into(),
        ];
        verify.extend(paths);
        process::checked_uncapped(nix, verify, None)?;
    }
    let registered =
        package::StoreSnapshot::query(nix, &roots.iter().map(Path::new).collect::<Vec<_>>())?;
    for (path, (fingerprint, key)) in expected {
        let path = Path::new(&path);
        if registered.fingerprint(path)? != fingerprint {
            return Err("image path changed during proof registration".into());
        }
        // CA validity alone is not publisher trust. Require the bound key's
        // retained signature, including for every source-only any release.
        package::verify_local_signature(
            nix,
            Some(&registered),
            path,
            &key,
            package::StoreContents::TrustRegistered,
        )?;
    }
    // Make local proof registration durable before recovery can use it.
    package::sync_store()?;
    Ok(())
}

type Metadata = BTreeMap<String, package::NarInfo>;

/// Read every selected archive's metadata concurrently, one Nix call per
/// archive cache, and the local metadata of all their paths with one call.
/// This only prepares reads: the caller's ordered loop performs every check
/// and reports each refusal. A selection that cannot be prepared here (an
/// invalid record, or a path that is not valid locally) gets `None`, and the
/// loop reads it itself, so its refusal is unchanged.
fn proof_metadata(
    nix: &Path,
    catalog: &Catalog,
    platform: &str,
    bindings: &package::PublisherBindings,
    pairs: &[&[String]],
) -> (Vec<Option<Result<Metadata, String>>>, Option<Metadata>) {
    let supplied = crate::parallel::map(pairs, |pair| {
        let record = record(catalog, &pair[0], platform, bindings).ok()?;
        let cache = PathBuf::from(&pair[1]);
        let uri = archive::path_to_file_uri(&cache).ok()?;
        let mut query = archive::cache_paths(&cache).ok()?;
        if !query.contains(&record.store_path) {
            query.push(record.store_path.clone());
        }
        Some(package::path_infos(nix, &query, Some(&uri)))
    });
    let mut paths = BTreeSet::new();
    for pair in pairs {
        match archive::cache_paths(Path::new(&pair[1])) {
            Ok(listed) => paths.extend(listed),
            Err(_) => return (supplied, None),
        }
    }
    let local = package::path_infos(nix, &paths.into_iter().collect::<Vec<_>>(), None).ok();
    (supplied, local)
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
