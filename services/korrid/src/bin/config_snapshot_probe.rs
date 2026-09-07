//! Review the production fixed local configuration snapshot loader.
//!
//! The probe intentionally uses ConfigSnapshotCoordinator, the same coordinator
//! wired into the brain runtime. It creates no watchers and performs no route
//! resolution or launch effects.

use std::{env, fs, path::PathBuf, process};

use korrid::config::snapshot::{
    ConfigSnapshotCoordinator, DEVICE_FILE_NAME, GAMES_FILE_NAME, RELEASES_FILE_NAME,
};

const CHECKPOINT_DEVICE: &str =
    include_str!("../../../../docs/research/android-app-plugin-schema-checkpoint/device.yaml");
const CHECKPOINT_GAMES: &str = include_str!(
    "../../../../docs/research/android-app-plugin-schema-checkpoint/catalog/games.yaml"
);

fn main() {
    if let Err(error) = run() {
        eprintln!("CONFIG SNAPSHOT FAILED: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
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
        GAMES_FILE_NAME,
        fs::read(root.join(GAMES_FILE_NAME)).map_err(|error| error.to_string())?
    );

    fs::write(root.join(DEVICE_FILE_NAME), CHECKPOINT_DEVICE)
        .map_err(|error| format!("cannot write checkpoint device: {error}"))?;
    fs::write(root.join(GAMES_FILE_NAME), CHECKPOINT_GAMES)
        .map_err(|error| format!("cannot write checkpoint games: {error}"))?;

    fs::write(
        root.join(RELEASES_FILE_NAME),
        include_str!(
            "../../../../docs/research/android-app-plugin-schema-checkpoint/catalog/releases.yaml"
        ),
    )
    .map_err(|error| error.to_string())?;
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
                .games
                .contains_key("01K4J6K8Y00000000000000001")
        )
    );

    fs::write(
        root.join(GAMES_FILE_NAME),
        "games:\n  bad id:\n    releases: []\n",
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
                .games
                .contains_key("01K4J6K8Y00000000000000001")
        )
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
    println!("game records: {}", state.snapshot.games.len());
    println!("release records: {}", state.snapshot.releases.len());
    println!("located releases: {}", state.snapshot.locations.len());
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}
