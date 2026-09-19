//! Image-time seeding must produce exactly the approval the device re-derives.
//!
//! The device trusts a seeded receipt only because `restore-all` calls the same
//! `package::load` it always used. These tests prove the seed entry point agrees
//! with that runtime report and that it still fails closed on a bad path.
use korri_plugin_host::{
    package,
    provenance::Provenance,
    selection::{Desired, Receipt},
};
use std::{env, fs, path::Path, process::Command};

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

fn write_package(directory: &Path, name: &str) -> String {
    let package = directory.join(format!("{name}-package"));
    fs::create_dir(&package).unwrap();
    fs::write(
        package.join("plugin.ts"),
        format!("export const name = '{name}';"),
    )
    .unwrap();
    fs::write(
        package.join("manifest.json"),
        serde_json::to_vec(&serde_json::json!({
            "publisher": {"namespace": "@example"},
            "entry": "plugin.ts",
            "sources": ["plugin.ts"],
        }))
        .unwrap(),
    )
    .unwrap();
    package.to_str().unwrap().to_owned()
}

#[test]
fn seeding_refuses_anything_that_is_not_an_exact_store_output() {
    let provenance = Provenance::RawCache {
        cache_url: "file:///cache".into(),
    };
    for path in ["/tmp/not-a-store-path", "/nix/store/not-a-hash"] {
        let error = package::load_for_seed(Path::new(path), provenance.clone())
            .err()
            .unwrap();
        assert!(error.contains("exact /nix/store output"), "{path}: {error}");
    }
}

#[test]
#[ignore = "writes build-machine Nix store; run korri-publisher-check"]
fn seeded_approval_equals_the_runtime_report_and_round_trips() {
    let directory = tempfile::tempdir().unwrap();
    let source = write_package(directory.path(), "seed");
    let path = nix(&["store", "add-path", &source]);
    let provenance = Provenance::RawCache {
        cache_url: "file:///cache".into(),
    };

    let seeded = package::load_for_seed(Path::new(&path), provenance.clone()).unwrap();
    let runtime = package::load(
        Path::new(&env::var("KORRI_PUBLISH_NIX").unwrap()),
        Path::new(&path),
        provenance.clone(),
    )
    .unwrap();
    assert_eq!(seeded.approval, runtime.approval);
    assert_eq!(seeded.id, runtime.id);

    let receipt = Receipt {
        id: seeded.id.clone(),
        package: seeded.package.clone(),
        provenance,
        approval: seeded.approval.clone(),
        desired: Desired::Enabled,
        previous: None,
    };
    let encoded = serde_json::to_vec(&receipt).unwrap();
    let decoded: Receipt = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, receipt);

    // The digest binds the source, so a changed plugin cannot reuse it.
    let other_source = write_package(directory.path(), "changed");
    fs::write(
        Path::new(&other_source).join("plugin.ts"),
        "export const name = 'seed-changed';",
    )
    .unwrap();
    let other = nix(&["store", "add-path", &other_source]);
    let changed = package::load_for_seed(Path::new(&other), receipt.provenance.clone()).unwrap();
    assert_ne!(changed.approval, receipt.approval);
}
