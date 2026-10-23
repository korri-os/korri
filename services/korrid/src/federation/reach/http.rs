//! HTTP/1 over a QUIC stream. There is deliberately no TCP listener or local
//! proxy: a peer must never acquire loopback authority through this transport.
use axum::{
    body::Bytes,
    http::{HeaderMap, StatusCode},
};
use http_body_util::{BodyExt, Full};
use hyper_util::rt::TokioIo;
use iroh::endpoint::Connection;
use std::io;

pub(crate) enum Response {
    Tcp(reqwest::Response),
    Quic(hyper::Response<hyper::body::Incoming>),
}
impl Response {
    pub(crate) fn status(&self) -> StatusCode {
        match self {
            Self::Tcp(r) => r.status(),
            Self::Quic(r) => r.status(),
        }
    }
    pub(crate) fn headers(&self) -> &HeaderMap {
        match self {
            Self::Tcp(r) => r.headers(),
            Self::Quic(r) => r.headers(),
        }
    }
    pub(crate) fn content_length(&self) -> Option<u64> {
        self.headers()
            .get(hyper::header::CONTENT_LENGTH)?
            .to_str()
            .ok()?
            .parse()
            .ok()
    }
    pub(crate) async fn chunk(&mut self) -> io::Result<Option<Bytes>> {
        match self {
            Self::Tcp(r) => r.chunk().await.map_err(io::Error::other),
            Self::Quic(r) => loop {
                match r.body_mut().frame().await {
                    None => return Ok(None),
                    Some(Err(e)) => return Err(io::Error::other(e)),
                    Some(Ok(frame)) => {
                        if let Ok(bytes) = frame.into_data() {
                            return Ok(Some(bytes));
                        }
                    }
                }
            },
        }
    }
}

pub(crate) async fn post(
    connection: &Connection,
    path: &str,
    body: String,
    range: Option<String>,
) -> io::Result<Response> {
    let (mut send, recv) = connection.open_bi().await.map_err(io::Error::other)?;
    send.write_all(&[0]).await.map_err(io::Error::other)?;
    let (mut client, task) =
        hyper::client::conn::http1::handshake(TokioIo::new(tokio::io::join(recv, send)))
            .await
            .map_err(io::Error::other)?;
    // The task owns this stream only. It exits when the caller consumes or
    // drops the body. The endpoint closes all remaining streams at shutdown.
    tokio::spawn(async move {
        let _ = task.await;
    });
    let mut request = hyper::Request::post(path)
        .header(hyper::header::HOST, "peer")
        .header(hyper::header::CONTENT_TYPE, "application/json");
    if let Some(range) = range {
        request = request.header(hyper::header::RANGE, range);
    }
    let request = request
        .body(Full::new(Bytes::from(body)))
        .map_err(io::Error::other)?;
    client
        .send_request(request)
        .await
        .map(Response::Quic)
        .map_err(io::Error::other)
}

pub(super) async fn serve(
    send: iroh::endpoint::SendStream,
    recv: iroh::endpoint::RecvStream,
    router: axum::Router,
) {
    let service = hyper_util::service::TowerToHyperService::new(router);
    let _ = hyper::server::conn::http1::Builder::new()
        .serve_connection(TokioIo::new(tokio::io::join(recv, send)), service)
        .await;
}
