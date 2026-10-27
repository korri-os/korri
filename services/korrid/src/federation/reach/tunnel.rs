use super::{datagrams, http};
use iroh::endpoint::Connection;
use std::{
    collections::BTreeMap,
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{Arc, Mutex},
};
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream, UdpSocket},
    task::JoinSet,
};

pub(super) const TCP_PORTS: [u16; 3] = [47984, 47989, 48010];
pub(super) const UDP_PORTS: [u16; 5] = [47998, 47999, 48000, 48002, 48010];
#[derive(Clone)]
pub(super) struct Bindings {
    pub address: IpAddr,
    pub tcp: [u16; 3],
    pub udp: [u16; 5],
}
impl Default for Bindings {
    fn default() -> Self {
        Self {
            address: Ipv6Addr::LOCALHOST.into(),
            tcp: TCP_PORTS,
            udp: UDP_PORTS,
        }
    }
}
#[derive(Clone)]
struct Targets {
    netplay: u16,
    address: IpAddr,
    tcp: [u16; 3],
    udp: [u16; 5],
}
impl Default for Targets {
    fn default() -> Self {
        Self {
            netplay: 55435,
            address: Ipv4Addr::LOCALHOST.into(),
            tcp: TCP_PORTS,
            udp: UDP_PORTS,
        }
    }
}
pub(super) struct Forwards {
    pub address: String,
    tasks: JoinSet<()>,
    #[cfg(test)]
    pub tcp: Vec<SocketAddr>,
    #[cfg(test)]
    pub udp: Vec<SocketAddr>,
}
fn socket(address: SocketAddr, tcp: bool) -> io::Result<socket2::Socket> {
    let socket = socket2::Socket::new(
        socket2::Domain::for_address(address),
        if tcp {
            socket2::Type::STREAM
        } else {
            socket2::Type::DGRAM
        },
        None,
    )?;
    if address.is_ipv6() {
        socket.set_only_v6(true)?;
    }
    if tcp {
        socket.set_reuse_address(true)?;
    }
    socket.set_nonblocking(true)?;
    socket.bind(&address.into()).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("Can't open stream forward on {address}: {e}"),
        )
    })?;
    Ok(socket)
}
impl Forwards {
    pub async fn open(connection: Connection, bindings: Bindings) -> io::Result<Self> {
        let mut tcp = Vec::new();
        let mut udp = Vec::new();
        for (logical, port) in TCP_PORTS.into_iter().zip(bindings.tcp) {
            let socket = socket(SocketAddr::new(bindings.address, port), true)?;
            socket.listen(32)?;
            tcp.push((logical, TcpListener::from_std(socket.into())?));
        }
        for (logical, port) in UDP_PORTS.into_iter().zip(bindings.udp) {
            let socket = socket(SocketAddr::new(bindings.address, port), false)?;
            udp.push((logical, Arc::new(UdpSocket::from_std(socket.into())?)));
        }
        let address = tcp[1].1.local_addr()?.to_string();
        #[cfg(test)]
        let tcp_addresses = tcp.iter().map(|(_, s)| s.local_addr().unwrap()).collect();
        #[cfg(test)]
        let udp_addresses = udp.iter().map(|(_, s)| s.local_addr().unwrap()).collect();
        let mut tasks = JoinSet::new();
        for (port, listener) in tcp {
            let c = connection.clone();
            tasks.spawn(async move {
                let mut copies = JoinSet::new();
                loop {tokio::select! {
                    result = listener.accept(), if copies.len() < 32 => {
                        let Ok((mut tcp,_)) = result else {break};
                        let c = c.clone();
                        copies.spawn(async move {
                            let Ok((mut send,recv)) = c.open_bi().await else {return};
                            if send.write_all(&[1,(port>>8) as u8,port as u8]).await.is_err() {return;}
                            let _ = tokio::io::copy_bidirectional(&mut tcp,&mut tokio::io::join(recv,send)).await;
                        });
                    }
                    _ = copies.join_next(), if !copies.is_empty() => {}
                    _ = c.closed() => break,
                }}
            });
        }
        let clients = Arc::new(Mutex::new(BTreeMap::<u16, SocketAddr>::new()));
        for (port, socket) in &udp {
            let (port, socket, c, clients) =
                (*port, socket.clone(), connection.clone(), clients.clone());
            tasks.spawn(async move {
                // Read the whole IPv6 packet before enforcing the transport's
                // 65,507-byte bound. Never forward a truncated packet.
                let mut bytes = vec![0; 65535];
                loop {
                    tokio::select! {
                        result = socket.recv_from(&mut bytes) => {
                            let Ok((n,src)) = result else {break};
                            clients.lock().expect("stream clients").insert(port,src);
                            let _ = datagrams::send(&c,port,&bytes[..n]);
                        }
                        _ = c.closed() => break,
                    }
                }
            });
        }
        tasks.spawn(async move {
            let mut receiver = datagrams::Receiver::default();
            while let Ok(bytes) = connection.read_datagram().await {
                if let Some((port, payload)) = receiver.push(&bytes) {
                    let client = clients.lock().expect("stream clients").get(&port).copied();
                    if let (Some(client), Some((_, socket))) =
                        (client, udp.iter().find(|(p, _)| *p == port))
                    {
                        let _ = socket.send_to(&payload, client).await;
                    }
                }
            }
        });
        Ok(Self {
            address,
            tasks,
            #[cfg(test)]
            tcp: tcp_addresses,
            #[cfg(test)]
            udp: udp_addresses,
        })
    }
    pub fn abort(&mut self) {
        self.tasks.abort_all();
    }
    pub async fn shutdown(mut self) {
        self.tasks.shutdown().await;
    }
}

/// The accepted connection is the Sunshine side. Both devices dial after a
/// signal, so either can view the other without a direction field in a packet.
pub(super) async fn serve(connection: Connection, router: axum::Router) {
    serve_on(connection, router, Targets::default()).await;
}
async fn serve_on(connection: Connection, router: axum::Router, targets: Targets) {
    let mut tasks = JoinSet::new();
    let c = connection.clone();
    let udp_targets = targets.clone();
    tasks.spawn(async move {
        serve_udp(c, udp_targets).await;
    });
    loop {
        tokio::select! {
            result = connection.accept_bi(), if tasks.len() < 36 => {
                let Ok((mut send,mut recv)) = result else {break};
                let router = router.clone();let targets = targets.clone();
                tasks.spawn(async move {
                    let Ok(kind) = recv.read_u8().await else {return};
                    match kind {
                        0 => http::serve(send,recv,router).await,
                        1 => {
                            let Ok(port) = recv.read_u16().await else {return};
                            let Some(index) = TCP_PORTS.iter().position(|p| *p == port) else {return};
                            if let Ok(mut tcp) = TcpStream::connect((targets.address,targets.tcp[index])).await {
                                let _ = tokio::io::copy_bidirectional(&mut tcp,&mut tokio::io::join(recv,send)).await;
                            }
                        }
                        2 => {
                            match TcpStream::connect((Ipv4Addr::LOCALHOST, targets.netplay)).await {
                                Ok(mut tcp) => {
                                    if send.write_all(&[1]).await.is_ok() {
                                        let _ = tokio::io::copy_bidirectional(&mut tcp, &mut tokio::io::join(recv, send)).await;
                                    }
                                }
                                Err(_) => { let _ = send.write_all(&[0]).await; }
                            }
                        }
                        _ => {}
                    }
                });
            }
            _ = tasks.join_next(), if !tasks.is_empty() => {}
            _ = connection.closed() => break,
        }
    }
}
async fn serve_udp(connection: Connection, targets: Targets) {
    let mut tasks = JoinSet::new();
    let mut sockets = BTreeMap::new();
    for (port, target) in UDP_PORTS.into_iter().zip(targets.udp) {
        let Ok(socket) = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await else {
            return;
        };
        if socket.connect((targets.address, target)).await.is_err() {
            return;
        }
        let socket = Arc::new(socket);
        sockets.insert(port, socket.clone());
        let c = connection.clone();
        tasks.spawn(async move {
            let mut bytes = vec![0; 65535];
            loop {
                tokio::select! {
                    result = socket.recv(&mut bytes) => {
                        if let Ok(n) = result {let _ = datagrams::send(&c,port,&bytes[..n]);}
                        else {tokio::time::sleep(std::time::Duration::from_millis(50)).await;}
                    }
                    _ = c.closed() => break,
                }
            }
        });
    }
    let mut receiver = datagrams::Receiver::default();
    while let Ok(bytes) = connection.read_datagram().await {
        if let Some((port, payload)) = receiver.push(&bytes) {
            if let Some(socket) = sockets.get(&port) {
                let _ = socket.send(&payload).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;
    #[tokio::test]
    async fn reach_stream_ports_forward_bytes_without_the_admin_port() {
        let (a, b, outgoing, incoming) = crate::federation::reach::tests::pair().await;
        let mut servers = JoinSet::new();
        let mut targets = Targets::default();
        for port in &mut targets.tcp {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            *port = listener.local_addr().unwrap().port();
            servers.spawn(async move {
                let (tcp, _) = listener.accept().await.unwrap();
                let (mut read, mut write) = tcp.into_split();
                let _ = tokio::io::copy(&mut read, &mut write).await;
            });
        }
        for port in &mut targets.udp {
            let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            *port = socket.local_addr().unwrap().port();
            servers.spawn(async move {
                let mut bytes = vec![0; 65507];
                let (n, src) = socket.recv_from(&mut bytes).await.unwrap();
                socket.send_to(&bytes[..n], src).await.unwrap();
            });
        }
        let task = tokio::spawn(serve_on(incoming, axum::Router::new(), targets));
        let forwards = Forwards::open(
            outgoing,
            Bindings {
                address: Ipv4Addr::LOCALHOST.into(),
                tcp: [0; 3],
                udp: [0; 5],
            },
        )
        .await
        .unwrap();
        for address in &forwards.tcp {
            let mut tcp = TcpStream::connect(address).await.unwrap();
            tcp.write_all(b"stream TCP bytes").await.unwrap();
            let mut bytes = [0; 16];
            tokio::time::timeout(Duration::from_secs(5), tcp.read_exact(&mut bytes))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(&bytes, b"stream TCP bytes");
        }
        for address in &forwards.udp {
            let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let payload = vec![42; 1400];
            socket.send_to(&payload, address).await.unwrap();
            let mut bytes = vec![0; 65507];
            let (n, _) = tokio::time::timeout(Duration::from_secs(5), socket.recv_from(&mut bytes))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(&bytes[..n], payload);
        }
        assert!(!TCP_PORTS.contains(&47990));
        assert!(!UDP_PORTS.contains(&47990));
        forwards.shutdown().await;
        task.abort();
        a.close().await;
        b.close().await;
    }
    #[tokio::test]
    async fn reach_ipv6_only_listener_does_not_conflict_with_ipv4_wildcard() {
        let probe = match TcpListener::bind("[::1]:0").await {
            Ok(s) => s,
            Err(_) => return,
        };
        drop(probe);
        let ipv4 = TcpListener::bind("0.0.0.0:0").await.unwrap();
        let port = ipv4.local_addr().unwrap().port();
        let socket = socket(SocketAddr::new(Ipv6Addr::LOCALHOST.into(), port), true).unwrap();
        socket.listen(1).unwrap();
        let v6 = TcpListener::from_std(socket.into()).unwrap();
        assert_eq!(v6.local_addr().unwrap().port(), port);
        assert!(socket2::Socket::from(v6.into_std().unwrap())
            .only_v6()
            .unwrap());
    }
}

/// Netplay never takes the native-LAN shortcut used by a stream viewer.
/// This listener is ephemeral and loopback-only. The lease owns every copy.
pub(crate) struct NetplayForward {
    connection: Connection,
    disconnected: Arc<std::sync::atomic::AtomicBool>,
    connected: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) address: SocketAddr,
    tasks: JoinSet<()>,
}
impl NetplayForward {
    pub(crate) fn ready(&self) -> bool {
        self.has_connected() && !self.lost()
    }
    /// A fresh client must run long enough to open its local TCP connection.
    pub(crate) fn has_connected(&self) -> bool {
        self.connected.load(std::sync::atomic::Ordering::Acquire)
    }
    pub(crate) fn lost(&self) -> bool {
        self.connection.close_reason().is_some()
            || self.disconnected.load(std::sync::atomic::Ordering::Acquire)
    }
    pub(super) async fn open(connection: Connection) -> io::Result<Self> {
        let disconnected = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let lost = disconnected.clone();
        let connected = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ready = connected.clone();
        let observed_connection = connection.clone();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let address = listener.local_addr()?;
        let mut tasks = JoinSet::new();
        tasks.spawn(async move {
            let mut copies = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept(), if copies.len() < 4 => {
                        let Ok((mut tcp, _)) = accepted else { break; };
                        let c = connection.clone();
                        let lost = lost.clone(); let ready = ready.clone();
                        copies.spawn(async move {
                            let transfer = async {
                                let (mut send, mut recv) = c.open_bi().await.map_err(std::io::Error::other)?;
                                send.write_all(&[2]).await?;
                                let accepted = tokio::time::timeout(std::time::Duration::from_secs(2), recv.read_u8()).await.map_err(std::io::Error::other)??;
                                if accepted != 1 { return Err(std::io::Error::from(std::io::ErrorKind::ConnectionRefused)); }
                                ready.store(true, std::sync::atomic::Ordering::Release);
                                tokio::io::copy_bidirectional(&mut tcp, &mut tokio::io::join(recv, send)).await.map(|_| ())
                            }.await;
                            let _ = transfer;
                            lost.store(true, std::sync::atomic::Ordering::Release);
                        });
                    }
                    _ = copies.join_next(), if !copies.is_empty() => {}
                    _ = connection.closed() => break,
                }
            }
        });
        Ok(Self {
            address,
            tasks,
            connection: observed_connection,
            disconnected,
            connected,
        })
    }
}
impl Drop for NetplayForward {
    fn drop(&mut self) {
        self.tasks.abort_all();
    }
}

/// Run the production tunnel handler against a loopback TCP echo endpoint.
#[cfg(test)]
pub(super) async fn netplay_echo(connection: Connection) -> tokio::task::JoinHandle<()> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let targets = Targets {
        netplay: listener.local_addr().unwrap().port(),
        ..Default::default()
    };
    tokio::spawn(async move {
        let echo = async {
            loop {
                let (tcp, _) = listener.accept().await.unwrap();
                let (mut read, mut write) = tcp.into_split();
                let _ = tokio::io::copy(&mut read, &mut write).await;
            }
        };
        tokio::select! {
            _ = serve_on(connection, axum::Router::new(), targets) => {},
            _ = echo => {},
        }
    })
}

#[cfg(test)]
mod netplay_tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    #[tokio::test]
    async fn netplay_forward_carries_tcp_bytes_and_has_only_a_loopback_listener() {
        let (a, b, outgoing, incoming) = crate::federation::reach::tests::pair().await;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let targets = Targets {
            netplay: listener.local_addr().unwrap().port(),
            ..Default::default()
        };
        let echo = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let (mut read, mut write) = tcp.into_split();
            tokio::io::copy(&mut read, &mut write).await.unwrap();
        });
        let server = tokio::spawn(serve_on(incoming, axum::Router::new(), targets));
        let forward = NetplayForward::open(outgoing).await.unwrap();
        assert!(forward.address.ip().is_loopback());
        assert_ne!(forward.address.port(), 55435);
        assert!(!TCP_PORTS.contains(&55355));
        assert!(!forward.ready());
        assert!(!forward.has_connected());
        let mut client = TcpStream::connect(forward.address).await.unwrap();
        client.write_all(b"RetroArch netplay").await.unwrap();
        let mut bytes = [0; 17];
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            client.read_exact(&mut bytes),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(&bytes, b"RetroArch netplay");
        assert!(forward.ready());
        assert!(forward.has_connected());
        drop(client);
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !forward.lost() {
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        assert!(forward.has_connected());
        drop(forward);
        server.abort();
        echo.abort();
        a.close().await;
        b.close().await;
    }
}
