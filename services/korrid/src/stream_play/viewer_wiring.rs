//! Merge-only stand-in for the viewer worker's exact constructor contract.
//! At integration, import these three names from crate::linux_viewer in
//! stream_play/device.rs, then remove this module and its mod declaration.
use crate::{
    linux_viewer::{LinuxStreamViewer, LinuxViewerError},
    MoonlightCertificateProvisioned,
};
use futures::future::BoxFuture;
use std::{path::PathBuf, sync::Arc};

pub(crate) type CertificateProvision = Arc<
    dyn Fn(
            String,
            String,
        ) -> BoxFuture<'static, Result<MoonlightCertificateProvisioned, LinuxViewerError>>
        + Send
        + Sync,
>;
pub(crate) type LocalSeats =
    Arc<dyn Fn() -> BoxFuture<'static, Result<Vec<PathBuf>, LinuxViewerError>> + Send + Sync>;

pub(crate) fn production_viewer(
    _provision: CertificateProvision,
    _local_seats: LocalSeats,
) -> Arc<dyn LinuxStreamViewer> {
    #[cfg(not(test))]
    {
        Arc::new(crate::linux_viewer::UnavailableLinuxStreamViewer)
    }
    #[cfg(test)]
    {
        Arc::new(crate::linux_viewer::test_backend::TestLinuxStreamViewer::default())
    }
}
