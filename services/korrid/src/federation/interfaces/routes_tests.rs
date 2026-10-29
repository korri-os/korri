use super::*;
const CAPTURE: &[u8] = include_bytes!("test-data/routes.bin");
const DOWN: &[u8] = include_bytes!("test-data/routes-link-down.bin");
const SEQUENCE: u32 = 113;

fn packets(bytes: &[u8]) -> Vec<&[u8]> {
    let mut result = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let length = u32::from_ne_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        result.push(&bytes[at..at + length]);
        at += (length + 3) & !3;
    }
    result
}
fn selected(mut predicate: impl FnMut(&[u8]) -> bool) -> Vec<u8> {
    packets(CAPTURE)
        .into_iter()
        .filter(|packet| {
            u16::from_ne_bytes(packet[4..6].try_into().unwrap()) == 3 || predicate(packet)
        })
        .flatten()
        .copied()
        .collect()
}
fn parse(bytes: &[u8]) -> io::Result<BTreeSet<u32>> {
    let mut dump = RouteDump::default();
    dump.consume(bytes, SEQUENCE)?;
    dump.finish()
}
#[test]
fn captured_kernel_dump_combines_ipv4_ipv6_and_ecmp_defaults() {
    assert_eq!(parse(CAPTURE).unwrap(), BTreeSet::from([2, 3]));
    for family in [libc::AF_INET, libc::AF_INET6] {
        let bytes = selected(|packet| packet.len() > 28 && packet[16] == family as u8);
        assert_eq!(
            parse(&bytes).unwrap(),
            if family == libc::AF_INET {
                BTreeSet::from([2, 3])
            } else {
                BTreeSet::from([2])
            }
        );
    }
}
#[test]
fn captured_down_ecmp_leg_does_not_advertise_its_interface() {
    assert_eq!(parse(DOWN).unwrap(), BTreeSet::from([2]));
}
#[test]
fn captured_reject_blackhole_local_and_specific_routes_are_not_defaults() {
    let bytes = selected(|packet| packet.len() > 28 && (packet[17] != 0 || packet[23] != 1));
    assert!(parse(&bytes).unwrap().is_empty());
}
#[test]
fn dead_and_link_down_single_path_routes_are_not_defaults() {
    let default = packets(CAPTURE)
        .into_iter()
        .find(|packet| packet.len() > 28 && packet[16] == 2 && packet[17] == 0 && packet[23] == 1)
        .unwrap();
    for flags in [RouteFlags::Dead, RouteFlags::Linkdown] {
        let mut bytes = default.to_vec();
        bytes[24..28].copy_from_slice(&flags.bits().to_ne_bytes());
        bytes.extend_from_slice(packets(CAPTURE).last().unwrap());
        assert!(parse(&bytes).unwrap().is_empty());
    }
}
#[test]
fn multipart_dump_needs_a_successful_done_message() {
    let packets = packets(CAPTURE);
    let mut dump = RouteDump::default();
    for packet in &packets[..packets.len() - 1] {
        dump.consume(packet, SEQUENCE).unwrap();
    }
    assert!(!dump.complete);
    assert!(dump.finish().is_err());
    let mut dump = RouteDump::default();
    for packet in packets {
        dump.consume(packet, SEQUENCE).unwrap();
    }
    assert_eq!(dump.finish().unwrap(), BTreeSet::from([2, 3]));
}
#[test]
fn interrupted_failed_and_wrong_sequence_dumps_fail_closed() {
    let mut interrupted = CAPTURE.to_vec();
    let flags = u16::from_ne_bytes(interrupted[6..8].try_into().unwrap()) | NLM_F_DUMP_INTR;
    interrupted[6..8].copy_from_slice(&flags.to_ne_bytes());
    assert_eq!(
        parse(&interrupted).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    let mut dump = RouteDump::default();
    assert!(dump.consume(CAPTURE, SEQUENCE + 1).is_err());
    let mut done = packets(CAPTURE).last().unwrap().to_vec();
    done[16..20].copy_from_slice(&(-libc::EPERM).to_ne_bytes());
    assert!(parse(&done).is_err());
    let mut overrun = done[..16].to_vec();
    overrun[..4].copy_from_slice(&16u32.to_ne_bytes());
    overrun[4..6].copy_from_slice(&4u16.to_ne_bytes());
    assert!(parse(&overrun).is_err());
}
#[test]
fn kernel_error_ack_fails_without_returning_partial_routes() {
    let original = packets(CAPTURE)[0];
    let mut error = original[..16].to_vec();
    error[..4].copy_from_slice(&36u32.to_ne_bytes());
    error[4..6].copy_from_slice(&2u16.to_ne_bytes());
    error.extend_from_slice(&(-libc::EPERM).to_ne_bytes());
    error.extend_from_slice(&original[..16]);
    assert_eq!(
        parse(&error).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    error[16..20].copy_from_slice(&i32::MIN.to_ne_bytes());
    assert_eq!(
        parse(&error).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}
#[test]
fn truncated_malformed_and_post_completion_data_fail_closed() {
    for length in 0..16 {
        assert!(parse(&CAPTURE[..length]).is_err());
    }
    assert!(parse(&CAPTURE[..CAPTURE.len() - 1]).is_err());
    for length in [0, 8, u32::MAX] {
        let mut bytes = CAPTURE.to_vec();
        bytes[..4].copy_from_slice(&length.to_ne_bytes());
        assert!(parse(&bytes).is_err());
    }
    let mut bytes = CAPTURE.to_vec();
    bytes.extend_from_slice(packets(CAPTURE)[0]);
    assert!(parse(&bytes).is_err());
}

#[test]
fn default_routes_without_a_resolvable_output_fail_closed() {
    let mut checked_single = false;
    let mut checked_multipath = false;
    for bytes in packets(CAPTURE) {
        if bytes.len() <= 28 || bytes[16] != 2 || bytes[17] != 0 || bytes[23] != 1 {
            continue;
        }
        let mut message = NetlinkMessage::<RouteNetlinkMessage>::deserialize(bytes).unwrap();
        let NetlinkPayload::InnerMessage(RouteNetlinkMessage::NewRoute(route)) =
            &mut message.payload
        else {
            unreachable!()
        };
        if route
            .attributes
            .iter()
            .any(|attribute| matches!(attribute, RouteAttribute::Oif(_)))
        {
            route
                .attributes
                .retain(|attribute| !matches!(attribute, RouteAttribute::Oif(_)));
            checked_single = true;
        } else {
            for attribute in &mut route.attributes {
                if let RouteAttribute::MultiPath(hops) = attribute {
                    hops[0].interface_index = 0;
                    checked_multipath = true;
                }
            }
        }
        message.finalize();
        let mut broken = vec![0; message.buffer_len()];
        message.serialize(&mut broken);
        broken.extend_from_slice(packets(CAPTURE).last().unwrap());
        assert!(parse(&broken).is_err());
    }
    assert!(checked_single && checked_multipath);
}

#[test]
fn receive_timeout_applies_native_subsecond_bounds_and_rejects_zero() {
    let mut socket = Socket::new(NETLINK_ROUTE).unwrap();
    socket.bind_auto().unwrap();
    assert_eq!(
        receive_timeout(&socket, Duration::ZERO).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    for remaining in [Duration::from_nanos(1), Duration::from_micros(999_999)] {
        receive_timeout(&socket, remaining).unwrap();
        let mut timeout = libc::timeval {
            tv_sec: 0,
            tv_usec: 0,
        };
        let mut length = std::mem::size_of_val(&timeout) as libc::socklen_t;
        // SAFETY: timeout and length are writable, native getsockopt outputs.
        assert_eq!(
            unsafe {
                libc::getsockopt(
                    socket.as_raw_fd(),
                    libc::SOL_SOCKET,
                    libc::SO_RCVTIMEO,
                    (&mut timeout as *mut libc::timeval).cast(),
                    &mut length,
                )
            },
            0
        );
        assert_eq!(length as usize, std::mem::size_of_val(&timeout));
        let actual = Duration::from_secs(timeout.tv_sec.try_into().unwrap())
            + Duration::from_micros(timeout.tv_usec.try_into().unwrap());
        // Linux rounds up to the kernel tick. A positive timeout must not become
        // the all-zero timeval that would disable receive timeouts completely.
        assert!(actual >= remaining);
        assert!(actual <= Duration::from_secs(1));
    }
    receive_timeout(&socket, Duration::from_nanos(1)).unwrap();
    let mut buffer = [0u8; 16];
    assert_eq!(
        socket.recv(&mut &mut buffer[..], 0).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
}
