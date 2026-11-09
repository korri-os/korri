use super::*;
use base64::Engine;
use ring::signature::{Ed25519KeyPair, KeyPair};

pub(crate) const STORE_PATH: &str = "/nix/store/c0j4s87fx0j81n7s1szhszpimjlp9f48-korri-plugin";

fn nar_str(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    out.extend_from_slice(bytes);
    out.extend(std::iter::repeat_n(0, (8 - bytes.len() % 8) % 8));
}

/// A NAR of one directory holding the given regular files, sorted by name.
pub(crate) fn nar_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for t in ["nix-archive-1", "(", "type", "directory"] {
        nar_str(&mut out, t.as_bytes());
    }
    for (name, contents) in files {
        for t in ["entry", "(", "name"] {
            nar_str(&mut out, t.as_bytes());
        }
        nar_str(&mut out, name.as_bytes());
        for t in ["node", "(", "type", "regular", "contents"] {
            nar_str(&mut out, t.as_bytes());
        }
        nar_str(&mut out, contents);
        nar_str(&mut out, b")");
        nar_str(&mut out, b")");
    }
    nar_str(&mut out, b")");
    out
}

pub(crate) fn manifest(extra: &str) -> Vec<u8> {
    format!(
        r#"{{"publisher":{{"namespace":"@korri"}},"entry":"plugin.ts","sources":["plugin.ts"]{extra}}}"#
    )
    .into_bytes()
}

pub(crate) struct Fixture {
    pub(crate) archive: Vec<u8>,
    pub(crate) sha256: String,
    pub(crate) key: String,
}

pub(crate) fn fixture_with(nar: &[u8], references: &str, sign: bool) -> Fixture {
    let rng = ring::rand::SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
    let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
    let b64 = base64::engine::general_purpose::STANDARD;
    let key = format!("test-1:{}", b64.encode(pair.public_key().as_ref()));
    let hash = format!("sha256:{}", nix32(&Sha256::digest(nar)));
    let mut info = NarInfo {
        store_path: STORE_PATH.into(),
        url: "nar/0000000000000000000000000000000000000000000000000000.nar".into(),
        compression: "none".into(),
        nar_hash: hash.clone(),
        nar_size: nar.len() as u64,
        references: references.split_whitespace().map(str::to_owned).collect(),
        signatures: vec![],
        ca: None,
    };
    let sig = b64.encode(pair.sign(fingerprint(&info).unwrap().as_bytes()).as_ref());
    if sign {
        info.signatures.push(format!("test-1:{sig}"));
    }
    let narinfo = format!(
        "StorePath: {}\nURL: {}\nCompression: none\nNarHash: {}\nNarSize: {}\nReferences: {}\n{}",
        info.store_path,
        info.url,
        info.nar_hash,
        info.nar_size,
        references,
        info.signatures.iter().map(|s| format!("Sig: {s}\n")).collect::<String>()
    );
    let mut builder = tar::Builder::new(Vec::new());
    for (path, bytes) in [
        ("nix-cache-info", b"StoreDir: /nix/store\n".to_vec()),
        ("c0j4s87fx0j81n7s1szhszpimjlp9f48.narinfo", narinfo.into_bytes()),
        (info.url.as_str(), nar.to_vec()),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o444);
        header.set_cksum();
        builder.append_data(&mut header, path, bytes.as_slice()).unwrap();
    }
    let archive = builder.into_inner().unwrap();
    Fixture { sha256: hex::encode(Sha256::digest(&archive)), archive, key }
}

fn release(f: &Fixture) -> ReleaseRef<'_> {
    ReleaseRef { plugin_id: "@korri:homebrewhub", store_path: STORE_PATH, archive_sha256: &f.sha256 }
}

#[test]
fn installs_a_signed_declaration_only_release() {
    let nar = nar_of(&[("manifest.json", &manifest("")), ("plugin.ts", b"export const name = 'x'\n")]);
    let f = fixture_with(&nar, "", true);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("pkg");
    let package = install_archive(&f.archive, &release(&f), &f.key, &dest).unwrap();
    assert_eq!(package.id, "@korri:homebrewhub");
    assert_eq!(package.sources, vec!["plugin.ts".to_string()]);
    assert!(package.files.is_empty());
    assert_eq!(fs::read(dest.join("plugin.ts")).unwrap(), b"export const name = 'x'\n");
}

#[test]
fn refuses_a_tampered_archive_a_foreign_key_and_a_missing_signature() {
    let nar = nar_of(&[("manifest.json", &manifest("")), ("plugin.ts", b"x")]);
    let f = fixture_with(&nar, "", true);
    let dir = tempfile::tempdir().unwrap();
    let mut tampered = f.archive.clone();
    *tampered.last_mut().unwrap() ^= 1;
    assert_eq!(
        install_archive(&tampered, &release(&f), &f.key, &dir.path().join("a")).unwrap_err(),
        InstallError::ArchiveHash
    );
    let other = fixture_with(&nar, "", true);
    assert_eq!(
        install_archive(&f.archive, &release(&f), &other.key, &dir.path().join("b")).unwrap_err(),
        InstallError::Signature
    );
    let unsigned = fixture_with(&nar, "", false);
    assert_eq!(
        install_archive(&unsigned.archive, &release(&unsigned), &unsigned.key, &dir.path().join("c"))
            .unwrap_err(),
        InstallError::Signature
    );
    assert!(!dir.path().join("a").exists() && !dir.path().join("b").exists());
}

#[test]
fn refuses_native_files_and_store_references() {
    let dir = tempfile::tempdir().unwrap();
    for extra in [r#","files":{"x":"/nix/store/a"}"#, r#","packages":{"x":"/nix/store/a"}"#, r#","services":{"x":"y"}"#] {
        let nar = nar_of(&[("manifest.json", &manifest(extra)), ("plugin.ts", b"x")]);
        let f = fixture_with(&nar, "", true);
        let err = install_archive(&f.archive, &release(&f), &f.key, &dir.path().join(extra.len().to_string()))
            .unwrap_err();
        assert!(matches!(err, InstallError::NotDeclarationOnly(_)), "{extra}: {err}");
    }
    let nar = nar_of(&[("manifest.json", &manifest("")), ("plugin.ts", b"x")]);
    let f = fixture_with(&nar, "66c4hds7g34kra7jkl53wmrm42p60v2w-glibc", true);
    assert!(matches!(
        install_archive(&f.archive, &release(&f), &f.key, &dir.path().join("r")).unwrap_err(),
        InstallError::NotDeclarationOnly(_)
    ));
}

#[test]
fn nar_reader_refuses_traversal_symlinks_and_unsorted_names() {
    let dir = tempfile::tempdir().unwrap();
    let nar = nar_of(&[("..", b"x")]);
    assert!(unpack_nar(&nar, &dir.path().join("a")).is_err());
    let nar = nar_of(&[("b", b"x"), ("a", b"y")]);
    assert!(unpack_nar(&nar, &dir.path().join("b")).is_err());
    let mut link = Vec::new();
    for t in ["nix-archive-1", "(", "type", "directory", "entry", "(", "name", "l", "node", "(", "type", "symlink", "target", "/etc", ")", ")", ")"] {
        nar_str(&mut link, t.as_bytes());
    }
    assert!(unpack_nar(&link, &dir.path().join("c")).is_err());
}

#[test]
fn store_installs_and_enables_replacing_an_earlier_release() {
    let nar = nar_of(&[("manifest.json", &manifest(r#","hosts":["hh3.gbdev.io"]"#)), ("plugin.ts", b"x")]);
    let f = fixture_with(&nar, "", true);
    let dir = tempfile::tempdir().unwrap();
    let store = AndroidPluginStore::new(dir.path());
    assert!(store.read().unwrap().is_empty());
    store.install(&f.archive, &release(&f), &f.key).unwrap();
    store.install(&f.archive, &release(&f), &f.key).unwrap();
    let enabled = store.read().unwrap();
    assert_eq!(enabled.len(), 1);
    assert_eq!(enabled[0].hosts, vec!["hh3.gbdev.io".to_string()]);
}

/// The stop rule of the Android workflow: the reader must accept a real
/// archive that Nix signed. Set the three variables to run it, for example
/// from `nix/tasks` on a builder after `nix copy --to file://…` and
/// `korri-publish`. Skipped when they are unset.
#[test]
fn accepts_a_real_nix_signed_archive_when_given() {
    let (Ok(archive), Ok(store_path), Ok(key)) = (
        std::env::var("KORRI_NIXFREE_ARCHIVE"),
        std::env::var("KORRI_NIXFREE_STORE_PATH"),
        std::env::var("KORRI_NIXFREE_PUBLIC_KEY"),
    ) else {
        return;
    };
    let bytes = fs::read(archive).unwrap();
    let sha = hex::encode(Sha256::digest(&bytes));
    let dir = tempfile::tempdir().unwrap();
    let release = ReleaseRef { plugin_id: "@korri:homebrewhub", store_path: &store_path, archive_sha256: &sha };
    let package = install_archive(&bytes, &release, &key, &dir.path().join("pkg")).unwrap();
    assert!(dir.path().join("pkg/plugin.ts").is_file(), "{package:?}");
}
