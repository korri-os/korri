use korri_plugin_host::package::{verify_publisher, PublisherBinding};
use std::{collections::BTreeMap, env, fs, path::Path, process::Command};

fn nix(args: &[&str]) -> String {
    let output = Command::new(env::var("KORRI_PUBLISH_NIX").unwrap())
        .args(["--extra-experimental-features", "nix-command"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[test]
#[ignore = "writes build-machine Nix store; run korri-publisher-check"]
fn immutable_but_unreferenced_native_artifacts_are_not_package_authority() {
    let directory = tempfile::tempdir().unwrap();
    let artifact = directory.path().join("native");
    fs::create_dir(&artifact).unwrap();
    fs::write(artifact.join("daemon.service"), "[Service]\nType=exec\nExecStart=/nix/store/00000000000000000000000000000000-program/bin/run\n").unwrap();
    let native = nix(&["store", "add-path", artifact.to_str().unwrap()]);
    let package = directory.path().join("package");
    fs::create_dir(&package).unwrap();
    fs::write(
        package.join("plugin.ts"),
        "export const name='outside'; export const services=['daemon'];",
    )
    .unwrap();
    fs::write(package.join("manifest.json"), serde_json::to_vec(&serde_json::json!({"publisher":{"namespace":"@example"},"services":{"daemon":format!("{native}/daemon.service")}})).unwrap()).unwrap();
    // add-path imports a source NAR with no declared references. Being present
    // in the global store does not put the named unit in this package closure.
    let path = nix(&["store", "add-path", package.to_str().unwrap()]);
    let error = korri_plugin_host::package::load(
        Path::new(&env::var("KORRI_PUBLISH_NIX").unwrap()),
        Path::new(&path),
        korri_plugin_host::provenance::Provenance::RawCache {
            cache_url: "file:///test".into(),
        },
    )
    .err()
    .unwrap();
    assert!(error.contains("outside the selected closure"), "{error}");
}

#[test]
#[ignore = "writes build-machine Nix store; run korri-publisher-check"]
fn cached_output_requires_the_bound_full_key_not_a_second_trusted_signer_or_label() {
    let directory = tempfile::tempdir().unwrap();
    let public = nix(&["key", "generate-secret", "--key-name", "publisher"]);
    let other = nix(&["key", "generate-secret", "--key-name", "publisher"]);
    let key = directory.path().join("publisher.key");
    let other_key = directory.path().join("other.key");
    fs::write(&key, &public).unwrap();
    fs::write(&other_key, &other).unwrap();
    // key convert-secret-to-public reads stdin; use ring's standard Nix key layout.
    use base64::Engine;
    let to_public = |secret: &str| {
        let (name, secret) = secret.split_once(':').unwrap();
        let raw = base64::engine::general_purpose::STANDARD
            .decode(secret)
            .unwrap();
        format!(
            "{name}:{}",
            base64::engine::general_purpose::STANDARD.encode(&raw[32..])
        )
    };
    let package = directory.path().join("payload");
    fs::create_dir(&package).unwrap();
    fs::write(
        package.join("manifest.json"),
        r#"{"publisher":{"namespace":"@example"}}"#,
    )
    .unwrap();
    fs::write(package.join("plugin.ts"), "export const name = 'clock';").unwrap();
    // Unique contents prevent a signature from another test run masking the rejection.
    fs::write(package.join("test-run"), directory.path().to_str().unwrap()).unwrap();
    let path = nix(&["store", "add-path", package.to_str().unwrap()]);
    nix(&[
        "store",
        "sign",
        "--key-file",
        other_key.to_str().unwrap(),
        &path,
    ]);
    let cache = format!("file://{}", directory.path().join("cache").display());
    nix(&[
        "copy",
        "--to",
        &format!("{cache}?secret-key={}", other_key.display()),
        &path,
    ]);
    let mut bindings = BTreeMap::from([(
        "@example".into(),
        PublisherBinding {
            public_key: to_public(&public),
            cache_url: cache.clone(),
        },
    )]);
    let program = env::var("KORRI_PUBLISH_NIX").unwrap();
    assert!(
        verify_publisher(
            Path::new(&program),
            Path::new(&path),
            Some(&cache),
            &bindings
        )
        .is_err(),
        "the other key signed a cached content-addressed path under the same signature label"
    );
    let bound_cache_path = directory.path().join("bound-cache");
    let cache = format!("file://{}", bound_cache_path.display());
    nix(&[
        "copy",
        "--to",
        &format!("{cache}?secret-key={}", key.display()),
        &path,
    ]);
    bindings.get_mut("@example").unwrap().cache_url = cache.clone();
    assert_eq!(
        verify_publisher(
            Path::new(&program),
            Path::new(&path),
            Some(&cache),
            &bindings
        )
        .unwrap(),
        "@example"
    );
    fs::remove_dir_all(bound_cache_path).unwrap();
    assert_eq!(
        verify_publisher(
            Path::new(&program),
            Path::new(&path),
            Some(&cache),
            &bindings
        )
        .unwrap(),
        "@example",
        "the verified Nix signature must survive an offline cache"
    );
    assert!(verify_publisher(
        Path::new(&program),
        Path::new(&path),
        Some("https://other.example/cache"),
        &bindings
    )
    .is_err());
    let unbound = BTreeMap::new();
    assert!(verify_publisher(Path::new(&program), Path::new(&path), None, &unbound).is_err());

    // Even a signed package cannot introduce a second, self-declared key source.
    fs::write(package.join("manifest.json"), serde_json::to_vec(&serde_json::json!({"publisher": {"namespace": "@example", "publicKey": to_public(&other)}})).unwrap()).unwrap();
    let self_declared = nix(&["store", "add-path", package.to_str().unwrap()]);
    nix(&[
        "store",
        "sign",
        "--key-file",
        key.to_str().unwrap(),
        &self_declared,
    ]);
    let error = verify_publisher(
        Path::new(&program),
        Path::new(&self_declared),
        None,
        &bindings,
    )
    .unwrap_err();
    assert!(error.contains("unknown field `publicKey`"), "{error}");
}
