//! Read the live nearby-link and local-discovery facts without persisting
//! another address. Pure helpers below keep candidate ordering and port
//! handling injectable in tests.
use std::net::{IpAddr, SocketAddr};
/// Fresh live addresses go before the stored record's: the nearby link's
/// peer IP, then the addresses local discovery found for this device.
pub(crate) fn prepend(device: &str, candidates: &mut Vec<String>) {
    prepend_fresh(
        crate::network::nearby::nearby_peer_ip(device),
        crate::federation::local_discovery::found(device),
        candidates,
    );
}
fn prepend_fresh(ip: Option<IpAddr>, found: Vec<String>, candidates: &mut Vec<String>) {
    let port = candidates
        .iter()
        .chain(&found)
        .find_map(|origin| url::Url::parse(origin).ok()?.port_or_known_default());
    let nearby = ip
        .zip(port)
        .map(|(ip, port)| format!("http://{}", SocketAddr::new(ip, port)));
    let mut fresh: Vec<String> = nearby.into_iter().chain(found).collect();
    if fresh.is_empty() {
        return;
    }
    let mut seen = std::collections::BTreeSet::new();
    fresh.retain(|origin| seen.insert(origin.clone()));
    candidates.retain(|candidate| !fresh.contains(candidate));
    fresh.append(candidates);
    *candidates = fresh;
}
pub(crate) fn moonlight_control_origin(address: &str) -> Option<String> {
    let address = address.trim();
    if let Ok(ip) = address.parse::<IpAddr>() {
        return Some(format!("http://{}", SocketAddr::new(ip, 47989)));
    }
    let raw = format!("http://{address}");
    crate::federation::peer_origin(&raw).ok()?;
    let port = address
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .unwrap_or(47989);
    if port == 0 {
        return None;
    }
    let mut url = url::Url::parse(&raw).ok()?;
    url.set_port(Some(port)).ok()?;
    crate::federation::peer_origin(url.as_str()).ok()
}
pub(crate) fn moonlight_address(device: &str, current: Option<&str>) -> Option<String> {
    moonlight_address_ip(crate::network::nearby::nearby_peer_ip(device), current)
}
fn moonlight_address_ip(ip: Option<IpAddr>, current: Option<&str>) -> Option<String> {
    match ip {
        None => current.map(str::to_owned),
        Some(ip) => {
            let port = current
                .and_then(moonlight_control_origin)
                .and_then(|origin| url::Url::parse(&origin).ok()?.port_or_known_default())
                .unwrap_or(47989);
            Some(SocketAddr::new(ip, port).to_string())
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reach_sunshine_control_probe_preserves_explicit_ports_and_defaults_to_47989() {
        for (address, origin) in [
            ("sunshine", "http://sunshine:47989"),
            ("sunshine:80", "http://sunshine"),
            ("sunshine:49217", "http://sunshine:49217"),
            ("127.0.0.1", "http://127.0.0.1:47989"),
            ("[::1]", "http://[::1]:47989"),
            ("::1", "http://[::1]:47989"),
            ("[::1]:80", "http://[::1]"),
        ] {
            assert_eq!(moonlight_control_origin(address).as_deref(), Some(origin));
        }
        for address in ["user:pass@peer", "peer/path", "peer:0", "peer:"] {
            assert!(moonlight_control_origin(address).is_none());
        }
    }

    #[test]
    fn nearby_stream_address_uses_the_link_ip_and_keeps_the_control_port() {
        let ip = "10.42.0.2".parse().unwrap();
        assert_eq!(
            moonlight_address_ip(Some(ip), Some("old-lan")),
            Some("10.42.0.2:47989".into())
        );
        assert_eq!(
            moonlight_address_ip(Some(ip), Some("old-lan:49217")),
            Some("10.42.0.2:49217".into())
        );
        assert_eq!(
            moonlight_address_ip(Some(ip), Some("old-lan:80")),
            Some("10.42.0.2:80".into())
        );
        assert_eq!(
            moonlight_address_ip(Some("fd00::2".parse().unwrap()), Some("old-lan")),
            Some("[fd00::2]:47989".into())
        );
        assert_eq!(
            moonlight_address_ip(None, Some("old-lan")),
            Some("old-lan".into())
        );
        assert_eq!(moonlight_address_ip(None, None), None);
    }
    #[test]
    fn nearby_candidate_is_first_only_while_the_link_is_present() {
        let original = vec!["http://192.168.1.2:39217".into()];
        let mut candidates = original.clone();
        prepend_fresh(None, Vec::new(), &mut candidates);
        assert_eq!(candidates, original);
        prepend_fresh(
            Some("10.42.0.2".parse().unwrap()),
            Vec::new(),
            &mut candidates,
        );
        assert_eq!(
            candidates,
            ["http://10.42.0.2:39217", "http://192.168.1.2:39217"]
        );
        let mut empty = Vec::new();
        prepend_fresh(Some("10.42.0.2".parse().unwrap()), Vec::new(), &mut empty);
        assert!(empty.is_empty());
    }
    /// ADR 0089 try order: the nearby link, then local discovery, then the
    /// stored record's candidates, each address once.
    #[test]
    fn fresh_addresses_precede_the_stored_record() {
        let mut candidates = vec![
            "http://192.168.1.2:39217".into(),
            "http://100.64.0.2:39217".into(),
        ];
        prepend_fresh(
            Some("10.42.0.2".parse().unwrap()),
            vec![
                "http://192.168.1.30:39217".into(),
                "http://100.64.0.2:39217".into(),
            ],
            &mut candidates,
        );
        assert_eq!(
            candidates,
            [
                "http://10.42.0.2:39217",
                "http://192.168.1.30:39217",
                "http://100.64.0.2:39217",
                "http://192.168.1.2:39217",
            ]
        );
        // Found origins carry their own port, so they help a peer with no record.
        let mut none = Vec::new();
        prepend_fresh(None, vec!["http://192.168.1.30:39217".into()], &mut none);
        assert_eq!(none, ["http://192.168.1.30:39217"]);
    }
}
