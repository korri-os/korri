//! URL admission is repeated before every request, including DNS results.
use std::net::IpAddr;
use url::Url;

use super::Failure;

#[derive(Clone, Copy)]
pub(super) enum AddressPolicy {
    Public,
    #[cfg(test)]
    AllowLoopback,
}

impl AddressPolicy {
    pub(super) fn allows(self, ip: IpAddr) -> bool {
        #[cfg(test)]
        if matches!(self, Self::AllowLoopback) && ip.is_loopback() {
            return true;
        }
        public_address(ip)
    }
}

pub(super) fn public_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, _, _] = ip.octets();
            !matches!(a, 0 | 10 | 127)
                && !(a == 100 && (64..=127).contains(&b))
                && !(a == 169 && b == 254)
                && !(a == 172 && (16..=31).contains(&b))
                && !(a == 192 && b == 168)
        }
        IpAddr::V6(ip) => {
            let s = ip.segments();
            !ip.is_loopback()
                && !ip.is_unspecified()
                && s[0] & 0xffc0 != 0xfe80
                && s[0] & 0xfe00 != 0xfc00
                && s[..6] != [0x64, 0xff9b, 0, 0, 0, 0]
                && s[..6] != [0, 0, 0, 0, 0, 0xffff]
        }
    }
}

pub(super) fn validate(url: &Url, addresses: AddressPolicy) -> Result<(), Failure> {
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(Failure::UnsafeUrl);
    }
    match url.host().ok_or(Failure::UnsafeUrl)? {
        url::Host::Domain(host)
            if host.eq_ignore_ascii_case("localhost")
                || host.eq_ignore_ascii_case("localhost.") =>
        {
            Err(Failure::UnsafeUrl)
        }
        url::Host::Ipv4(ip) if !addresses.allows(ip.into()) => Err(Failure::UnsafeUrl),
        url::Host::Ipv6(ip) if !addresses.allows(ip.into()) => Err(Failure::UnsafeUrl),
        _ => Ok(()),
    }
}

pub(super) fn redirect(
    from: &Url,
    location: &str,
    hops: usize,
    addresses: AddressPolicy,
) -> Result<Url, Failure> {
    if hops >= 5 {
        return Err(Failure::Redirect);
    }
    let to = from.join(location).map_err(|_| Failure::UnsafeUrl)?;
    validate(from, addresses)?;
    validate(&to, addresses)?;
    if from.scheme() == "https" && to.scheme() != "https" {
        return Err(Failure::UnsafeUrl);
    }
    Ok(to)
}

/// Names come from the declared URL, not a server-controlled redirect or header.
pub(super) fn basename(value: &str) -> Result<String, Failure> {
    let url = Url::parse(value).map_err(|_| Failure::UnsafeUrl)?;
    let encoded = url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .ok_or(Failure::UnsafePath)?;
    let mut bytes = Vec::new();
    let mut input = encoded.bytes();
    while let Some(byte) = input.next() {
        bytes.push(if byte == b'%' {
            let a = input
                .next()
                .and_then(|b| (b as char).to_digit(16))
                .ok_or(Failure::UnsafePath)?;
            let b = input
                .next()
                .and_then(|b| (b as char).to_digit(16))
                .ok_or(Failure::UnsafePath)?;
            (a * 16 + b) as u8
        } else {
            byte
        });
    }
    let name = String::from_utf8(bytes).map_err(|_| Failure::UnsafePath)?;
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.chars().any(char::is_control)
    {
        return Err(Failure::UnsafePath);
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_public_url_policy() {
        for value in [
            "http://example.com/game",
            "https://example.com/game",
            "https://8.8.8.8/game",
            "https://[2606:4700:4700::1111]/game",
        ] {
            assert!(
                validate(&Url::parse(value).unwrap(), AddressPolicy::Public).is_ok(),
                "{value}"
            );
        }
        for host in [
            "localhost",
            "localhost.",
            "127.0.0.1",
            "0.1.2.3",
            "10.1.2.3",
            "100.64.0.1",
            "100.127.255.255",
            "169.254.1.2",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.0.1",
            "[::1]",
            "[::]",
            "[fe80::1]",
            "[febf::1]",
            "[fc00::1]",
            "[fdff::1]",
            "[64:ff9b::808:808]",
            "[::ffff:127.0.0.1]",
            "[::ffff:10.0.0.1]",
            "[::ffff:8.8.8.8]",
        ] {
            assert!(
                validate(
                    &Url::parse(&format!("https://{host}/file")).unwrap(),
                    AddressPolicy::Public
                )
                .is_err(),
                "{host}"
            );
        }
        for value in [
            "file:///tmp/game.zip",
            "ftp://example.com/file",
            "https://user:pass@example.com/file",
        ] {
            assert!(validate(&Url::parse(value).unwrap(), AddressPolicy::Public).is_err());
        }
    }
    #[test]
    fn redirects_revalidate_every_hop_and_refuse_downgrade_or_sixth_hop() {
        let from = Url::parse("https://example.com/start").unwrap();
        assert_eq!(
            redirect(&from, "/next", 4, AddressPolicy::Public)
                .unwrap()
                .as_str(),
            "https://example.com/next"
        );
        assert!(redirect(&from, "/next", 5, AddressPolicy::Public).is_err());
        assert!(redirect(&from, "http://example.com/file", 0, AddressPolicy::Public).is_err());
        assert!(redirect(&from, "https://127.0.0.1/file", 0, AddressPolicy::Public).is_err());
    }
    #[test]
    fn loopback_admission_preserves_redirect_limits_and_security_checks() {
        let from = Url::parse("https://127.0.0.1/start").unwrap();
        assert!(redirect(&from, "/next", 4, AddressPolicy::AllowLoopback).is_ok());
        assert!(redirect(&from, "/next", 5, AddressPolicy::AllowLoopback).is_err());
        for location in [
            "http://127.0.0.1/next",
            "https://user:pass@127.0.0.1/next",
            "https://10.0.0.1/next",
            "file:///tmp/file",
        ] {
            assert!(
                redirect(&from, location, 0, AddressPolicy::AllowLoopback).is_err(),
                "{location}"
            );
        }
        assert!(redirect(&from, "/next", 0, AddressPolicy::Public).is_err());
    }

    #[test]
    fn decoded_basenames_cannot_escape_the_work_directory() {
        assert_eq!(
            basename("https://example.com/a%20b.tar?token=secret").unwrap(),
            "a b.tar"
        );
        for name in ["", "%2fetc%2fpasswd", "%2e%2e", "bad%00file", "%ff"] {
            assert!(basename(&format!("https://example.com/{name}")).is_err());
        }
    }
}
