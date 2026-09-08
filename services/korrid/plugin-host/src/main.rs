use korri_plugin_host::{host::Host, package};
use std::{env, path::Path, process::ExitCode};

fn main() -> ExitCode {
    if unsafe { libc::geteuid() } != 0 {
        eprintln!("korri-plugin: administrator access is required");
        return ExitCode::FAILURE;
    }
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // Invalid declarations and cache errors can contain untrusted text.
            // Keep terminal control sequences out of the approval console.
            eprintln!("korri-plugin: {}", error.escape_default());
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let nix =
        env::var("KORRI_PLUGIN_NIX").map_err(|_| "host package must supply KORRI_PLUGIN_NIX")?;
    let systemctl = env::var("KORRI_PLUGIN_SYSTEMCTL")
        .map_err(|_| "host package must supply KORRI_PLUGIN_SYSTEMCTL")?;
    let host = Host::open(Path::new(&nix), Path::new(&systemctl))?;
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["inspect", source, path] => {
            let report = host.inspect(source, Path::new(path))?;
            println!("{}", serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?);
            host.release_download()
        }
        ["install", source, path, approval] => {
            let report = host.inspect(source, Path::new(path))?;
            println!("{}", serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?);
            host.install(report, approval, None)
        }
        ["update", id, source, path, approval] => {
            let report = host.inspect(source, Path::new(path))?;
            println!("{}", serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?);
            host.install(report, approval, Some(id))
        }
        ["enable", id] => host.set_enabled(id, true),
        ["disable", id] => host.set_enabled(id, false),
        ["remove", id] => host.remove(id, false),
        ["remove", id, "--purge"] => host.remove(id, true),
        ["status", id] => {
            let receipt = host.status(id)?;
            if let Some(receipt) = receipt {
                println!("{}", serde_json::to_string_pretty(&receipt).map_err(|e| e.to_string())?);
                Ok(())
            } else { Err("plugin is not installed".into()) }
        }
        ["restore"] => host.restore(),
        ["unit", id] => {
            korri_plugin_host::declaration::validate_id(id)?;
            println!("{}.service", package::unit_name(id));
            Ok(())
        }
        _ => Err("usage: korri-plugin inspect CACHE PACKAGE | install CACHE PACKAGE APPROVAL | update ID CACHE PACKAGE APPROVAL | enable ID | disable ID | remove ID [--purge] | status ID | unit ID | restore".into()),
    }
}
