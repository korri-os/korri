//! ADR 0046's build-side exception: no daemon, peer connection or target build.
use crate::{error::CliError, output::Output, prompt};
use clap::{Arg, ArgMatches, Command};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command as Process, Stdio},
};

const CONTRACT: &str = include_str!("../../../contracts/generated/korrid.ts");

pub fn commands() -> Vec<Command> {
    vec![
        Command::new("new")
            .about("Start a plugin with its own TypeScript contract; no daemon needed")
            .arg(Arg::new("name").required(true))
            .arg(
                Arg::new("namespace")
                    .long("namespace")
                    .value_name("@NAMESPACE"),
            ),
        Command::new("build")
            .about("Build locally with Nix and print the outputs; no daemon needed"),
        publication_options(
            Command::new("publish")
                .about("Build and sign a repository folder; does not upload it")
                .arg(Arg::new("out-dir").required(true)),
        ),
        Command::new("sideload")
            .about("Build locally, then review and approve a signed archive on a device")
            .arg(Arg::new("dir").default_value("."))
            .arg(
                Arg::new("platform")
                    .long("platform")
                    .value_name("NIX_SYSTEM_OR_ANY")
                    .help("Target platform, or any for a source-only plugin; peers do not report platforms yet"),
            )
            .arg(
                Arg::new("approval")
                    .long("approval")
                    .value_name("REVIEWED_DIGEST"),
            ),
    ]
}
fn publication_options(command: Command) -> Command {
    command
        .arg(
            Arg::new("release")
                .long("release")
                .required(true)
                .value_name("LABEL"),
        )
        .arg(
            Arg::new("url")
                .long("url")
                .required(true)
                .value_name("HTTPS_BASE_URL"),
        )
}

// Same labels and HTTPS policy as plugin-host/catalog.rs and publisher.rs.
fn release_label(value: &str) -> Result<String, String> {
    if value.is_empty()
        || value.len() > 64
        || value == "."
        || value == ".."
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._+".contains(&byte))
    {
        return Err("Use a release label of at most 64 letters, digits, dots, hyphens, underscores or plus signs.".into());
    }
    Ok(value.to_owned())
}
fn repository_url(value: &str) -> Result<String, String> {
    let valid = value.len() <= 4096
        && !value.chars().any(char::is_control)
        && value.ends_with('/')
        && reqwest::Url::parse(value).is_ok_and(|url| {
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
        });
    if !valid {
        return Err(
            "Use an HTTPS base URL ending in /, without credentials, a query or a fragment.".into(),
        );
    }
    Ok(value.to_owned())
}

pub fn is_offline(args: &ArgMatches) -> bool {
    matches!(args.subcommand_name(), Some("new" | "build" | "publish"))
}
pub fn run(args: &ArgMatches) -> Result<Output, CliError> {
    match args.subcommand() {
        Some(("new", args)) => new(args),
        Some(("build", _)) => {
            let paths = build(Path::new("."), None)?;
            Ok(Output::text(
                paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ))
        }
        Some(("publish", args)) => {
            let release = release_label(args.get_one::<String>("release").expect("required"))
                .map_err(CliError::usage)?;
            let url = repository_url(args.get_one::<String>("url").expect("required"))
                .map_err(CliError::usage)?;
            let outputs = build(Path::new("."), None)?;
            let namespace = namespace(&outputs)?;
            let key = author_key(&namespace)?;
            let directory = Path::new(args.get_one::<String>("out-dir").expect("required"));
            fs::create_dir_all(directory)
                .map_err(|_| file_error("create the repository folder"))?;
            publish(&outputs, directory, &key, &release, &url)?;
            Ok(Output::text(format!(
                "Repository folder: {}\nPublisher: {}\nFingerprint: {}",
                directory.display(),
                namespace,
                fingerprint(&key.public)?
            )))
        }
        _ => Err(CliError::usage("Choose plugins new, build or publish.")),
    }
}

fn valid_name(name: &str) -> bool {
    // plugin-host/declaration.rs's valid_name; author input is also a path.
    !name.is_empty()
        && name.len() <= 64
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"-_.".contains(&c))
}
pub fn validate_namespace(namespace: &str) -> Result<(), CliError> {
    if !namespace.strip_prefix('@').is_some_and(valid_name) {
        return Err(CliError::usage("A publisher namespace must be @ followed by a lowercase name (letters, digits, dots, underscores or hyphens; at most 64 characters)."));
    }
    Ok(())
}
fn new(args: &ArgMatches) -> Result<Output, CliError> {
    let name = args.get_one::<String>("name").expect("required");
    if !valid_name(name) || name == "." || name == ".." {
        return Err(CliError::usage("Use a lowercase plugin name of at most 64 letters, digits, dots, underscores or hyphens, starting with a letter or digit."));
    }
    let namespace = match args.get_one::<String>("namespace") {
        Some(namespace) => namespace.clone(),
        None if args.get_flag("json") => {
            return Err(CliError::usage(
                "Supply --namespace @name when using --json.",
            ))
        }
        None => prompt::read("Publisher namespace (@name)")?,
    };
    validate_namespace(&namespace)?;
    let root = Path::new(name);
    fs::create_dir(root).map_err(|_| {
        CliError::failure(
            "PluginProjectUnavailable",
            "Could not create the plugin folder. Existing folders are never overwritten.",
            "Choose a new plugin name in a writable folder.",
        )
    })?;
    let source = format!(
        r#"import type {{ PluginLaunchInput }} from "./korrid"

export const name = "{name}"
export const title = "Hello"
export const description = "Prints Hello files to the game journal."

export const systems = {{
  hello: {{ title: "Hello files", content: {{ extensions: ["hello"] }} }},
}}

export const runners = {{
  hello: {{ program: "cat" satisfies PluginLaunchInput["program"], systems: ["hello"] }},
}}
"#
    );
    let flake = format!(
        r#"{{
  inputs.korri.url = "github:korri-os/korri";
  inputs.nixpkgs.follows = "korri/nixpkgs";

  outputs = {{ korri, nixpkgs, ... }}: {{
    packages = nixpkgs.lib.genAttrs [ "x86_64-linux" "aarch64-linux" ] (system:
      let pkgs = nixpkgs.legacyPackages.${{system}};
      in {{
        default = korri.lib.${{system}}.mkPlugin {{ inherit pkgs; }} {{
          publisher.namespace = "{namespace}";
          source = ./.;
          plugin = ./plugin.nix;
        }};
      }});
  }};
}}
"#
    );
    let native = r#"{ pkgs }:
{
  # This runner prints the .hello file. Replace cat with the program your runner
  # should start, such as an emulator from nixpkgs.
  # A plugin without a runner can delete this file and the flake's plugin line.
  packages.coreutils = pkgs.coreutils;
  files.cat = "${pkgs.coreutils}/bin/cat";
}
"#;
    for (file, content) in [
        ("plugin.ts", source.as_str()),
        ("plugin.nix", native),
        ("flake.nix", flake.as_str()),
        ("korrid.ts", CONTRACT),
    ] {
        fs::write(root.join(file), content).map_err(|_| file_error("write the plugin files"))?;
    }
    Ok(Output::text(format!(
        "Plugin folder: {name}\nNext: Edit plugin.ts, then run korri plugins build in {name}."
    )))
}

#[derive(Deserialize)]
struct BuildResult {
    outputs: BTreeMap<String, PathBuf>,
}
#[derive(Deserialize)]
struct NixSetting<T> {
    value: T,
}
#[derive(Deserialize)]
struct NixConfig {
    system: NixSetting<String>,
    #[serde(rename = "extra-platforms")]
    extra_platforms: NixSetting<Vec<String>>,
}

pub fn build(directory: &Path, platform: Option<&str>) -> Result<Vec<PathBuf>, CliError> {
    // "any" is release metadata, not a Nix package attribute. Build locally
    // like an omitted platform; the publisher must prove architecture independence.
    let platforms = if let Some(platform) = platform.filter(|platform| *platform != "any") {
        // Match plugin-host/catalog.rs validate_platform before forming an attribute path.
        if platform.len() > 64
            || !platform
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            || !platform.contains('-')
        {
            return Err(CliError::usage("Use a Nix platform such as x86_64-linux, or any for an architecture-independent plugin. Platform names allow at most 64 letters, digits, hyphens or underscores."));
        }
        vec![platform.to_owned()]
    } else {
        let config: NixConfig = decode(&checked(
            Process::new("nix").args(["config", "show", "--json"]),
            "PluginBuildFailed",
            "read Nix's build platforms",
        )?)?;
        let available: Vec<String> = decode(&checked(
            Process::new("nix").current_dir(directory).args([
                "eval",
                "--json",
                ".#packages",
                "--apply",
                "builtins.attrNames",
            ]),
            "PluginBuildFailed",
            "read the plugin's build platforms",
        )?)?;
        available
            .into_iter()
            .filter(|system| {
                system == &config.system.value || config.extra_platforms.value.contains(system)
            })
            .collect()
    };
    if platforms.is_empty() {
        return Err(CliError::failure(
            "PluginBuildUnavailable",
            "This plugin has no output for a platform this computer can build.",
            "Check the plugin's flake.nix and this computer's Nix build platforms.",
        ));
    }
    let mut command = Process::new("nix");
    command
        .current_dir(directory)
        .args(["build", "--json", "--no-link"]);
    for platform in platforms {
        command.arg(format!(".#packages.{platform}.default"));
    }
    let results: Vec<BuildResult> = decode(&checked(
        &mut command,
        "PluginBuildFailed",
        "build the plugin",
    )?)?;
    let outputs: Vec<PathBuf> = results
        .into_iter()
        .flat_map(|result| result.outputs.into_values())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if outputs.is_empty() || outputs.iter().any(|path| !path.is_absolute()) {
        return Err(CliError::failure(
            "PluginBuildFailed",
            "Nix returned no usable plugin output.",
            "Check the plugin's flake.nix, then run korri plugins build again.",
        ));
    }
    Ok(outputs)
}
fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, CliError> {
    serde_json::from_slice(bytes).map_err(|_| {
        CliError::failure(
            "PluginBuildFailed",
            "The build tool returned invalid output.",
            "Check your Nix installation and the plugin's flake.nix.",
        )
    })
}

pub fn namespace(outputs: &[PathBuf]) -> Result<String, CliError> {
    // Grounding: plugin-host/package.rs Manifest.publisher.namespace.
    #[derive(Deserialize)]
    struct Publisher {
        namespace: String,
    }
    #[derive(Deserialize)]
    struct Manifest {
        publisher: Publisher,
    }
    let namespaces = outputs
        .iter()
        .map(|path| {
            let bytes = fs::read(path.join("manifest.json"))
                .map_err(|_| file_error("read the built plugin's manifest"))?;
            let manifest: Manifest = decode(&bytes)?;
            validate_namespace(&manifest.publisher.namespace)?;
            Ok(manifest.publisher.namespace)
        })
        .collect::<Result<BTreeSet<_>, CliError>>()?;
    if namespaces.len() != 1 {
        return Err(CliError::failure(
            "PluginPublisherMismatch",
            "The builds do not have one publisher namespace.",
            "Publish one publisher's plugin builds at a time.",
        ));
    }
    Ok(namespaces.into_iter().next().expect("one namespace"))
}

pub struct AuthorKey {
    pub path: PathBuf,
    pub public: String,
}
pub fn author_key(namespace: &str) -> Result<AuthorKey, CliError> {
    validate_namespace(namespace)?;
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .filter(|path| path.is_absolute())
        .ok_or_else(|| {
            CliError::failure(
                "PluginKeyUnavailable",
                "The publisher key has no absolute configuration folder.",
                "Set XDG_CONFIG_HOME to an absolute private folder.",
            )
        })?;
    let directory = config.join("korri/publishers");
    fs::create_dir_all(&directory).map_err(|_| file_error("create the publisher key folder"))?;
    let path = directory.join(format!("{namespace}.secret"));
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
    {
        Ok(mut file) => {
            let secret = match checked(
                Process::new("nix").args([
                    "key",
                    "generate-secret",
                    "--key-name",
                    namespace.trim_start_matches('@'),
                ]),
                "PluginKeyUnavailable",
                "create the publisher key",
            ) {
                Ok(secret) => secret,
                Err(error) => {
                    let _ = fs::remove_file(&path);
                    return Err(error);
                }
            };
            if let Err(error) = file.write_all(&secret).and_then(|_| file.sync_all()) {
                // A partial key is not retained as an apparently usable key.
                let _ = fs::remove_file(&path);
                return Err(file_error(&format!(
                    "save the publisher key ({})",
                    error.kind()
                )));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err(file_error("create the publisher key")),
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .map_err(|_| file_error("open the publisher key"))?;
    let metadata = file
        .metadata()
        .map_err(|_| file_error("check the publisher key"))?;
    if !metadata.is_file()
        || metadata.permissions().mode() & 0o777 != 0o600
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.len() == 0
    {
        return Err(CliError::failure(
            "PluginKeyUnavailable",
            "The publisher key must be a nonempty private file owned by you, with mode 0600.",
            format!(
                "Check {}. Do not replace a published key: existing devices pin it.",
                path.display()
            ),
        ));
    }
    let output = Process::new("nix")
        .args([
            "key",
            "convert-secret-to-public",
            "--extra-experimental-features",
            "nix-command",
        ])
        .stdin(Stdio::from(file))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| tool_error("PluginKeyUnavailable", "read the publisher's public key"))?;
    if !output.status.success() {
        return Err(tool_error(
            "PluginKeyUnavailable",
            "read the publisher's public key",
        ));
    }
    let public = String::from_utf8(output.stdout)
        .map_err(|_| tool_error("PluginKeyUnavailable", "read the publisher's public key"))?
        .trim()
        .to_owned();
    fingerprint(&public)?;
    Ok(AuthorKey { path, public })
}
// Approved schema sheet row 16. The matching host formatter lives in plugin-host,
// which is not a CLI dependency; pin this display contract in the author tests.
pub fn fingerprint(public: &str) -> Result<String, CliError> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let (label, key) = public
        .split_once(':')
        .filter(|(label, key)| {
            !label.is_empty()
                && label.bytes().all(|byte| byte.is_ascii_graphic())
                && key.len() >= 16
                && STANDARD.decode(key).is_ok_and(|bytes| bytes.len() == 32)
        })
        .ok_or_else(|| tool_error("PluginKeyUnavailable", "read the publisher's public key"))?;
    let groups = key.as_bytes()[..16]
        .chunks(4)
        .map(|bytes| std::str::from_utf8(bytes).expect("ASCII"))
        .collect::<Vec<_>>()
        .join(" ");
    Ok(format!("{label} {groups}"))
}
pub fn archive(
    output: &Path,
    directory: &Path,
    key: &AuthorKey,
) -> Result<crate::plugin_sideload::Archive, CliError> {
    let bytes = checked(
        Process::new("korri-publish")
            .args(["archive", "--secret-key"])
            .arg(&key.path)
            .arg(directory)
            .arg(output),
        "PluginPublishFailed",
        "make the signed archive",
    )?;
    decode(&bytes)
}
pub fn publish(
    outputs: &[PathBuf],
    directory: &Path,
    key: &AuthorKey,
    release: &str,
    url: &str,
) -> Result<(), CliError> {
    let mut command = Process::new("korri-publish");
    command
        .args(["repository", "--secret-key"])
        .arg(&key.path)
        .args(["--release", release, "--base-url", url])
        .arg(directory)
        .args(outputs);
    checked(
        &mut command,
        "PluginPublishFailed",
        "publish the signed repository folder",
    )?;
    Ok(())
}
fn checked(command: &mut Process, code: &str, action: &str) -> Result<Vec<u8>, CliError> {
    if command.get_program() == "nix" {
        command.args(["--extra-experimental-features", "nix-command flakes"]);
    }
    let output = command
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| tool_error(code, action))?;
    if !output.status.success() {
        return Err(tool_error(code, action));
    }
    Ok(output.stdout)
}
fn tool_error(code: &str, action: &str) -> CliError {
    CliError::failure(code, format!("Could not {action}."), "Check that Nix and korri-publish are installed, check the plugin's flake.nix, then try again.")
}
fn file_error(action: &str) -> CliError {
    CliError::failure(
        "PluginFilesUnavailable",
        format!("Could not {action}."),
        "Check the folder and its permissions, then try again.",
    )
}

/// Read archive bytes only on the author computer, never on a target.
pub fn read_archive(path: &Path) -> Result<fs::File, CliError> {
    fs::File::open(path).map_err(|_| file_error("open the built archive"))
}
pub fn hash_archive(path: &Path) -> Result<(String, u64), CliError> {
    use sha2::{Digest, Sha256};
    let mut file = read_archive(path)?;
    let mut hash = Sha256::new();
    let mut size = 0;
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| file_error("read the built archive"))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
        size += count as u64;
    }
    Ok((format!("{:x}", hash.finalize()), size))
}
