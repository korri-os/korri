//! A logind `sleep` block inhibitor held while a job downloads or unpacks
//! (ADR 0013). The screen may still blank; the device must not suspend.
//! Shutdown is never blocked. Without logind, korrid logs and continues.
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

#[derive(Clone)]
pub(super) enum SleepBlock {
    /// `org.freedesktop.login1.Manager.Inhibit("sleep", …, "block")`.
    Logind,
    /// Counts held blocks instead of calling the system bus.
    #[cfg_attr(not(test), allow(dead_code))]
    InMemory(Arc<InMemoryBlocks>),
}

#[derive(Default)]
pub(super) struct InMemoryBlocks {
    pub held: AtomicUsize,
    pub taken: AtomicUsize,
}

impl Default for SleepBlock {
    fn default() -> Self {
        // Unit tests must never take a real inhibitor on the build machine.
        #[cfg(test)]
        return Self::InMemory(Arc::default());
        #[cfg(not(test))]
        Self::Logind
    }
}

/// Dropping the guard closes the inhibitor file descriptor, which releases it.
pub(super) struct SleepGuard {
    _fd: Option<zbus::zvariant::OwnedFd>,
    blocks: Option<Arc<InMemoryBlocks>>,
}

impl Drop for SleepGuard {
    fn drop(&mut self) {
        if let Some(blocks) = &self.blocks {
            blocks.held.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

impl SleepBlock {
    pub(super) async fn block(&self) -> SleepGuard {
        match self {
            Self::InMemory(blocks) => {
                blocks.held.fetch_add(1, Ordering::SeqCst);
                blocks.taken.fetch_add(1, Ordering::SeqCst);
                SleepGuard {
                    _fd: None,
                    blocks: Some(blocks.clone()),
                }
            }
            Self::Logind => {
                let fd = match tokio::time::timeout(Duration::from_secs(2), inhibit()).await {
                    Ok(Ok(fd)) => Some(fd),
                    Ok(Err(error)) => {
                        eprintln!("korrid acquisition: sleep inhibitor unavailable: {error}");
                        None
                    }
                    Err(_) => {
                        eprintln!("korrid acquisition: sleep inhibitor timed out");
                        None
                    }
                };
                SleepGuard {
                    _fd: fd,
                    blocks: None,
                }
            }
        }
    }
}

async fn inhibit() -> zbus::Result<zbus::zvariant::OwnedFd> {
    let bus = zbus::Connection::system().await?;
    let manager = zbus::Proxy::new(
        &bus,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
    .await?;
    manager
        .call(
            "Inhibit",
            &("sleep", "korrid", "Downloading game content", "block"),
        )
        .await
}
