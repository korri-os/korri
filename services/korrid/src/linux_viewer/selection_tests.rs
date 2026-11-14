use super::*;
use crate::linux_viewer::test_backend::{LinuxViewerAction, TestLinuxStreamViewer};
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

    // Not installed: Moonlight, exactly as before this plugin existed.
    assert_eq!(
        select(std::slice::from_ref(&moonlight), &read).unwrap(),
        ViewerPlugin::Moonlight
    );
    assert_eq!(select(&[], &read).unwrap(), ViewerPlugin::Moonlight);

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

    // One changed byte: refused, and Moonlight stays the viewer.
    let other = package("@korri:viewer", Some(changed));
    assert_eq!(
        select(&[moonlight.clone(), other.clone()], &read).unwrap(),
        ViewerPlugin::Moonlight
    );
    let refusal = select(std::slice::from_ref(&other), &read)
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("different viewer contract"), "{refusal}");
    assert!(refusal.contains(&sha256(&changed_bytes)), "{refusal}");
    assert!(refusal.contains(&sha256(TREATY)), "{refusal}");

    // No treaty file: refused the same way.
    let unknown = package("@korri:viewer", None);
    assert_eq!(
        select(&[moonlight.clone(), unknown.clone()], &read).unwrap(),
        ViewerPlugin::Moonlight
    );
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
async fn each_launch_stays_with_the_viewer_that_started_it() {
    let moonlight = TestLinuxStreamViewer::default();
    let korri = TestLinuxStreamViewer::default();
    let choice = Arc::new(Mutex::new(Ok(ViewerPlugin::Korri)));
    let chosen = choice.clone();
    let viewer = InstalledLinuxStreamViewer::new(
        Arc::new(moonlight.clone()),
        Arc::new(korri.clone()),
        Arc::new(move || chosen.lock().unwrap().clone()),
    );
    let first = "0123456789abcdef0123456789abcdef";
    let second = "fedcba9876543210fedcba9876543210";
    viewer.start(first, "192.0.2.1", None).await.unwrap();
    *choice.lock().unwrap() = Ok(ViewerPlugin::Moonlight);
    viewer.start(second, "192.0.2.1", None).await.unwrap();
    // The plugin set changed between the two starts. Neither launch moves.
    viewer.background(first).await.unwrap();
    viewer.status(second).await.unwrap();
    viewer.stop(first).await.unwrap();
    let actions = |backend: &TestLinuxStreamViewer| {
        backend
            .calls()
            .into_iter()
            .map(|call| (call.launch_id, call.action))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        actions(&korri),
        vec![
            (first.to_owned(), LinuxViewerAction::Start),
            (first.to_owned(), LinuxViewerAction::Background),
            (first.to_owned(), LinuxViewerAction::Stop),
        ]
    );
    assert_eq!(
        actions(&moonlight),
        vec![
            (second.to_owned(), LinuxViewerAction::Start),
            (second.to_owned(), LinuxViewerAction::Status),
        ]
    );
    // A stopped or unknown launch goes where a start would go now.
    viewer.status(first).await.unwrap();
    assert_eq!(
        actions(&moonlight).last(),
        Some(&(first.to_owned(), LinuxViewerAction::Status))
    );
    // A failed start is forgotten; its adapter already cleaned up.
    *choice.lock().unwrap() = Ok(ViewerPlugin::Korri);
    korri.fail_next(LinuxViewerAction::Start, "no connection");
    let third = "00000000000000000000000000000003";
    assert!(viewer.start(third, "192.0.2.1", None).await.is_err());
    *choice.lock().unwrap() = Ok(ViewerPlugin::Moonlight);
    viewer.status(third).await.unwrap();
    assert_eq!(
        actions(&moonlight).last(),
        Some(&(third.to_owned(), LinuxViewerAction::Status))
    );
    // A refusal with no Moonlight to fall back to fails the start itself.
    *choice.lock().unwrap() = Err(LinuxViewerError::new("korrid refuses @korri:viewer"));
    let fourth = "00000000000000000000000000000004";
    assert_eq!(
        viewer
            .start(fourth, "192.0.2.1", None)
            .await
            .unwrap_err()
            .to_string(),
        "korrid refuses @korri:viewer"
    );
    // An unknown launch is still observed when no choice can be made, by
    // Moonlight's adapter as before this plugin existed.
    viewer.status(fourth).await.unwrap();
    assert_eq!(
        actions(&moonlight).last(),
        Some(&(fourth.to_owned(), LinuxViewerAction::Status))
    );
}
