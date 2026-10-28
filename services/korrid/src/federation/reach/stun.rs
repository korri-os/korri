//! RFC 8489 binding on the UDP port that iroh will bind. Iroh does not expose
//! its socket. A network change therefore closes the endpoint and repeats this.
use super::diagnostics::{mapped_address, server_label, Diagnostics};
use std::{
    io,
    net::{Ipv4Addr, SocketAddr},
    time::{Duration, Instant},
};
use tokio::{net::UdpSocket, time::timeout};

pub(super) async fn discover(
    servers: &[String],
    diagnostics: &Diagnostics,
) -> io::Result<(u16, Vec<SocketAddr>)> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let port = socket.local_addr()?.port();
    let mut found = Vec::new();
    for server in servers {
        let started = Instant::now();
        let result = probe_server(&socket, server).await;
        let server_ms = started.elapsed().as_millis();
        let server = server_label(server);
        match result {
            Ok(mapped) => {
                eprintln!("korrid: reach: {} stage=stun_result server={server} mapped={} server_ms={server_ms}", diagnostics.stamp(), mapped_address(mapped));
                if !found.contains(&mapped) { found.push(mapped); }
            }
            Err(error) => eprintln!("korrid: reach: {} stage=stun_result server={server} error={error} server_ms={server_ms}", diagnostics.stamp()),
        }
    }
    Ok((port, found))
}

async fn probe_server(socket: &UdpSocket, server: &str) -> Result<SocketAddr, &'static str> {
    let address = match timeout(Duration::from_secs(1), tokio::net::lookup_host(server)).await {
        Ok(Ok(mut addresses)) => addresses.find(SocketAddr::is_ipv4).ok_or("no_ipv4")?,
        Ok(Err(_)) => return Err("dns_failed"),
        Err(_) => return Err("dns_timeout"),
    };
    let transaction: [u8; 12] = rand::random();
    let mut request = vec![0, 1, 0, 0, 0x21, 0x12, 0xa4, 0x42];
    request.extend_from_slice(&transaction);
    socket
        .send_to(&request, address)
        .await
        .map_err(|_| "send_failed")?;
    let mut bytes = [0u8; 2048];
    let (n, source) = match timeout(Duration::from_secs(1), socket.recv_from(&mut bytes)).await {
        Ok(Ok(received)) => received,
        Ok(Err(_)) => return Err("receive_failed"),
        Err(_) => return Err("receive_timeout"),
    };
    if source != address {
        return Err("unexpected_source");
    }
    decode(&bytes[..n], transaction).ok_or("invalid_response")
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
        let (port, addresses) = discover(&[address.to_string()], &Diagnostics::default())
            .await
            .unwrap();
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

    #[tokio::test]
    async fn reach_stun_classifies_invalid_responses_without_returning_their_contents() {
        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = server.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let mut request = [0; 128];
            let (_, source) = server.recv_from(&mut request).await.unwrap();
            server
                .send_to(b"private invalid server response", source)
                .await
                .unwrap();
        });
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        assert_eq!(
            probe_server(&socket, &address.to_string()).await,
            Err("invalid_response")
        );
        task.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn reach_stun_classifies_a_receive_timeout_and_an_ipv6_only_server() {
        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        assert_eq!(
            probe_server(&socket, &server.local_addr().unwrap().to_string()).await,
            Err("receive_timeout")
        );
        assert_eq!(probe_server(&socket, "[::1]:3478").await, Err("no_ipv4"));
    }
}
