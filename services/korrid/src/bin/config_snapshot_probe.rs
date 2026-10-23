//! Review the production fixed local configuration snapshot loader.
//!
//! The probe intentionally uses ConfigSnapshotCoordinator, the same coordinator
//! wired into the brain runner. It creates no watchers and performs no route
//! resolution or launch effects.

use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process,
};

use korrid::config::snapshot::{ConfigSnapshotCoordinator, DEVICE_FILE_NAME, LIBRARY_FILE_NAME};

const CHECKPOINT_DEVICE: &str =
    include_str!("../../../../docs/research/android-app-plugin-schema-checkpoint/device.yaml");
const CHECKPOINT_LIBRARY: &str = "library:\n  tmnt-shredders-revenge:\n    title: TMNT\n    releases:\n      - id: android\n        system: android\n";

fn main() {
    if let Err(error) = run() {
        eprintln!("CONFIG SNAPSHOT FAILED: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let arguments: Vec<_> = env::args_os().skip(1).collect();
    if arguments
        .first()
        .is_some_and(|v| v == OsStr::new("--check"))
    {
        if arguments.len() != 3 {
            return Err(
                "usage: config_snapshot_probe --check <device.yaml> <catalog/library.yaml>".into(),
            );
        }
        return check_documents(Path::new(&arguments[1]), Path::new(&arguments[2]));
    }
    let root = env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| "usage: config_snapshot_probe <storage-root>".to_owned())?;
    fs::create_dir_all(&root).map_err(|error| format!("cannot create review root: {error}"))?;
    let coordinator = ConfigSnapshotCoordinator::new(&root);

    println!("== empty initialization ==");
    let empty = coordinator.reload();
    print_state(&empty);
    println!(
        "{} bytes: {:?}",
        DEVICE_FILE_NAME,
        fs::read(root.join(DEVICE_FILE_NAME)).map_err(|error| error.to_string())?
    );
    println!(
        "{} bytes: {:?}",
        LIBRARY_FILE_NAME,
        fs::read(root.join(LIBRARY_FILE_NAME)).map_err(|error| error.to_string())?
    );

    fs::write(root.join(DEVICE_FILE_NAME), CHECKPOINT_DEVICE)
        .map_err(|error| format!("cannot write checkpoint device: {error}"))?;
    fs::write(root.join(LIBRARY_FILE_NAME), CHECKPOINT_LIBRARY)
        .map_err(|error| format!("cannot write checkpoint games: {error}"))?;

    println!();
    println!("== exact checkpoint load ==");
    let checkpoint = coordinator.reload();
    print_state(&checkpoint);
    println!(
        "host.title: {}",
        checkpoint
            .snapshot
            .host
            .as_ref()
            .and_then(|host| host.title.as_deref())
            .unwrap_or("<none>")
    );
    println!(
        "tmnt present: {}",
        yes_no(
            checkpoint
                .snapshot
                .library
                .contains_key("tmnt-shredders-revenge")
        )
    );

    fs::write(
        root.join(LIBRARY_FILE_NAME),
        "library:\n  bad id:\n    releases: []\n",
    )
    .map_err(|error| format!("cannot write rejected edit: {error}"))?;

    println!();
    println!("== rejected edit keeps last known good ==");
    let rejected = coordinator.reload();
    print_state(&rejected);
    println!(
        "retained tmnt: {}",
        yes_no(
            rejected
                .snapshot
                .library
                .contains_key("tmnt-shredders-revenge")
        )
    );

    Ok(())
}

/// Validate captured documents only. Unlike review mode, this never initializes,
/// rewrites or watches the source root, including when validation fails.
fn check_documents(device_path: &Path, library_path: &Path) -> Result<(), String> {
    let read =
        |path: &Path| fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()));
    let snapshot =
        korrid::config::decode_config_documents(&read(device_path)?, &read(library_path)?)
            .map_err(|e| e.to_string())?;
    korrid::config::classify_snapshot_support(&snapshot).map_err(|e| e.to_string())?;
    println!(
        "CONFIG VALID: {} and {}",
        device_path.display(),
        library_path.display()
    );
    Ok(())
}

fn print_state(state: &korrid::config::snapshot::ConfigSnapshotState) {
    println!("generation: {}", state.generation);
    println!("authorization: {:?}", state.authorization);
    match &state.diagnostic {
        Some(diagnostic) => {
            println!("diagnostic.code: {:?}", diagnostic.code);
            println!("diagnostic.message: {}", diagnostic.message);
        }
        None => println!("diagnostic: none"),
    }
    println!("game records: {}", state.snapshot.library.len());
    println!(
        "release records: {}",
        state
            .snapshot
            .library
            .values()
            .map(|g| g.releases.len())
            .sum::<usize>()
    );
    println!("located releases: {}", state.snapshot.locations.len());
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}
