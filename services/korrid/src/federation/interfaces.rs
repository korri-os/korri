//! Linux interface addresses are live transport evidence, never configuration.
use super::coordinator::DiscoveryControl;
use std::{
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};

pub const ADDRESS_POLL: Duration = Duration::from_secs(1);

pub fn endpoints(bind: SocketAddr) -> io::Result<Vec<String>> {
    Ok(derive_endpoints(bind, interface_addresses()?))
}

/// Prefer cross-network Tailscale routes, then LAN IPv4, other IPv4, ULA and
/// global IPv6, then IPv4 link-local. Numeric address order breaks ties. The
/// existing EndpointRecord treaty permits eight candidates, not eight per NIC.
pub fn derive_endpoints(
    bind: SocketAddr,
    addresses: impl IntoIterator<Item = IpAddr>,
) -> Vec<String> {
    let mut addresses: Vec<_> = addresses
        .into_iter()
        .map(|ip| match ip {
            IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(ip),
            _ => ip,
        })
        .filter(|ip| {
            if ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || bind.ip().is_loopback()
            {
                return false;
            }
            if !bind.ip().is_unspecified() && bind.ip() != *ip {
                return false;
            }
            match ip {
                IpAddr::V4(ip) => !ip.is_broadcast() && ip.octets()[0] != 0,
                IpAddr::V6(ip) => !bind.is_ipv4() && !ip.is_unicast_link_local(),
            }
        })
        .collect();
    addresses.sort_by_key(|ip| (priority(*ip), *ip));
    addresses.dedup();
    addresses.truncate(8);
    addresses
        .into_iter()
        .map(|ip| format!("http://{}", SocketAddr::new(ip, bind.port())))
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

pub fn interface_addresses() -> io::Result<Vec<IpAddr>> {
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
                match (*entry.ifa_addr).sa_family as i32 {
                    libc::AF_INET => {
                        let addr = &*entry.ifa_addr.cast::<libc::sockaddr_in>();
                        addresses.push(IpAddr::V4(Ipv4Addr::from(
                            addr.sin_addr.s_addr.to_ne_bytes(),
                        )));
                    }
                    libc::AF_INET6 => {
                        let addr = &*entry.ifa_addr.cast::<libc::sockaddr_in6>();
                        addresses.push(IpAddr::V6(Ipv6Addr::from(addr.sin6_addr.s6_addr)));
                    }
                    _ => {}
                }
            }
            current = entry.ifa_next;
        }
    }
    Ok(addresses)
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
            values.iter().map(|ip| ip.parse().unwrap()),
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
                "http://10.1.0.2:39217"
            ]
        );
        assert_eq!(
            result,
            derive(&values.into_iter().rev().collect::<Vec<_>>())
        );
    }
    #[test]
    fn advertise_only_addresses_the_listener_can_accept() {
        let addresses = ["10.0.0.2".parse().unwrap(), "fd00::1".parse().unwrap()];
        assert_eq!(
            derive_endpoints("0.0.0.0:80".parse().unwrap(), addresses),
            ["http://10.0.0.2:80"]
        );
        assert!(derive_endpoints("127.0.0.1:80".parse().unwrap(), addresses).is_empty());
        assert_eq!(
            derive_endpoints("[fd00::1]:80".parse().unwrap(), addresses),
            ["http://[fd00::1]:80"]
        );
    }
    #[test]
    fn linux_interface_collection_runs() {
        interface_addresses().unwrap();
    }
}
