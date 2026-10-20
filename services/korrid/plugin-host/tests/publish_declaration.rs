use std::{fs, process::Command};

fn evaluate(root: &std::path::Path, sources: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_korri-publish"))
        .args(["declaration", "@korri"])
        .arg(root)
        .args(sources)
        .output()
        .unwrap()
}

#[test]
fn build_metadata_uses_the_real_declaration_and_explicit_source_inventory() {
    let root = tempfile::tempdir().unwrap();
    let source = include_str!("fixtures/steam.plugin.ts");
    fs::write(root.path().join("plugin.ts"), source).unwrap();
    let output = evaluate(root.path(), &["plugin.ts"]);
    assert!(output.status.success(), "{:?}", output);
    let declaration: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(declaration["name"], "steam");
    assert_eq!(
        declaration["discovery"][0]["id"],
        "@korri:steam/installed-apps"
    );
    assert!(!evaluate(root.path(), &[]).status.success());
    assert!(!evaluate(root.path(), &["../plugin.ts"]).status.success());
}

#[test]
fn build_metadata_loads_only_selected_modules_in_the_empty_sandbox() {
    let root = tempfile::tempdir().unwrap();
    // Split the actual declaration into its identity and contribution module.
    let source = include_str!("fixtures/steam.plugin.ts");
    let (identity, contributions) = source.split_once("export const discovery").unwrap();
    fs::write(
        root.path().join("plugin.ts"),
        format!("{identity}\nexport * from './discovery.ts';"),
    )
    .unwrap();
    fs::write(
        root.path().join("discovery.ts"),
        format!("export const discovery{contributions}"),
    )
    .unwrap();
    assert!(!evaluate(root.path(), &["plugin.ts"]).status.success());
    let output = evaluate(root.path(), &["discovery.ts", "plugin.ts"]);
    assert!(output.status.success(), "{:?}", output);
    fs::write(
        root.path().join("discovery.ts"),
        "fetch('https://example.com');",
    )
    .unwrap();
    assert!(!evaluate(root.path(), &["discovery.ts", "plugin.ts"])
        .status
        .success());
}
