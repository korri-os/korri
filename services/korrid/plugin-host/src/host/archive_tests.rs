//! Real Nix proofs use only unique, disposable outputs made by this test.
use super::*;
use crate::{
    publisher::{self, PublishArgs},
    slim,
};
use std::{os::unix::fs::PermissionsExt, process::Command};

fn run(nix: &Path, args: &[&str]) -> String {
    let result = Command::new(nix)
        .args(["--extra-experimental-features", "nix-command"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{:?}: {}{}",
        args,
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().to_owned()
}

/// Cleanup is deliberately independent of fixture assertions. Failed runs must
/// not poison the next run, or turn a cleanup error into a second panic.
fn cleanup_fixture_store(nix: &Path, run_id: &str) {
    let listed = Command::new(nix)
        .args([
            "--extra-experimental-features",
            "nix-command",
            "path-info",
            "--all",
        ])
        .output();
    let output = match listed {
        Ok(output) if output.status.success() => output,
        other => {
            eprintln!("slim fixture {run_id}: cannot list stale outputs: {other:?}");
            return;
        }
    };
    let paths: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|path| {
            Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(run_id))
        })
        .map(str::to_owned)
        .collect();
    cleanup_fixture_paths(nix, run_id, &paths);
}

fn cleanup_fixture_paths(nix: &Path, run_id: &str, paths: &[String]) {
    if paths.is_empty() {
        return;
    }
    match Command::new(nix)
        .args([
            "--extra-experimental-features",
            "nix-command",
            "store",
            "delete",
        ])
        .args(paths)
        .output()
    {
        Ok(output) if output.status.success() => {}
        Ok(output) => eprintln!(
            "slim fixture {run_id}: best-effort store cleanup failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ),
        Err(error) => eprintln!("slim fixture {run_id}: best-effort store cleanup failed: {error}"),
    }
}

struct FixtureRun {
    directory: Option<tempfile::TempDir>,
    nix: PathBuf,
    id: String,
}
impl FixtureRun {
    fn new(nix: &Path) -> Self {
        use ring::rand::{SecureRandom, SystemRandom};
        let mut bytes = [0u8; 16];
        SystemRandom::new().fill(&mut bytes).unwrap();
        let id = format!("{:032x}", u128::from_be_bytes(bytes));
        let directory = tempfile::Builder::new()
            .prefix(&format!("korri-slim-{id}-"))
            .tempdir()
            .unwrap();
        Self {
            directory: Some(directory),
            nix: nix.to_owned(),
            id,
        }
    }
    fn directory(&self) -> &tempfile::TempDir {
        self.directory.as_ref().unwrap()
    }
}
impl Drop for FixtureRun {
    fn drop(&mut self) {
        // Every Host::for_test, selection and candidate root lives below this
        // directory. Unlink those roots before asking Nix to delete outputs.
        if let Some(directory) = self.directory.take() {
            if let Err(error) = directory.close() {
                eprintln!("slim fixture {}: removing roots failed: {error}", self.id);
            }
        }
        cleanup_fixture_store(&self.nix, &self.id);
    }
}

#[test]
fn stale_fixture_cleanup_is_best_effort() {
    let directory = tempfile::tempdir().unwrap();
    let nix = directory.path().join("nix");
    fs::write(&nix, "#!/bin/sh\ncase \" $* \" in *\" path-info --all \"*) echo /nix/store/00000000000000000000000000000000-stale-fixture-run;; *) echo 'still alive' >&2; exit 1;; esac\n").unwrap();
    fs::set_permissions(&nix, fs::Permissions::from_mode(0o755)).unwrap();
    cleanup_fixture_store(&nix, "stale-fixture-run");
    fs::write(&nix, "#!/bin/sh\nexit 1\n").unwrap();
    cleanup_fixture_store(&nix, "stale-fixture-run");
}

/// Publish different, valid NAR bytes under an input-addressed name, signed by
/// the attacker. Nix still produces the NAR and parses the resulting metadata.
fn forge_cache_path(nix: &Path, cache: &Path, target: &str, alternate: &str, secret: &Path) {
    use base64::{engine::general_purpose::STANDARD, Engine};
    run(
        nix,
        &[
            "copy",
            "--to",
            &archive::path_to_file_uri(cache).unwrap(),
            alternate,
        ],
    );
    let name = |path: &str| {
        format!(
            "{}.narinfo",
            &Path::new(path).file_name().unwrap().to_str().unwrap()[..32]
        )
    };
    let original = cache.join(name(alternate));
    let text = fs::read_to_string(&original).unwrap();
    let info = package::path_info(nix, alternate, None).unwrap();
    let secret = fs::read_to_string(secret).unwrap();
    let (label, encoded) = secret.trim().split_once(':').unwrap();
    let bytes = STANDARD.decode(encoded).unwrap();
    let pair =
        ring::signature::Ed25519KeyPair::from_seed_and_public_key(&bytes[..32], &bytes[32..])
            .unwrap();
    let signature = STANDARD.encode(
        pair.sign(package::fingerprint(target, &info).unwrap().as_bytes())
            .as_ref(),
    );
    let mut text = text
        .lines()
        .filter(|line| !line.starts_with("CA:") && !line.starts_with("Sig:"))
        .map(|line| {
            if line.starts_with("StorePath:") {
                format!("StorePath: {target}")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    text.push_str(&format!("\nSig: {label}:{signature}\n"));
    fs::write(cache.join(name(target)), text).unwrap();
    fs::remove_file(original).unwrap();
}

#[test]
#[ignore = "requires writable build-machine Nix; creates and removes only this test's unique outputs"]
fn slim_real_nix_publish_install_security_and_offline() {
    let nix = PathBuf::from(std::env::var_os("KORRI_PUBLISH_NIX").expect("immutable Nix"));
    let helper = PathBuf::from(std::env::var_os("KORRI_TEST_TRUE").expect("immutable coreutils"));
    let fixture = FixtureRun::new(&nix);
    let root = fixture.directory();
    let run_id = &fixture.id;
    let secret = run(
        &nix,
        &["key", "generate-secret", "--key-name", "slim-publisher"],
    );
    let publisher_key = publisher::public_half(&secret).unwrap();
    let key = root.path().join("publisher.secret");
    fs::write(&key, secret).unwrap();
    let secret = run(
        &nix,
        &["key", "generate-secret", "--key-name", "slim-upstream"],
    );
    let upstream_key = publisher::public_half(&secret).unwrap();
    let upstream_secret = root.path().join("upstream.secret");
    fs::write(&upstream_secret, secret).unwrap();
    // An immutable tool binding supplies this test cache's independent normal
    // trusted key. Explicit per-invocation keys in the importer still override it.
    let tools = root.path().join(format!("tools-{run_id}"));
    fs::create_dir(&tools).unwrap();
    let script = tools.join("nix");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nexec {} --option trusted-public-keys '{} {}' \"$@\"\n",
            nix.display(),
            upstream_key,
            publisher_key
        ),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink(nix.with_file_name("nix-store"), tools.join("nix-store")).unwrap();
    let bound_nix = PathBuf::from(run(&nix, &["store", "add-path", tools.to_str().unwrap()]));
    let nix = bound_nix.join("nix");
    let shared_source = root.path().join(format!("shared-{run_id}"));
    fs::write(&shared_source, format!("shared {}", root.path().display())).unwrap();
    let shared = run(
        &nix,
        &["store", "add-path", shared_source.to_str().unwrap()],
    );
    run(
        &nix,
        &[
            "store",
            "sign",
            "--key-file",
            upstream_secret.to_str().unwrap(),
            &shared,
        ],
    );
    let upstream_cache = root.path().join("upstream");
    let upstream_uri = format!("file://{}", upstream_cache.display());
    run(&nix, &["copy", "--to", &upstream_uri, &shared]);
    let source = root.path().join(format!("source-{run_id}"));
    fs::create_dir(&source).unwrap();
    fs::write(source.join("plugin.ts"), "export const name = 'native';").unwrap();
    fs::write(source.join("manifest.json"), serde_json::json!({"publisher":{"namespace":"@slim"},"entry":"plugin.ts","sources":["plugin.ts"],"packages":{"shared":shared}}).to_string()).unwrap();
    let source_path = run(&nix, &["store", "add-path", source.to_str().unwrap()]);
    let shell = fs::canonicalize("/bin/sh").unwrap();
    let expression = format!("derivation {{ name = \"slim-native-{}\"; system = \"{}\"; builder = \"{}\"; args = [\"-c\" \"{} -R $source $out\"]; source = builtins.storePath \"{}\"; shared = builtins.storePath \"{}\"; }}", run_id, current_platform(), shell.display(), helper.with_file_name("cp").display(), source_path, shared);
    let outputs = run(
        &nix,
        &[
            "build",
            "--impure",
            "--option",
            "sandbox",
            "false",
            "--no-link",
            "--json",
            "--expr",
            &expression,
        ],
    );
    let outputs: serde_json::Value = serde_json::from_str(&outputs).unwrap();
    let package = PathBuf::from(outputs[0]["outputs"]["out"].as_str().unwrap());
    let derivation = outputs[0]["drvPath"].as_str().unwrap();
    assert!(package::path_info(&nix, package.to_str().unwrap(), None)
        .unwrap()
        .ca
        .is_none());
    let output = root.path().join("publication");
    fs::create_dir(&output).unwrap();
    let args = PublishArgs {
        nix: nix.clone(),
        secret_key: Some(key.clone()),
        self_contained: false,
        input_store_path: package.clone(),
        release_version: "slim".into(),
        platform: current_platform().into(),
        archive_url: String::new(),
        output_dir: output.clone(),
    };
    let mut upstream = slim::Upstream::file(&nix, &upstream_cache);
    let published = publisher::publish_archive_with_upstream(&args, Some(&mut upstream)).unwrap();
    assert_eq!(published.ca_store_path, package.to_str().unwrap());
    let cache = root.path().join("slim-cache");
    fs::create_dir(&cache).unwrap();
    archive::extract(&published.archive_path, &cache).unwrap();
    assert_eq!(
        archive::cache_paths(&cache).unwrap(),
        [package.to_string_lossy()]
    );
    assert_eq!(
        package::archive_manifest(&nix, &archive::path_to_file_uri(&cache).unwrap(), &package)
            .unwrap()
            .0,
        "@slim"
    );
    // Rule (a): even a publisher-signed copy of a shared path is refused.
    run(
        &nix,
        &[
            "copy",
            "--no-recursive",
            "--to",
            &archive::path_to_file_uri(&cache).unwrap(),
            &shared,
        ],
    );
    let error = slim::import(
        &nix,
        &cache,
        &package,
        &publisher_key,
        &mut upstream,
        &BTreeSet::new(),
    )
    .unwrap_err();
    assert!(error.contains("archive must not supply it"), "{error}");
    let hash = &Path::new(&shared).file_name().unwrap().to_str().unwrap()[..32];
    fs::remove_file(cache.join(format!("{hash}.narinfo"))).unwrap();
    // Delete only the unique unrooted outputs just created by this test. This
    // makes successful installation prove real upstream download, not reuse.
    run(
        &nix,
        &[
            "store",
            "delete",
            package.to_str().unwrap(),
            derivation,
            &shared,
        ],
    );
    assert!(!package.exists());
    assert!(!Path::new(&shared).exists());
    let mut unavailable = slim::Upstream::new(
        &nix,
        &Configuration {
            curl: helper.with_file_name("false"),
            ca_bundle: None,
            official: None,
        },
    )
    .unwrap();
    let error = slim::import(
        &nix,
        &cache,
        &package,
        &publisher_key,
        &mut unavailable,
        &BTreeSet::new(),
    )
    .unwrap_err();
    assert!(error.contains("1 shared packages need https://cache.nixos.org; connect to the network or use a self-contained archive"), "{error}");
    assert!(!package.exists());
    // Rule (a) also applies to already-valid CA dependencies: a publisher
    // signature must never stand in for the upstream key.
    run(&nix, &["copy", "--from", &upstream_uri, &shared]);
    run(
        &nix,
        &[
            "store",
            "sign",
            "--key-file",
            key.to_str().unwrap(),
            &shared,
        ],
    );
    let publisher_shared = root.path().join("publisher-shared");
    let publisher_shared_uri = format!("file://{}", publisher_shared.display());
    run(&nix, &["copy", "--to", &publisher_shared_uri, &shared]);
    for note in fs::read_dir(&publisher_shared)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "narinfo"))
    {
        let text = fs::read_to_string(note.path()).unwrap();
        fs::write(
            note.path(),
            text.lines()
                .filter(|line| {
                    !line.starts_with("Sig:") || line.starts_with("Sig: slim-publisher:")
                })
                .collect::<Vec<_>>()
                .join("\n")
                + "\n",
        )
        .unwrap();
    }
    run(&nix, &["store", "delete", &shared]);
    run(&nix, &["copy", "--from", &publisher_shared_uri, &shared]);
    // Rule (a)(2): a publisher signature is not authority, but the same
    // NAR hash and size authenticated by the upstream key prove these bytes.
    let mut upstream = slim::Upstream::file(&nix, &upstream_cache);
    slim::import(
        &nix,
        &cache,
        &package,
        &publisher_key,
        &mut upstream,
        &BTreeSet::new(),
    )
    .unwrap();
    assert!(package.exists());
    run(
        &nix,
        &["store", "delete", package.to_str().unwrap(), &shared],
    );
    // The attack has different NAR bytes, not merely a different signature
    // on the actual upstream bytes. Its signature is a valid publisher one.
    let forged_source = root.path().join(format!("forged-shared-{run_id}"));
    fs::write(&forged_source, "not the upstream shared package").unwrap();
    let alternate = run(
        &nix,
        &["store", "add-path", forged_source.to_str().unwrap()],
    );
    let publisher_shared = root.path().join("publisher-poison");
    let publisher_shared_uri = archive::path_to_file_uri(&publisher_shared).unwrap();
    forge_cache_path(&nix, &publisher_shared, &shared, &alternate, &key);
    run(&nix, &["copy", "--from", &publisher_shared_uri, &shared]);
    package::verify_local_signature(
        &nix,
        None,
        Path::new(&shared),
        &publisher_key,
        package::StoreContents::Rehash,
    )
    .unwrap();
    let mut upstream = slim::Upstream::file(&nix, &upstream_cache);
    let error = slim::import(
        &nix,
        &cache,
        &package,
        &publisher_key,
        &mut upstream,
        &BTreeSet::new(),
    )
    .unwrap_err();
    assert!(
        error.contains(
            "is installed under another key and does not match cache.nixos.org; refusing"
        ),
        "{error}"
    );
    assert!(!package.exists());
    run(&nix, &["store", "delete", &shared]);
    let mut upstream = slim::Upstream::file(&nix, &upstream_cache);
    slim::import(
        &nix,
        &cache,
        &package,
        &publisher_key,
        &mut upstream,
        &BTreeSet::new(),
    )
    .unwrap();
    assert!(package.exists());
    assert!(Path::new(&shared).exists());
    package::verify_local_signature(
        &nix,
        None,
        Path::new(&shared),
        &upstream_key,
        package::StoreContents::Rehash,
    )
    .unwrap();
    assert!(package::verify_local_signature(
        &nix,
        None,
        Path::new(&shared),
        &publisher_key,
        package::StoreContents::TrustRegistered
    )
    .is_err());
    let mut unavailable = slim::Upstream::new(
        &nix,
        &Configuration {
            curl: helper.with_file_name("false"),
            ca_bundle: None,
            official: None,
        },
    )
    .unwrap();
    let error = slim::import(
        &nix,
        &cache,
        &package,
        &publisher_key,
        &mut unavailable,
        &BTreeSet::new(),
    )
    .unwrap_err();
    assert!(
        error.contains("cannot check shared package authority at https://cache.nixos.org"),
        "{error}"
    );
    let host_dir = root.path().join("slim-sideload-host");
    fs::create_dir(&host_dir).unwrap();
    let mut host = Host::for_test(
        &host_dir,
        nix.clone(),
        LifecyclePolicy::new(Vec::new()).unwrap(),
        TestRuntime::default(),
    )
    .unwrap();
    host.test_runtime = None;
    host.archive_upstream = Some(upstream_cache.clone());
    host.units.systemctl = helper.clone();
    host.units.firewall.ipv4 = helper.clone();
    host.units.firewall.ipv6 = helper.clone();
    host.trust_publisher("@slim", &publisher_key).unwrap();
    let config = Configuration {
        curl: "/unavailable-curl".into(),
        ca_bundle: None,
        official: None,
    };
    let review = host
        .inspect_archive(&config, &published.archive_path)
        .unwrap();
    assert!(matches!(
        review.provenance,
        Provenance::Sideload { signed: true, .. }
    ));
    host.install_reviewed(review.clone(), &review.approval)
        .unwrap();
    assert_eq!(
        host.status("@slim:native").unwrap().unwrap().package,
        package
    );
    host.selection("@slim:native").remove().unwrap();
    drop(host);
    // A config-bound publisher must not route an upstream forgery through a
    // second plugin's RawCache provenance and the required-closure exception.
    run(
        &nix,
        &[
            "copy",
            "--to",
            &publisher_shared_uri,
            package.to_str().unwrap(),
        ],
    );
    let parent_source = root.path().join(format!("parent-source-{run_id}"));
    fs::create_dir(&parent_source).unwrap();
    fs::write(
        parent_source.join("plugin.ts"),
        "export const name = 'pack';",
    )
    .unwrap();
    fs::write(parent_source.join("manifest.json"), serde_json::json!({"publisher":{"namespace":"@slim"},"entry":"plugin.ts","sources":["plugin.ts"],"requires":[package]}).to_string()).unwrap();
    let parent_source_path = run(
        &nix,
        &["store", "add-path", parent_source.to_str().unwrap()],
    );
    let parent_expression = expression
        .replace(
            &format!("slim-native-{run_id}"),
            &format!("slim-parent-{run_id}"),
        )
        .replace(&source_path, &parent_source_path)
        .replace(&shared, package.to_str().unwrap());
    let built = run(
        &nix,
        &[
            "build",
            "--impure",
            "--option",
            "sandbox",
            "false",
            "--no-link",
            "--json",
            "--expr",
            &parent_expression,
        ],
    );
    let built: serde_json::Value = serde_json::from_str(&built).unwrap();
    let parent = PathBuf::from(built[0]["outputs"]["out"].as_str().unwrap());
    let parent_derivation = built[0]["drvPath"].as_str().unwrap();
    let parent_archive = publisher::publish_archive_with_upstream(
        &PublishArgs {
            nix: nix.clone(),
            secret_key: Some(key.clone()),
            self_contained: false,
            input_store_path: parent.clone(),
            release_version: "requires".into(),
            platform: current_platform().into(),
            archive_url: String::new(),
            output_dir: output.clone(),
        },
        Some(&mut upstream),
    )
    .unwrap();
    run(
        &nix,
        &[
            "store",
            "delete",
            parent.to_str().unwrap(),
            parent_derivation,
        ],
    );
    let directory = root.path().join("bound-requirement-host");
    fs::create_dir(&directory).unwrap();
    let mut bound = Host::for_test(
        &directory,
        nix.clone(),
        LifecyclePolicy::new(Vec::new()).unwrap(),
        TestRuntime::default(),
    )
    .unwrap();
    bound.test_runtime = None;
    bound.archive_upstream = Some(upstream_cache.clone());
    bound.units.systemctl = helper.clone();
    bound.units.firewall.ipv4 = helper.clone();
    bound.units.firewall.ipv6 = helper.clone();
    bound.publishers.insert(
        "@slim".into(),
        package::PublisherBinding {
            public_key: publisher_key.clone(),
            cache_url: publisher_shared_uri.clone(),
        },
    );
    let error = bound
        .inspect_archive(&config, &parent_archive.archive_path)
        .err()
        .unwrap();
    assert!(
        error.contains("publisher-signed cache copy cannot supply an upstream path"),
        "{error}"
    );
    assert!(!parent.exists(), "refuse before importing the parent");
    assert!(bound.installed().unwrap().is_empty());
    assert!(
        !bound.paths.registry_path.exists(),
        "refuse before native or selection effects"
    );
    // The explicit raw-cache route applies the same per-path rule (#349): the
    // bound publisher's cache cannot supply a path that the upstream serves,
    // although Nix's global key list trusts that publisher.
    let error = bound
        .inspect(&config, &publisher_shared_uri, &package)
        .err()
        .unwrap();
    assert!(
        error.contains("publisher-signed cache copy cannot supply an upstream path"),
        "{error}"
    );
    assert!(bound.installed().unwrap().is_empty());
    package::verify_local_signature(
        &nix,
        None,
        Path::new(&shared),
        &upstream_key,
        package::StoreContents::Rehash,
    )
    .unwrap();
    // A different bound publisher is not a normal upstream authority either.
    bound.publishers.insert(
        "@other".into(),
        package::PublisherBinding {
            public_key: upstream_key.clone(),
            cache_url: publisher_shared_uri.clone(),
        },
    );
    let error = bound
        .inspect_archive(&config, &parent_archive.archive_path)
        .err()
        .unwrap();
    assert!(error.contains("cannot supply an upstream path"), "{error}");
    bound.publishers.remove("@other");
    // A requirement's legitimate private cache still works, keeps RawCache
    // provenance, and can share separately verified upstream packages.
    let good_uri = format!(
        "file://{}",
        root.path().join("good-required-cache").display()
    );
    run(
        &nix,
        &["copy", "--to", &good_uri, package.to_str().unwrap()],
    );
    bound.publishers.get_mut("@slim").unwrap().cache_url = good_uri.clone();
    let review = bound
        .inspect_archive(&config, &parent_archive.archive_path)
        .unwrap();
    assert!(
        matches!(&review.brings[0].report.provenance, Provenance::RawCache { cache_url } if cache_url == &good_uri)
    );
    bound
        .install_reviewed(review.clone(), &bound.review_approval(&review).unwrap())
        .unwrap();
    bound.selection("@slim:pack").remove().unwrap();
    bound.selection("@slim:native").remove().unwrap();
    // A legitimate raw cache keeps working, but only at the bound address.
    let error = bound
        .inspect(&config, &publisher_shared_uri, &package)
        .err()
        .unwrap();
    assert!(error.contains("is bound to cache"), "{error}");
    let raw = bound.inspect(&config, &good_uri, &package).unwrap();
    assert!(
        matches!(&raw.provenance, Provenance::RawCache { cache_url } if cache_url == &good_uri)
    );
    drop(bound);
    // A self-contained pack keeps its existing offline route. Its rewritten
    // CA requirement is in the archive, not necessarily in the bound raw cache.
    let offline_parent = publisher::publish_archive(&PublishArgs {
        nix: nix.clone(),
        secret_key: Some(key.clone()),
        self_contained: true,
        input_store_path: parent.clone(),
        release_version: "offline-pack".into(),
        platform: current_platform().into(),
        archive_url: String::new(),
        output_dir: output.clone(),
    })
    .unwrap();
    let directory = root.path().join("offline-pack-host");
    fs::create_dir(&directory).unwrap();
    let mut offline = Host::for_test(
        &directory,
        nix.clone(),
        LifecyclePolicy::new(Vec::new()).unwrap(),
        TestRuntime::default(),
    )
    .unwrap();
    offline.test_runtime = None;
    offline.units.systemctl = helper.clone();
    offline.units.firewall.ipv4 = helper.clone();
    offline.units.firewall.ipv6 = helper.clone();
    offline.publishers.insert(
        "@slim".into(),
        package::PublisherBinding {
            public_key: publisher_key.clone(),
            cache_url: good_uri,
        },
    );
    let review = offline
        .inspect_archive(&config, &offline_parent.archive_path)
        .unwrap();
    assert_ne!(review.brings[0].report.package, package);
    offline
        .install_reviewed(review.clone(), &offline.review_approval(&review).unwrap())
        .unwrap();
    offline.selection("@slim:pack").remove().unwrap();
    offline.selection("@slim:native").remove().unwrap();
    drop(offline);
    if let Err(error) = fs::remove_dir_all(&directory) {
        eprintln!("slim fixture {run_id}: removing pack roots failed: {error}");
    }
    // The CA pack refers to the canonical native output used below. Remove
    // this run's pack first, even when none of its selection roots remain.
    cleanup_fixture_paths(&nix, run_id, &[offline_parent.ca_store_path]);
    run(&nix, &["store", "delete", parent.to_str().unwrap()]);
    // The same bytes may already be valid under another signer, such as a
    // build host's own local build. The publisher's archive signs that exact
    // fingerprint, so import retains the publisher signature, not refuses.
    let other_secret = run(
        &nix,
        &["key", "generate-secret", "--key-name", "other-publisher"],
    );
    let other_key = publisher::public_half(&other_secret).unwrap();
    let other = root.path().join("other.secret");
    fs::write(&other, other_secret).unwrap();
    run(
        &nix,
        &[
            "store",
            "sign",
            "--key-file",
            other.to_str().unwrap(),
            package.to_str().unwrap(),
        ],
    );
    let foreign_cache = root.path().join("foreign-cache");
    let foreign_uri = format!("file://{}", foreign_cache.display());
    run(
        &nix,
        &["copy", "--to", &foreign_uri, package.to_str().unwrap()],
    );
    for note in fs::read_dir(&foreign_cache)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "narinfo"))
    {
        let text = fs::read_to_string(note.path()).unwrap();
        fs::write(
            note.path(),
            text.lines()
                .filter(|line| !line.starts_with("Sig: slim-publisher:"))
                .collect::<Vec<_>>()
                .join("\n")
                + "\n",
        )
        .unwrap();
    }
    run(&nix, &["store", "delete", package.to_str().unwrap()]);
    run(
        &nix,
        &[
            "--option",
            "trusted-public-keys",
            &other_key,
            "copy",
            "--from",
            &foreign_uri,
            package.to_str().unwrap(),
        ],
    );
    slim::import(
        &nix,
        &cache,
        &package,
        &publisher_key,
        &mut upstream,
        &BTreeSet::new(),
    )
    .unwrap();
    let adopted = package::path_info(&nix, package.to_str().unwrap(), None).unwrap();
    let fingerprint = package::fingerprint(package.to_str().unwrap(), &adopted).unwrap();
    assert!(package::signed_by(
        &adopted,
        &fingerprint,
        &package::public_key_bytes(&publisher_key).unwrap()
    ));
    assert!(package::signed_by(
        &adopted,
        &fingerprint,
        &package::public_key_bytes(&other_key).unwrap()
    ));
    run(
        &nix,
        &[
            "store",
            "sign",
            "--key-file",
            key.to_str().unwrap(),
            package.to_str().unwrap(),
        ],
    );
    // Self-contained and unsigned native publishing always use CA paths.
    for (release, signing_key, requested) in [
        ("offline", Some(key.clone()), true),
        ("unsigned", None, false),
    ] {
        let result = publisher::publish_archive(&PublishArgs {
            secret_key: signing_key.clone(),
            self_contained: requested,
            release_version: release.into(),
            ..PublishArgs {
                nix: nix.clone(),
                secret_key: None,
                self_contained: false,
                input_store_path: package.clone(),
                release_version: String::new(),
                platform: current_platform().into(),
                archive_url: String::new(),
                output_dir: output.clone(),
            }
        })
        .unwrap();
        assert_ne!(result.ca_store_path, package.to_string_lossy());
        let extracted = root.path().join(release);
        fs::create_dir(&extracted).unwrap();
        archive::extract(&result.archive_path, &extracted).unwrap();
        archive::preflight_cache(&nix, &result.ca_store_path, &extracted).unwrap();
        run(&nix, &["store", "delete", &result.ca_store_path]);
        assert!(!Path::new(&result.ca_store_path).exists());
        package::import_archive(
            &nix,
            &archive::path_to_file_uri(&extracted).unwrap(),
            Path::new(&result.ca_store_path),
        )
        .unwrap();
        let host_dir = root.path().join(format!("{release}-host"));
        fs::create_dir(&host_dir).unwrap();
        let mut host = Host::for_test(
            &host_dir,
            nix.clone(),
            LifecyclePolicy::new(Vec::new()).unwrap(),
            TestRuntime::default(),
        )
        .unwrap();
        host.test_runtime = None;
        host.units.systemctl = helper.clone();
        host.units.firewall.ipv4 = helper.clone();
        host.units.firewall.ipv6 = helper.clone();
        if signing_key.is_some() {
            host.trust_publisher("@slim", &publisher_key).unwrap();
        }
        let review = host.inspect_archive(&config, &result.archive_path).unwrap();
        assert!(
            matches!(review.provenance, Provenance::Sideload { signed, .. } if signed == signing_key.is_some())
        );
        host.install_reviewed(review.clone(), &review.approval)
            .unwrap();
        host.selection("@slim:native").remove().unwrap();
    }
    let source = root.path().join(format!("source-only-{run_id}"));
    fs::create_dir(&source).unwrap();
    fs::write(source.join("plugin.ts"), "export const name = 'source';").unwrap();
    fs::write(
        source.join("manifest.json"),
        r#"{"publisher":{"namespace":"@slim"},"entry":"plugin.ts","sources":["plugin.ts"]}"#,
    )
    .unwrap();
    let input = run(&nix, &["store", "add-path", source.to_str().unwrap()]);
    let result = publisher::publish_archive(&PublishArgs {
        nix: nix.clone(),
        secret_key: Some(key),
        self_contained: false,
        input_store_path: input.into(),
        release_version: "source".into(),
        platform: current_platform().into(),
        archive_url: String::new(),
        output_dir: output,
    })
    .unwrap();
    assert_eq!(result.platform, "any");
    assert!(package::path_info(&nix, &result.ca_store_path, None)
        .unwrap()
        .ca
        .is_some());
}

/// Owner decision 2026-11-05: a signed record whose exact closure is already
/// valid with trusted local signatures makes no network request. A missing or
/// unsigned package still takes the normal archive route, and an archive that
/// the record's key did not sign is refused there.
#[test]
#[ignore = "requires writable build-machine Nix; creates and removes only this test's unique outputs"]
fn present_signed_closure_installs_without_any_network_request() {
    let nix = PathBuf::from(std::env::var_os("KORRI_PUBLISH_NIX").expect("immutable Nix"));
    let helper = PathBuf::from(std::env::var_os("KORRI_TEST_TRUE").expect("immutable coreutils"));
    let fixture = FixtureRun::new(&nix);
    let root = fixture.directory();
    let run_id = &fixture.id;
    let generate = |name: &str| {
        let secret = run(&nix, &["key", "generate-secret", "--key-name", name]);
        let public = publisher::public_half(&secret).unwrap();
        let file = root.path().join(format!("{name}.secret"));
        fs::write(&file, secret).unwrap();
        (file, public)
    };
    let (key, publisher_key) = generate("present-publisher");
    let (upstream_secret, upstream_key) = generate("present-upstream");
    let (other_secret, _) = generate("present-other");
    let tools = root.path().join(format!("tools-{run_id}"));
    fs::create_dir(&tools).unwrap();
    fs::write(
        tools.join("nix"),
        format!(
            "#!/bin/sh\nexec {} --option trusted-public-keys '{} {}' \"$@\"\n",
            nix.display(),
            upstream_key,
            publisher_key
        ),
    )
    .unwrap();
    fs::set_permissions(tools.join("nix"), fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink(nix.with_file_name("nix-store"), tools.join("nix-store")).unwrap();
    let nix = PathBuf::from(run(&nix, &["store", "add-path", tools.to_str().unwrap()])).join("nix");
    let shared_source = root.path().join(format!("shared-{run_id}"));
    fs::write(&shared_source, format!("shared {}", root.path().display())).unwrap();
    let shared = run(
        &nix,
        &["store", "add-path", shared_source.to_str().unwrap()],
    );
    run(
        &nix,
        &[
            "store",
            "sign",
            "--key-file",
            upstream_secret.to_str().unwrap(),
            &shared,
        ],
    );
    let upstream_cache = root.path().join("upstream");
    run(
        &nix,
        &[
            "copy",
            "--to",
            &format!("file://{}", upstream_cache.display()),
            &shared,
        ],
    );
    let source = root.path().join(format!("source-{run_id}"));
    fs::create_dir(&source).unwrap();
    fs::write(source.join("plugin.ts"), "export const name = 'native';").unwrap();
    fs::write(source.join("manifest.json"), serde_json::json!({"publisher":{"namespace":"@present"},"entry":"plugin.ts","sources":["plugin.ts"],"packages":{"shared":shared}}).to_string()).unwrap();
    let source_path = run(&nix, &["store", "add-path", source.to_str().unwrap()]);
    let expression = format!("derivation {{ name = \"present-native-{}\"; system = \"{}\"; builder = \"{}\"; args = [\"-c\" \"{} -R $source $out\"]; source = builtins.storePath \"{}\"; shared = builtins.storePath \"{}\"; }}", run_id, current_platform(), fs::canonicalize("/bin/sh").unwrap().display(), helper.with_file_name("cp").display(), source_path, shared);
    let build = || {
        let built: serde_json::Value = serde_json::from_str(&run(
            &nix,
            &[
                "build",
                "--impure",
                "--option",
                "sandbox",
                "false",
                "--no-link",
                "--json",
                "--expr",
                &expression,
            ],
        ))
        .unwrap();
        (
            PathBuf::from(built[0]["outputs"]["out"].as_str().unwrap()),
            built[0]["drvPath"].as_str().unwrap().to_owned(),
        )
    };
    let (package, derivation) = build();
    let mut upstream = slim::Upstream::file(&nix, &upstream_cache);
    let publish = |secret: &Path, release: &str, upstream: &mut slim::Upstream| {
        let output = root.path().join(format!("publication-{release}"));
        fs::create_dir(&output).unwrap();
        publisher::publish_archive_with_upstream(
            &PublishArgs {
                nix: nix.clone(),
                secret_key: Some(secret.to_owned()),
                self_contained: false,
                input_store_path: package.clone(),
                release_version: release.into(),
                platform: current_platform().into(),
                archive_url: String::new(),
                output_dir: output,
            },
            Some(upstream),
        )
        .unwrap()
    };
    // The other key signs first: its archive's root carries only that key.
    let foreign = publish(&other_secret, "foreign", &mut upstream);
    let signed = publish(&key, "signed", &mut upstream);
    let record = |published: &publisher::ArchiveResult, name: &str| crate::catalog::CatalogRecord {
        plugin_id: published.declaration.id(),
        title: None,
        description: None,
        release_version: name.into(),
        platform: published.platform.clone(),
        store_path: published.ca_store_path.clone(),
        archive_url: format!("https://archive.test/{name}.tar"),
        archive_sha256: published.archive_sha256.clone(),
        public_key: Some(publisher_key.clone()),
    };
    let good = record(&signed, "signed");
    let forged = record(&foreign, "foreign");
    assert_eq!(good.store_path, package.to_string_lossy());
    // Every request goes through this curl. It logs each call and serves only
    // the two archives; any other address fails like an unreachable network.
    let log = root.path().join("curl.log");
    let curl = root.path().join("curl");
    fs::write(&curl, format!(
        "#!/bin/sh\necho \"$*\" >> {log}\nout=; url=\nwhile [ $# -gt 0 ]; do case \"$1\" in --output) out=$2; shift;; --url) url=$2; shift;; esac; shift; done\ncase \"$url\" in\n {good}) exec {cp} {signed} \"$out\";;\n {forged}) exec {cp} {foreign} \"$out\";;\nesac\nexit 7\n",
        log = log.display(), good = good.archive_url, forged = forged.archive_url,
        cp = helper.with_file_name("cp").display(),
        signed = signed.archive_path.display(), foreign = foreign.archive_path.display(),
    )).unwrap();
    fs::set_permissions(&curl, fs::Permissions::from_mode(0o755)).unwrap();
    let config = Configuration {
        curl,
        ca_bundle: None,
        official: None,
    };
    let repository = SourceUrl::parse("https://catalog.test/").unwrap();
    let requests = || fs::read_to_string(&log).unwrap_or_default();
    let new_host = |name: &str, upstream: Option<PathBuf>| {
        let directory = root.path().join(name);
        fs::create_dir(&directory).unwrap();
        let mut host = Host::for_test(
            &directory,
            nix.clone(),
            LifecyclePolicy::new(Vec::new()).unwrap(),
            TestRuntime::default(),
        )
        .unwrap();
        host.test_runtime = None;
        host.archive_upstream = upstream;
        host.trust_publisher("@present", &publisher_key).unwrap();
        host
    };
    let inspect = |host: &Host, record: &crate::catalog::CatalogRecord| {
        host.fetch_record(&config, &repository, record)
            .and_then(|package| {
                let provenance = host.candidates.borrow()[&package].0.clone();
                host.load(&package, provenance)
            })
    };

    // Missing package: the normal route downloads the archive.
    run(
        &nix,
        &["store", "delete", package.to_str().unwrap(), &derivation],
    );
    let host = new_host("missing", Some(upstream_cache.clone()));
    let first = inspect(&host, &good).unwrap();
    assert!(requests().contains(&good.archive_url), "{}", requests());
    drop(host);

    // Present and signed: no curl call at all. Upstream lookups would use the
    // same logging curl, because this host has no file-cache stand-in.
    fs::remove_file(&log).unwrap();
    let host = new_host("present", None);
    let started = std::time::Instant::now();
    let again = inspect(&host, &good).unwrap();
    eprintln!("present closure inspected in {:?}", started.elapsed());
    assert_eq!(
        requests(),
        "",
        "a present signed closure made network requests"
    );
    assert_eq!(again.approval, first.approval);
    assert_eq!(again.provenance, first.provenance);
    drop(host);

    // Present but unsigned: the local build carries no publisher signature.
    run(&nix, &["store", "delete", package.to_str().unwrap()]);
    let (rebuilt, _) = build();
    assert_eq!(rebuilt, package);
    let host = new_host("unsigned", Some(upstream_cache.clone()));
    let error = inspect(&host, &forged).err().unwrap();
    assert!(requests().contains(&forged.archive_url), "{}", requests());
    assert!(
        error.contains("does not verify against the publisher public key"),
        "{error}"
    );
    let local = package::path_info(&nix, package.to_str().unwrap(), None).unwrap();
    assert!(!slim::signed(package.to_str().unwrap(), &local, std::slice::from_ref(&publisher_key)).unwrap());
    // The publisher's own archive still admits the same bytes normally.
    fs::remove_file(&log).unwrap();
    let adopted = inspect(&host, &good).unwrap();
    assert!(requests().contains(&good.archive_url), "{}", requests());
    assert_eq!(adopted.approval, first.approval);
}
