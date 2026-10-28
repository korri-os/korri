use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};
#[path = "support/plugin_tools.rs"]
mod plugin_tools;
use plugin_tools::AuthorTools;

fn cli(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_korri"));
    command
        .current_dir(root)
        .env("KORRI_CLI_SOCKET", root.join("missing.sock"));
    command
}

#[test]
fn plugin_new_is_self_contained_and_never_overwrites_a_project() {
    let tools = AuthorTools::new();
    let root = &tools.root;
    let output = tools
        .cli()
        .args(["plugins", "new", "hello", "--namespace", "@author"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let project = root.path().join("hello");
    let source = fs::read_to_string(project.join("plugin.ts")).unwrap();
    assert!(source.contains("from \"./korrid\""));
    assert!(!source.contains("id:"));
    assert!(!source.contains("handlers"));
    assert_eq!(
        fs::read_to_string(project.join("korrid.ts")).unwrap(),
        include_str!("../../../contracts/generated/korrid.ts")
    );
    let flake = fs::read_to_string(project.join("flake.nix")).unwrap();
    assert!(flake.contains("korri.lib.${system}.mkPlugin { inherit pkgs; }"));
    assert!(flake.contains("publisher.namespace = \"@author\""));
    assert!(flake.contains("plugin = ./plugin.nix;"));
    assert!(source.contains("program: \"cat\""));
    let native = fs::read_to_string(project.join("plugin.nix")).unwrap();
    assert!(native.contains("packages.coreutils = pkgs.coreutils;"));
    assert!(native.contains("files.cat = \"${pkgs.coreutils}/bin/cat\";"));
    let output = tools
        .cli()
        .args(["plugins", "new", "hello", "--namespace", "@author"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        fs::read_to_string(project.join("plugin.ts")).unwrap(),
        source
    );
    assert!(!root.path().join("nix.calls").exists());
    assert!(!root.path().join("publisher.calls").exists());
}

#[test]
fn plugin_new_typescript_passes_the_repository_typescript_compiler() {
    let root = tempfile::tempdir().unwrap();
    assert!(cli(root.path())
        .args(["plugins", "new", "hello", "--namespace", "@author"])
        .status()
        .unwrap()
        .success());
    // The focused CLI Nix check puts the repository's pkgs.typescript on PATH.
    let output = Command::new("tsc")
        .current_dir(root.path().join("hello"))
        .args([
            "--strict",
            "--noUnusedLocals",
            "--noEmit",
            "--target",
            "ES2022",
            "--module",
            "ESNext",
            "--moduleResolution",
            "Bundler",
            "plugin.ts",
        ])
        .output()
        .expect("run this test with the repository's Nix TypeScript on PATH");
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn plugin_build_prints_outputs_for_the_systems_this_machine_can_build() {
    let tools = AuthorTools::new();
    let output = tools
        .cli()
        .args(["plugins", "build", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let calls = fs::read_to_string(tools.root.path().join("nix.calls")).unwrap();
    assert!(calls.contains(".#packages.x86_64-linux.default"));
    assert!(calls.contains(".#packages.aarch64-linux.default"));
    let text: String = serde_json::from_slice(&output.stdout).unwrap();
    assert!(text.contains("/output"));
}

#[test]
fn plugin_publish_creates_a_private_key_once_and_passes_it_to_the_publisher() {
    let tools = AuthorTools::new();
    for _ in 0..2 {
        let output = tools
            .cli()
            .args([
                "plugins",
                "publish",
                "repository",
                "--release",
                "v1",
                "--url",
                "https://example.com/plugins/",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("repository"));
        assert!(text.contains("author AbCd EfGh IjKl MnOp"));
        assert!(!text.contains("secret-bytes"));
    }
    let path = tools
        .root
        .path()
        .join("config/korri/publishers/@author.secret");
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let calls = fs::read_to_string(tools.root.path().join("nix.calls")).unwrap();
    assert_eq!(calls.matches("key generate-secret").count(), 1);
    let calls = fs::read_to_string(tools.root.path().join("publisher.calls")).unwrap();
    assert!(calls.contains("--secret-key"));
    assert!(calls.contains("@author.secret"));
}

#[test]
fn plugin_publish_stops_when_the_publisher_fails() {
    let tools = AuthorTools::new();
    let output = tools
        .cli()
        .env("PUBLISH_EXIT", "7")
        .args([
            "plugins",
            "publish",
            "repository",
            "--release",
            "v1",
            "--url",
            "https://example.com/plugins/",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error: korrid::RpcFailure = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error.code, "PluginPublishFailed");
}

#[test]
#[ignore = "Uses real Nix, which initializes Nix state; run explicitly outside the build sandbox"]
fn plugin_publish_uses_the_real_nix_secret_key_format() {
    let nix = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|path| path.join("nix"))
        .find(|path| path.is_file())
        .expect("Nix is installed for author checks");
    let tools = AuthorTools::new();
    let output = tools
        .cli()
        .env("NIX_REAL_KEYS", &nix)
        .args([
            "plugins",
            "publish",
            "repository",
            "--release",
            "v1",
            "--url",
            "https://example.com/plugins/",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let secret_path = tools
        .root
        .path()
        .join("config/korri/publishers/@author.secret");
    let secret = fs::read_to_string(&secret_path).unwrap();
    assert!(secret.starts_with("author:"));
    use base64::Engine;
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(secret.trim().split_once(':').unwrap().1)
            .unwrap()
            .len(),
        64
    );
    let public = Command::new(nix)
        .args([
            "key",
            "convert-secret-to-public",
            "--extra-experimental-features",
            "nix-command",
        ])
        .stdin(fs::File::open(secret_path).unwrap())
        .output()
        .unwrap();
    assert!(public.status.success());
    let public = String::from_utf8(public.stdout).unwrap();
    let (label, key) = public.trim().split_once(':').unwrap();
    let expected = format!(
        "{label} {} {} {} {}",
        &key[0..4],
        &key[4..8],
        &key[8..12],
        &key[12..16]
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(&expected));
    assert!(!String::from_utf8_lossy(&output.stdout).contains(secret.trim()));
}

#[test]
fn plugin_author_invalid_inputs_and_tool_failures_do_not_succeed_or_overwrite_keys() {
    let root = tempfile::tempdir().unwrap();
    for args in [
        vec!["plugins", "new", "../escape", "--namespace", "@author"],
        vec!["plugins", "new", "hello", "--namespace", "@../escape"],
        vec!["plugins", "new", "hello", "--json"],
        vec![
            "plugins",
            "new",
            "hello",
            "--namespace",
            "@author",
            "--device",
            "There",
        ],
    ] {
        let output = cli(root.path()).args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    let tools = AuthorTools::new();
    let output = tools
        .cli()
        .env("NIX_EXIT", "7")
        .args(["plugins", "build", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let path = tools
        .root
        .path()
        .join("config/korri/publishers/@author.secret");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "author:existing-secret").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let output = tools
        .cli()
        .args([
            "plugins",
            "publish",
            "repository",
            "--release",
            "v1",
            "--url",
            "https://example.com/plugins/",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read_to_string(path).unwrap(), "author:existing-secret");
    assert!(!tools.root.path().join("publisher.calls").exists());
}

#[test]
fn plugin_publish_validates_release_and_repository_address_before_building_or_creating_keys() {
    let tools = AuthorTools::new();
    for (release, url) in [
        ("../v1", "https://example.com/plugins/"),
        ("v1", "http://example.com/plugins/"),
        ("v1", "https://user:password@example.com/plugins/"),
        ("v1", "https://example.com/plugins/?token=secret"),
        ("v1", "https://example.com/plugins"),
    ] {
        let output = tools
            .cli()
            .args([
                "plugins",
                "publish",
                "repository",
                "--release",
                release,
                "--url",
                url,
                "--json",
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("password"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("secret"));
    }
    assert!(!tools.root.path().join("nix.calls").exists());
    assert!(!tools.root.path().join("config").exists());
}

// Preserve real Nix syntax and evaluation assertions in the explicit build
// proof. Nix initializes its state even for --parse, outside the author fixture.
fn assert_generated_nix_files_parse_and_evaluate(project: &Path) {
    for file in ["flake.nix", "plugin.nix"] {
        let parsed = Command::new("nix-instantiate")
            .arg("--parse")
            .arg(project.join(file))
            .output()
            .unwrap();
        assert!(
            parsed.status.success(),
            "{}",
            String::from_utf8_lossy(&parsed.stderr)
        );
    }
    let evaluated = Command::new("nix")
        .args(["eval", "--json", "--impure", "--expr"])
        .arg(format!(
            "import {} {{ pkgs.coreutils = \"/nix/store/test-coreutils\"; }}",
            project.join("plugin.nix").display()
        ))
        .args(["--extra-experimental-features", "nix-command"])
        .output()
        .unwrap();
    assert!(
        evaluated.status.success(),
        "{}",
        String::from_utf8_lossy(&evaluated.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&evaluated.stdout).unwrap()["files"]["cat"],
        "/nix/store/test-coreutils/bin/cat"
    );
}

/// Separate real-build proof; ordinary CLI tests use configured author tools.
#[test]
#[ignore = "Performs a local Nix plugin build; run explicitly with --ignored --exact"]
fn plugin_new_builds_and_admits_the_generated_runner() {
    use std::{collections::BTreeMap, path::PathBuf};
    let root = tempfile::tempdir().unwrap();
    assert!(cli(root.path())
        .args(["plugins", "new", "hello", "--namespace", "@author"])
        .status()
        .unwrap()
        .success());
    assert_generated_nix_files_parse_and_evaluate(&root.path().join("hello"));
    let system = Command::new("nix")
        .args([
            "eval",
            "--impure",
            "--raw",
            "--expr",
            "builtins.currentSystem",
            "--extra-experimental-features",
            "nix-command flakes",
        ])
        .output()
        .unwrap();
    assert!(
        system.status.success(),
        "{}",
        String::from_utf8_lossy(&system.stderr)
    );
    let system = String::from_utf8(system.stdout).unwrap();
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    // The project's own input must resolve to this checkout for both the build
    // and the later read-only namespace lookup. A per-command override would
    // not persist in the project's native lock file.
    let flake_path = root.path().join("hello/flake.nix");
    let flake = fs::read_to_string(&flake_path).unwrap().replace(
        "github:korri-os/korri",
        &format!("git+file://{}", repository.display()),
    );
    fs::write(flake_path, flake).unwrap();
    let output = Command::new("nix")
        .current_dir(root.path().join("hello"))
        .args(["build", "--json", "--no-link"])
        .arg(format!(".#packages.{system}.default"))
        .args(["--extra-experimental-features", "nix-command flakes"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        korri::plugin_author::project_namespace(&root.path().join("hello")).unwrap(),
        "@author"
    );
    let builds: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let package = PathBuf::from(builds[0]["outputs"]["out"].as_str().unwrap());
    #[derive(serde::Deserialize)]
    struct Manifest {
        files: BTreeMap<String, PathBuf>,
        entry: String,
        sources: Vec<String>,
        #[serde(default)]
        requires: Vec<PathBuf>,
        #[serde(default)]
        hosts: Vec<String>,
    }
    let manifest: Manifest =
        serde_json::from_slice(&fs::read(package.join("manifest.json")).unwrap()).unwrap();
    let cat = manifest.files["cat"].clone();
    assert!(cat.is_file());
    let registry = korrid::plugin::PluginRegistry::from_installed(vec![
        korrid::plugin_installation::EnabledPackage {
            id: "@author:hello".into(),
            package,
            files: manifest.files,
            entry: manifest.entry,
            sources: manifest.sources,
            requires: manifest.requires,
            hosts: manifest.hosts,
        },
    ])
    .unwrap();
    registry.native_runner("@author:hello/hello").unwrap();
    assert_eq!(
        registry
            .installed_file("@author:hello/hello", "cat")
            .unwrap(),
        cat
    );
    // ADR 0045's handler-free launch passes this file as the one argument.
    let content = root.path().join("example.hello");
    fs::write(&content, "Hello from the generated plugin.\n").unwrap();
    let output = Command::new(cat).arg(content).output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"Hello from the generated plugin.\n");
}

#[test]
fn plugin_fingerprint_uses_the_approved_row_16_label_and_four_groups() {
    assert_eq!(
        korri::plugin_author::fingerprint(
            "korri-plugins-1:AbCdEfGhIjKlMnOpAAAAAAAAAAAAAAAAAAAAAAAAAAA="
        )
        .unwrap(),
        "korri-plugins-1 AbCd EfGh IjKl MnOp",
    );
}
