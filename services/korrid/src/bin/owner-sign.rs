//! Hidden dev-only workstation operator command (`owner_sign.rs`). It never
//! ships to a device: korrid's package builds it apart (package.nix, `ownerSign`).
use std::{ffi::OsString, path::Path};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args == [OsString::from("--help")] {
        println!("{}", korrid::owner_sign::HELP);
        return;
    }
    let [template_flag, template, owner_flag, owner] = args.as_slice() else {
        eprintln!("{}", korrid::owner_sign::HELP);
        std::process::exit(2);
    };
    if template_flag != "--template" || owner_flag != "--expected-owner" {
        eprintln!("{}", korrid::owner_sign::HELP);
        std::process::exit(2);
    }
    let no_core = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: valid resource and a live plain rlimit struct for this process.
    if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &no_core) } != 0 {
        eprintln!("owner-sign: could not disable core dumps");
        std::process::exit(1);
    }
    let Some(owner) = owner.to_str() else {
        eprintln!("owner-sign: expected owner must be a public key");
        std::process::exit(2);
    };
    match korrid::owner_sign::run(Path::new(template), owner).await {
        Ok(event) => println!("{event}"),
        Err(error) => {
            eprintln!("owner-sign: {error}");
            std::process::exit(1);
        }
    }
}
