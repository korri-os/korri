use korri_plugin_host::{archive, catalog::Catalog, package};
use std::{fs, path::Path, process::Command};

#[test]
fn publication_rejects_incomplete_or_obsolete_forms() {
    for args in [
        vec!["catalog"],
        vec!["verify-release"],
        vec!["archive-name"],
        vec![
            "/nix/store/00000000000000000000000000000000-plugin",
            "v1",
            "any",
            "https://example.invalid/",
            "/tmp",
        ],
        vec!["repository", "--release", "v1"],
        vec!["archive"],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_korri-publish"))
            .args(args)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8(result.stderr).unwrap().contains("usage:"));
    }
}

#[test]
#[ignore = "requires writable build-machine Nix; no target device"]
fn repository_and_archive_commands_sign_and_write_complete_outputs() {
    let nix = std::env::var("KORRI_PUBLISH_NIX").expect("immutable Nix");
    let run = |args: &[&str]| {
        let result = Command::new(&nix)
            .args(["--extra-experimental-features", "nix-command"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap().trim().to_owned()
    };
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("plugin");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("plugin.ts"), "export const name = 'hello';").unwrap();
    fs::write(
        source.join("manifest.json"),
        r#"{"publisher":{"namespace":"@cli-test"},"entry":"plugin.ts","sources":["plugin.ts"]}"#,
    )
    .unwrap();
    let path = run(&["store", "add-path", source.to_str().unwrap()]);
    let key = root.path().join("secret");
    fs::write(
        &key,
        run(&[
            "key",
            "generate-secret",
            "--key-name",
            "cli-publication-test",
        ]),
    )
    .unwrap();
    let repository = root.path().join("repository");
    let result = Command::new(env!("CARGO_BIN_EXE_korri-publish"))
        .env("KORRI_PUBLISH_NIX", &nix)
        .args([
            "repository",
            "--secret-key",
            key.to_str().unwrap(),
            "--platform",
            "aarch64-linux",
            "--release",
            "v1",
            "--base-url",
            "https://example.invalid/releases/",
            repository.to_str().unwrap(),
            &path,
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let catalog = Catalog::from_json(&fs::read(repository.join("catalog.json")).unwrap()).unwrap();
    let record = &catalog.records[0];
    assert_eq!(catalog.latest["@cli-test:hello"], "v1");
    assert_eq!(record.platform, "any");
    let archive_path = repository.join(record.archive_url.rsplit('/').next().unwrap());
    let cache = root.path().join("cache");
    fs::create_dir(&cache).unwrap();
    archive::extract(&archive_path, &cache).unwrap();
    assert!(package::archive_signature(
        Path::new(&nix),
        Path::new(&record.store_path),
        &archive::path_to_file_uri(&cache).unwrap(),
        record.public_key.as_deref()
    )
    .unwrap());
    let sideload = root.path().join("sideload");
    let result = Command::new(env!("CARGO_BIN_EXE_korri-publish"))
        .env("KORRI_PUBLISH_NIX", &nix)
        .args([
            "archive",
            "--platform",
            "x86_64-linux",
            "--secret-key",
            key.to_str().unwrap(),
            sideload.to_str().unwrap(),
            &path,
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(result.stdout.iter().filter(|b| **b == b'\n').count(), 1);
    let output: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(output["platform"], "any");
    assert_eq!(output["storePath"], record.store_path);
    assert!(Path::new(output["archive"].as_str().unwrap()).is_file());
    assert!(!sideload.join("catalog.json").exists());
}
