//! Velvet passes the verifier the image build runs (`korri-shell install`,
//! clients/shell/nix/installed.nix), panes included (ADR 0086): swept with
//! the Stage and the Floor in one window and in a window each. A view of the
//! Floor's window with a field its `Surface` does not declare once failed
//! only at image build time.

use korri_shell::{engine::Limits, package::Package, verify};
use std::path::Path;

#[test]
fn velvet_passes_the_verifier_with_its_panes() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("packages/velvet");
    let summary = verify::verify(&Package::at(&dir).unwrap(), Limits::default());
    assert!(summary.pass, "{:?}", summary.reasons);
    assert_eq!(
        summary.panes.as_ref().map(|panes| panes.panes.clone()),
        Some(vec!["stage".to_owned(), "floor".to_owned()])
    );
    // Swept with one window and with a window each.
    let windows: Vec<usize> = summary.runs.iter().map(|run| run.windows).collect();
    assert!(windows.contains(&1) && windows.contains(&2), "{windows:?}");
}
