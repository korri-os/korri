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
