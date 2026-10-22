//! Linux interface addresses are live transport evidence, never configuration.
use super::coordinator::DiscoveryControl;
use std::{
    collections::BTreeSet,
    ffi::CStr,
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};

pub const ADDRESS_POLL: Duration = Duration::from_secs(1);

pub fn endpoints(bind: SocketAddr) -> io::Result<Vec<String>> {
    Ok(derive_endpoints(bind, interface_addresses()?))
}

// Process-local evidence from getifaddrs and Linux's native proc route tables;
// not a persisted or announcement shape.
#[derive(Clone, Debug)]
pub struct InterfaceAddress {
    pub interface: String,
    pub address: IpAddr,
    pub has_default_route: bool,
}

/// Tailscale and default-route interfaces precede other interfaces, so virtual
/// networks cannot fill the existing eight-candidate bound before the real LAN.
/// Address class, numeric address and interface name break ties deterministically.
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
            if ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || bind.ip().is_loopback()
            {
                return false;
            }
            if !bind.ip().is_unspecified() && bind.ip() != ip {
                return false;
            }
            match ip {
                IpAddr::V4(ip) => !ip.is_broadcast() && ip.octets()[0] != 0,
                IpAddr::V6(ip) => !bind.is_ipv4() && !ip.is_unicast_link_local(),
            }
        })
        .collect();
    addresses.sort_by_key(|entry| {
        let class = priority(entry.address);
        let interface = if class <= 1 || entry.interface == "tailscale0" {
            0
        } else if entry.has_default_route {
            1
        } else {
            2
        };
        (interface, class, entry.address, entry.interface.clone())
    });
    let mut seen = BTreeSet::new();
    addresses.retain(|entry| seen.insert(entry.address));
    addresses.truncate(8);
    addresses
        .into_iter()
        .map(|entry| format!("http://{}", SocketAddr::new(entry.address, bind.port())))
        .collect()
}

fn priority(ip: IpAddr) -> u8 {
    match ip {
        IpAddr::V4(ip) if u32::from(ip) & 0xffc00000 == 0x64400000 => 0,
        IpAddr::V6(ip) if ip.segments()[..3] == [0xfd7a, 0x115c, 0xa1e0] => 1,
        IpAddr::V4(ip) if ip.is_private() => 2,
        IpAddr::V4(ip) if ip.is_link_local() => 6,
        IpAddr::V4(_) => 3,
        IpAddr::V6(ip) if ip.is_unique_local() => 4,
        IpAddr::V6(_) => 5,
    }
}

pub fn interface_addresses() -> io::Result<Vec<InterfaceAddress>> {
    fn table(path: &str) -> io::Result<String> {
        match std::fs::read_to_string(path) {
            Ok(table) => Ok(table),
            // A kernel with no IPv6 support has no IPv6 route table.
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
            Err(error) => Err(error),
        }
    }
    let defaults =
        default_route_interfaces(&table("/proc/net/route")?, &table("/proc/net/ipv6_route")?);
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

// Linux /proc/net/route: iface,destination,gateway,flags,...,mask.
// Linux /proc/net/ipv6_route: destination,prefix,source,prefix,gateway,...,flags,iface.
fn default_route_interfaces(ipv4: &str, ipv6: &str) -> BTreeSet<String> {
    let up = |flags: &str| {
        u32::from_str_radix(flags, 16).is_ok_and(|flags| flags & 1 != 0 && flags & 0x200 == 0)
    };
    let mut interfaces = BTreeSet::new();
    for line in ipv4.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() >= 8 && fields[1] == "00000000" && fields[7] == "00000000" && up(fields[3])
        {
            interfaces.insert(fields[0].to_owned());
        }
    }
    for line in ipv6.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() == 10
            && fields[0] == "00000000000000000000000000000000"
            && fields[1] == "00"
            && up(fields[8])
        {
            interfaces.insert(fields[9].to_owned());
        }
    }
    interfaces
}

/// A cheap one-second getifaddrs poll avoids a netlink dependency and never
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
            "100.100.1.2",
            "fd7a:115c:a1e0::1",
            "192.168.1.2",
            "10.42.0.1",
            "172.16.1.2",
            "169.254.1.2",
            "2001:db8::1",
        ]);
        assert_eq!(
            result,
            [
                "http://100.100.1.2:39217",
                "http://[fd7a:115c:a1e0::1]:39217",
                "http://10.42.0.1:39217",
                "http://172.16.1.2:39217",
                "http://192.168.1.2:39217",
                "http://[2001:db8::1]:39217",
                "http://169.254.1.2:39217"
            ]
        );
        for origin in result {
            super::super::peer_origin(&origin).unwrap();
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
            "fd7a:115c:a1e0::2",
            "100.64.0.1",
            "::ffff:100.64.0.1",
        ];
        let result = derive(&values);
        assert_eq!(result.len(), 8);
        assert_eq!(
            &result[..3],
            [
                "http://100.64.0.1:39217",
                "http://[fd7a:115c:a1e0::2]:39217",
                "http://192.168.1.9:39217"
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
    fn default_routes_use_both_linux_tables_and_ignore_reject_down_and_specific_routes() {
        let v4 = "Iface Destination Gateway Flags RefCnt Use Metric Mask\neth0 00000000 0101A8C0 0003 0 0 100 00000000\ndocker0 00000000 00000000 0001 0 0 0 00FFFFFF\ndown0 00000000 00000000 0000 0 0 0 00000000\nreject0 00000000 00000000 0201 0 0 0 00000000\n";
        let v6 = "00000000000000000000000000000000 00 00000000000000000000000000000000 00 fe800000000000000000000000000001 00000064 00000001 00000000 00000003 wlan0\n00000000000000000000000000000000 00 00000000000000000000000000000000 00 00000000000000000000000000000000 ffffffff 00000001 00000000 00200200 lo\n";
        assert_eq!(
            default_route_interfaces(v4, v6),
            BTreeSet::from(["eth0".into(), "wlan0".into()])
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
    fn linux_interface_collection_runs() {
        interface_addresses().unwrap();
    }
}
