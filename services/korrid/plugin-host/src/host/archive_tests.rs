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

#[test]
#[ignore = "requires writable build-machine Nix; creates and removes only this test's unique outputs"]
fn slim_real_nix_publish_install_security_and_offline() {
    let nix = PathBuf::from(std::env::var_os("KORRI_PUBLISH_NIX").expect("immutable Nix"));
    let helper = PathBuf::from(std::env::var_os("KORRI_TEST_TRUE").expect("immutable coreutils"));
    let root = tempfile::tempdir().unwrap();
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
    let tools = root.path().join("tools");
    fs::create_dir(&tools).unwrap();
    let script = tools.join("nix");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nexec {} --option trusted-public-keys '{}' \"$@\"\n",
            nix.display(),
            upstream_key
        ),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink(nix.with_file_name("nix-store"), tools.join("nix-store")).unwrap();
    let bound_nix = PathBuf::from(run(&nix, &["store", "add-path", tools.to_str().unwrap()]));
    let nix = bound_nix.join("nix");
    let shared_source = root.path().join("shared");
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
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("plugin.ts"), "export const name = 'native';").unwrap();
    fs::write(source.join("manifest.json"), serde_json::json!({"publisher":{"namespace":"@slim"},"entry":"plugin.ts","sources":["plugin.ts"],"packages":{"shared":shared}}).to_string()).unwrap();
    let source_path = run(&nix, &["store", "add-path", source.to_str().unwrap()]);
    let shell = fs::canonicalize("/bin/sh").unwrap();
    let expression = format!("derivation {{ name = \"slim-{}\"; system = \"{}\"; builder = \"{}\"; args = [\"-c\" \"{} -R $source $out\"]; source = builtins.storePath \"{}\"; shared = builtins.storePath \"{}\"; }}", root.path().file_name().unwrap().to_str().unwrap(), current_platform(), shell.display(), helper.with_file_name("cp").display(), source_path, shared);
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
        error.contains("without a trusted NixOS cache signature"),
        "{error}"
    );
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
    // A second publisher cannot adopt an existing input-addressed output,
    // even when its archive has a valid signature for those same bytes.
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
    let error = slim::import(
        &nix,
        &cache,
        &package,
        &publisher_key,
        &mut upstream,
        &BTreeSet::new(),
    )
    .unwrap_err();
    assert!(error.contains("already installed from another publisher; remove that plugin or rebuild with a different output"), "{error}");
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
    let source = root.path().join("source-only");
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
