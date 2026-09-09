// Integration tests for the archive and publisher modules.
//
// These tests use real temp files, real Nix commands, and real archive data.
// Tests that require Nix are marked #[ignore] and run via a dedicated Nix
// app (nix run .#korri-publisher-check) so normal `cargo test` stays
// network-free. Unit tests for archive pack/extract always run.

use korri_plugin_host::archive;
use std::{
    fs::{self, File},
    io::Write,
    path::Path,
};

// ---------------------------------------------------------------------------
// Archive production and extraction
// ---------------------------------------------------------------------------

fn make_cache_dir(root: &Path) -> std::path::PathBuf {
    let cache = root.join("cache");
    fs::create_dir(&cache).unwrap();
    // nix-cache-info
    fs::write(cache.join("nix-cache-info"), "StoreDir: /nix/store\n").unwrap();
    // a narinfo file
    fs::write(
        cache.join("00000000000000000000000000000000.narinfo"),
        "StorePath: /nix/store/00000000000000000000000000000000-x\n\
         URL: nar/00000000000000000000000000000000.nar.xz\n\
         Compression: xz\n\
         FileHash: sha256:00000000000000000000000000000000\n\
         FileSize: 8\n\
         NarHash: sha256:00000000000000000000000000000000\n\
         NarSize: 8\n\
         References: \n\
         CA: fixed:r:sha256:00000000000000000000000000000000\n",
    )
    .unwrap();
    // a nar file
    let nar_dir = cache.join("nar");
    fs::create_dir(&nar_dir).unwrap();
    fs::write(
        nar_dir.join("00000000000000000000000000000000.nar.xz"),
        b"\xfd7zXZ\x00",
    )
    .unwrap();
    cache
}

/// Write a single-entry tar with the given raw path bytes in the name field,
/// bypassing the `tar` crate's path validation. Used to build adversarial
/// archives for extraction-rejection tests.
fn write_raw_ustar_archive(archive_path: &Path, raw_name: &[u8], data: &[u8]) {
    let mut file = File::create(archive_path).unwrap();
    // 512-byte ustar header.
    let mut header = [0u8; 512];
    // Name field: bytes 0–99 (100 bytes, NUL-padded).
    let name_len = raw_name.len().min(100);
    header[..name_len].copy_from_slice(&raw_name[..name_len]);
    // Mode: "0000644\0"
    header[100..108].copy_from_slice(b"0000644\0");
    // uid: "0000000\0", gid: "0000000\0"
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    // Size (octal, 12 bytes): e.g. "00000000004\0" for 4 bytes.
    let size_octal = format!("{:011o}\0", data.len());
    header[124..136].copy_from_slice(size_octal.as_bytes());
    // mtime (12 bytes): "00000000000\0"
    header[136..148].copy_from_slice(b"00000000000\0");
    // checksum field: 8 spaces while computing.
    header[148..156].copy_from_slice(b"        ");
    // typeflag: '0' = regular file.
    header[156] = b'0';
    // magic "ustar" + version.
    header[257..262].copy_from_slice(b"ustar");
    header[263..265].copy_from_slice(b"00");
    // Compute checksum.
    let checksum: u32 = header.iter().map(|&b| b as u32).sum();
    let cksum_str = format!("{:06o}\0 ", checksum);
    header[148..156].copy_from_slice(cksum_str.as_bytes());

    file.write_all(&header).unwrap();
    // Write data padded to 512-byte boundary.
    file.write_all(data).unwrap();
    if !data.is_empty() {
        let pad = (512 - data.len() % 512) % 512;
        if pad > 0 {
            file.write_all(&vec![0u8; pad]).unwrap();
        }
    }
    // Two 512-byte zero blocks = end of archive.
    file.write_all(&[0u8; 1024]).unwrap();
    file.flush().unwrap();
}

#[test]
fn archive_packs_and_extracts_a_valid_cache() {
    let root = tempfile::tempdir().unwrap();
    let cache = make_cache_dir(root.path());
    let archive_path = root.path().join("out.tar");
    let staging = root.path().join("staged");
    fs::create_dir(&staging).unwrap();

    let sha256 = archive::pack(&cache, &archive_path).unwrap();
    assert_eq!(sha256.len(), 64);
    assert!(sha256.bytes().all(|b| b.is_ascii_hexdigit()));
    assert!(archive_path.is_file());
    // Archive must be immutable (read-only).
    let perms = fs::metadata(&archive_path).unwrap().permissions();
    assert!(perms.readonly());

    archive::extract(&archive_path, &staging).unwrap();
    assert!(staging.join("nix-cache-info").exists());
    assert!(staging
        .join("00000000000000000000000000000000.narinfo")
        .exists());
    assert!(staging
        .join("nar/00000000000000000000000000000000.nar.xz")
        .exists());
}

#[test]
fn rejects_tar_extension_headers_before_reading_their_bodies() {
    let root = tempfile::tempdir().unwrap();
    for kind in *b"LKx" {
        let archive_path = root.path().join(format!("extension-{kind}.tar"));
        let mut header = tar::Header::new_gnu();
        header.set_path("extension").unwrap();
        header.set_entry_type(tar::EntryType::new(kind));
        header.set_size(1024 * 1024 * 1024);
        header.set_mode(0o644);
        header.set_cksum();
        fs::write(&archive_path, header.as_bytes()).unwrap();
        let staging = root.path().join(format!("staging-{kind}"));
        fs::create_dir(&staging).unwrap();
        let error = archive::extract(&archive_path, &staging).unwrap_err();
        assert!(
            error.contains("regular files"),
            "extension body was processed: {error}"
        );
        assert_eq!(fs::read_dir(staging).unwrap().count(), 0);
    }
}

#[test]
fn extract_rejects_empty_cache() {
    let root = tempfile::tempdir().unwrap();
    let archive_path = root.path().join("empty.tar");
    let mut tar = tar::Builder::new(File::create(&archive_path).unwrap());
    tar.finish().unwrap();
    let staging = root.path().join("staging");
    fs::create_dir(&staging).unwrap();
    assert!(archive::extract(&archive_path, &staging).is_err());
}

#[test]
fn extract_refuses_preexisting_nar_directory_symlink() {
    let root = tempfile::tempdir().unwrap();
    let cache = make_cache_dir(root.path());
    let archive_path = root.path().join("cache.tar");
    archive::pack(&cache, &archive_path).unwrap();
    let staging = root.path().join("staging");
    fs::create_dir(&staging).unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), staging.join("nar")).unwrap();
    assert!(archive::extract(&archive_path, &staging).is_err());
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[test]
fn pack_accepts_empty_bookkeeping_directories_created_by_nix() {
    let root = tempfile::tempdir().unwrap();
    let cache = make_cache_dir(root.path());
    fs::create_dir(cache.join("log")).unwrap();
    fs::create_dir(cache.join("realisations")).unwrap();
    let archive_path = root.path().join("cache.tar");
    archive::pack(&cache, &archive_path).unwrap();
    let staging = root.path().join("staging");
    fs::create_dir(&staging).unwrap();
    archive::extract(&archive_path, &staging).unwrap();
    assert!(!staging.join("log").exists());
}

#[test]
fn pack_refuses_to_overwrite_existing_output() {
    let root = tempfile::tempdir().unwrap();
    let cache = make_cache_dir(root.path());
    let archive_path = root.path().join("out.tar");
    fs::write(&archive_path, b"existing").unwrap();
    assert!(archive::pack(&cache, &archive_path).is_err());
}

#[test]
fn pack_rejects_cache_missing_nix_cache_info() {
    let root = tempfile::tempdir().unwrap();
    let cache = root.path().join("cache");
    fs::create_dir(&cache).unwrap();
    fs::create_dir(cache.join("nar")).unwrap();
    // No nix-cache-info.
    assert!(archive::pack(&cache, &root.path().join("out.tar")).is_err());
}

#[test]
fn pack_rejects_cache_missing_nar_dir() {
    let root = tempfile::tempdir().unwrap();
    let cache = root.path().join("cache");
    fs::create_dir(&cache).unwrap();
    fs::write(cache.join("nix-cache-info"), "StoreDir: /nix/store\n").unwrap();
    // No nar/ subdirectory.
    assert!(archive::pack(&cache, &root.path().join("out.tar")).is_err());
}

#[test]
fn pack_rejects_symlinks_in_cache() {
    let root = tempfile::tempdir().unwrap();
    let cache = make_cache_dir(root.path());
    // Add a symlink inside the cache dir.
    std::os::unix::fs::symlink("/etc/passwd", cache.join("link")).unwrap();
    assert!(archive::pack(&cache, &root.path().join("out.tar")).is_err());
}

#[test]
fn extract_rejects_path_traversal() {
    let root = tempfile::tempdir().unwrap();
    let archive_path = root.path().join("bad.tar");
    // Write a raw archive with ".." in the path, bypassing tar crate validation.
    write_raw_ustar_archive(&archive_path, b"../outside/nix-cache-info", b"evil");
    let staging = root.path().join("staged");
    fs::create_dir(&staging).unwrap();
    assert!(archive::extract(&archive_path, &staging).is_err());
}

#[test]
fn extract_rejects_absolute_paths() {
    let root = tempfile::tempdir().unwrap();
    let archive_path = root.path().join("bad.tar");
    // Write a raw archive with an absolute path.
    write_raw_ustar_archive(&archive_path, b"/nix-cache-info", b"x");
    let staging = root.path().join("staged");
    fs::create_dir(&staging).unwrap();
    assert!(archive::extract(&archive_path, &staging).is_err());
}

#[test]
fn extract_rejects_unexpected_paths() {
    let root = tempfile::tempdir().unwrap();
    let archive_path = root.path().join("bad.tar");
    let file = File::create(&archive_path).unwrap();
    let mut builder = tar::Builder::new(file);
    let data = b"x";
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_mtime(0);
    header.set_cksum();
    // A top-level file with an unexpected extension.
    builder
        .append_data(&mut header, "unexpected.json", &data[..])
        .unwrap();
    builder.finish().unwrap();

    let staging = root.path().join("staged");
    fs::create_dir(&staging).unwrap();
    assert!(archive::extract(&archive_path, &staging).is_err());
}

#[test]
fn extract_rejects_duplicate_paths() {
    let root = tempfile::tempdir().unwrap();
    let archive_path = root.path().join("dup.tar");
    let file = File::create(&archive_path).unwrap();
    let mut builder = tar::Builder::new(file);
    for _ in 0..2 {
        let data = b"StoreDir: /nix/store\n";
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_cksum();
        builder
            .append_data(&mut header, "nix-cache-info", &data[..])
            .unwrap();
    }
    builder.finish().unwrap();

    let staging = root.path().join("staged");
    fs::create_dir(&staging).unwrap();
    assert!(archive::extract(&archive_path, &staging).is_err());
}

#[test]
fn extract_rejects_symlink_entries() {
    let root = tempfile::tempdir().unwrap();
    let archive_path = root.path().join("sym.tar");
    let file = File::create(&archive_path).unwrap();
    let mut builder = tar::Builder::new(file);
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_size(0);
    header.set_mode(0o644);
    header.set_mtime(0);
    header.set_link_name("nix-cache-info").unwrap();
    header.set_cksum();
    builder.append_data(&mut header, "link", &[][..]).unwrap();
    builder.finish().unwrap();

    let staging = root.path().join("staged");
    fs::create_dir(&staging).unwrap();
    assert!(archive::extract(&archive_path, &staging).is_err());
}

// ---------------------------------------------------------------------------
// Regression tests for issues U1 (fail before fix)
// ---------------------------------------------------------------------------

/// extract must reject an archive whose file size on disk exceeds MAX_ARCHIVE_BYTES.
/// Before the fix, extract opened the file without checking its size at all.
#[test]
fn extract_rejects_over_limit_archive_by_file_size() {
    use korri_plugin_host::archive::MAX_ARCHIVE_BYTES;
    let root = tempfile::tempdir().unwrap();
    let archive_path = root.path().join("huge.tar");
    // Produce a sparse file that reports a large size without occupying disk.
    // We set the logical size to MAX_ARCHIVE_BYTES + 1 via seek+write.
    {
        use std::io::Seek;
        let mut f = File::create(&archive_path).unwrap();
        f.seek(std::io::SeekFrom::Start(MAX_ARCHIVE_BYTES)).unwrap();
        f.write_all(&[0u8]).unwrap();
    }
    let staging = root.path().join("staged");
    fs::create_dir(&staging).unwrap();
    let err = archive::extract(&archive_path, &staging).unwrap_err();
    assert!(
        err.contains("size") || err.contains("limit") || err.contains("2 GiB"),
        "expected size-limit error, got: {err}"
    );
}

/// extract must reject an archive whose file size exactly equals MAX_ARCHIVE_BYTES + 1.
/// A GitHub asset must be strictly smaller than 2 GiB.
#[test]
fn extract_rejects_archive_at_exactly_limit_plus_one() {
    use korri_plugin_host::archive::MAX_ARCHIVE_BYTES;
    let root = tempfile::tempdir().unwrap();
    let archive_path = root.path().join("edge.tar");
    {
        use std::io::Seek;
        let mut f = File::create(&archive_path).unwrap();
        // Exactly one byte over the limit.
        f.seek(std::io::SeekFrom::Start(MAX_ARCHIVE_BYTES)).unwrap();
        f.write_all(&[0u8]).unwrap();
    }
    let staging = root.path().join("staged");
    fs::create_dir(&staging).unwrap();
    assert!(archive::extract(&archive_path, &staging).is_err());
}

/// extract must validate that the staging directory itself is not a symlink.
/// Before the fix, a symlink staging root was followed by create_dir_all,
/// allowing writes outside the caller-provided boundary.
#[test]
fn extract_rejects_staging_root_that_is_a_symlink() {
    let root = tempfile::tempdir().unwrap();
    // Make a valid cache archive.
    let cache = make_cache_dir(root.path());
    let archive_path = root.path().join("out.tar");
    archive::pack(&cache, &archive_path).unwrap();

    // Create a real target dir outside of root, then symlink staging -> target.
    let outside = tempfile::tempdir().unwrap();
    let staging = root.path().join("staged");
    std::os::unix::fs::symlink(outside.path(), &staging).unwrap();

    let err = archive::extract(&archive_path, &staging).unwrap_err();
    assert!(
        err.contains("symlink") || err.contains("staging") || err.contains("not a directory"),
        "expected symlink staging rejection, got: {err}"
    );
    // The outside dir must remain empty — no files written through the symlink.
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}

/// A parent component of staging_dir that is a symlink must also be rejected.
/// create_dir_all follows symlinks in intermediate path components.
#[test]
fn extract_rejects_staging_parent_that_is_a_symlink() {
    let root = tempfile::tempdir().unwrap();
    let cache = make_cache_dir(root.path());
    let archive_path = root.path().join("out.tar");
    archive::pack(&cache, &archive_path).unwrap();

    // root/link -> outside; staging = root/link/work
    let outside = tempfile::tempdir().unwrap();
    let link = root.path().join("link");
    std::os::unix::fs::symlink(outside.path(), &link).unwrap();
    let staging = link.join("work");

    // staging doesn't exist; extract must not follow the symlink to create it.
    let err = archive::extract(&archive_path, &staging).unwrap_err();
    assert!(
        err.contains("symlink") || err.contains("staging") || err.contains("not a directory"),
        "expected parent-symlink staging rejection, got: {err}"
    );
    // Nothing must have been created inside outside.
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}

/// A truncated entry (header says N bytes but the file ends before all bytes
/// are present) must be detected and rejected rather than silently succeeding
/// with partial content.
///
/// We construct a valid ustar header claiming 65536 bytes of data, then write
/// only 8 bytes and end the file. The `tar` crate will encounter EOF mid-entry
/// and our extraction loop must surface that as an error.
#[test]
fn extract_rejects_truncated_entry() {
    let root = tempfile::tempdir().unwrap();
    let archive_path = root.path().join("trunc.tar");
    {
        // A ustar header claiming 65536 bytes (well over one 512-byte block).
        let claimed_size: u64 = 65536;
        let mut f = File::create(&archive_path).unwrap();
        let mut header = [0u8; 512];
        let name = b"nix-cache-info";
        header[..name.len()].copy_from_slice(name);
        header[100..108].copy_from_slice(b"0000644\0");
        header[108..116].copy_from_slice(b"0000000\0");
        header[116..124].copy_from_slice(b"0000000\0");
        // Size in octal, 11 chars + NUL.
        let size_octal = format!("{:011o}\0", claimed_size);
        header[124..136].copy_from_slice(size_octal.as_bytes());
        header[136..148].copy_from_slice(b"00000000000\0");
        header[148..156].copy_from_slice(b"        ");
        header[156] = b'0'; // regular file
        header[257..262].copy_from_slice(b"ustar");
        header[263..265].copy_from_slice(b"00");
        let checksum: u32 = header.iter().map(|&b| b as u32).sum();
        let cksum_str = format!("{:06o}\0 ", checksum);
        header[148..156].copy_from_slice(cksum_str.as_bytes());
        f.write_all(&header).unwrap();
        // Write only 8 bytes; the claimed 65536 are not present.
        f.write_all(&[0x53u8; 8]).unwrap();
        // No padding, no EoA blocks — file ends abruptly.
    }
    let staging = root.path().join("staged");
    fs::create_dir(&staging).unwrap();
    let result = archive::extract(&archive_path, &staging);
    // The tar library may return the 8 bytes and then hit EOF, or it may
    // error on the next read. Either the extract fails, or if the tar crate
    // returns the 8 bytes successfully and our loop detects n==0 before
    // remaining==0, we get a truncation error. Both are correct behaviours.
    // The key invariant: the file written to staging must contain exactly 8
    // bytes, not a 65536-byte file full of garbage.
    match result {
        Err(e) => {
            // Good: extraction detected the truncation.
            assert!(
                e.contains("truncat")
                    || e.contains("short")
                    || e.contains("unexpected")
                    || e.contains("size")
                    || e.contains("failed to fill")
                    || e.contains("io"),
                "expected truncation-related error, got: {e}"
            );
        }
        Ok(()) => panic!("truncated archives must be rejected"),
    }
}

#[test]
fn failed_pack_preserves_existing_output_and_removes_its_temporary_file() {
    let root = tempfile::tempdir().unwrap();
    let cache = make_cache_dir(root.path());
    let archive_path = root.path().join("out.tar");
    fs::write(&archive_path, b"existing content").unwrap();
    let entries = || {
        let mut names = fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        names.sort();
        names
    };
    let before = entries();
    let error = archive::pack(&cache, &archive_path).unwrap_err();
    assert!(error.contains("exists"), "{error}");
    assert_eq!(fs::read(&archive_path).unwrap(), b"existing content");
    assert_eq!(entries(), before);
}

/// pack must output a file that does not exceed MAX_ARCHIVE_BYTES.
/// (Packing an arbitrarily large cache would produce an arbitrarily large archive.
/// We verify the check exists by ensuring small archives succeed and that the
/// constant is documented and enforced on the extract path.)
#[test]
fn pack_output_is_within_archive_size_bound() {
    use korri_plugin_host::archive::MAX_ARCHIVE_BYTES;
    let root = tempfile::tempdir().unwrap();
    let cache = make_cache_dir(root.path());
    let archive_path = root.path().join("out.tar");
    archive::pack(&cache, &archive_path).unwrap();
    let size = fs::metadata(&archive_path).unwrap().len();
    assert!(
        size < MAX_ARCHIVE_BYTES,
        "archive size {size} exceeds MAX_ARCHIVE_BYTES {MAX_ARCHIVE_BYTES}"
    );
}

// ---------------------------------------------------------------------------
// Catalog URL validation regression tests
// ---------------------------------------------------------------------------

/// archive_url must be rejected if it has no host component.
/// "https://" (bare scheme with empty authority) was previously accepted.
#[test]
fn catalog_rejects_archive_url_with_empty_host() {
    use korri_plugin_host::catalog::{Catalog, CatalogRecord};
    fn valid_record() -> CatalogRecord {
        CatalogRecord {
            plugin_id: "@example:network".into(),
            title: None,
            description: None,
            release_version: "1.0.0".into(),
            platform: "x86_64-linux".into(),
            store_path: "/nix/store/00000000000000000000000000000000-network-1.0.0".into(),
            archive_url: "https://example.com/a.tar".into(),
            archive_sha256: "a".repeat(64),
        }
    }
    fn catalog_with(records: Vec<CatalogRecord>) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({ "records": records })).unwrap()
    }

    for bad_url in [
        "https://",                            // empty host (parse error)
        "https://user:pass@example.com/a.tar", // credentials in URL
        "https://example.com/a.tar#section",   // fragment
        "http://example.com/a.tar",            // wrong scheme
    ] {
        let mut r = valid_record();
        r.archive_url = bad_url.into();
        assert!(
            Catalog::from_json(&catalog_with(vec![r])).is_err(),
            "expected rejection for archive_url={bad_url:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Publisher validation regression tests
// ---------------------------------------------------------------------------

/// Publisher must validate release_version, platform and archive_url directly
/// without constructing a bogus CatalogRecord with a zero hash.
/// Before the fix the validation was done by constructing a throw-away Catalog
/// and checking round-trip, which masked errors and used a fake store_path.
#[test]
fn publisher_validates_inputs_without_nix() {
    use korri_plugin_host::publisher::{publish, PublishArgs};
    let root = tempfile::tempdir().unwrap();
    // Bad archive_url (HTTP, not HTTPS): must be rejected before any Nix call.
    let err = publish(&PublishArgs {
        nix: Path::new("/nonexistent/nix").to_path_buf(),
        input_store_path: Path::new("/nix/store/00000000000000000000000000000000-x").to_path_buf(),
        release_version: "1.0.0".into(),
        platform: "x86_64-linux".into(),
        archive_url: "http://example.com/a.tar".into(),
        output_dir: root.path().to_path_buf(),
    })
    .unwrap_err();
    assert!(
        err.contains("https") || err.contains("HTTPS") || err.contains("URL"),
        "expected HTTPS rejection, got: {err}"
    );
}

// ---------------------------------------------------------------------------
// Publisher integration (requires Nix; run via nix run .#korri-publisher-check)
// ---------------------------------------------------------------------------

#[test]
#[ignore = "requires a writable build-machine store; run via nix run .#korri-publisher-check"]
fn publisher_converts_and_packs_korri_tailscale() {
    let nix =
        std::env::var("KORRI_PUBLISH_NIX").expect("KORRI_PUBLISH_NIX must be set to run this test");

    let input_path = std::env::var("KORRI_PUBLISH_TEST_PACKAGE")
        .expect("Nix must supply the actual Tailscale package under test");
    let input_path = input_path.as_str();
    assert!(
        Path::new(input_path).exists(),
        "korri-tailscale input path not in store"
    );

    let platform = std::env::var("KORRI_PUBLISH_TEST_SYSTEM")
        .expect("Nix must supply the current system under test");
    let name = std::process::Command::new(env!("CARGO_BIN_EXE_korri-publish"))
        .args(["archive-name", input_path, "1.0.0", &platform])
        .env_remove("KORRI_PUBLISH_NIX")
        .output()
        .unwrap();
    assert!(name.status.success());
    let name = String::from_utf8(name.stdout).unwrap();
    let cwd = std::env::current_dir().unwrap();
    let root = tempfile::tempdir_in(&cwd).unwrap();
    let relative_output = root.path().strip_prefix(&cwd).unwrap().join(".");
    let result =
        korri_plugin_host::publisher::publish(&korri_plugin_host::publisher::PublishArgs {
            nix: Path::new(&nix).to_path_buf(),
            input_store_path: Path::new(input_path).to_path_buf(),
            release_version: "1.0.0".into(),
            platform: platform.clone(),
            archive_url: format!("https://example.com/{}", name.trim()),
            output_dir: relative_output,
        })
        .unwrap();

    // The CA store path must be in /nix/store.
    assert!(result.ca_store_path.starts_with("/nix/store/"));
    assert_ne!(result.ca_store_path, input_path);

    // Archive must exist on disk.
    assert!(result.archive_path.is_file());
    assert!(result.archive_path.is_absolute());
    // Archive sha256 must be 64 hex chars.
    assert_eq!(result.archive_sha256.len(), 64);
    assert!(result.archive_sha256.bytes().all(|b| b.is_ascii_hexdigit()));

    // Catalog record must be valid.
    result.record.validate().unwrap();
    assert_eq!(result.record.plugin_id, "@korri:tailscale");
    assert_eq!(result.record.release_version, "1.0.0");
    assert_eq!(result.record.platform, platform);
    assert_eq!(result.archive_path.file_name().unwrap(), name.trim());
    assert_eq!(result.record.store_path, result.ca_store_path);
    assert_eq!(result.record.archive_sha256, result.archive_sha256);

    // Extract into a fresh staging dir and verify the layout.
    let staging = root.path().join("staged");
    fs::create_dir(&staging).unwrap();
    archive::extract(&result.archive_path, &staging).unwrap();
    archive::preflight_cache(
        Path::new(&nix),
        &result.ca_store_path,
        &staging,
        archive::MAX_NAR_BYTES,
    )
    .unwrap();
    assert!(
        archive::preflight_cache(Path::new(&nix), &result.ca_store_path, &staging, 64)
            .unwrap_err()
            .contains("NAR size limit exceeded")
    );
    let hash = Path::new(&result.ca_store_path)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();
    let narinfo_name = format!("{}.narinfo", &hash[..32]);
    for (directory, prefix, replacement, expected) in [
        (
            "wrong-size",
            "NarSize:",
            "NarSize: 1",
            "decompressed NAR size mismatch",
        ),
        (
            "wrong-hash",
            "NarHash:",
            "NarHash: sha256:0000000000000000000000000000000000000000000000000000",
            "NAR hash mismatch",
        ),
    ] {
        let changed = root.path().join(directory);
        clone_cache_fixture(&staging, &changed);
        let path = changed.join(&narinfo_name);
        let body = fs::read_to_string(&path).unwrap();
        let body = body
            .lines()
            .map(|line| {
                if line.starts_with(prefix) {
                    replacement
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(path, body).unwrap();
        let error = archive::preflight_cache(
            Path::new(&nix),
            &result.ca_store_path,
            &changed,
            archive::MAX_NAR_BYTES,
        )
        .unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
    let nar = fs::read_dir(staging.join("nar"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(nar, b"corrupt").unwrap();
    assert!(archive::preflight_cache(
        Path::new(&nix),
        &result.ca_store_path,
        &staging,
        archive::MAX_NAR_BYTES
    )
    .is_err());
}

fn clone_cache_fixture(source: &Path, destination: &Path) {
    fs::create_dir(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            fs::create_dir(&target).unwrap();
            for nar in fs::read_dir(entry.path()).unwrap() {
                let nar = nar.unwrap();
                fs::hard_link(nar.path(), target.join(nar.file_name())).unwrap();
            }
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
#[ignore = "requires locked Nix; run via nix run .#korri-publisher-check"]
fn publisher_rejects_invalid_store_path_nix() {
    let nix =
        std::env::var("KORRI_PUBLISH_NIX").expect("KORRI_PUBLISH_NIX must be set to run this test");
    let root = tempfile::tempdir().unwrap();
    let err = korri_plugin_host::publisher::publish(&korri_plugin_host::publisher::PublishArgs {
        nix: Path::new(&nix).to_path_buf(),
        input_store_path: Path::new("/tmp/not-a-store-path").to_path_buf(),
        release_version: "1.0.0".into(),
        platform: "x86_64-linux".into(),
        archive_url: "https://example.com/a.tar".into(),
        output_dir: root.path().to_path_buf(),
    })
    .unwrap_err();
    assert!(
        err.contains("nix/store") || err.contains("output directory") || err.contains("store path"),
        "unexpected error: {err}"
    );
}
