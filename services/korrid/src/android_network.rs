//! Android's network and power observations, as federation consumes them
//! (`federation::reach::Environment`). Two producers fill [`Snapshot`]:
//! Artemis's `KorridNetwork.java` (JSON over JNI) and the app with no Korri
//! Java, which reads Android's own classes through JNI
//! (`android/framework_network.rs`) and applies [`snapshot`] here. The rules
//! are `KorridNetwork.snapshot`'s, unchanged.
use crate::federation::{interfaces::InterfaceAddress, reach::RelayFacts};
use std::net::IpAddr;

#[derive(Debug, serde::Deserialize)]
pub(crate) struct Snapshot {
    pub interfaces: Vec<InterfaceAddress>,
    #[serde(flatten)]
    pub facts: RelayFacts,
}

/// One `android.net.Network` as `ConnectivityManager` describes it. A field
/// that Android returned as null is `None`.
#[derive(Clone, Debug, Default)]
pub(crate) struct ObservedNetwork {
    /// `NetworkCapabilities`: whether it is a VPN and is not metered.
    pub capabilities: Option<Capabilities>,
    /// `LinkProperties`.
    pub link: Option<Link>,
    /// `network.equals(connectivity.getActiveNetwork())`.
    pub active: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Capabilities {
    pub vpn: bool,
    pub not_metered: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Link {
    pub interface: Option<String>,
    /// Any of its routes is a default route.
    pub default_route: bool,
    pub addresses: Vec<IpAddr>,
}

/// QUIC uses physical links. VPNs stay available to ordinary peer HTTP
/// through Android's routing, never as a required path. With no active
/// network, a link with a default route counts as the default.
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
        if capabilities.vpn {
            continue;
        }
        let has_default_route = network.active || (!any_active && link.default_route);
        if network.active {
            unmetered = capabilities.not_metered;
        }
        for address in link.addresses.iter().copied().filter(|ip| usable(*ip)) {
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

/// `InetAddress`'s loopback, any-local, multicast and link-local addresses
/// are left out.
fn usable(ip: IpAddr) -> bool {
    let link_local = match ip {
        IpAddr::V4(ip) => ip.is_link_local(),
        IpAddr::V6(ip) => ip.segments()[0] & 0xffc0 == 0xfe80,
    };
    !(ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() || link_local)
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
            capabilities: Some(Capabilities {
                vpn: false,
                not_metered: true,
            }),
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

    #[test]
    fn vpns_unnamed_links_and_missing_answers_are_left_out() {
        let mut vpn = network("tun0", true, true, &["100.64.0.2"]);
        vpn.capabilities.as_mut().unwrap().vpn = true;
        let mut unnamed = network("x", false, true, &["10.9.0.1"]);
        unnamed.link.as_mut().unwrap().interface = None;
        let no_capabilities = ObservedNetwork {
            capabilities: None,
            ..network("eth1", false, true, &["10.8.0.1"])
        };
        let no_link = ObservedNetwork {
            link: None,
            ..network("eth2", false, true, &["10.7.0.1"])
        };
        let snapshot = snapshot(&[vpn, unnamed, no_capabilities, no_link], true, true);
        assert!(snapshot.interfaces.is_empty());
        // The VPN was the active network, so it still decides metering.
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

    #[test]
    fn the_java_producers_json_reads_into_the_same_snapshot() {
        let json = r#"{"interfaces":[{"interface":"wlan0","address":"192.168.1.20","has_default_route":true}],"external_power":true,"unmetered":false}"#;
        let read: Snapshot = serde_json::from_str(json).unwrap();
        let derived = snapshot(
            &[ObservedNetwork {
                capabilities: Some(Capabilities {
                    vpn: false,
                    not_metered: false,
                }),
                ..network("wlan0", true, true, &["192.168.1.20"])
            }],
            true,
            true,
        );
        assert_eq!(listed(&read), listed(&derived));
        assert_eq!(read.facts, derived.facts);
    }
}
