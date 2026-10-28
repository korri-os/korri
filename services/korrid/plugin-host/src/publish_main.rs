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
            let platform = publisher::detected_platform(Path::new(&nix), Path::new(path))?;
            std::fs::create_dir_all(output).map_err(|e| e.to_string())?;
            let result = publisher::publish_archive(&publisher::PublishArgs { nix: nix.into(), secret_key: key.into(), input_store_path: path.into(), release_version: "sideload".into(), platform, archive_url: String::new(), output_dir: output.into() })?;
            println!("{}", serde_json::json!({ "archive": result.archive_path, "sha256": result.archive_sha256, "platform": result.platform, "storePath": result.ca_store_path }));
            Ok(())
        }
        ["repository", "--secret-key", key, "--release", release, "--base-url", base, output, paths @ ..] => {
            let nix = env::var("KORRI_PUBLISH_NIX").map_err(|_| "package must supply KORRI_PUBLISH_NIX")?;
            publisher::publish_repository(Path::new(&nix), Path::new(key), release, base, Path::new(output), &paths.iter().map(PathBuf::from).collect::<Vec<_>>())?;
            println!("{output}");
            Ok(())
        }
        _ => Err(concat!(
            "usage: korri-publish repository --secret-key FILE --release LABEL --base-url URL OUTPUT_DIR STORE_PATH...\n",
            "       korri-publish archive --secret-key FILE OUTPUT_DIR STORE_PATH\n",
            "       korri-publish declaration NAMESPACE SOURCE_ROOT SOURCE..."
        )
        .into()),
    }
}
