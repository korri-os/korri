//! Where korrid keeps its private state in the Android app:
//! `<no_backup>/korrid-state`, as `KorriBrainService.privateStateRoot` names it.
use std::{
    io,
    path::{Path, PathBuf},
};

/// `no_backup` is `Context.getNoBackupFilesDir()`, directly inside the app's
/// data directory. Android can link its user-directory prefix (`/data/user/0`
/// to `/data/data`) in the app's mount namespace: resolve only that prefix,
/// which the system owns. A link that the app owns, `korrid-state` itself
/// included, stays visible to korrid's strict private-storage checks.
pub(crate) fn private_state_root(no_backup: &Path) -> io::Result<PathBuf> {
    let malformed = || io::Error::other("Android's no_backup directory has no app data parent");
    let app_data = no_backup.parent().ok_or_else(malformed)?;
    let user_data = app_data.parent().ok_or_else(malformed)?.canonicalize()?;
    let (Some(app), Some(no_backup)) = (app_data.file_name(), no_backup.file_name()) else {
        return Err(malformed());
    };
    Ok(user_data.join(app).join(no_backup).join("korrid-state"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn resolves_the_user_prefix_and_keeps_app_owned_links() {
        let root = tempfile::tempdir().unwrap();
        let real_user = root.path().join("data/data");
        let app = real_user.join("com.example.app");
        std::fs::create_dir_all(app.join("no_backup")).unwrap();
        std::fs::create_dir_all(root.path().join("data/user")).unwrap();
        symlink(&real_user, root.path().join("data/user/0")).unwrap();
        // An app-owned link inside the app's directory is not resolved.
        symlink("/elsewhere", app.join("no_backup/korrid-state")).unwrap();
        let no_backup = root.path().join("data/user/0/com.example.app/no_backup");
        let real_root = root.path().canonicalize().unwrap();
        assert_eq!(
            private_state_root(&no_backup).unwrap(),
            real_root.join("data/data/com.example.app/no_backup/korrid-state")
        );
    }

    #[test]
    fn a_path_without_an_app_data_parent_is_refused() {
        assert!(private_state_root(Path::new("/")).is_err());
        assert!(private_state_root(Path::new("/no_backup")).is_err());
    }
}
