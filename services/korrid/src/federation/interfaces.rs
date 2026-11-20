//! Linux interface addresses are live transport evidence, never configuration.
use super::coordinator::DiscoveryControl;
mod routes;
use std::{
    collections::BTreeSet,
    ffi::CStr,
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Once,
    time::Duration,
};

pub const ADDRESS_POLL: Duration = Duration::from_secs(1);

pub fn endpoints(bind: SocketAddr) -> io::Result<Vec<String>> {
    Ok(derive_endpoints(bind, interface_addresses()?))
}

/// Missing interface evidence must not block public membership discovery.
/// Never announce retained addresses as fresh evidence after enumeration fails.
pub fn endpoints_or_query_only(
    observation: io::Result<Vec<String>>,
    warning: &Once,
) -> Vec<String> {
    observation.unwrap_or_else(|error| {
        warning.call_once(|| {
            eprintln!(
                "korrid: federation interface enumeration failed: {error}; continuing owner publication and discovery without endpoint candidates"
            );
        });
        Vec::new()
    })
}

// Process-local evidence from getifaddrs and Linux's native route dump;
// not a persisted or announcement shape.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct InterfaceAddress {
    pub interface: String,
    pub address: IpAddr,
    pub has_default_route: bool,
}

/// The one candidate rule (ADR 0089): every address on every interface is a
/// candidate unless it is loopback, unspecified, multicast or link-local.
/// Limited broadcast and IPv4 "this network" (0.0.0.0/8) are unspecified
/// destinations too. No network, interface name or range is singled out; a
/// wrong address fails the device-key check. Peer `/rpc` origins, iroh direct
/// addresses, relay-duty listeners and the published endpoint all use it.
pub fn is_candidate(ip: IpAddr) -> bool {
    let ip = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(ip),
        ip => ip,
    };
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
        return false;
    }
    match ip {
        IpAddr::V4(ip) => !ip.is_broadcast() && !ip.is_link_local() && ip.octets()[0] != 0,
        IpAddr::V6(ip) => !ip.is_unicast_link_local(),
    }
}

/// Default-route interfaces precede other interfaces, so many virtual
/// networks cannot fill the existing eight-candidate bound before the real
/// LAN. Address class, numeric address and interface name break ties
/// deterministically. The order only bounds the list; peers race what is in it.
pub fn derive_endpoints(
    bind: SocketAddr,
    addresses: impl IntoIterator<Item = InterfaceAddress>,
) -> Vec<String> {
    let mut addresses: Vec<_> = addresses
        .into_iter()
        .map(|mut entry| {
            if let IpAddr::V6(ip) = entry.address {
                entry.address = ip.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(entry.address);
            }
            entry
        })
        .filter(|entry| {
            let ip = entry.address;
            if !is_candidate(ip) || bind.ip().is_loopback() {
                return false;
            }
            if !bind.ip().is_unspecified() && bind.ip() != ip {
                return false;
            }
            !(ip.is_ipv6() && bind.is_ipv4())
        })
        .collect();
    addresses.sort_by_key(|entry| {
        (
            !entry.has_default_route,
            class(entry.address),
            entry.address,
            entry.interface.clone(),
        )
    });
    let mut seen = BTreeSet::new();
    addresses.retain(|entry| seen.insert(entry.address));
    addresses.truncate(8);
    addresses
        .into_iter()
        .map(|entry| format!("http://{}", SocketAddr::new(entry.address, bind.port())))
        .collect()
}

fn class(ip: IpAddr) -> u8 {
    match ip {
        IpAddr::V4(ip) if ip.is_private() => 0,
        IpAddr::V4(_) => 1,
        IpAddr::V6(ip) if ip.is_unique_local() => 2,
        IpAddr::V6(_) => 3,
    }
}

pub fn interface_addresses() -> io::Result<Vec<InterfaceAddress>> {
    let defaults = default_route_interfaces()?;
    struct Addresses(*mut libc::ifaddrs);
    impl Drop for Addresses {
        fn drop(&mut self) {
            // SAFETY: this is the list returned by getifaddrs, freed once.
            unsafe { libc::freeifaddrs(self.0) };
        }
    }
    let mut head = std::ptr::null_mut();
    // SAFETY: head is an initialized output pointer; libc owns the list.
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let list = Addresses(head);
    let mut current = list.0;
    let mut addresses = Vec::new();
    while !current.is_null() {
        // SAFETY: all nodes and their family-sized sockaddr values live until list drops.
        unsafe {
            let entry = &*current;
            if entry.ifa_flags & libc::IFF_UP as u32 != 0
                && entry.ifa_flags & libc::IFF_LOOPBACK as u32 == 0
                && !entry.ifa_addr.is_null()
            {
                let interface = CStr::from_ptr(entry.ifa_name)
                    .to_string_lossy()
                    .into_owned();
                let address = match (*entry.ifa_addr).sa_family as i32 {
                    libc::AF_INET => {
                        let addr = &*entry.ifa_addr.cast::<libc::sockaddr_in>();
                        Some(IpAddr::V4(Ipv4Addr::from(
                            addr.sin_addr.s_addr.to_ne_bytes(),
                        )))
                    }
                    libc::AF_INET6 => {
                        let addr = &*entry.ifa_addr.cast::<libc::sockaddr_in6>();
                        Some(IpAddr::V6(Ipv6Addr::from(addr.sin6_addr.s6_addr)))
                    }
                    _ => None,
                };
                if let Some(address) = address {
                    addresses.push(InterfaceAddress {
                        has_default_route: defaults.contains(&interface),
                        interface,
                        address,
                    });
                }
            }
            current = entry.ifa_next;
        }
    }
    Ok(addresses)
}

pub(crate) fn default_route_interfaces() -> io::Result<BTreeSet<String>> {
    routes::default_route_indices()?
        .into_iter()
        .map(|index| {
            let mut name = [0 as libc::c_char; libc::IF_NAMESIZE];
            // SAFETY: name has the IF_NAMESIZE bytes required by if_indextoname.
            if unsafe { libc::if_indextoname(index, name.as_mut_ptr()) }.is_null() {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: successful if_indextoname writes a null-terminated name.
            Ok(unsafe { CStr::from_ptr(name.as_ptr()) }
                .to_string_lossy()
                .into_owned())
        })
        .collect()
}

/// A one-second address and route poll never
/// contacts relays unless the advertised set changes. Failed reads retain the
/// last observation; the publication provider independently refuses stale data.
pub async fn watch_endpoints(
    control: DiscoveryControl,
    read: impl Fn() -> io::Result<Vec<String>>,
    poll: Duration,
) {
    let mut previous = read().ok();
    loop {
        tokio::time::sleep(poll).await;
        if let Ok(current) = read() {
            if previous.as_ref() != Some(&current) && control.wake().is_ok() {
                previous = Some(current);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn derive(values: &[&str]) -> Vec<String> {
        derive_endpoints(
            "[::]:39217".parse().unwrap(),
            values.iter().map(|ip| InterfaceAddress {
                interface: if *ip == "192.168.1.9" {
                    "wlan0".into()
                } else {
                    "docker0".into()
                },
                address: ip.parse().unwrap(),
                has_default_route: *ip == "192.168.1.9",
            }),
        )
    }
    #[test]
    fn filters_unusable_addresses_and_formats_both_families() {
        let result = derive(&[
            "127.0.0.1",
            "::1",
            "0.0.0.0",
            "::",
            "fe80::1",
            "ff02::1",
            "224.0.0.1",
            "255.255.255.255",
            "0.1.2.3",
            "169.254.1.2",
            "::ffff:169.254.1.3",
            "100.100.1.2",
            "fd7a:115c:a1e0::1",
            "192.168.1.2",
            "10.42.0.1",
            "172.16.1.2",
            "2001:db8::1",
        ]);
        assert_eq!(
            result,
            [
                "http://10.42.0.1:39217",
                "http://172.16.1.2:39217",
                "http://192.168.1.2:39217",
                "http://100.100.1.2:39217",
                "http://[fd7a:115c:a1e0::1]:39217",
                "http://[2001:db8::1]:39217",
            ]
        );
        for origin in result {
            super::super::peer_origin(&origin).unwrap();
        }
    }
    /// Test fixture: a private network's addresses (here 100.x and fd7a::)
    /// are kept like any other address. Nothing names or prefers them.
    #[test]
    fn candidate_rule_keeps_every_address_a_peer_could_reach() {
        for kept in [
            "100.64.0.1",
            "100.100.1.2",
            "fd7a:115c:a1e0::1",
            "10.0.0.2",
            "192.168.1.2",
            "172.17.0.1",
            "203.0.113.9",
            "fd00::1",
            "2001:db8::1",
            "::ffff:100.64.0.1",
        ] {
            assert!(is_candidate(kept.parse().unwrap()), "{kept} was dropped");
        }
        for skipped in [
            "127.0.0.1",
            "::1",
            "0.0.0.0",
            "::",
            "0.1.2.3",
            "255.255.255.255",
            "224.0.0.251",
            "ff02::fb",
            "169.254.1.2",
            "fe80::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(
                !is_candidate(skipped.parse().unwrap()),
                "{skipped} was kept"
            );
        }
        let any = |interface: &str, ip: &str| InterfaceAddress {
            interface: interface.into(),
            address: ip.parse().unwrap(),
            has_default_route: false,
        };
        // The interface name never matters: the same address on any name
        // gives the same candidate.
        for interface in ["tailscale0", "zt0", "wg0", "eth1"] {
            assert_eq!(
                derive_endpoints(
                    "[::]:39217".parse().unwrap(),
                    [
                        any(interface, "100.64.0.1"),
                        any(interface, "fd7a:115c:a1e0::2")
                    ]
                ),
                [
                    "http://100.64.0.1:39217",
                    "http://[fd7a:115c:a1e0::2]:39217"
                ]
            );
        }
    }
    #[test]
    fn ordering_deduplication_and_eight_candidate_cap_are_stable() {
        let values = [
            "192.168.1.9",
            "10.1.0.9",
            "10.1.0.8",
            "10.1.0.7",
            "10.1.0.6",
            "10.1.0.5",
            "10.1.0.4",
            "10.1.0.3",
            "10.1.0.2",
            "100.64.0.1",
            "fd00::2",
            "100.64.0.1",
            "::ffff:100.64.0.1",
        ];
        let result = derive(&values);
        assert_eq!(result.len(), 8);
        assert_eq!(
            &result[..3],
            [
                "http://192.168.1.9:39217",
                "http://10.1.0.2:39217",
                "http://10.1.0.3:39217",
            ]
        );
        assert_eq!(
            result,
            derive(&values.into_iter().rev().collect::<Vec<_>>())
        );
    }
    #[test]
    fn advertise_only_addresses_the_listener_can_accept() {
        let addresses = ["10.0.0.2", "fd00::1"].map(|ip| InterfaceAddress {
            interface: "eth0".into(),
            address: ip.parse().unwrap(),
            has_default_route: true,
        });
        assert_eq!(
            derive_endpoints("0.0.0.0:80".parse().unwrap(), addresses.clone()),
            ["http://10.0.0.2:80"]
        );
        assert!(derive_endpoints("127.0.0.1:80".parse().unwrap(), addresses.clone()).is_empty());
        assert_eq!(
            derive_endpoints("[fd00::1]:80".parse().unwrap(), addresses),
            ["http://[fd00::1]:80"]
        );
    }
    #[test]
    fn default_route_ipv6_survives_many_virtual_ipv4_addresses() {
        let mut addresses: Vec<_> = (1..=12)
            .map(|n| InterfaceAddress {
                interface: format!("veth{n}"),
                address: IpAddr::V4(Ipv4Addr::new(10, 0, 0, n)),
                has_default_route: false,
            })
            .collect();
        addresses.push(InterfaceAddress {
            interface: "wlan0".into(),
            address: "2001:db8::1".parse().unwrap(),
            has_default_route: true,
        });
        assert_eq!(
            derive_endpoints("[::]:39217".parse().unwrap(), addresses)[0],
            "http://[2001:db8::1]:39217"
        );
    }
    #[test]
    fn missing_interface_evidence_warns_once_without_retaining_stale_candidates() {
        let warning = Once::new();
        let candidates = vec!["http://192.168.1.9:39217".into()];
        assert_eq!(
            endpoints_or_query_only(Ok(candidates.clone()), &warning),
            candidates
        );
        assert!(!warning.is_completed());
        for _ in 0..2 {
            assert!(endpoints_or_query_only(
                Err(io::Error::from_raw_os_error(libc::EAFNOSUPPORT)),
                &warning,
            )
            .is_empty());
            assert!(warning.is_completed());
        }
        assert_eq!(
            endpoints_or_query_only(Ok(candidates.clone()), &warning),
            candidates
        );
    }
    #[test]
    fn linux_interface_collection_runs() {
        interface_addresses().unwrap();
    }
}
