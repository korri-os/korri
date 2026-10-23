//! RFC 8489 binding on the UDP port that iroh will bind. Iroh does not expose
//! its socket. A network change therefore closes the endpoint and repeats this.
use std::{
    io,
    net::{Ipv4Addr, SocketAddr},
    time::Duration,
};
use tokio::{net::UdpSocket, time::timeout};

pub(super) async fn discover(servers: &[String]) -> io::Result<(u16, Vec<SocketAddr>)> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let port = socket.local_addr()?.port();
    let mut found = Vec::new();
    for server in servers {
        let address = match timeout(Duration::from_secs(1), tokio::net::lookup_host(server)).await {
            Ok(Ok(mut addresses)) => addresses.find(SocketAddr::is_ipv4),
            _ => None,
        };
        let Some(address) = address else { continue };
        let transaction: [u8; 12] = rand::random();
        let mut request = vec![0, 1, 0, 0, 0x21, 0x12, 0xa4, 0x42];
        request.extend_from_slice(&transaction);
        if socket.send_to(&request, address).await.is_err() {
            continue;
        }
        let mut bytes = [0u8; 2048];
        if let Ok(Ok((n, source))) =
            timeout(Duration::from_secs(1), socket.recv_from(&mut bytes)).await
        {
            if source == address {
                if let Some(mapped) = decode(&bytes[..n], transaction) {
                    if !found.contains(&mapped) {
                        found.push(mapped);
                    }
                }
            }
        }
    }
    Ok((port, found))
}

fn decode(bytes: &[u8], transaction: [u8; 12]) -> Option<SocketAddr> {
    if bytes.len() < 20
        || bytes[..2] != [1, 1]
        || bytes[4..8] != [0x21, 0x12, 0xa4, 0x42]
        || bytes[8..20] != transaction
    {
        return None;
    }
    let length = u16::from_be_bytes(bytes[2..4].try_into().ok()?) as usize;
    if !length.is_multiple_of(4) || length + 20 != bytes.len() {
        return None;
    }
    let mut at = 20;
    while at + 4 <= bytes.len() {
        let kind = u16::from_be_bytes(bytes[at..at + 2].try_into().ok()?);
        let len = u16::from_be_bytes(bytes[at + 2..at + 4].try_into().ok()?) as usize;
        at += 4;
        let body = bytes.get(at..at + len)?;
        if kind == 0x20 && len == 8 && body[1] == 1 {
            let port = u16::from_be_bytes(body[2..4].try_into().ok()?) ^ 0x2112;
            let ip = Ipv4Addr::new(
                body[4] ^ 0x21,
                body[5] ^ 0x12,
                body[6] ^ 0xa4,
                body[7] ^ 0x42,
            );
            if port == 0 || ip.is_unspecified() || ip.is_multicast() {
                return None;
            }
            return Some(SocketAddr::new(ip.into(), port));
        }
        at += (len + 3) & !3;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn reach_stun_uses_the_port_then_iroh_can_rebind_it() {
        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = server.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let mut bytes = [0u8; 128];
            let (_, source) = server.recv_from(&mut bytes).await.unwrap();
            let mut response = vec![1, 1, 0, 12, 0x21, 0x12, 0xa4, 0x42];
            response.extend_from_slice(&bytes[8..20]);
            response.extend_from_slice(&[0, 0x20, 0, 8, 0, 1]);
            response.extend_from_slice(&(source.port() ^ 0x2112).to_be_bytes());
            response.extend_from_slice(&[127 ^ 0x21, 0x12, 0xa4, 1 ^ 0x42]);
            server.send_to(&response, source).await.unwrap();
        });
        let (port, addresses) = discover(&[address.to_string()]).await.unwrap();
        assert_eq!(addresses, [SocketAddr::from(([127, 0, 0, 1], port))]);
        let endpoint = iroh::endpoint::Builder::empty()
            .crypto_provider(iroh::tls::default_provider())
            .portmapper_config(iroh::endpoint::PortmapperConfig::Disabled)
            .bind_addr(SocketAddr::from(([127, 0, 0, 1], port)))
            .unwrap()
            .bind()
            .await
            .unwrap();
        endpoint.add_external_addr(addresses[0]).await;
        endpoint.close().await;
        task.await.unwrap();
    }
}
