// SPDX-License-Identifier: GPL-3.0-or-later
use korri_viewer::lifecycle::Lifecycle;
use std::{os::unix::fs::MetadataExt, path::Path};

/// The Nix build sandbox maps only the build user, so its `/` belongs to an
/// unmapped UID and the viewer correctly refuses every location there. A test
/// that needs a bound socket then proves that refusal and stops. The
/// development shell, where root owns `/`, runs the test in full.
pub fn refused_below_an_unmapped_root(socket: &Path) -> bool {
    let viewer = unsafe { libc::geteuid() };
    let owner = std::fs::metadata("/").unwrap().uid();
    if owner == 0 || owner == viewer {
        return false;
    }
    let error = Lifecycle::bind(socket, "a", viewer)
        .err()
        .expect("the viewer trusted a `/` that neither root nor the viewer owns");
    assert!(
        format!("{error:#}").contains("ancestor / is owned by UID"),
        "{error:#}"
    );
    assert!(!socket.parent().unwrap().exists());
    true
}
