use futures::future::BoxFuture;
use std::{io, path::Path};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RelayFacts {
    pub external_power: bool,
    pub unmetered: bool,
}
impl RelayFacts {
    pub fn eligible(self) -> bool {
        self.external_power && self.unmetered
    }
}

/// One injectable boundary for both eligibility facts. Errors deny relaying.
pub(crate) trait RelayEnvironment: Send + Sync + 'static {
    fn facts(&self) -> BoxFuture<'_, io::Result<RelayFacts>>;
}
pub(crate) struct LinuxEnvironment {
    bus: tokio::sync::OnceCell<zbus::Connection>,
}
impl LinuxEnvironment {
    pub fn new() -> Self {
        Self {
            bus: tokio::sync::OnceCell::new(),
        }
    }
}
impl RelayEnvironment for LinuxEnvironment {
    fn facts(&self) -> BoxFuture<'_, io::Result<RelayFacts>> {
        Box::pin(async move {
            let external_power = external_power(Path::new("/sys/class/power_supply"))?;
            let bus = self
                .bus
                .get_or_try_init(zbus::Connection::system)
                .await
                .map_err(io::Error::other)?;
            let proxy = zbus::Proxy::new(
                bus,
                "org.freedesktop.NetworkManager",
                "/org/freedesktop/NetworkManager",
                "org.freedesktop.NetworkManager",
            )
            .await
            .map_err(io::Error::other)?;
            let metered: u32 = proxy
                .get_property("Metered")
                .await
                .map_err(io::Error::other)?;
            // NetworkManager: UNKNOWN=0, YES=1, NO=2, GUESS_YES=3, GUESS_NO=4.
            Ok(RelayFacts {
                external_power,
                unmetered: matches!(metered, 2 | 4),
            })
        })
    }
}
fn external_power(root: &Path) -> io::Result<bool> {
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        let kind = std::fs::read_to_string(path.join("type")).unwrap_or_default();
        if matches!(kind.trim(), "Mains" | "USB")
            && std::fs::read_to_string(path.join("online")).is_ok_and(|s| s.trim() == "1")
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reach_external_power_checks_type_and_online() {
        let root = tempfile::tempdir().unwrap();
        let supply = root.path().join("supply");
        std::fs::create_dir(&supply).unwrap();
        for (kind, online, expected) in [
            ("Battery", "1", false),
            ("Mains", "0", false),
            ("USB", "1", true),
            ("Mains", "1", true),
        ] {
            std::fs::write(supply.join("type"), kind).unwrap();
            std::fs::write(supply.join("online"), online).unwrap();
            assert_eq!(external_power(root.path()).unwrap(), expected);
        }
    }
}
