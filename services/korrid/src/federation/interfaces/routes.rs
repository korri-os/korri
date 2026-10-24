//! Read a complete kernel route dump without requiring /proc/net visibility.
use netlink_packet_core::{
    NetlinkMessage, NetlinkPayload, NLM_F_DUMP, NLM_F_DUMP_INTR, NLM_F_REQUEST,
};
use netlink_packet_route::{
    route::{RouteAttribute, RouteFlags, RouteMessage, RouteNextHopFlags, RouteType},
    AddressFamily, RouteNetlinkMessage,
};
use netlink_sys::{protocols::NETLINK_ROUTE, Socket, SocketAddr};
use std::{
    collections::BTreeSet,
    io,
    os::fd::AsRawFd,
    time::{Duration, Instant},
};

fn invalid(message: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[derive(Default)]
struct RouteDump {
    interfaces: BTreeSet<u32>,
    complete: bool,
}
impl RouteDump {
    fn consume(&mut self, mut bytes: &[u8], sequence: u32) -> io::Result<()> {
        while !bytes.is_empty() {
            if self.complete {
                return Err(invalid("route data follows dump completion"));
            }
            let message =
                NetlinkMessage::<RouteNetlinkMessage>::deserialize(bytes).map_err(invalid)?;
            let length = message.header.length as usize;
            if length < 16 || length > bytes.len() {
                return Err(invalid("invalid route message length"));
            }
            if message.header.sequence_number != sequence {
                return Err(invalid("unexpected route sequence"));
            }
            if message.header.flags & NLM_F_DUMP_INTR != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "kernel interrupted route dump",
                ));
            }
            match message.payload {
                NetlinkPayload::InnerMessage(RouteNetlinkMessage::NewRoute(route)) => {
                    self.route(route)?
                }
                NetlinkPayload::Done(done) if done.code == 0 => self.complete = true,
                NetlinkPayload::Done(_) => return Err(invalid("kernel rejected route dump")),
                NetlinkPayload::Error(error) if error.code.is_some() => {
                    let code = error
                        .raw_code()
                        .checked_abs()
                        .ok_or_else(|| invalid("invalid kernel error code"))?;
                    return Err(io::Error::from_raw_os_error(code));
                }
                NetlinkPayload::Error(_) | NetlinkPayload::Noop => {}
                NetlinkPayload::Overrun(_) => return Err(invalid("kernel route dump overrun")),
                _ => return Err(invalid("unexpected route dump message")),
            }
            let aligned = (length + 3) & !3;
            bytes = if aligned <= bytes.len() {
                &bytes[aligned..]
            } else if length == bytes.len() {
                &[]
            } else {
                return Err(invalid("truncated route message padding"));
            };
        }
        Ok(())
    }
    fn route(&mut self, route: RouteMessage) -> io::Result<()> {
        if !matches!(
            route.header.address_family,
            AddressFamily::Inet | AddressFamily::Inet6
        ) || route.header.destination_prefix_length != 0
            || route.header.kind != RouteType::Unicast
            || route
                .header
                .flags
                .intersects(RouteFlags::Dead | RouteFlags::Linkdown)
        {
            return Ok(());
        }
        let mut found = false;
        for attr in route.attributes {
            match attr {
                RouteAttribute::Oif(index) if index != 0 => {
                    self.interfaces.insert(index);
                    found = true;
                }
                RouteAttribute::MultiPath(hops) => {
                    if hops.is_empty() {
                        return Err(invalid("default route has no next hops"));
                    }
                    found = true;
                    for hop in hops {
                        // A down ECMP leg does not make its interface an uplink.
                        if hop
                            .flags
                            .intersects(RouteNextHopFlags::Dead | RouteNextHopFlags::Linkdown)
                        {
                            continue;
                        }
                        if hop.interface_index == 0 {
                            return Err(invalid("default route next hop has no output interface"));
                        }
                        self.interfaces.insert(hop.interface_index);
                    }
                }
                _ => {}
            }
        }
        if !found {
            return Err(invalid("default route has no output interface"));
        }
        Ok(())
    }
    fn finish(self) -> io::Result<BTreeSet<u32>> {
        if !self.complete {
            return Err(invalid("incomplete kernel route dump"));
        }
        Ok(self.interfaces)
    }
}

fn receive_timeout(socket: &Socket, remaining: Duration) -> io::Result<()> {
    if remaining.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "kernel route dump timed out",
        ));
    }
    let timeout = libc::timeval {
        tv_sec: remaining.as_secs() as libc::time_t,
        tv_usec: remaining.subsec_micros().max(1).into(),
    };
    // SAFETY: timeout has the native SO_RCVTIMEO layout and lives through this call.
    if unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            (&timeout as *const libc::timeval).cast(),
            std::mem::size_of_val(&timeout) as libc::socklen_t,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(super) fn default_route_indices() -> io::Result<BTreeSet<u32>> {
    let mut socket = Socket::new(NETLINK_ROUTE)?;
    socket.bind_auto()?;
    socket.connect(&SocketAddr::new(0, 0))?;
    let mut route = RouteMessage::default();
    route.header.address_family = AddressFamily::Unspec;
    let mut request = NetlinkMessage::from(RouteNetlinkMessage::GetRoute(route));
    request.header.flags = NLM_F_REQUEST | NLM_F_DUMP;
    request.header.sequence_number = 1;
    request.finalize();
    let mut bytes = vec![0; request.buffer_len()];
    request.serialize(&mut bytes);
    if socket.send(&bytes, 0)? != bytes.len() {
        return Err(io::Error::new(
            io::ErrorKind::WriteZero,
            "incomplete route dump request",
        ));
    }
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut dump = RouteDump::default();
    let mut buffer = vec![0; 65536];
    while !dump.complete {
        receive_timeout(&socket, deadline.saturating_duration_since(Instant::now()))?;
        let (length, sender) = match socket.recv_from(&mut &mut buffer[..], libc::MSG_TRUNC) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if sender.port_number() != 0
            || sender.multicast_groups() != 0
            || length == 0
            || length > buffer.len()
        {
            return Err(invalid("invalid kernel route datagram"));
        }
        dump.consume(&buffer[..length], 1)?;
    }
    dump.finish()
}

#[cfg(all(test, target_endian = "little"))]
#[path = "routes_tests.rs"]
mod tests;
