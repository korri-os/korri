//! THROWAWAY verification and cost experiment. Not a product package format.
mod allocation;
mod conformance;
mod index;
mod large;
mod quickjs;
#[path = "../src/view.rs"]
#[allow(dead_code)]
mod view;
use anyhow::{bail, Context, Result};
use std::path::Path;

pub fn self_command() -> Result<std::process::Command> {
    // An explicit bundled ld-linux launch identifies the loader as current_exe.
    let program = match std::env::var_os("SURFACE_VERIFY_LAUNCHER") {
        Some(path) => std::path::PathBuf::from(path),
        None => std::env::current_exe()?,
    };
    Ok(std::process::Command::new(program))
}

fn main() -> Result<()> {
    // Reproducible timezone for trusted package timestamp formatting.
    std::env::set_var("TZ", "UTC");
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("verify") if args.len() == 4 => {
            let model = std::fs::read_to_string(&args[3])?;
            let mut result = conformance::verify(Path::new(&args[2]), &model, true)?;
            result["worker_hwm_kib"] = serde_json::json!(index::rss("VmHWM:"));
            println!("{}", result);
            if result["pass"] != true { std::process::exit(1); }
        }
        Some("phase") if args.len() == 5 => println!("{}", index::phase(&args[2], Path::new(&args[3]), Path::new(&args[4]))?),
        Some("many") if (4..=5).contains(&args.len()) => index::many(Path::new(&args[2]), args[3].parse()?, args.get(4).is_some_and(|s| s == "--hardlinks"))?,
        Some("large") if args.len() == 4 => large::run(Path::new(&args[2]), Path::new(&args[3]))?,
        Some("large-one") if args.len() == 5 => large::one(Path::new(&args[2]), Path::new(&args[3]), args[4].parse()?)?,
        _ => bail!("usage: surface-verify verify PACKAGE MODEL | many SPIKE_ROOT N [--hardlinks] | large SPIKE_ROOT OUTPUT | large-one PACKAGE MODEL N | phase METHOD PACKAGES MODEL"),
    }
    std::io::Write::flush(&mut std::io::stdout()).context("flush results")?;
    Ok(())
}
