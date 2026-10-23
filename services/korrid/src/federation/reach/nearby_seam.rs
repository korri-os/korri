//! Read the live nearby-link fact without persisting another address. Pure
//! helpers below keep candidate ordering and port handling injectable in tests.
use std::net::{IpAddr, SocketAddr};
pub fn nearby_peer_ip(device_public_key: &str) -> Option<IpAddr> {
    crate::network::nearby::nearby_peer_ip(device_public_key)
}

pub(crate) fn prepend(device: &str, candidates: &mut Vec<String>) {
    prepend_ip(nearby_peer_ip(device), candidates);
}
fn prepend_ip(ip: Option<IpAddr>, candidates: &mut Vec<String>) {
    let port = candidates
        .iter()
        .find_map(|origin| url::Url::parse(origin).ok()?.port_or_known_default());
    if let (Some(ip), Some(port)) = (ip, port) {
        let origin = format!("http://{}", SocketAddr::new(ip, port));
        candidates.retain(|candidate| candidate != &origin);
        candidates.insert(0, origin);
    }
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
    moonlight_address_ip(nearby_peer_ip(device), current)
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
        prepend_ip(None, &mut candidates);
        assert_eq!(candidates, original);
        prepend_ip(Some("10.42.0.2".parse().unwrap()), &mut candidates);
        assert_eq!(
            candidates,
            ["http://10.42.0.2:39217", "http://192.168.1.2:39217"]
        );
        let mut empty = Vec::new();
        prepend_ip(Some("10.42.0.2".parse().unwrap()), &mut empty);
        assert!(empty.is_empty());
    }
}
