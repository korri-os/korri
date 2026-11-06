//! The sample-data korrid process, using the same server as the shell tests.
fn main() -> std::io::Result<()> {
    korri_shell::fixture::process::run()
}
