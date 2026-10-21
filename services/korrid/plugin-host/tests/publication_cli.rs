use korri_plugin_host::{
    catalog::{Catalog, CatalogRecord},
    publisher,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_korri-publish"))
        .args(args)
        .env_remove("KORRI_PUBLISH_NIX")
        .output()
        .unwrap()
}

fn record(root: &Path, platform: &str) -> CatalogRecord {
    let name = publisher::archive_name("@korri:tailscale", "plugin-2", platform).unwrap();
    let bytes = format!("asset for {platform}");
    fs::write(root.join(&name), &bytes).unwrap();
    CatalogRecord {
        plugin_id: "@korri:tailscale".into(),
        title: None,
        description: None,
        release_version: "plugin-2".into(),
        platform: platform.into(),
        store_path: "/nix/store/00000000000000000000000000000000-tailscale".into(),
        archive_url: format!("https://example.com/download/{name}"),
        archive_sha256: hex::encode(Sha256::digest(bytes)),
    }
}

#[test]
fn metadata_cli_merges_validates_and_checks_every_uploaded_asset_without_nix() {
    let root = tempfile::tempdir().unwrap();
    let records = [
        record(root.path(), "x86_64-linux"),
        record(root.path(), "aarch64-linux"),
    ];
    let paths: Vec<_> = records
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let path = root.path().join(format!("{i}.json"));
            fs::write(&path, serde_json::to_vec(r).unwrap()).unwrap();
            path
        })
        .collect();
    let catalog = cli(&[
        "catalog",
        paths[0].to_str().unwrap(),
        paths[1].to_str().unwrap(),
    ]);
    assert!(catalog.status.success(), "{:?}", catalog.stderr);
    assert_eq!(
        Catalog::from_json(&catalog.stdout).unwrap().records.len(),
        2
    );
    let catalog_path = root.path().join("catalog.json");
    fs::write(&catalog_path, catalog.stdout).unwrap();
    let args = [
        "verify-release",
        catalog_path.to_str().unwrap(),
        root.path().to_str().unwrap(),
        "https://example.com/download/",
        "@korri:tailscale",
        "plugin-2",
        "x86_64-linux",
        "aarch64-linux",
    ];
    let checked = cli(&args);
    assert!(checked.status.success(), "{:?}", checked.stderr);
    assert_eq!(
        String::from_utf8(checked.stdout).unwrap().lines().count(),
        2
    );

    // A missing architecture and duplicate records both fail before upload.
    assert!(!cli(&args[..args.len() - 1]).status.success());
    assert!(!cli(&[
        "catalog",
        paths[0].to_str().unwrap(),
        paths[0].to_str().unwrap()
    ])
    .status
    .success());
    assert!(!cli(&["catalog"]).status.success());
    fs::write(&paths[0], b"{\"records\": []}").unwrap();
    assert!(!cli(&["catalog", paths[0].to_str().unwrap()])
        .status
        .success());

    let asset = root
        .path()
        .join(publisher::archive_name("@korri:tailscale", "plugin-2", "aarch64-linux").unwrap());
    fs::write(&asset, b"changed").unwrap();
    assert!(!cli(&args).status.success());
    fs::remove_file(&asset).unwrap();
    assert!(!cli(&args).status.success());
}

#[test]
fn publication_rejects_wrong_urls_symlinks_oversized_assets_and_release_selection() {
    let root = tempfile::tempdir().unwrap();
    let mut r = record(root.path(), "x86_64-linux");
    let path = root.path().join("catalog.json");
    let check = |record: &CatalogRecord, release: &str| {
        fs::write(
            &path,
            Catalog {
                records: vec![record.clone()],
            }
            .to_json()
            .unwrap(),
        )
        .unwrap();
        publisher::release_assets(
            &path,
            root.path(),
            "https://example.com/download/",
            "@korri:tailscale",
            release,
            &["x86_64-linux".into()],
        )
    };
    assert!(check(&r, "wrong-release").is_err());
    r.archive_url = "https://example.com/download/wrong-name.tar".into();
    assert!(check(&r, "plugin-2").is_err());
    r = record(root.path(), "x86_64-linux");
    let asset = root
        .path()
        .join(publisher::archive_name(&r.plugin_id, &r.release_version, &r.platform).unwrap());
    fs::remove_file(&asset).unwrap();
    std::os::unix::fs::symlink(&path, &asset).unwrap();
    assert!(check(&r, "plugin-2").is_err());
    fs::remove_file(&asset).unwrap();
    fs::File::create(&asset)
        .unwrap()
        .set_len(2 * 1024 * 1024 * 1024)
        .unwrap();
    assert!(check(&r, "plugin-2").is_err());
    assert!(publisher::archive_name("@korri:tailscale", "../escape", "x86_64-linux").is_err());
    assert_ne!(
        publisher::archive_name("@a-b:c", "1", "x86_64-linux").unwrap(),
        publisher::archive_name("@a:b-c", "1", "x86_64-linux").unwrap()
    );
}
