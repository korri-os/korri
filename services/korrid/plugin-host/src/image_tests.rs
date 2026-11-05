use super::*;

fn catalog() -> Catalog {
    Catalog::from_json(serde_json::to_string(&serde_json::json!({
        "latest": {"@korri:sample": "published"},
        "records": [{
            "plugin_id": "@korri:sample", "release_version": "published",
            "platform": "any", "store_path": "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-korri-plugin",
            "archive_url": "https://example.invalid/sample.tar", "archive_sha256": "a".repeat(64),
            "publicKey": "image:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
        }]
    })).unwrap().as_bytes()).unwrap()
}

#[test]
fn image_record_requires_bound_first_party_identity_and_exact_key() {
    let mut bindings = package::PublisherBindings::new();
    let catalog = catalog();
    assert!(record(&catalog, "@korri:sample", "x86_64-linux", &bindings).is_err());
    bindings.insert(
        "@korri".into(),
        package::PublisherBinding {
            public_key: "image:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into(),
            cache_url: "https://unused.example.invalid/".into(),
        },
    );
    for platform in ["aarch64-linux", "x86_64-linux"] {
        assert_eq!(
            record(&catalog, "@korri:sample", platform, &bindings)
                .unwrap()
                .platform,
            "any"
        );
    }
    assert!(
        record(&catalog, "@outside:sample", "x86_64-linux", &bindings)
            .unwrap_err()
            .contains("only @korri")
    );
    bindings.get_mut("@korri").unwrap().public_key =
        "other:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into();
    assert!(record(&catalog, "@korri:sample", "x86_64-linux", &bindings)
        .unwrap_err()
        .contains("bound @korri key"));
}

#[test]
fn image_extract_checks_digest_before_creating_output() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("archive");
    let output = directory.path().join("output");
    fs::write(&input, b"not an archive").unwrap();
    assert!(extract(&input, &"a".repeat(64), &output)
        .unwrap_err()
        .contains("SHA256"));
    assert!(!output.exists());
}

/// Boot proofs on an already registered image read metadata only: when every
/// archive path already carries the bound signature locally, no proof is
/// copied and Nix verifies nothing again. A forged archive is still refused.
#[test]
#[ignore = "requires writable build-machine Nix; creates and removes only this test's unique outputs"]
fn image_proofs_skip_registration_when_every_path_is_already_signed() {
    use std::os::unix::fs::PermissionsExt;
    let real = PathBuf::from(std::env::var_os("KORRI_PUBLISH_NIX").expect("immutable Nix"));
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let run = |args: &[&str]| {
        let output = std::process::Command::new(&real)
            .args(["--extra-experimental-features", "nix-command"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    let key = |name: &str| {
        let secret = run(&["key", "generate-secret", "--key-name", name]);
        let public = crate::publisher::public_half(&secret).unwrap();
        let file = root.join(name);
        fs::write(&file, secret).unwrap();
        (file, public)
    };
    let (secret, public) = key("image-proof-publisher");
    let (other_secret, _) = key("image-proof-other");
    let source = root.join(format!(
        "image-proof-{}",
        root.file_name().unwrap().to_str().unwrap().replace('.', "")
    ));
    fs::create_dir(&source).unwrap();
    fs::write(source.join("plugin.ts"), "export const name = 'sample';").unwrap();
    fs::write(
        source.join("manifest.json"),
        r#"{"publisher":{"namespace":"@korri"},"entry":"plugin.ts","sources":["plugin.ts"]}"#,
    )
    .unwrap();
    let path = run(&["store", "add-path", source.to_str().unwrap()]);
    let cache_for = |name: &str, signer: &Path, keep: &str| {
        run(&[
            "store",
            "sign",
            "--key-file",
            signer.to_str().unwrap(),
            &path,
        ]);
        let cache = root.join(name);
        run(&[
            "copy",
            "--to",
            &format!("file://{}", cache.display()),
            &path,
        ]);
        for note in fs::read_dir(&cache).unwrap().filter_map(Result::ok) {
            if note.path().extension().is_some_and(|x| x == "narinfo") {
                let text = fs::read_to_string(note.path()).unwrap();
                let kept: Vec<_> = text
                    .lines()
                    .filter(|line| !line.starts_with("Sig:") || line.starts_with(keep))
                    .collect();
                fs::write(note.path(), kept.join("\n") + "\n").unwrap();
            }
        }
        cache
    };
    let good = cache_for("good", &secret, "Sig: image-proof-publisher:");
    let forged = cache_for("forged", &other_secret, "Sig: image-proof-other:");
    // Start like a fresh image: the same bytes, registered without signatures.
    run(&["store", "delete", &path]);
    assert_eq!(run(&["store", "add-path", source.to_str().unwrap()]), path);
    let catalog = root.join("catalog.json");
    fs::write(
        &catalog,
        serde_json::json!({
            "latest": {"@korri:sample": "published"},
            "records": [{
                "plugin_id": "@korri:sample", "release_version": "published", "platform": "any",
                "store_path": path, "archive_url": "https://example.invalid/sample.tar",
                "archive_sha256": "a".repeat(64), "publicKey": public
            }]
        })
        .to_string(),
    )
    .unwrap();
    let bindings = root.join("bindings.json");
    fs::write(
        &bindings,
        serde_json::json!({
            "@korri": {"publicKey": public, "cacheUrl": "https://unused.example.invalid/"}
        })
        .to_string(),
    )
    .unwrap();
    let log = root.join("nix.log");
    let nix = root.join("nix");
    fs::write(
        &nix,
        format!(
            "#!/bin/sh\necho \"$*\" >> {}\nexec {} \"$@\"\n",
            log.display(),
            real.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&nix, fs::Permissions::from_mode(0o755)).unwrap();
    let config = Configuration {
        curl: "/unavailable-curl".into(),
        official: None,
        ca_bundle: None,
    };
    let proofs = |cache: &Path| {
        let _ = fs::remove_file(&log);
        let args: Vec<String> = [
            "proofs",
            bindings.to_str().unwrap(),
            "x86_64-linux",
            catalog.to_str().unwrap(),
            "https://example.invalid/catalog.json",
            "@korri:sample",
            cache.to_str().unwrap(),
        ]
        .map(String::from)
        .to_vec();
        let result = run_proofs(&nix, &config, &args);
        (result, fs::read_to_string(&log).unwrap_or_default())
    };
    // First boot registers the proof.
    let (result, calls) = proofs(&good);
    result.unwrap();
    assert!(calls.contains("copy-sigs"), "{calls}");
    let info = package::path_info(&real, &path, None).unwrap();
    let fingerprint = package::fingerprint(&path, &info).unwrap();
    assert!(package::signed_by(
        &info,
        &fingerprint,
        &package::public_key_bytes(&public).unwrap()
    ));
    // Later boots: metadata reads only.
    let (result, calls) = proofs(&good);
    result.unwrap();
    assert!(
        !calls.contains("copy-sigs") && !calls.contains(" verify"),
        "{calls}"
    );
    assert_eq!(
        calls.lines().count(),
        2,
        "one archive and one local metadata read: {calls}"
    );
    // The fast path still refuses an archive that the bound key did not sign.
    let (result, _) = proofs(&forged);
    let error = result.unwrap_err();
    assert_eq!(
        error,
        format!("image archive proof for {path}: archive signature does not verify against the publisher public key")
    );
    run(&["store", "delete", &path]);
}

fn run_proofs(nix: &Path, config: &Configuration, args: &[String]) -> Result<(), String> {
    run(nix, config, args)
}
