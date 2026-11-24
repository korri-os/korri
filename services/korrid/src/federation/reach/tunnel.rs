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
    } else {
        buffers(socket2::SockRef::from(&socket));
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
/// Socket buffers for every local stream UDP socket. Sunshine sends a video
/// frame of up to about 160 packets of 1,200 bytes in about 0.14 ms. The
/// 212,992-byte Linux default holds about 90 of them, so the kernel dropped
/// the rest of a large frame before korrid could read it (#600). Linux caps
/// the request at `net.core.rmem_max`, 4 MiB on Korri devices; a lower cap
/// gives a smaller buffer, not an error.
const UDP_BUFFER: usize = 4 << 20;
fn buffers(socket: socket2::SockRef<'_>) {
    let _ = socket.set_recv_buffer_size(UDP_BUFFER);
    let _ = socket.set_send_buffer_size(UDP_BUFFER);
}
/// Stream packets the viewer task lost because it fell behind the responses
/// channel. Logged at most every 10 seconds, so a burst of loss is one line.
#[derive(Default)]
struct Lost {
    unreported: u64,
    reported: Option<std::time::Instant>,
}
impl Lost {
    fn add(&mut self, packets: u64) {
        self.unreported += packets;
        if self
            .reported
            .is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(10))
        {
            eprintln!(
                "korrid: reach: stream viewer fell behind; lost_packets={}",
                self.unreported
            );
            self.unreported = 0;
            self.reported = Some(std::time::Instant::now());
        }
    }
}
impl Forwards {
    pub async fn open(
        connection: Connection,
        bindings: Bindings,
        mut responses: tokio::sync::broadcast::Receiver<(u16, Vec<u8>)>,
    ) -> io::Result<Self> {
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
        // This side's requests; the connection's serve task counts replies (#601).
        let counters = Arc::new(datagrams::Counters::default());
        let report = Arc::new(datagrams::Report {
            side: "viewer",
            connection: connection.clone(),
            counters: counters.clone(),
        });
        for (port, socket) in &udp {
            let (port, socket, c, clients, counters, report) = (
                *port,
                socket.clone(),
                connection.clone(),
                clients.clone(),
                counters.clone(),
                report.clone(),
            );
            tasks.spawn(async move {
                let _report = report;
                // Read the whole IPv6 packet before enforcing the transport's
                // 65,507-byte bound. Never forward a truncated packet.
                let mut bytes = vec![0; 65535];
                loop {
                    tokio::select! {
                        result = socket.recv_from(&mut bytes) => {
                            let Ok((n,src)) = result else {break};
                            clients.lock().expect("stream clients").insert(port,src);
                            let _ = datagrams::send(&c,&counters,port,&bytes[..n], false);
                        }
                        _ = c.closed() => break,
                    }
                }
            });
        }
        tasks.spawn(async move {
            let mut lost = Lost::default();
            loop {
                let (port, payload) = tokio::select! {
                    packet = responses.recv() => match packet {
                        Ok(packet) => packet,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            lost.add(n);
                            continue;
                        }
                        Err(_) => break,
                    },
                    _ = connection.closed() => break,
                };
                let client = clients.lock().expect("stream clients").get(&port).copied();
                if let (Some(client), Some((_, socket))) =
                    (client, udp.iter().find(|(p, _)| *p == port))
                {
                    let _ = socket.send_to(&payload, client).await;
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

pub(super) type Responses = tokio::sync::broadcast::Sender<(u16, Vec<u8>)>;
/// Packets the responses channel holds for a viewer that has not read them.
/// Sunshine sends a whole video frame, up to about 160 packets, in about
/// 0.14 ms, faster than the viewer task writes them to its local socket. A
/// full channel overwrites its oldest packets, so the old capacity of 64 cut
/// the start off every frame of more than about 64 packets (#600). 4,096
/// slots hold many frames; the channel allocates them once per connection.
pub(super) const RESPONSES: usize = 4096;
/// Make the channel that carries one connection's stream replies to its viewer.
pub(super) fn responses() -> Responses {
    tokio::sync::broadcast::channel(RESPONSES).0
}
/// Each side serves streams and demultiplexes UDP requests and replies exactly once.
pub(super) async fn serve(
    connection: Connection,
    router: axum::Router,
    responses: Responses,
    binding: Option<super::Binding>,
) {
    serve_on(connection, router, Targets::default(), responses, binding).await;
}
async fn serve_on(
    connection: Connection,
    router: axum::Router,
    targets: Targets,
    responses: Responses,
    binding: Option<super::Binding>,
) {
    let mut tasks = JoinSet::new();
    let c = connection.clone();
    let udp_targets = targets.clone();
    let udp_binding = binding.clone();
    tasks.spawn(async move {
        serve_udp(c, udp_targets, responses, udp_binding).await;
    });
    loop {
        tokio::select! {
            result = connection.accept_bi(), if tasks.len() < 36 => {
                let Ok((mut send,mut recv)) = result else {break};
                let router = router.clone();let targets = targets.clone(); let binding = binding.clone();
                tasks.spawn(async move {
                    let Ok(kind) = recv.read_u8().await else {return};
                    if kind != 0 && binding.as_ref().is_some_and(|binding| !binding.allows_streams()) { return; }
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
async fn serve_udp(
    connection: Connection,
    targets: Targets,
    responses: Responses,
    binding: Option<super::Binding>,
) {
    let mut tasks = JoinSet::new();
    let mut sockets = BTreeMap::new();
    let counters = Arc::new(datagrams::Counters::default());
    for (port, target) in UDP_PORTS.into_iter().zip(targets.udp) {
        let Ok(socket) = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await else {
            return;
        };
        buffers(socket2::SockRef::from(&socket));
        if socket.connect((targets.address, target)).await.is_err() {
            return;
        }
        let socket = Arc::new(socket);
        sockets.insert(port, socket.clone());
        let (c, counters) = (connection.clone(), counters.clone());
        tasks.spawn(async move {
            let mut bytes = vec![0; 65535];
            loop {
                tokio::select! {
                    result = socket.recv(&mut bytes) => {
                        if let Ok(n) = result {let _ = datagrams::send(&c,&counters,port,&bytes[..n], true);}
                        else {tokio::time::sleep(std::time::Duration::from_millis(50)).await;}
                    }
                    _ = c.closed() => break,
                }
            }
        });
    }
    let _report = datagrams::Report {
        side: "serve",
        connection: connection.clone(),
        counters: counters.clone(),
    };
    let mut receiver = datagrams::Receiver::new(counters);
    while let Ok(bytes) = connection.read_datagram().await {
        if binding
            .as_ref()
            .is_some_and(|binding| !binding.allows_streams())
        {
            continue;
        }
        if let Some((port, payload)) = receiver.push(&bytes) {
            if bytes[0] == 2 {
                let _ = responses.send((port, payload));
            } else if let Some(socket) = sockets.get(&port) {
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
        let task = tokio::spawn(serve_on(
            incoming,
            axum::Router::new(),
            targets,
            responses(),
            None,
        ));
        let responses = responses();
        let local = tokio::spawn(serve(
            outgoing.clone(),
            axum::Router::new(),
            responses.clone(),
            None,
        ));
        let forwards = Forwards::open(
            outgoing,
            Bindings {
                address: Ipv4Addr::LOCALHOST.into(),
                tcp: [0; 3],
                udp: [0; 5],
            },
            responses.subscribe(),
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
        local.abort();
        task.abort();
        a.close().await;
        b.close().await;
    }
    /// #600: every local stream UDP socket asks for the large buffer.
    #[tokio::test]
    async fn reach_stream_udp_sockets_get_large_buffers() {
        let Some(limit) = std::fs::read_to_string("/proc/sys/net/core/rmem_max")
            .ok()
            .and_then(|s| s.trim().parse::<usize>().ok())
        else {
            return;
        };
        let socket = socket(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0), false).unwrap();
        // Linux grants at most rmem_max and reports twice what it granted.
        assert_eq!(
            socket.recv_buffer_size().unwrap(),
            2 * UDP_BUFFER.min(limit)
        );
    }
    /// #600: a viewer that has read nothing yet still gets every packet of
    /// the largest measured Sunshine frame, 158 packets, in order.
    #[tokio::test]
    async fn reach_responses_hold_a_whole_frame_burst() {
        let sender = responses();
        let mut viewer = sender.subscribe();
        for packet in 0..160u16 {
            sender.send((47998, packet.to_be_bytes().to_vec())).unwrap();
        }
        for packet in 0..160u16 {
            assert_eq!(
                viewer.recv().await.unwrap(),
                (47998, packet.to_be_bytes().to_vec())
            );
        }
    }
    /// Measurement probe for #600: Sunshine-sized frame bursts through the
    /// real forwards over a loopback iroh pair. Timing-dependent, so ignored.
    /// Run `nix run .#korrid-test -- --release --lib reach_burst_probe --
    /// --ignored --nocapture`, optionally with `BURSTS=60,160`. On zao, with
    /// the 64-slot responses channel 160-packet bursts lost 7,962 of 16,000
    /// packets and 80-packet bursts 967 of 8,000; with 4,096 slots, 0 and 0.
    /// Default socket buffers alone cost 179 of 16,000 at 160 packets.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore]
    async fn reach_burst_probe() {
        let (a, b, outgoing, incoming) = crate::federation::reach::tests::pair().await;
        let sunshine = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut targets = Targets::default();
        targets.udp[0] = sunshine.local_addr().unwrap().port();
        let task = tokio::spawn(serve_on(
            incoming,
            axum::Router::new(),
            targets,
            responses(),
            None,
        ));
        let responses = responses();
        let local = tokio::spawn(serve(
            outgoing.clone(),
            axum::Router::new(),
            responses.clone(),
            None,
        ));
        let forwards = Forwards::open(
            outgoing,
            Bindings {
                address: Ipv4Addr::LOCALHOST.into(),
                tcp: [0; 3],
                udp: [0; 5],
            },
            responses.subscribe(),
        )
        .await
        .unwrap();
        let viewer = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        socket2::SockRef::from(&*viewer)
            .set_recv_buffer_size(32 << 20)
            .unwrap();
        viewer.send_to(b"ping", forwards.udp[0]).await.unwrap();
        let mut bytes = vec![0; 2048];
        let (_, relay) = sunshine.recv_from(&mut bytes).await.unwrap();
        let sizes: Vec<usize> = std::env::var("BURSTS")
            .unwrap_or("40,60,80,100,120,160".into())
            .split(',')
            .map(|s| s.parse().unwrap())
            .collect();
        let counter = viewer.clone();
        let got = tokio::spawn(async move {
            let mut seen = std::collections::HashSet::new();
            let mut bytes = vec![0; 2048];
            while let Ok(Ok((_, _))) =
                tokio::time::timeout(Duration::from_secs(2), counter.recv_from(&mut bytes)).await
            {
                seen.insert(u64::from_be_bytes(bytes[..8].try_into().unwrap()));
            }
            seen
        });
        let mut sent = Vec::new();
        let mut tick = tokio::time::interval(Duration::from_micros(16_667));
        for frame in 0..600usize {
            tick.tick().await;
            let size = sizes[frame % sizes.len()];
            for packet in 0..size {
                let mut bytes = vec![0u8; 1200];
                let id = ((frame as u64) << 16) | packet as u64;
                bytes[..8].copy_from_slice(&id.to_be_bytes());
                sunshine.send_to(&bytes, relay).await.unwrap();
                sent.push((size, id));
            }
        }
        let seen = got.await.unwrap();
        for size in &sizes {
            let total = sent.iter().filter(|(s, _)| s == size).count();
            let lost = sent
                .iter()
                .filter(|(s, id)| s == size && !seen.contains(id))
                .count();
            eprintln!("burst={size} lost={lost}/{total}");
        }
        forwards.shutdown().await;
        task.abort();
        local.abort();
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

/// This ephemeral loopback listener carries netplay on the peer connection.
/// Its lease owns every copy task.
pub(crate) struct NetplayForward {
    connection: Connection,
    disconnected: Arc<std::sync::atomic::AtomicBool>,
    connected: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) address: SocketAddr,
    tasks: JoinSet<()>,
}
impl NetplayForward {
    pub(crate) fn connection(&self) -> &Connection {
        &self.connection
    }
    pub(crate) fn ready(&self) -> bool {
        self.has_connected() && !self.lost()
    }
    /// A fresh client must run long enough to open its local TCP connection.
    pub(crate) fn has_connected(&self) -> bool {
        self.connected.load(std::sync::atomic::Ordering::Acquire)
    }
    pub(crate) fn connection_gone(&self) -> bool {
        self.connection.close_reason().is_some()
    }
    pub(crate) fn lost(&self) -> bool {
        self.connection_gone() || self.disconnected.load(std::sync::atomic::Ordering::Acquire)
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
            _ = serve_on(connection, axum::Router::new(), targets, responses(), None) => {},
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
        let server = tokio::spawn(serve_on(
            incoming,
            axum::Router::new(),
            targets,
            responses(),
            None,
        ));
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
