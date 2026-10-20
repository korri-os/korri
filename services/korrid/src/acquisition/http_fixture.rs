//! A real loopback HTTP/1.1 server with configured routes for acquisition
//! tests. It records every request line and header block it receives.
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
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
}

impl Route {
    pub fn ok(content_type: &str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            headers: vec![("Content-Type".into(), content_type.into())],
            body: body.into(),
            pace: None,
        }
    }
    pub fn paced(mut self, piece: usize, pause: std::time::Duration) -> Self {
        self.pace = Some((piece, pause));
        self
    }
    pub fn redirect(location: &str) -> Self {
        Self {
            status: 302,
            headers: vec![("Location".into(), location.into())],
            body: Vec::new(),
            pace: None,
        }
    }
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
}

#[derive(Clone, Debug)]
pub(super) struct Received {
    pub path: String,
    pub headers: Vec<String>,
}

pub(super) struct HttpFixture {
    pub address: SocketAddr,
    routes: Arc<Mutex<BTreeMap<String, Route>>>,
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
        let (served_routes, log) = (routes.clone(), received.clone());
        thread::spawn(move || {
            for socket in listener.incoming() {
                let Ok(mut socket) = socket else { continue };
                let routes = served_routes.clone();
                let log = log.clone();
                thread::spawn(move || {
                    let mut reader = BufReader::new(socket.try_clone().unwrap());
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() {
                        return;
                    }
                    let path = line.split_whitespace().nth(1).unwrap_or("/").to_owned();
                    let mut headers = Vec::new();
                    loop {
                        let mut header = String::new();
                        if reader.read_line(&mut header).is_err()
                            || header == "\r\n"
                            || header.is_empty()
                        {
                            break;
                        }
                        headers.push(header.trim_end().to_ascii_lowercase());
                    }
                    log.lock().unwrap().push(Received {
                        path: path.clone(),
                        headers,
                    });
                    let route = routes.lock().unwrap().get(&path).cloned().unwrap_or(Route {
                        status: 404,
                        headers: Vec::new(),
                        body: b"missing".to_vec(),
                        pace: None,
                    });
                    let mut head = format!("HTTP/1.1 {} Status\r\n", route.status);
                    let declares_length = route
                        .headers
                        .iter()
                        .any(|(name, _)| name.eq_ignore_ascii_case("content-length"));
                    for (name, value) in &route.headers {
                        head.push_str(&format!("{name}: {value}\r\n"));
                    }
                    if !declares_length {
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
            received,
        }
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
