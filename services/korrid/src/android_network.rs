//! Android's network and power observations, as federation consumes them
//! (`federation::reach::Environment`). The app with no Korri Java reads
//! Android's own classes through JNI (`android/framework_network.rs`) and
//! applies [`snapshot`] here. The rules are those of the Artemis app's
//! `KorridNetwork.snapshot` (tag `artemis-final`), except that ADR 0089 ends
//! its VPN exclusion: addresses follow korrid's one candidate rule.
use crate::federation::{
    interfaces::{is_candidate, InterfaceAddress},
    reach::RelayFacts,
};
use std::net::IpAddr;

#[derive(Debug)]
pub(crate) struct Snapshot {
    pub interfaces: Vec<InterfaceAddress>,
    pub facts: RelayFacts,
}

/// One `android.net.Network` as `ConnectivityManager` describes it. A field
/// that Android returned as null is `None`.
#[derive(Clone, Debug, Default)]
pub(crate) struct ObservedNetwork {
    /// `NetworkCapabilities`: whether it is not metered.
    pub capabilities: Option<Capabilities>,
    /// `LinkProperties`.
    pub link: Option<Link>,
    /// `network.equals(connectivity.getActiveNetwork())`.
    pub active: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Capabilities {
    pub not_metered: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Link {
    pub interface: Option<String>,
    /// Any of its routes is a default route.
    pub default_route: bool,
    pub addresses: Vec<IpAddr>,
}

/// Every network's addresses are candidates by the one rule korrid uses
/// everywhere (ADR 0089): a VPN or private network is not left out, and
/// paths are chosen by measurement. With no active network, a link with a
/// default route counts as the default.
pub(crate) fn snapshot(
    networks: &[ObservedNetwork],
    any_active: bool,
    external_power: bool,
) -> Snapshot {
    let mut interfaces = Vec::new();
    let mut unmetered = false;
    for network in networks {
        let (Some(capabilities), Some(link)) = (network.capabilities, &network.link) else {
            continue;
        };
        let Some(interface) = &link.interface else {
            continue;
        };
        let has_default_route = network.active || (!any_active && link.default_route);
        if network.active {
            unmetered = capabilities.not_metered;
        }
        for address in link
            .addresses
            .iter()
            .copied()
            .filter(|ip| is_candidate(*ip))
        {
            interfaces.push(InterfaceAddress {
                interface: interface.clone(),
                address,
                has_default_route,
            });
        }
    }
    Snapshot {
        interfaces,
        facts: RelayFacts {
            external_power,
            unmetered,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn network(
        interface: &str,
        active: bool,
        default_route: bool,
        addresses: &[&str],
    ) -> ObservedNetwork {
        ObservedNetwork {
            capabilities: Some(Capabilities { not_metered: true }),
            link: Some(Link {
                interface: Some(interface.into()),
                default_route,
                addresses: addresses.iter().map(|a| a.parse().unwrap()).collect(),
            }),
            active,
        }
    }

    fn listed(snapshot: &Snapshot) -> Vec<(String, String, bool)> {
        snapshot
            .interfaces
            .iter()
            .map(|a| {
                (
                    a.interface.clone(),
                    a.address.to_string(),
                    a.has_default_route,
                )
            })
            .collect()
    }

    #[test]
    fn active_network_is_the_default_route_and_decides_metering() {
        let mut cellular = network("rmnet0", false, true, &["10.20.0.5"]);
        cellular.capabilities.as_mut().unwrap().not_metered = false;
        let wifi = network("wlan0", true, true, &["192.168.1.20", "2001:db8::20"]);
        let snapshot = snapshot(&[cellular, wifi], true, true);
        assert_eq!(
            listed(&snapshot),
            vec![
                ("rmnet0".into(), "10.20.0.5".into(), false),
                ("wlan0".into(), "192.168.1.20".into(), true),
                ("wlan0".into(), "2001:db8::20".into(), true),
            ]
        );
        assert_eq!(
            snapshot.facts,
            RelayFacts {
                external_power: true,
                unmetered: true
            }
        );
    }

    #[test]
    fn without_an_active_network_a_default_route_counts_and_nothing_is_unmetered() {
        let snapshot = snapshot(
            &[
                network("eth0", false, true, &["10.0.2.15"]),
                network("wlan1", false, false, &["10.1.0.4"]),
            ],
            false,
            false,
        );
        assert_eq!(
            listed(&snapshot),
            vec![
                ("eth0".into(), "10.0.2.15".into(), true),
                ("wlan1".into(), "10.1.0.4".into(), false),
            ]
        );
        assert!(!snapshot.facts.unmetered && !snapshot.facts.external_power);
    }

    /// Test fixture: a VPN or private network (here `tun0` with a 100.x and
    /// an fd7a address) is a candidate like any other network.
    #[test]
    fn a_vpn_network_is_kept_like_any_other() {
        let vpn = network("tun0", true, false, &["100.64.0.2", "fd7a:115c:a1e0::2"]);
        let wifi = network("wlan0", false, true, &["192.168.1.20"]);
        let snapshot = snapshot(&[vpn, wifi], true, true);
        assert_eq!(
            listed(&snapshot),
            vec![
                ("tun0".into(), "100.64.0.2".into(), true),
                ("tun0".into(), "fd7a:115c:a1e0::2".into(), true),
                ("wlan0".into(), "192.168.1.20".into(), false),
            ]
        );
        // The active network decides metering, whatever its transport.
        assert!(snapshot.facts.unmetered);
    }

    #[test]
    fn unnamed_links_and_missing_answers_are_left_out() {
        let mut unnamed = network("x", true, true, &["10.9.0.1"]);
        unnamed.link.as_mut().unwrap().interface = None;
        let no_capabilities = ObservedNetwork {
            capabilities: None,
            ..network("eth1", false, true, &["10.8.0.1"])
        };
        let no_link = ObservedNetwork {
            link: None,
            ..network("eth2", false, true, &["10.7.0.1"])
        };
        let snapshot = snapshot(&[unnamed, no_capabilities, no_link], true, true);
        assert!(snapshot.interfaces.is_empty());
        // The unnamed link was the active network, but a left-out network
        // decides nothing: its metering is never read, so nothing is unmetered.
        assert!(!snapshot.facts.unmetered);
    }

    #[test]
    fn loopback_any_multicast_and_link_local_addresses_are_left_out() {
        let snapshot = snapshot(
            &[network(
                "wlan0",
                true,
                true,
                &[
                    "127.0.0.1",
                    "0.0.0.0",
                    "224.0.0.1",
                    "169.254.3.4",
                    "::1",
                    "::",
                    "ff02::1",
                    "fe80::1",
                    "febf::1",
                    "fec0::1",
                    "192.168.1.20",
                ],
            )],
            true,
            true,
        );
        assert_eq!(
            snapshot
                .interfaces
                .iter()
                .map(|a| a.address.to_string())
                .collect::<Vec<_>>(),
            vec!["fec0::1", "192.168.1.20"]
        );
    }
}
