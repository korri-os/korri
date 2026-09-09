use korri_plugin_host::catalog::{Catalog, CatalogRecord};

// A minimal valid record; tests mutate fields to exercise rejection paths.
fn valid_record() -> CatalogRecord {
    CatalogRecord {
        plugin_id: "@example:network".into(),
        title: Some("Network".into()),
        description: Some("Provides networking.".into()),
        release_version: "1.0.0".into(),
        platform: "x86_64-linux".into(),
        store_path: "/nix/store/00000000000000000000000000000000-network-1.0.0".into(),
        archive_url: "https://example.com/network-1.0.0-x86_64-linux.tar".into(),
        archive_sha256: "a".repeat(64),
    }
}

fn catalog_with(records: Vec<CatalogRecord>) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({ "records": records })).unwrap()
}

#[test]
fn rejects_archive_urls_that_exceed_the_limit_after_normalization() {
    let mut record = valid_record();
    record.archive_url = format!("https://example.com/{}.tar", "é".repeat(700));
    assert!(record.archive_url.len() < 4096);
    assert!(Catalog::from_json(&catalog_with(vec![record])).is_err());
}

#[test]
fn valid_record_round_trips() {
    let record = valid_record();
    let bytes = catalog_with(vec![record.clone()]);
    let catalog = Catalog::from_json(&bytes).unwrap();
    assert_eq!(catalog.records.len(), 1);
    assert_eq!(catalog.records[0].plugin_id, "@example:network");
    assert_eq!(catalog.records[0].release_version, "1.0.0");
}

#[test]
fn empty_catalog_is_valid() {
    let bytes = catalog_with(vec![]);
    let catalog = Catalog::from_json(&bytes).unwrap();
    assert_eq!(catalog.records.len(), 0);
}

#[test]
fn unknown_field_in_record_is_rejected() {
    let json = r#"{"records":[{"plugin_id":"@x:y","release_version":"1","platform":"x86_64-linux","store_path":"/nix/store/00000000000000000000000000000000-y-1","archive_url":"https://e.com/a.tar","archive_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","extra":"bad"}]}"#;
    assert!(Catalog::from_json(json.as_bytes()).is_err());
}

#[test]
fn unknown_top_level_field_is_rejected() {
    let json = r#"{"records":[],"extra":"bad"}"#;
    assert!(Catalog::from_json(json.as_bytes()).is_err());
}

#[test]
fn invalid_plugin_id_is_rejected() {
    for bad_id in [
        "example:network",
        "@:network",
        "@example:",
        "@Example:network",
        "@example:Network",
    ] {
        let mut r = valid_record();
        r.plugin_id = bad_id.into();
        assert!(
            Catalog::from_json(&catalog_with(vec![r])).is_err(),
            "expected rejection for plugin_id={bad_id}"
        );
    }
}

#[test]
fn invalid_release_version_is_rejected() {
    for bad in ["", "has space", "../escape", &"x".repeat(65)] {
        let mut r = valid_record();
        r.release_version = bad.into();
        assert!(
            Catalog::from_json(&catalog_with(vec![r])).is_err(),
            "expected rejection for release_version={bad:?}"
        );
    }
}

#[test]
fn release_labels_are_not_plugin_names_or_version_ordering() {
    let mut record = valid_record();
    record.release_version = "v1.0.0-RC1+revision.2".into();
    assert!(Catalog::from_json(&catalog_with(vec![record])).is_ok());
}

#[test]
fn invalid_platform_is_rejected() {
    for bad in ["", "linux", "x86_64", "x86 64-linux"] {
        let mut r = valid_record();
        r.platform = bad.into();
        assert!(
            Catalog::from_json(&catalog_with(vec![r])).is_err(),
            "expected rejection for platform={bad:?}"
        );
    }
}

#[test]
fn invalid_store_path_is_rejected() {
    for bad in [
        "/tmp/plugin",
        "/nix/store/not-a-hash",
        "/nix/store/00000000000000000000000000000000-x.drv",
        "00000000000000000000000000000000-x",
    ] {
        let mut r = valid_record();
        r.store_path = bad.into();
        assert!(
            Catalog::from_json(&catalog_with(vec![r])).is_err(),
            "expected rejection for store_path={bad}"
        );
    }
}

#[test]
fn non_https_archive_url_is_rejected() {
    for bad in [
        "http://example.com/a.tar",
        "ftp://e.com/a.tar",
        "file:///tmp/a.tar",
    ] {
        let mut r = valid_record();
        r.archive_url = bad.into();
        assert!(
            Catalog::from_json(&catalog_with(vec![r])).is_err(),
            "expected rejection for archive_url={bad}"
        );
    }
}

#[test]
fn malformed_sha256_is_rejected() {
    for bad in [
        "",
        "aabbcc",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        &"g".repeat(64),
    ] {
        let mut r = valid_record();
        r.archive_sha256 = bad.into();
        assert!(
            Catalog::from_json(&catalog_with(vec![r])).is_err(),
            "expected rejection for archive_sha256={bad:?}"
        );
    }
}

#[test]
fn ambiguous_duplicate_record_is_rejected() {
    let r = valid_record();
    // Two identical records for the same plugin/version/platform.
    assert!(Catalog::from_json(&catalog_with(vec![r.clone(), r])).is_err());
}

#[test]
fn different_platform_records_are_accepted() {
    let mut r2 = valid_record();
    r2.platform = "aarch64-linux".into();
    let bytes = catalog_with(vec![valid_record(), r2]);
    let catalog = Catalog::from_json(&bytes).unwrap();
    assert_eq!(catalog.records.len(), 2);
}

#[test]
fn different_release_version_records_are_accepted() {
    let mut r2 = valid_record();
    r2.release_version = "2.0.0".into();
    r2.store_path = "/nix/store/11111111111111111111111111111111-network-2.0.0".into();
    let bytes = catalog_with(vec![valid_record(), r2]);
    let catalog = Catalog::from_json(&bytes).unwrap();
    assert_eq!(catalog.records.len(), 2);
}

#[test]
fn records_for_filters_by_id() {
    let mut other = valid_record();
    other.plugin_id = "@example:other".into();
    let bytes = catalog_with(vec![valid_record(), other]);
    let catalog = Catalog::from_json(&bytes).unwrap();
    let found = catalog.records_for("@example:network");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].plugin_id, "@example:network");
}

#[test]
fn record_lookup_by_id_version_platform() {
    let catalog = Catalog::from_json(&catalog_with(vec![valid_record()])).unwrap();
    let r = catalog
        .record("@example:network", "1.0.0", "x86_64-linux")
        .unwrap();
    assert_eq!(
        r.store_path,
        "/nix/store/00000000000000000000000000000000-network-1.0.0"
    );
    assert!(catalog
        .record("@example:network", "9.0.0", "x86_64-linux")
        .is_err());
}

#[test]
fn catalog_exceeding_size_limit_is_rejected() {
    // A catalog blob larger than 4 MiB must fail without parsing.
    let big = vec![b' '; 4 * 1024 * 1024 + 1];
    assert!(Catalog::from_json(&big).is_err());
}

#[test]
fn title_and_description_bounds_are_checked() {
    let mut r = valid_record();
    r.title = Some("".into());
    assert!(Catalog::from_json(&catalog_with(vec![r.clone()])).is_err());

    r.title = Some("x".repeat(129));
    assert!(Catalog::from_json(&catalog_with(vec![r.clone()])).is_err());

    r.title = Some("Good title".into());
    r.description = Some("x".repeat(1025));
    assert!(Catalog::from_json(&catalog_with(vec![r.clone()])).is_err());

    r.description = Some("Good description.".into());
    assert!(Catalog::from_json(&catalog_with(vec![r])).is_ok());
}

#[test]
fn null_optional_fields_are_rejected_by_deny_unknown_null() {
    // The catalog uses optional_non_null for title/description just like
    // declaration.rs. Explicit null must not be treated as absent.
    let json = r#"{"records":[{"plugin_id":"@x:y","title":null,"release_version":"1","platform":"x86_64-linux","store_path":"/nix/store/00000000000000000000000000000000-y-1","archive_url":"https://e.com/a.tar","archive_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}]}"#;
    // serde will call optional_non_null which tries to deserialize null as
    // String, producing a type error.
    assert!(Catalog::from_json(json.as_bytes()).is_err());
}

/// archive_url must be rejected for credentials (userinfo), empty host, and fragment.
/// Previously only the "https://" prefix was checked; a real URL parser is now used.
///
/// Notes on url crate 2.5.x (WHATWG URL, "special" schemes):
///   "https://"                 → parse error "empty host"  (caught at parse step)
///   "https://user:pass@h/"    → Ok but username/password set  (caught by policy)
///   "https://h/path#frag"     → Ok but fragment set  (caught by policy)
///   "https:///path"           → NOT empty-host: parsed as host="path" by WHATWG spec
#[test]
fn archive_url_real_url_parsing_enforced() {
    for bad_url in [
        "https://",                            // empty host — parse error
        "https://user:pass@example.com/a.tar", // credentials in URL
        "https://example.com/a.tar#frag",      // fragment
        "http://example.com/a.tar",            // insecure scheme
    ] {
        let mut r = valid_record();
        r.archive_url = bad_url.into();
        assert!(
            Catalog::from_json(&catalog_with(vec![r])).is_err(),
            "expected rejection for archive_url={bad_url:?}"
        );
    }
}

/// release_version labels with legal punctuation (dots, dashes) must not produce
/// filenames that escape or collide when composed with other label components.
/// A release_version that contains path separators must be rejected.
#[test]
fn release_version_path_separator_is_rejected() {
    for bad in ["../evil", "1.0/hack", "a\x00b"] {
        let mut r = valid_record();
        r.release_version = bad.into();
        assert!(
            Catalog::from_json(&catalog_with(vec![r])).is_err(),
            "expected rejection for release_version={bad:?}"
        );
    }
}

#[test]
fn serialized_catalog_round_trips_through_from_json() {
    let catalog = Catalog {
        records: vec![valid_record()],
    };
    let bytes = catalog.to_json().unwrap();
    let reparsed = Catalog::from_json(&bytes).unwrap();
    assert_eq!(reparsed.records.len(), 1);
    assert_eq!(reparsed.records[0].archive_sha256, "a".repeat(64));
}
