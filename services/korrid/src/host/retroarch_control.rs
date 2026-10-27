use std::{
    io,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    time::Duration,
};

const RETROARCH_CONTROL_PORT: u16 = 55355;
const CONTROL_TIMEOUT: Duration = Duration::from_millis(250);
const READBACK_ATTEMPTS: usize = 5;
const READBACK_DELAY: Duration = Duration::from_millis(20);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RetroarchCommand {
    OpenMenu,
    NetplayHostToggle,
    Quit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PreparedRetroarchCommand {
    OpenMenu,
    NetplayHostToggle,
    Quit,
}

pub(crate) trait RetroarchControlExecutor: Send + Sync {
    /// Validate only static control configuration before korrid changes the
    /// freezer state. A frozen process cannot answer runtime probes.
    fn prepare(&self, command: RetroarchCommand) -> Result<PreparedRetroarchCommand, String>;
    fn invoke(&self, command: PreparedRetroarchCommand) -> Result<(), String>;
}

#[derive(Clone, Debug)]
pub(crate) struct NetworkRetroarchControl {
    endpoint: SocketAddr,
    netplay_endpoint: SocketAddr,
}

impl Default for NetworkRetroarchControl {
    fn default() -> Self {
        Self::new(RETROARCH_CONTROL_PORT)
    }
}

impl NetworkRetroarchControl {
    pub(crate) fn new(port: u16) -> Self {
        Self {
            netplay_endpoint: SocketAddr::from((Ipv4Addr::LOCALHOST, 55435)),
            endpoint: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)),
        }
    }

    fn status(&self) -> Result<RetroarchStatus, String> {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
            .map_err(|error| format!("RetroArch control bind failed: {error}"))?;
        socket
            .set_read_timeout(Some(CONTROL_TIMEOUT))
            .map_err(|error| format!("RetroArch control timeout setup failed: {error}"))?;
        socket
            .send_to(b"GET_STATUS\n", self.endpoint)
            .map_err(|error| format!("RetroArch status request failed: {error}"))?;
        let mut reply = [0_u8; 4096];
        let (length, source) =
            socket
                .recv_from(&mut reply)
                .map_err(|error| match error.kind() {
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => {
                        "RetroArch did not answer the status readback.".to_owned()
                    }
                    _ => format!("RetroArch status readback failed: {error}"),
                })?;
        if source != self.endpoint {
            return Err("RetroArch status readback came from the wrong endpoint.".into());
        }
        parse_status(&reply[..length])
    }

    fn send(&self, command: &[u8]) -> Result<(), String> {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
            .map_err(|error| format!("RetroArch control bind failed: {error}"))?;
        socket
            .send_to(command, self.endpoint)
            .map_err(|error| format!("RetroArch command failed: {error}"))?;
        Ok(())
    }
}

impl RetroarchControlExecutor for NetworkRetroarchControl {
    fn prepare(&self, command: RetroarchCommand) -> Result<PreparedRetroarchCommand, String> {
        if !self.endpoint.ip().is_loopback() || self.endpoint.port() == 0 {
            return Err("RetroArch control endpoint is not configured.".into());
        }
        Ok(match command {
            RetroarchCommand::OpenMenu => PreparedRetroarchCommand::OpenMenu,
            RetroarchCommand::NetplayHostToggle => PreparedRetroarchCommand::NetplayHostToggle,
            RetroarchCommand::Quit => PreparedRetroarchCommand::Quit,
        })
    }

    fn invoke(&self, command: PreparedRetroarchCommand) -> Result<(), String> {
        match command {
            PreparedRetroarchCommand::OpenMenu => {
                let before = self.status()?;
                if before.paused {
                    return Ok(());
                }
                self.send(b"MENU_TOGGLE\n")?;
                for _ in 0..READBACK_ATTEMPTS {
                    std::thread::sleep(READBACK_DELAY);
                    if self.status().is_ok_and(|status| {
                        status.paused
                            && status.content == before.content
                            && status.crc32 == before.crc32
                    }) {
                        return Ok(());
                    }
                }
                Err("RetroArch did not confirm the menu state.".into())
            }
            PreparedRetroarchCommand::NetplayHostToggle => {
                let before = self.status()?;
                // A daemon restart loses its process-local prepared reply, but
                // not RetroArch or inputd's seat reservation. Never toggle an
                // already hosting session off when that device takes part again.
                if std::net::TcpStream::connect_timeout(&self.netplay_endpoint, CONTROL_TIMEOUT)
                    .is_ok()
                {
                    return Ok(());
                }
                self.send(b"NETPLAY_HOST_TOGGLE\n")?;
                for _ in 0..READBACK_ATTEMPTS {
                    std::thread::sleep(READBACK_DELAY);
                    // GET_STATUS has no hosting bit. Confirm the exact content
                    // is still live, then probe RetroArch's fixed host socket.
                    if self.status().is_ok_and(|after| {
                        after.content == before.content && after.crc32 == before.crc32
                    }) && std::net::TcpStream::connect_timeout(
                        &self.netplay_endpoint,
                        CONTROL_TIMEOUT,
                    )
                    .is_ok()
                    {
                        return Ok(());
                    }
                }
                Err("RetroArch did not confirm netplay hosting.".into())
            }
            PreparedRetroarchCommand::Quit => self.send(b"QUIT\n"),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct RetroarchStatus {
    paused: bool,
    content: String,
    crc32: String,
}

fn parse_status(reply: &[u8]) -> Result<RetroarchStatus, String> {
    let reply = std::str::from_utf8(reply)
        .map_err(|_| "RetroArch status readback was not UTF-8.".to_owned())?
        .trim_end();
    let (state, details) = reply
        .strip_prefix("GET_STATUS ")
        .and_then(|value| value.split_once(' '))
        .ok_or_else(|| "RetroArch status readback was malformed.".to_owned())?;
    let paused = match state {
        "PAUSED" => true,
        "PLAYING" => false,
        _ => return Err("RetroArch status readback had an unknown state.".into()),
    };
    let (_, content, crc32) = details
        .split_once(',')
        .and_then(|(system, rest)| {
            let (content, crc32) = rest.rsplit_once(",crc32=")?;
            Some((system, content, crc32))
        })
        .ok_or_else(|| "RetroArch status readback lacked content identity.".to_owned())?;
    if content.is_empty() || crc32.is_empty() || !crc32.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("RetroArch status readback had invalid content identity.".into());
    }
    Ok(RetroarchStatus {
        paused,
        content: content.to_owned(),
        crc32: crc32.to_ascii_lowercase(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_is_static_and_sends_nothing_to_a_frozen_runtime() {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(25)))
            .unwrap();
        let executor = NetworkRetroarchControl::new(socket.local_addr().unwrap().port());

        assert_eq!(
            executor.prepare(RetroarchCommand::OpenMenu).unwrap(),
            PreparedRetroarchCommand::OpenMenu
        );
        let mut request = [0_u8; 32];
        assert!(matches!(
            socket.recv_from(&mut request).unwrap_err().kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
        ));
        assert!(NetworkRetroarchControl::new(0)
            .prepare(RetroarchCommand::Quit)
            .is_err());
    }

    #[test]
    fn parses_the_pinned_retroarch_status_shape() {
        assert_eq!(
            parse_status(b"GET_STATUS PAUSED gba,wl4.gba,crc32=1a2b3c4d\n").unwrap(),
            RetroarchStatus {
                paused: true,
                content: "wl4.gba".into(),
                crc32: "1a2b3c4d".into(),
            }
        );
        assert!(parse_status(b"GET_STATUS CONTENTLESS").is_err());
    }
}

#[cfg(test)]
mod netplay_tests {
    use super::*;
    #[test]
    fn netplay_host_toggle_reads_back_the_exact_content_and_host_socket() {
        let control = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        control
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let hosting =
            socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None).unwrap();
        hosting
            .bind(&SocketAddr::from((Ipv4Addr::LOCALHOST, 0)).into())
            .unwrap();
        let mut executor = NetworkRetroarchControl::new(control.local_addr().unwrap().port());
        executor.netplay_endpoint = hosting.local_addr().unwrap().as_socket().unwrap();
        let runtime = std::thread::spawn(move || {
            let mut bytes = [0; 128];
            for expected in [
                b"GET_STATUS\n".as_slice(),
                b"NETPLAY_HOST_TOGGLE\n",
                b"GET_STATUS\n",
            ] {
                let (n, source) = control.recv_from(&mut bytes).unwrap();
                assert_eq!(&bytes[..n], expected);
                if expected == b"NETPLAY_HOST_TOGGLE\n" {
                    hosting.listen(1).unwrap();
                }
                if expected == b"GET_STATUS\n" {
                    control
                        .send_to(
                            b"GET_STATUS PLAYING snes,Turtles.sfc,crc32=1234abcd\n",
                            source,
                        )
                        .unwrap();
                }
            }
            let listener: std::net::TcpListener = hosting.into();
            listener.accept().unwrap();
        });
        let command = executor
            .prepare(RetroarchCommand::NetplayHostToggle)
            .unwrap();
        assert_eq!(command, PreparedRetroarchCommand::NetplayHostToggle);
        executor.invoke(command).unwrap();
        runtime.join().unwrap();
    }
}
