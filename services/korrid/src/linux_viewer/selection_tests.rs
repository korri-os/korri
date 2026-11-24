use super::*;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn package(id: &str, treaty: Option<PathBuf>) -> EnabledPackage {
    EnabledPackage {
        id: id.into(),
        package: "/nix/store/00000000000000000000000000000000-plugin".into(),
        files: treaty
            .into_iter()
            .map(|path| ("treaty".to_owned(), path))
            .collect::<BTreeMap<_, _>>(),
        entry: "plugin.ts".into(),
        sources: vec!["plugin.ts".into()],
        requires: vec![],
        hosts: vec![],
        credential_host: None,
        credential_token_url: None,
    }
}

fn read(path: &Path) -> std::io::Result<Vec<u8>> {
    std::fs::read(path)
}

#[test]
fn the_korri_viewer_is_chosen_only_when_its_treaty_is_korrids_byte_for_byte() {
    let dir = tempfile::tempdir().unwrap();
    let same = dir.path().join("same.ts");
    std::fs::write(&same, TREATY).unwrap();
    let mut changed_bytes = TREATY.to_vec();
    changed_bytes[0] ^= 1;
    let changed = dir.path().join("changed.ts");
    std::fs::write(&changed, &changed_bytes).unwrap();
    let moonlight = package("@korri:moonlight", None);

    // Missing or disabled viewer: a clear refusal, never another client.
    assert!(select(&[], &read)
        .unwrap_err()
        .to_string()
        .contains("@korri:viewer is not enabled"));
    assert!(select(std::slice::from_ref(&moonlight), &read).is_err());

    // Equal bytes: the Korri viewer, with or without Moonlight installed.
    let korri = package("@korri:viewer", Some(same.clone()));
    assert_eq!(
        select(&[moonlight.clone(), korri.clone()], &read).unwrap(),
        ViewerPlugin::Korri
    );
    assert_eq!(
        select(std::slice::from_ref(&korri), &read).unwrap(),
        ViewerPlugin::Korri
    );

    // One changed byte: refused even if an obsolete client remains installed.
    let other = package("@korri:viewer", Some(changed));
    assert!(select(&[moonlight.clone(), other.clone()], &read).is_err());
    let refusal = select(std::slice::from_ref(&other), &read)
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("different viewer contract"), "{refusal}");
    assert!(refusal.contains(&sha256(&changed_bytes)), "{refusal}");
    assert!(refusal.contains(&sha256(TREATY)), "{refusal}");

    // No treaty file: refused the same way.
    let unknown = package("@korri:viewer", None);
    assert!(select(&[moonlight.clone(), unknown.clone()], &read).is_err());
    let refusal = select(&[unknown], &read).unwrap_err().to_string();
    assert!(refusal.contains("declares no treaty file"), "{refusal}");

    // An unreadable treaty is a refusal, not a guess.
    let missing = package("@korri:viewer", Some(dir.path().join("absent.ts")));
    assert!(select(&[missing], &read).is_err());

    // Two packages with one id are not a choice.
    assert!(select(&[korri.clone(), korri], &read).is_err());
}

#[test]
fn the_production_read_takes_only_an_immutable_store_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("viewer.ts");
    std::fs::write(&file, TREATY).unwrap();
    assert!(read_immutable(&file).is_err());
    assert!(read_immutable(Path::new("/nix/store")).is_err());
}

#[tokio::test]
async fn disabling_the_plugin_refuses_new_starts_but_keeps_existing_lifecycle_controls() {
    use crate::linux_viewer::test_backend::{LinuxViewerAction, TestLinuxStreamViewer};
    use std::sync::Mutex;
    let backend = TestLinuxStreamViewer::default();
    let choice = Arc::new(Mutex::new(Ok(ViewerPlugin::Korri)));
    let chosen = choice.clone();
    let viewer = InstalledLinuxStreamViewer::new(
        Arc::new(backend.clone()),
        Arc::new(move || chosen.lock().unwrap().clone()),
    );
    let launch = "0123456789abcdef0123456789abcdef";
    viewer.start(launch, "192.0.2.1", None).await.unwrap();
    *choice.lock().unwrap() = Err(error("@korri:viewer is not enabled"));
    assert!(viewer
        .start("22222222222222222222222222222222", "192.0.2.1", None)
        .await
        .is_err());
    viewer.background(launch).await.unwrap();
    viewer.status(launch).await.unwrap();
    viewer.stop(launch).await.unwrap();
    assert_eq!(
        backend
            .calls()
            .into_iter()
            .map(|call| call.action)
            .collect::<Vec<_>>(),
        vec![
            LinuxViewerAction::Start,
            LinuxViewerAction::Background,
            LinuxViewerAction::Status,
            LinuxViewerAction::Stop
        ]
    );
}
