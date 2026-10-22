//! Bounded public HTTP/1 transport. Browser upgrades and streaming save replies
//! remain supported; deadlines cover request headers, not response bodies.
use axum::{extract::ConnectInfo, Extension, Router};
use hyper_util::{
    rt::{TokioIo, TokioTimer},
    service::TowerToHyperService,
};
use std::{io, net::SocketAddr, os::fd::AsRawFd, sync::Arc, time::Duration};
use tokio::{
    net::{TcpListener, TcpSocket},
    sync::Semaphore,
    task::JoinSet,
};

const MAX_REMOTE_CONNECTIONS: usize = 32;
const HEADER_TIMEOUT: Duration = Duration::from_secs(10);

pub fn bind(address: SocketAddr) -> io::Result<TcpListener> {
    let socket = if address.is_ipv6() {
        let socket = TcpSocket::new_v6()?;
        let only_v6: libc::c_int = 0;
        // SAFETY: a valid IPv6 socket and an integer option of the required size.
        let result = unsafe {
            libc::setsockopt(
                socket.as_raw_fd(),
                libc::IPPROTO_IPV6,
                libc::IPV6_V6ONLY,
                (&only_v6 as *const libc::c_int).cast(),
                std::mem::size_of_val(&only_v6) as libc::socklen_t,
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        socket
    } else {
        TcpSocket::new_v4()?
    };
    socket.set_reuseaddr(true)?;
    socket.bind(address)?;
    socket.listen(128)
}

pub async fn serve(listener: TcpListener, router: Router) -> io::Result<()> {
    serve_with_limits(
        listener,
        router,
        MAX_REMOTE_CONNECTIONS,
        HEADER_TIMEOUT,
        true,
    )
    .await
}

async fn serve_with_limits(
    listener: TcpListener,
    router: Router,
    limit: usize,
    header_timeout: Duration,
    exempt_loopback: bool,
) -> io::Result<()> {
    let permits = Arc::new(Semaphore::new(limit));
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            Some(_) = connections.join_next(), if !connections.is_empty() => {},
            accepted = listener.accept() => {
                let (stream, remote) = accepted?;
                let permit = if exempt_loopback && remote.ip().to_canonical().is_loopback() { None } else {
                    match permits.clone().try_acquire_owned() {
                        Ok(permit) => Some(permit),
                        Err(_) => continue,
                    }
                };
                let app = router.clone().layer(Extension(ConnectInfo(remote)));
                connections.spawn(async move {
                    let _permit = permit;
                    // HTTP/1 is the existing native and browser transport. A
                    // bounded header buffer also bounds unauthenticated parsing.
                    let _ = hyper::server::conn::http1::Builder::new()
                        .timer(TokioTimer::new())
                        .header_read_timeout(header_timeout)
                        .max_headers(32)
                        .max_buf_size(16 * 1024)
                        .serve_connection(TokioIo::new(stream), TowerToHyperService::new(app))
                        .with_upgrades().await;
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[tokio::test]
    async fn wildcard_listener_accepts_ipv4_and_ipv6() {
        let listener = bind("[::]:0".parse().unwrap()).unwrap();
        let port = listener.local_addr().unwrap().port();
        for address in [format!("127.0.0.1:{port}"), format!("[::1]:{port}")] {
            let _client = tokio::net::TcpStream::connect(address).await.unwrap();
            listener.accept().await.unwrap();
        }
    }
    #[tokio::test]
    async fn listener_can_restart_while_previous_connections_are_in_time_wait() {
        let listener = bind("[::]:0".parse().unwrap()).unwrap();
        let address = listener.local_addr().unwrap();
        let mut client =
            tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, address.port()))
                .await
                .unwrap();
        let (stream, _) = listener.accept().await.unwrap();
        drop(stream);
        let mut byte = [0];
        assert_eq!(client.read(&mut byte).await.unwrap(), 0);
        drop(client);
        drop(listener);
        let _restarted = bind(address).unwrap();
    }

    #[tokio::test]
    async fn stalled_request_headers_are_closed() {
        let listener = bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(serve_with_limits(
            listener,
            Router::new(),
            1,
            Duration::from_millis(40),
            true,
        ));
        let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
        stream
            .write_all(b"POST /peer-rpc HTTP/1.1\r\n")
            .await
            .unwrap();
        let mut body = Vec::new();
        tokio::time::timeout(Duration::from_secs(1), stream.read_to_end(&mut body))
            .await
            .unwrap()
            .unwrap();
        task.abort();
    }
    #[tokio::test]
    async fn remote_connection_limit_refuses_excess_connections() {
        let listener = bind("0.0.0.0:0".parse().unwrap()).unwrap();
        let port = listener.local_addr().unwrap().port();
        let app = Router::new().route("/", axum::routing::get(|| async { "ok" }));
        let task = tokio::spawn(serve_with_limits(listener, app, 1, HEADER_TIMEOUT, false));
        let mut first = tokio::net::TcpStream::connect(SocketAddr::from(([127, 0, 0, 1], port)))
            .await
            .unwrap();
        first.write_all(b"GET / HTTP/1.1\r\n").await.unwrap();
        // Apply the public limit to loopback too, so this test also runs in a
        // network namespace with no external interface.
        tokio::time::sleep(Duration::from_millis(10)).await;
        let mut excess = tokio::net::TcpStream::connect(SocketAddr::from(([127, 0, 0, 1], port)))
            .await
            .unwrap();
        let mut byte = [0];
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), excess.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        drop(first);
        task.abort();
    }
}
