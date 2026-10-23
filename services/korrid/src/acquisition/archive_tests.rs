//! The reader on small archives built in code, including hostile ones.
use super::*;
use std::io::{Cursor, Write};

const ROM: &[u8] = b"\x00\x01 a lawful homebrew rom image";

/// One stored zip entry, written by hand so that tests control every field.
struct Raw<'a> {
    name: &'a [u8],
    data: &'a [u8],
    mode: Option<u32>,
    flags: u16,
    declared: Option<u32>,
}
fn raw(name: &str, data: &'static [u8]) -> Raw<'static> {
    Raw {
        name: Box::leak(name.as_bytes().to_vec().into_boxed_slice()),
        data,
        mode: None,
        flags: 0,
        declared: None,
    }
}

fn raw_zip(entries: &[Raw<'_>]) -> Vec<u8> {
    let count = entries.len() as u16;
    raw_zip_counts(entries, 0, count, count)
}

/// The end record's disk number, on-this-disk count and total count, which
/// a forged archive can set apart from the entries it holds.
fn raw_zip_counts(entries: &[Raw<'_>], disk: u16, on_disk: u16, total: u16) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for entry in entries {
        let offset = out.len() as u32;
        let crc = crc32fast::hash(entry.data);
        let size = entry.data.len() as u32;
        let declared = entry.declared.unwrap_or(size);
        out.extend_from_slice(&0x04034b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&entry.flags.to_le_bytes());
        out.extend_from_slice(&[0; 6]); // stored, time, date
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&declared.to_le_bytes());
        out.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(entry.name);
        out.extend_from_slice(entry.data);

        central.extend_from_slice(&0x02014b50u32.to_le_bytes());
        let made_by: u16 = if entry.mode.is_some() {
            3 << 8 | 20
        } else {
            20
        };
        central.extend_from_slice(&made_by.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&entry.flags.to_le_bytes());
        central.extend_from_slice(&[0; 6]);
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&declared.to_le_bytes());
        central.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0; 8]); // extra, comment, disk, internal
        central.extend_from_slice(&(entry.mode.unwrap_or(0) << 16).to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(entry.name);
    }
    let start = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x06054b50u32.to_le_bytes());
    out.extend_from_slice(&disk.to_le_bytes());
    out.extend_from_slice(&disk.to_le_bytes());
    out.extend_from_slice(&on_disk.to_le_bytes());
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// Rewrite a small zip's end record as zip64: a zip64 end record and its
/// locator before an end record whose counts say 0xFFFF.
fn zip64(zip: &[u8], on_disk: u64, total: u64) -> Vec<u8> {
    let end = zip.len() - 22;
    let size = u64::from(u32::from_le_bytes(
        zip[end + 12..end + 16].try_into().unwrap(),
    ));
    let offset = u64::from(u32::from_le_bytes(
        zip[end + 16..end + 20].try_into().unwrap(),
    ));
    let mut out = zip[..end].to_vec();
    let record_at = out.len() as u64;
    out.extend_from_slice(b"PK\x06\x06");
    out.extend_from_slice(&44u64.to_le_bytes());
    out.extend_from_slice(&45u16.to_le_bytes());
    out.extend_from_slice(&45u16.to_le_bytes());
    out.extend_from_slice(&[0; 8]); // this disk, central directory disk
    out.extend_from_slice(&on_disk.to_le_bytes());
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(b"PK\x06\x07");
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&record_at.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(b"PK\x05\x06");
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&[0xff; 4]);
    out.extend_from_slice(&[0xff; 8]);
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn deflated_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(true);
    for (name, data) in entries {
        if let Some(folder) = name.strip_suffix('/') {
            writer.add_directory(folder, options).unwrap();
        } else {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
    }
    writer.finish().unwrap().into_inner()
}

fn seven_z(
    entries: &[(&str, &[u8])],
    configure: impl FnOnce(&mut sevenz_rust2::ArchiveWriter<Cursor<Vec<u8>>>),
) -> Vec<u8> {
    use sevenz_rust2::{ArchiveEntry, ArchiveWriter};
    let mut writer = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    configure(&mut writer);
    for (name, data) in entries {
        if let Some(folder) = name.strip_suffix('/') {
            writer
                .push_archive_entry::<&[u8]>(ArchiveEntry::new_directory(folder), None)
                .unwrap();
        } else {
            writer
                .push_archive_entry(ArchiveEntry::new_file(name), Some(*data))
                .unwrap();
        }
    }
    writer.finish().unwrap().into_inner()
}

fn rar(build: impl FnOnce(&mut rars::Builder)) -> Vec<u8> {
    let mut builder = rars::Builder::new(rars::ArchiveVersion::Rar50);
    build(&mut builder);
    builder.to_bytes().unwrap()
}

fn limits() -> Limits {
    Limits {
        budget: TraversalBudget::default(),
        memory: 1 << 34,
        space: Some(space::fixed(1 << 30)),
        cancel: None,
    }
}

struct Run {
    _folder: tempfile::TempDir,
    work: PathBuf,
    result: Result<(), Stop>,
}

fn unpack_with(bytes: &[u8], limits: &Limits) -> Run {
    let folder = tempfile::tempdir().unwrap();
    let archive = folder.path().join("download");
    fs::write(&archive, bytes).unwrap();
    let work = folder.path().join("work");
    fs::create_dir(&work).unwrap();
    let result = extract(&archive, &work, limits);
    Run {
        _folder: folder,
        work,
        result,
    }
}
fn unpack(bytes: &[u8]) -> Run {
    unpack_with(bytes, &limits())
}

fn tree(root: &Path) -> Vec<(String, u32)> {
    let mut found = Vec::new();
    fn walk(root: &Path, path: &Path, found: &mut Vec<(String, u32)>) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let metadata = fs::symlink_metadata(entry.path()).unwrap();
            let relative = entry
                .path()
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            found.push((relative, metadata.permissions().mode() & 0o7777));
            if metadata.is_dir() {
                walk(root, &entry.path(), found);
            }
        }
    }
    walk(root, root, &mut found);
    found.sort();
    found
}

/// `why` is part of the diagnostic, so a test cannot pass for another reason.
#[track_caller]
fn refused(run: Run, failure: Failure, why: &str) {
    let stop = run.result.expect_err("the archive must be refused");
    assert_eq!(stop.failure, failure, "{}", stop.reason);
    assert!(
        stop.reason.contains(why),
        "{why:?} not in {:?}",
        stop.reason
    );
    assert!(
        tree(&run.work).is_empty(),
        "nothing is written before a refusal"
    );
}

#[test]
fn magic_bytes_decide_the_format_and_names_decide_only_the_early_check() {
    assert_eq!(format(b"PK\x03\x04rest"), Some(Format::Zip));
    assert_eq!(format(b"PK\x05\x06"), Some(Format::Zip));
    assert_eq!(format(b"PK\x07\x08"), Some(Format::Zip));
    assert_eq!(format(b"7z\xbc\xaf\x27\x1c\x00\x04"), Some(Format::SevenZ));
    assert_eq!(format(b"Rar!\x1a\x07\x01\x00"), Some(Format::Rar));
    assert_eq!(format(b"Rar!\x1a\x07\x00"), Some(Format::Rar));
    for other in [&b"PK"[..], b"\x1f\x8b", b"<html>", b"", b"\x00\x01 rom"] {
        assert_eq!(format(other), None);
    }
    assert!(
        archive_name("Sonic the Hedgehog.ZIP") && archive_name("a.7z") && archive_name("b.rar")
    );
    assert!(!archive_name("game.gba") && !archive_name("zip") && !archive_name("a.tar.gz"));
    assert_eq!(
        mem_available("MemTotal: 1 kB\nMemAvailable:     2048 kB\n").unwrap(),
        2 * 1024 * 1024
    );
    assert!(mem_available("MemTotal: 1 kB\n").is_err());
}

#[test]
fn unsafe_names_are_refused_by_segment() {
    for name in [
        "",
        "/etc/passwd",
        "../x.gba",
        "a/../x.gba",
        "./x.gba",
        "a//x.gba",
        "a\\x.gba",
        "C:/x.gba",
        "c:x.gba",
        "a/\u{7}.gba",
        "a\0b",
        "a/\n.gba",
        "x.gba/",
    ] {
        assert!(segments(name, false, false).is_err(), "{name:?}");
    }
    assert_eq!(
        segments("a/b/Game (USA).gba", false, false).unwrap(),
        ["a", "b", "Game (USA).gba"]
    );
    assert_eq!(segments("folder/", true, false).unwrap(), ["folder"]);
    // RAR 1.3 to 4.x headers use `\` as the separator; `..` stays refused.
    assert_eq!(segments("a\\b.bin", false, true).unwrap(), ["a", "b.bin"]);
    assert!(segments("..\\b.bin", false, true).is_err());
}

#[test]
fn each_format_unpacks_files_and_folders_with_0644_and_0755() {
    let zip = deflated_zip(&[
        ("Disc/", b""),
        ("Disc/Game.cue", b"FILE"),
        ("Disc/Game.bin", ROM),
        ("readme.txt", b"hi"),
    ]);
    let seven = seven_z(
        &[
            ("Disc/", b""),
            ("Disc/Game.cue", b"FILE"),
            ("Disc/Game.bin", ROM),
            ("readme.txt", b"hi"),
        ],
        |_| {},
    );
    let rar = rar(|builder| {
        builder.add_directory(b"Disc".to_vec(), None, None).unwrap();
        builder
            .add_bytes(
                b"Disc/Game.cue".to_vec(),
                b"FILE".to_vec(),
                None,
                Some(0o755),
            )
            .unwrap();
        builder
            .add_bytes(b"Disc/Game.bin".to_vec(), ROM.to_vec(), None, None)
            .unwrap();
        builder
            .add_bytes(b"readme.txt".to_vec(), b"hi".to_vec(), None, None)
            .unwrap();
    });
    for (kind, bytes) in [("zip", zip), ("7z", seven), ("rar", rar)] {
        let run = unpack(&bytes);
        run.result
            .as_ref()
            .map_err(|stop| &stop.reason)
            .expect(kind);
        assert_eq!(
            tree(&run.work),
            [
                ("Disc".to_owned(), 0o755),
                ("Disc/Game.bin".to_owned(), 0o644),
                ("Disc/Game.cue".to_owned(), 0o644),
                ("readme.txt".to_owned(), 0o644),
            ],
            "{kind}: no executable bit comes from an archive"
        );
        assert_eq!(
            fs::read(run.work.join("Disc/Game.bin")).unwrap(),
            ROM,
            "{kind}"
        );
    }
    // An empty entry and a stored zip work too.
    let run = unpack(&raw_zip(&[raw("Game.gba", ROM), raw("empty.txt", b"")]));
    assert!(run.result.is_ok());
    assert_eq!(tree(&run.work).len(), 2);
}

#[test]
fn traversal_absolute_and_drive_paths_refuse_the_whole_archive() {
    for (name, why) in [
        ("../escape.gba", "unsafe path segment"),
        ("/tmp/escape.gba", "absolute"),
        ("C:/escape.gba", "drive letter"),
        ("a\\escape.gba", "unsafe path segment"),
        ("a/./b.gba", "unsafe path segment"),
        ("bell\u{7}.gba", "unsafe path segment"),
    ] {
        // The safe entry comes first: nothing at all may be written.
        refused(
            unpack(&raw_zip(&[raw("Game.gba", ROM), raw(name, ROM)])),
            Failure::Unpack,
            why,
        );
        refused(
            unpack(&seven_z(&[("Game.gba", ROM), (name, ROM)], |_| {})),
            Failure::Unpack,
            why,
        );
    }
}

#[test]
fn links_and_special_files_are_refused() {
    for mode in [0o120777, 0o020644, 0o060644, 0o010644, 0o140644] {
        let mut link = raw("link.gba", b"Game.gba");
        link.mode = Some(mode);
        refused(
            unpack(&raw_zip(&[raw("Game.gba", ROM), link])),
            Failure::Unpack,
            "link or special file",
        );
    }
    // A plain Unix file mode is fine; its executable bit is dropped.
    let mut plain = raw("Game.gba", ROM);
    plain.mode = Some(0o100755);
    let run = unpack(&raw_zip(&[plain]));
    assert!(run.result.is_ok());
    assert_eq!(tree(&run.work), [("Game.gba".to_owned(), 0o644)]);

    // 7z: the Unix extension bit carries st_mode; 0x400 is a Windows link.
    for attributes in [0x8000 | (0o120777 << 16), 0x400] {
        let mut writer = sevenz_rust2::ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
        let mut entry = sevenz_rust2::ArchiveEntry::new_file("link.gba");
        entry.has_windows_attributes = true;
        entry.windows_attributes = attributes;
        writer
            .push_archive_entry(entry, Some(&b"Game.gba"[..]))
            .unwrap();
        writer
            .push_archive_entry(sevenz_rust2::ArchiveEntry::new_file("Game.gba"), Some(ROM))
            .unwrap();
        refused(
            unpack(&writer.finish().unwrap().into_inner()),
            Failure::Unpack,
            "link or special file",
        );
    }

    let link = rar(|builder| {
        builder
            .add_bytes(b"Game.gba".to_vec(), ROM.to_vec(), None, None)
            .unwrap();
        builder
            .add_unix_symlink(
                b"link.gba".to_vec(),
                b"../../etc/passwd".to_vec(),
                false,
                None,
                None,
            )
            .unwrap();
    });
    refused(unpack(&link), Failure::Unpack, "link or special file");
}

#[test]
fn encrypted_entries_are_refused() {
    let mut locked = raw("Game.gba", ROM);
    locked.flags = 1;
    refused(
        unpack(&raw_zip(&[locked])),
        Failure::Unpack,
        "encrypted entry",
    );

    use sevenz_rust2::{encoder_options::AesEncoderOptions, EncoderConfiguration, EncoderMethod};
    for header in [true, false] {
        let bytes = seven_z(&[("Game.gba", ROM)], |writer| {
            writer.set_encrypt_header(header);
            writer.set_content_methods(vec![
                EncoderConfiguration::new(EncoderMethod::AES256_SHA256)
                    .with_options(AesEncoderOptions::new("secret".into()).into()),
                EncoderConfiguration::new(EncoderMethod::LZMA2),
            ]);
        });
        refused(
            unpack(&bytes),
            Failure::Unpack,
            if header { "7z: " } else { "encrypted content" },
        );
    }

    let mut builder =
        rars::Builder::new(rars::ArchiveVersion::Rar50).password(Some(b"secret".to_vec()));
    builder
        .add_bytes(b"Game.gba".to_vec(), ROM.to_vec(), None, None)
        .unwrap();
    refused(
        unpack(&builder.to_bytes().unwrap()),
        Failure::Unpack,
        "encrypted entry",
    );
}

#[test]
fn repeated_paths_and_files_used_as_folders_are_refused() {
    refused(
        unpack(&raw_zip(&[raw("Game.gba", ROM), raw("Game.gba", b"other")])),
        Failure::Unpack,
        "repeated",
    );
    refused(
        unpack(&seven_z(
            &[("Game.gba", ROM), ("Game.gba", b"other")],
            |_| {},
        )),
        Failure::Unpack,
        "repeated",
    );
    refused(
        unpack(&seven_z(&[("Game", ROM), ("Game/x.gba", ROM)], |_| {})),
        Failure::Unpack,
        "also a folder",
    );
    refused(
        unpack(&raw_zip(&[raw("Disc/", b""), raw("Disc/", b"")])),
        Failure::Unpack,
        "repeated",
    );
}

/// Review P1-1: the reader walks the on-this-disk count, so a forged total
/// of 1 once hid a second `Game.gba` behind the first, also when the hidden
/// first entry was a symbolic link.
#[test]
fn forged_end_record_counts_cannot_hide_a_repeated_or_linked_entry() {
    let mut link = raw("Game.gba", b"../../etc/passwd");
    link.mode = Some(0o120777);
    for first in [raw("Game.gba", ROM), link] {
        let entries = [first, raw("Game.gba", b"\x00\x02 second")];
        for (disk, on_disk, total) in [(0, 2, 1), (0, 1, 2), (1, 2, 2)] {
            let bytes = raw_zip_counts(&entries, disk, on_disk, total);
            let run = unpack(&bytes);
            let stop = run.result.expect_err("a forged end record must be refused");
            assert_eq!(stop.failure, Failure::Unpack, "{}", stop.reason);
            assert!(tree(&run.work).is_empty(), "{disk} {on_disk} {total}");
        }
    }
    // The zip64 end record is the one that counts when present.
    for (on_disk, total, ok) in [(2u64, 2u64, true), (2, 1, false), (1, 2, false)] {
        let bytes = zip64(
            &raw_zip(&[raw("A.gba", ROM), raw("B.gba", b"\x00\x02")]),
            on_disk,
            total,
        );
        let run = unpack(&bytes);
        assert_eq!(run.result.is_ok(), ok, "zip64 {on_disk} {total}");
    }
    // Honest counts with distinct names still unpack.
    let honest = raw_zip_counts(&[raw("A.gba", ROM), raw("B.gba", ROM)], 0, 2, 2);
    assert!(unpack(&honest).result.is_ok());
}

#[test]
fn an_entry_longer_than_declared_is_a_corrupt_archive() {
    let mut liar = raw("Game.gba", ROM);
    liar.declared = Some(4);
    let run = unpack(&raw_zip(&[liar]));
    assert_eq!(run.result.unwrap_err().failure, Failure::Unpack);
}

/// A zip bomb: 64 MiB of zeros compress to about 64 KiB. Byte counting stops
/// it at the floor, long before the declared size.
#[test]
fn a_zip_bomb_stops_at_the_free_space_floor() {
    let zeros = vec![0u8; 64 * 1024 * 1024];
    let bomb = deflated_zip(&[("bomb.gba", &zeros)]);
    assert!(bomb.len() < 1024 * 1024);
    let mut limits = limits();
    limits.space = Some(space::fixed(1024 * 1024));
    let run = unpack_with(&bomb, &limits);
    assert_eq!(run.result.as_ref().unwrap_err().failure, Failure::NoSpace);
    let written = fs::metadata(run.work.join("bomb.gba")).unwrap().len();
    assert!(written <= 1024 * 1024, "{written}");
}

#[test]
fn a_dictionary_larger_than_mem_available_is_refused_before_decoding() {
    use sevenz_rust2::{encoder_options::Lzma2Options, EncoderConfiguration, EncoderMethod};
    let bytes = seven_z(&[("Game.gba", ROM)], |writer| {
        let mut options = Lzma2Options::from_level(1);
        options.set_dictionary_size(16 * 1024 * 1024);
        writer.set_content_methods(vec![
            EncoderConfiguration::new(EncoderMethod::LZMA2).with_options(options.into())
        ]);
    });
    let mut small = limits();
    small.memory = 1024 * 1024;
    refused(
        unpack_with(&bytes, &small),
        Failure::Unpack,
        "exceeds MemAvailable",
    );
    assert!(
        unpack(&bytes).result.is_ok(),
        "the same archive fits a larger device"
    );

    let compressed = rar(|builder| {
        builder
            .add_bytes(b"Game.gba".to_vec(), ROM.repeat(64), None, None)
            .unwrap();
    });
    let mut tiny = limits();
    tiny.memory = 1024;
    refused(
        unpack_with(&compressed, &tiny),
        Failure::Unpack,
        "exceeds MemAvailable",
    );
    assert!(unpack(&compressed).result.is_ok());
}

#[test]
fn archives_above_the_traversal_budget_are_refused() {
    let mut budget = limits();
    budget.budget.max_entries = 2;
    refused(
        unpack_with(
            &raw_zip(&[raw("a.gba", ROM), raw("b.gba", ROM), raw("c.gba", ROM)]),
            &budget,
        ),
        Failure::Unpack,
        "traversal budget",
    );
    let mut depth = limits();
    depth.budget.max_depth = 2;
    refused(
        unpack_with(&raw_zip(&[raw("a/b/c/d.gba", ROM)]), &depth),
        Failure::Unpack,
        "traversal budget",
    );
    assert!(unpack_with(&raw_zip(&[raw("a/b/d.gba", ROM)]), &depth)
        .result
        .is_ok());
}

#[test]
fn split_multi_volume_and_unreadable_files_fail_as_unpack() {
    let mut builder = rars::Builder::new(rars::ArchiveVersion::Rar50).volume_size(Some(512));
    let mut state = 0x1234_5678u32;
    let noise: Vec<u8> = (0..16 * 1024)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect();
    builder
        .add_bytes(b"Game.gba".to_vec(), noise, None, None)
        .unwrap();
    let volumes = builder.build_volumes(None).unwrap();
    assert!(volumes.len() > 1);
    refused(unpack(&volumes[0]), Failure::Unpack, "multi-volume");
    let seven = seven_z(&[("Game.gba", &[7u8; 4096])], |_| {});
    refused(unpack(&seven[..seven.len() / 2]), Failure::Unpack, "7z: ");
    refused(
        unpack(b"Rar!\x1a\x07\x01\x00 truncated"),
        Failure::Unpack,
        "rar: ",
    );
    refused(unpack(b"PK\x03\x04 truncated"), Failure::Unpack, "zip");
}

#[test]
fn cancel_stops_the_reader_between_writes() {
    let mut limits = limits();
    let cancel = Arc::new(AtomicBool::new(true));
    limits.cancel = Some(cancel);
    let run = unpack_with(&raw_zip(&[raw("Game.gba", ROM)]), &limits);
    assert_eq!(run.result.unwrap_err().failure, Failure::Cancelled);
}

/// Review P1-2: `Archive::read` decodes a compressed 7z header with LZMA
/// readers that take no memory limit, before korrid sees any coder. korrid
/// reads the encoded header's coders first and refuses a dictionary above
/// `MemAvailable`. Any decoder that still overreaches is killed by the
/// unit's `MemoryMax=`; that cap is exercised only under systemd
/// (`unit::tests::a_store_job_unit_is_named_from_its_provider_cache_path`
/// checks the property).
#[test]
fn a_compressed_7z_header_with_a_large_dictionary_is_refused_before_it_decodes() {
    let names: Vec<String> = (0..400)
        .map(|i| format!("folder/Game file {i:04}.gba"))
        .collect();
    let entries: Vec<(&str, &[u8])> = names.iter().map(|name| (name.as_str(), &b""[..])).collect();
    let bytes = seven_z(&entries, |_| {});
    // Precondition: the writer compressed the header (K_ENCODED_HEADER).
    let next = 32 + u64::from_le_bytes(bytes[12..20].try_into().unwrap()) as usize;
    assert_eq!(
        bytes[next], 0x17,
        "the header must be encoded for this test"
    );
    let mut small = limits();
    small.memory = 64 * 1024;
    refused(
        unpack_with(&bytes, &small),
        Failure::Unpack,
        "encoded header",
    );
    assert!(unpack(&bytes).result.is_ok(), "a larger device reads it");
}

/// How the reader unit's `RuntimeMaxSec=` was chosen (review P1-3): the
/// reader's throughput per format on 64 MiB of half-compressible data, in a
/// release build. Run with
/// `cargo test --release --lib acquisition::archive::tests::reader_throughput -- --ignored --nocapture`.
#[test]
#[ignore = "measurement, not a check"]
fn reader_throughput() {
    let mut state = 0x9e37_79b9u32;
    let data: Vec<u8> = (0..64 * 1024 * 1024)
        .map(|i: usize| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            // Half the bytes are noise, half a repeating pattern, like a ROM.
            if i.is_multiple_of(2) {
                state as u8
            } else {
                (i / 64) as u8
            }
        })
        .collect();
    let archives = [
        ("zip deflate", deflated_zip(&[("Game.iso", &data)])),
        ("7z lzma2", seven_z(&[("Game.iso", &data)], |_| {})),
        (
            "rar5",
            rar(|builder| {
                builder
                    .add_bytes(b"Game.iso".to_vec(), data.clone(), None, None)
                    .unwrap()
            }),
        ),
    ];
    for (kind, bytes) in archives {
        let started = std::time::Instant::now();
        let run = unpack(&bytes);
        run.result.as_ref().map_err(|stop| &stop.reason).unwrap();
        let seconds = started.elapsed().as_secs_f64();
        println!(
            "{kind}: {:.1} MiB/s ({seconds:.2} s for 64 MiB)",
            64.0 / seconds
        );
    }
}

/// Review P2-1: folders and empty files take file system blocks too. Each
/// entry is counted as one block (4096 bytes in the fixed probe), so an
/// archive of many empty entries stops at the floor.
#[test]
fn many_empty_entries_near_the_floor_fail_as_no_space() {
    let names: Vec<String> = (0..100).map(|i| format!("empty {i:03}.txt")).collect();
    let entries: Vec<Raw<'_>> = names.iter().map(|name| raw(name, b"")).collect();
    let mut near = limits();
    near.space = Some(space::fixed(64 * 1024));
    let run = unpack_with(&raw_zip(&entries), &near);
    assert_eq!(run.result.unwrap_err().failure, Failure::NoSpace);

    let folders: Vec<String> = (0..40).map(|i| format!("folder {i:03}/")).collect();
    let entries: Vec<Raw<'_>> = folders.iter().map(|name| raw(name, b"")).collect();
    let run = unpack_with(&raw_zip(&entries), &near);
    assert_eq!(run.result.unwrap_err().failure, Failure::NoSpace);
    // Ten empty entries fit.
    let run = unpack_with(&raw_zip(&entries[..10]), &near);
    assert!(run.result.is_ok());
}

/// Review P2 (round 2): `korrid unpack-archive` writes `no-space` next to the
/// work folder before it exits 1 at the floor, and nothing for another
/// failure. The marker is outside the work folder, so no archive entry can
/// forge it.
#[test]
fn the_reader_command_marks_no_space_next_to_the_work_folder() {
    let names: Vec<String> = (0..100).map(|i| format!("empty {i:03}.txt")).collect();
    let entries: Vec<Raw<'_>> = names.iter().map(|name| raw(name, b"")).collect();
    let mut near = limits();
    near.space = Some(space::fixed(65_536));
    for (bytes, marker) in [
        (raw_zip(&entries), Some("no-space\n")),
        (raw_zip(&[raw("../escape.gba", ROM)]), None),
    ] {
        let folder = tempfile::tempdir().unwrap();
        let archive = folder.path().join("download");
        fs::write(&archive, &bytes).unwrap();
        let work = folder.path().join(".tmp-0123");
        fs::create_dir(&work).unwrap();
        assert!(run_with(&work, &archive, &near).is_err());
        let written = fs::read_to_string(result_marker(&work)).ok();
        assert_eq!(written.as_deref(), marker);
    }
}
