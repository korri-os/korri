fn main() -> std::process::ExitCode {
    let result = korri_kiosk::Options::parse(std::env::args().skip(1)).and_then(korri_kiosk::run);
    match result {
        Ok(()) | Err(korri_kiosk::Error::Interrupted) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            // CLI diagnostics contain only a fixed error class, never a payload.
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
