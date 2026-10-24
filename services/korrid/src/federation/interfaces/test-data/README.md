# Kernel route captures

`routes.bin` and `routes-link-down.bin` contain little-endian Linux
`NETLINK_ROUTE` replies to `RTM_GETROUTE`, with `NLM_F_REQUEST | NLM_F_DUMP`
and sequence 113. The final message is `NLMSG_DONE`. They are native kernel
messages, not messages serialized by the parser under test.

The capture ran on 2026-10-23 in a temporary user and network namespace.
The process created two dummy interfaces, `nltest0` (index 2) and `nltest1`
(index 3). No route or interface changed in the host namespace. The namespace
ended with the capture process.

The namespace used these documentation addresses and routes:

```sh
ip link add nltest0 type dummy
ip link add nltest1 type dummy
ip link set nltest0 up
ip link set nltest1 up
ip address add 192.0.2.2/24 dev nltest0
ip address add 198.51.100.2/24 dev nltest1
ip -6 address add 2001:db8:1::2/64 dev nltest0 nodad
ip -6 address add 2001:db8:2::2/64 dev nltest1 nodad
ip route add default via 192.0.2.1 dev nltest0 metric 10
ip route add unreachable default metric 100
ip route add default metric 20 \
  nexthop via 192.0.2.1 dev nltest0 weight 1 \
  nexthop via 198.51.100.1 dev nltest1 weight 1
ip -6 route add default via 2001:db8:1::1 dev nltest0 metric 10
ip -6 route add blackhole default metric 100
```

The first capture includes both usable interfaces, IPv4 and IPv6 defaults,
an ECMP default, reject routes, local routes and specific routes. Before the
second capture, `ip link set nltest1 down` marked the second ECMP leg dead and
link-down. The expected usable default interface set then contains only index 2.

Each socket bound an automatically assigned port and no multicast groups.
The capture concatenated whole datagrams through the successful done message.
The IPv6 link-local addresses came from these temporary dummy interfaces.
No fixture contains an address or identity from a real device.
