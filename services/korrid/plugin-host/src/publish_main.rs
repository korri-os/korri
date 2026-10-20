// Build-side CLI. Metadata modes need no writable store or ambient Nix.
use korri_plugin_host::{package, publisher};
use std::{env, io::Write, path::Path, path::PathBuf, process::ExitCode};

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
        ["archive-name", store_path, release, platform] => {
            let declaration = package::load_declaration(Path::new(store_path))?;
            println!(
                "{}",
                publisher::archive_name(&declaration.id(), release, platform)?
            );
            Ok(())
        }
        ["cache-record", store_path, release_version, platform, cache_url] => {
            // A record for an exact output in a signed Nix cache. Reads only
            // the output's declaration; needs no Nix and writes nothing.
            let record = publisher::cache_record(
                Path::new(store_path),
                release_version,
                platform,
                cache_url,
            )?;
            println!(
                "{}",
                serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        ["catalog", records @ ..] => {
            let paths = records.iter().map(PathBuf::from).collect::<Vec<_>>();
            std::io::stdout()
                .write_all(&publisher::catalog_from_records(&paths)?)
                .map_err(|e| e.to_string())
        }
        ["verify-release", catalog, assets, base_url, id, release, platforms @ ..] => {
            let platforms = platforms.iter().map(|s| s.to_string()).collect::<Vec<_>>();
            for name in publisher::release_assets(
                Path::new(catalog),
                Path::new(assets),
                base_url,
                id,
                release,
                &platforms,
            )? {
                println!("{name}");
            }
            Ok(())
        }
        [store_path, release_version, platform, archive_url, output_dir] => {
            let nix = env::var("KORRI_PUBLISH_NIX")
                .map_err(|_| "package must supply KORRI_PUBLISH_NIX")?;
            let result = publisher::publish(&publisher::PublishArgs {
                nix: PathBuf::from(nix),
                input_store_path: PathBuf::from(store_path),
                release_version: release_version.to_string(),
                platform: platform.to_string(),
                archive_url: archive_url.to_string(),
                output_dir: PathBuf::from(output_dir),
            })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&result.record).map_err(|e| e.to_string())?
            );
            eprintln!("archive: {}", result.archive_path.display());
            eprintln!("sha256:  {}", result.archive_sha256);
            eprintln!("ca path: {}", result.ca_store_path);
            Ok(())
        }
        _ => Err(concat!(
            "usage: korri-publish STORE_PATH RELEASE PLATFORM ARCHIVE_URL OUTPUT_DIR\n",
            "       korri-publish declaration NAMESPACE SOURCE_ROOT SOURCE...\n",
            "       korri-publish archive-name STORE_PATH RELEASE PLATFORM\n",
            "       korri-publish cache-record STORE_PATH RELEASE PLATFORM CACHE_URL\n",
            "       korri-publish catalog RECORD_FILE...\n",
            "       korri-publish verify-release CATALOG ASSET_DIR BASE_URL ID RELEASE PLATFORM..."
        )
        .into()),
    }
}
