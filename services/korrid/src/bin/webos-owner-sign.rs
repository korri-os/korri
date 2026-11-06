//! Hidden dev-only workstation operator command. Not installed in the TV IPK.
use std::{ffi::OsString, path::Path};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args == [OsString::from("--help")] {
        println!("{}", korrid::webos_owner_sign::HELP);
        return;
    }
    let [template_flag, template, owner_flag, owner] = args.as_slice() else {
        eprintln!("{}", korrid::webos_owner_sign::HELP);
        std::process::exit(2);
    };
    if template_flag != "--template" || owner_flag != "--expected-owner" {
        eprintln!("{}", korrid::webos_owner_sign::HELP);
        std::process::exit(2);
    }
    let no_core = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: valid resource and a live plain rlimit struct for this process.
    if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &no_core) } != 0 {
        eprintln!("webos-owner-sign: could not disable core dumps");
        std::process::exit(1);
    }
    let Some(owner) = owner.to_str() else {
        eprintln!("webos-owner-sign: expected owner must be a public key");
        std::process::exit(2);
    };
    match korrid::webos_owner_sign::run(Path::new(template), owner).await {
        Ok(event) => println!("{event}"),
        Err(error) => {
            eprintln!("webos-owner-sign: {error}");
            std::process::exit(1);
        }
    }
}
