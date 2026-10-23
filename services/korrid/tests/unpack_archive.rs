//! ADR 0030: `korrid unpack-archive <work-dir> <file>` keeps the ADR 0012
//! unpack-program contract: exit 0 or 1, diagnostics on stderr, no stdout.
use std::{
    fs,
    io::{Cursor, Write},
    os::unix::fs::PermissionsExt,
    process::Command,
};

fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o755);
    for (name, data) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(data).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn run(bytes: &[u8]) -> (std::process::Output, tempfile::TempDir) {
    let folder = tempfile::tempdir().unwrap();
    let archive = folder.path().join("download");
    fs::write(&archive, bytes).unwrap();
    let work = folder.path().join("work");
    fs::create_dir(&work).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_korrid"))
        .arg("unpack-archive")
        .arg(&work)
        .arg(&archive)
        .output()
        .unwrap();
    (output, folder)
}

#[test]
fn the_reader_subcommand_unpacks_and_exits_zero_with_no_stdout() {
    let (output, folder) = run(&zip(&[("Game (USA).gba", b"\x00\x01rom")]));
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty());
    let file = folder.path().join("work/Game (USA).gba");
    assert_eq!(fs::read(&file).unwrap(), b"\x00\x01rom");
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o644
    );
}

#[test]
fn the_reader_subcommand_refuses_with_exit_one_and_writes_nothing() {
    let (output, folder) = run(&zip(&[
        ("Game.gba", b"\x00\x01rom"),
        ("../escape.gba", b"x"),
    ]));
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("korrid unpack-archive"), "{stderr}");
    assert_eq!(fs::read_dir(folder.path().join("work")).unwrap().count(), 0);
    assert!(!folder.path().join("escape.gba").exists());

    let (usage, _) = {
        let output = Command::new(env!("CARGO_BIN_EXE_korrid"))
            .arg("unpack-archive")
            .output()
            .unwrap();
        (output, ())
    };
    assert_eq!(usage.status.code(), Some(1));
}
