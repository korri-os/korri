// Build-side CLI. Metadata modes need no writable store or ambient Nix.
use korri_plugin_host::publisher;
use std::{env, path::Path, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("korri-publish: {}", error.escape_default());
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let (args, platform) = if matches!(
        args.first().map(String::as_str),
        Some("archive" | "repository")
    ) {
        platform_argument(args)?
    } else {
        (args, None)
    };
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["declaration", namespace, source_root, sources @ ..] => {
            // Build-side metadata from an explicit, closed source inventory.
            // This evaluates no native files and grants no installation authority.
            let sources = sources
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>();
            let snapshot = korri_plugin_host::script::source::SourceSnapshot::package_plugin(
                Path::new(source_root),
                "plugin.ts",
                &sources,
            )?;
            let declaration = korri_plugin_host::declaration::Declaration::evaluate_snapshot(
                namespace, &snapshot,
            )?;
            println!(
                "{}",
                serde_json::to_string(&declaration).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        ["archive", "--secret-key", key, output, path] => {
            let nix = env::var("KORRI_PUBLISH_NIX").map_err(|_| "package must supply KORRI_PUBLISH_NIX")?;
            let platform = publisher::publication_platform(Path::new(&nix), Path::new(path), platform.as_deref())?;
            std::fs::create_dir_all(output).map_err(|e| e.to_string())?;
            let result = publisher::publish_archive(&publisher::PublishArgs { nix: nix.into(), secret_key: key.into(), input_store_path: path.into(), release_version: "sideload".into(), platform, archive_url: String::new(), output_dir: output.into() })?;
            println!("{}", serde_json::json!({ "archive": result.archive_path, "sha256": result.archive_sha256, "platform": result.platform, "storePath": result.ca_store_path }));
            Ok(())
        }
        ["repository", "--secret-key", key, "--release", release, "--base-url", base, output, paths @ ..] => {
            let nix = env::var("KORRI_PUBLISH_NIX").map_err(|_| "package must supply KORRI_PUBLISH_NIX")?;
            publisher::publish_repository(Path::new(&nix), Path::new(key), release, base, Path::new(output), &paths.iter().map(PathBuf::from).collect::<Vec<_>>(), platform.as_deref())?;
            println!("{output}");
            Ok(())
        }
        _ => Err(concat!(
            "usage: korri-publish repository --secret-key FILE --release LABEL --base-url URL [--platform SYSTEM] OUTPUT_DIR STORE_PATH...\n",
            "       korri-publish archive --secret-key FILE [--platform SYSTEM] OUTPUT_DIR STORE_PATH\n",
            "       korri-publish declaration NAMESPACE SOURCE_ROOT SOURCE..."
        )
        .into()),
    }
}

fn platform_argument(args: Vec<String>) -> Result<(Vec<String>, Option<String>), String> {
    let mut args = args.into_iter();
    let mut positional = Vec::new();
    let mut platform = None;
    while let Some(argument) = args.next() {
        if argument != "--platform" {
            positional.push(argument);
            continue;
        }
        if platform.is_some() {
            return Err("--platform may be supplied only once".into());
        }
        let value = args.next().ok_or("--platform needs a Nix system")?;
        korri_plugin_host::catalog::validate_platform(&value)?;
        platform = Some(value);
    }
    Ok((positional, platform))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn platform_argument_is_optional_validated_and_not_repeatable() {
        let input = |args: &[&str]| args.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            platform_argument(input(&[
                "archive",
                "--platform",
                "aarch64-linux",
                "output",
                "path"
            ]))
            .unwrap(),
            (
                input(&["archive", "output", "path"]),
                Some("aarch64-linux".into())
            )
        );
        for args in [
            vec!["--platform"],
            vec!["--platform", "invalid"],
            vec!["--platform", "x86_64-linux", "--platform", "aarch64-linux"],
        ] {
            assert!(platform_argument(input(&args)).is_err());
        }
    }
}
