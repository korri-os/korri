//! A real loopback HTTP/1.1 server with configured routes for acquisition
//! tests. It records every request line and header block it receives.
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpListener},
    sync::{Arc, Mutex},
    thread,
};

#[derive(Clone)]
pub(super) struct Route {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Send the body in pieces of this size with this pause between them.
    pub pace: Option<(usize, std::time::Duration)>,
    /// Send no Content-Length: the body ends when the connection closes.
    pub unsized_body: bool,
    /// Repeat the request's own `Authorization` header in the response, as a
    /// debugging endpoint or an error page does.
    pub echo_authorization: Option<Echo>,
    /// Serve `Range: bytes=<n>-` as 206 when `If-Range` names this entity
    /// tag, which every response carries (RFC 9110 sections 14 and 13.1.5).
    pub etag: Option<String>,
}

/// Where a route repeats the request's `Authorization` header.
#[derive(Clone)]
pub(super) enum Echo {
    /// In a response header.
    Header,
    /// In a response header, every byte percent-encoded.
    HeaderPercentEncoded,
    /// In a JSON body, which replaces the route's own body.
    Body,
    /// In a JSON body, with every `/` written as `\/`, as PHP writes it.
    BodySlashEscaped,
    /// In a JSON body, with every character of it written as `\uXXXX`.
    BodyUnicodeEscaped,
    /// As an object key of a JSON body.
    Key,
    /// The secret alone, without `Bearer `, in the address of a redirect
    /// to `to`.
    Redirect { to: String, place: Place },
}

/// Where in a redirect's address a route puts the secret.
#[derive(Clone, Copy)]
pub(super) enum Place {
    Query,
    Path,
    /// In the query, every byte percent-encoded.
    QueryPercentEncoded,
    /// In the query, with a tab in the middle of it. A URL parser drops the
    /// tab, so the address the redirect resolves to holds the secret and the
    /// `Location` value as written does not.
    QuerySplitByTab,
}

fn percent_encoded(text: &str) -> String {
    text.bytes().map(|byte| format!("%{byte:02X}")).collect()
}

impl Route {
    pub fn ok(content_type: &str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            headers: vec![("Content-Type".into(), content_type.into())],
            body: body.into(),
            pace: None,
            unsized_body: false,
            echo_authorization: None,
            etag: None,
        }
    }
    /// Answer ranges for this strong entity tag.
    pub fn ranges(mut self, etag: &str) -> Self {
        self.etag = Some(etag.into());
        self
    }
    pub fn unsized_body(mut self) -> Self {
        self.unsized_body = true;
        self
    }
    pub fn paced(mut self, piece: usize, pause: std::time::Duration) -> Self {
        self.pace = Some((piece, pause));
        self
    }
    pub fn redirect(location: &str) -> Self {
        Self::redirect_with(302, location)
    }
    pub fn redirect_with(status: u16, location: &str) -> Self {
        Self {
            status,
            headers: vec![("Location".into(), location.into())],
            body: Vec::new(),
            pace: None,
            unsized_body: false,
            echo_authorization: None,
            etag: None,
        }
    }
    pub fn echoing(mut self, echo: Echo) -> Self {
        self.echo_authorization = Some(echo);
        self
    }
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
}

#[derive(Clone, Debug)]
pub(super) struct Received {
    pub method: String,
    pub path: String,
    pub headers: Vec<String>,
    pub body: Vec<u8>,
}

pub(super) struct HttpFixture {
    pub address: SocketAddr,
    routes: Arc<Mutex<BTreeMap<String, Route>>>,
    /// Routes answered in turn: each request takes the first, and the last
    /// one stays.
    queues: Arc<Mutex<BTreeMap<String, std::collections::VecDeque<Route>>>>,
    received: Arc<Mutex<Vec<Received>>>,
}

impl HttpFixture {
    pub fn start(routes: impl IntoIterator<Item = (&'static str, Route)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let routes: BTreeMap<String, Route> = routes
            .into_iter()
            .map(|(path, route)| (path.to_owned(), route))
            .collect();
        let routes = Arc::new(Mutex::new(routes));
        let received = Arc::new(Mutex::new(Vec::new()));
        let queues = Arc::new(Mutex::new(BTreeMap::<
            String,
            std::collections::VecDeque<Route>,
        >::new()));
        let (served_routes, log, served_queues) =
            (routes.clone(), received.clone(), queues.clone());
        thread::spawn(move || {
            for socket in listener.incoming() {
                let Ok(mut socket) = socket else { continue };
                let routes = served_routes.clone();
                let queues = served_queues.clone();
                let log = log.clone();
                thread::spawn(move || {
                    let mut reader = BufReader::new(socket.try_clone().unwrap());
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() {
                        return;
                    }
                    let method = line.split_whitespace().next().unwrap_or("").to_owned();
                    let path = line.split_whitespace().nth(1).unwrap_or("/").to_owned();
                    let mut headers = Vec::new();
                    // The value exactly as it arrived, for a route that echoes it.
                    let mut authorization: Option<String> = None;
                    loop {
                        let mut header = String::new();
                        if reader.read_line(&mut header).is_err()
                            || header == "\r\n"
                            || header.is_empty()
                        {
                            break;
                        }
                        if let Some((name, value)) = header.trim_end().split_once(':') {
                            if name.eq_ignore_ascii_case("authorization") {
                                authorization = Some(value.trim().to_owned());
                            }
                        }
                        headers.push(header.trim_end().to_ascii_lowercase());
                    }
                    let length = headers
                        .iter()
                        .find_map(|header| header.strip_prefix("content-length: "))
                        .and_then(|length| length.parse::<usize>().ok())
                        .unwrap_or(0);
                    let mut body = vec![0; length];
                    if reader.read_exact(&mut body).is_err() {
                        return;
                    }
                    log.lock().unwrap().push(Received {
                        method,
                        path: path.clone(),
                        headers: headers.clone(),
                        body,
                    });
                    let queued = queues.lock().unwrap().get_mut(&path).and_then(|queue| {
                        if queue.len() > 1 {
                            queue.pop_front()
                        } else {
                            queue.front().cloned()
                        }
                    });
                    let mut route = queued
                        .or_else(|| routes.lock().unwrap().get(&path).cloned())
                        .unwrap_or(Route {
                            status: 404,
                            headers: Vec::new(),
                            body: b"missing".to_vec(),
                            pace: None,
                            unsized_body: false,
                            echo_authorization: None,
                            etag: None,
                        });
                    let sent = authorization.clone().unwrap_or_default();
                    let body_with = |value: serde_json::Value| {
                        serde_json::json!({
                            "success": true,
                            "data": [],
                            "request": { "headers": { "Authorization": value } },
                        })
                        .to_string()
                    };
                    match route.echo_authorization.clone() {
                        Some(Echo::Header) => {
                            route.headers.push(("X-Request-Authorization".into(), sent))
                        }
                        Some(Echo::HeaderPercentEncoded) => route
                            .headers
                            .push(("X-Request-Authorization".into(), percent_encoded(&sent))),
                        Some(Echo::Body) => {
                            route.body = body_with(serde_json::json!(authorization)).into_bytes()
                        }
                        Some(Echo::BodySlashEscaped) => {
                            route.body = body_with(serde_json::json!(authorization))
                                .replace('/', "\\/")
                                .into_bytes()
                        }
                        Some(Echo::BodyUnicodeEscaped) => {
                            let escaped: String = sent
                                .chars()
                                .map(|character| format!("\\u{:04x}", character as u32))
                                .collect();
                            route.body = body_with(serde_json::json!("PLACEHOLDER"))
                                .replace("PLACEHOLDER", &escaped)
                                .into_bytes()
                        }
                        Some(Echo::Key) => {
                            route.body = serde_json::json!({
                                "success": true,
                                "data": [],
                                "seen": { sent: true },
                            })
                            .to_string()
                            .into_bytes()
                        }
                        Some(Echo::Redirect { to, place }) => {
                            let token = sent.strip_prefix("Bearer ").unwrap_or(&sent);
                            route.status = 302;
                            route.body = Vec::new();
                            route.headers.push((
                                "Location".into(),
                                match place {
                                    Place::Query => format!("{to}?key={token}"),
                                    Place::Path => format!("{to}/{token}"),
                                    Place::QueryPercentEncoded => {
                                        format!("{to}?key={}", percent_encoded(token))
                                    }
                                    Place::QuerySplitByTab => {
                                        let (start, end) = token.split_at(token.len() / 2);
                                        format!("{to}?key={start}\t{end}")
                                    }
                                },
                            ));
                        }
                        None => {}
                    }
                    if let Some(etag) = route.etag.clone() {
                        let value = |name: &str| {
                            headers.iter().find_map(|header| {
                                header.strip_prefix(&format!("{name}: ")).map(str::to_owned)
                            })
                        };
                        let start = value("range")
                            .and_then(|range| {
                                range
                                    .strip_prefix("bytes=")?
                                    .strip_suffix('-')?
                                    .parse()
                                    .ok()
                            })
                            .filter(|_| value("if-range").as_deref() == Some(etag.as_str()));
                        if let Some(start) = start.filter(|start: &usize| *start < route.body.len())
                        {
                            let total = route.body.len();
                            route.status = 206;
                            route.headers.push((
                                "Content-Range".into(),
                                format!("bytes {start}-{}/{total}", total - 1),
                            ));
                            route.body = route.body[start..].to_vec();
                        }
                        route.headers.push(("ETag".into(), etag));
                    }
                    let mut head = format!("HTTP/1.1 {} Status\r\n", route.status);
                    let declares_length = route
                        .headers
                        .iter()
                        .any(|(name, _)| name.eq_ignore_ascii_case("content-length"));
                    for (name, value) in &route.headers {
                        head.push_str(&format!("{name}: {value}\r\n"));
                    }
                    if !declares_length && !route.unsized_body {
                        head.push_str(&format!("Content-Length: {}\r\n", route.body.len()));
                    }
                    head.push_str("Connection: close\r\n\r\n");
                    let _ = socket.write_all(head.as_bytes());
                    match route.pace {
                        None => {
                            let _ = socket.write_all(&route.body);
                        }
                        Some((piece, pause)) => {
                            for part in route.body.chunks(piece) {
                                if socket
                                    .write_all(part)
                                    .and_then(|()| socket.flush())
                                    .is_err()
                                {
                                    break;
                                }
                                thread::sleep(pause);
                            }
                        }
                    }
                });
            }
        });
        Self {
            address,
            routes,
            queues,
            received,
        }
    }

    /// Answer `path` with `routes` in turn; the last one answers every
    /// request after it.
    pub fn queue(&self, path: &str, routes: Vec<Route>) {
        self.queues
            .lock()
            .unwrap()
            .insert(path.into(), routes.into());
    }

    pub fn route(&self, path: &str, route: Route) {
        self.routes.lock().unwrap().insert(path.into(), route);
    }

    pub fn received(&self) -> Vec<Received> {
        self.received.lock().unwrap().clone()
    }

    /// An absolute URL on a declared DNS name that the test engine maps here.
    pub fn url(&self, host: &str, path: &str) -> String {
        format!("http://{host}:{}{path}", self.address.port())
    }
}
