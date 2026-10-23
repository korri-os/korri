# Iroh carries federation connections

**Owner decision, 2026-10-23.** Use iroh 1.4.1 for ADR 0026.

## Decision

Use iroh with its defaults disabled. Public Nostr relays carry the signal.
Configured public STUN servers find addresses. Only a device of the same
federation may relay encrypted traffic. No n0 relay or discovery is used.

The measured spike favoured iroh. In the build-machine lab, it connected in
349 of 349 attempts without symmetric NAT or a strict firewall. WebRTC
connected in 337 of 350. With symmetric NAT or a strict firewall and no
relaying device, iroh connected in 11 of 210 attempts. WebRTC connected in
none. A federation device rescued symmetric NAT for both. It rescued the
UDP-blocked case for iroh through TCP; the spike's UDP-only TURN did not.

At 20 Mbit/s offered on the home profile, iroh delivered 17.9 Mbit/s with
11% loss. WebRTC delivered 1.9 Mbit/s with 90% loss. At 10 and 5 Mbit/s,
iroh delivered 9.98 and 4.98 Mbit/s with under 0.3% loss. Iroh answered
about 240 peer calls per run alongside the stream. WebRTC answered one;
its SCTP scheduler put unordered packets before ordered calls.
These are measurements of the spikes, not every implementation of WebRTC.

The iroh spike passed 18 of 18 byte checks for Moonlight's ports. A real
Moonlight session was not run because no paired client was available.
The aarch64 spike binary was 19.4 MB, against 7.7 MB for WebRTC. Iroh had
338 Rust dependencies, against 256. Both use MIT or Apache-2.0 licences.
Iroh already has port mapping and an embedded Rust relay. WebRTC still
needed port mapping, TCP or TLS TURN, and a scheduling fix.

## Evidence and limits

The lab made 1,120 runs, with 5 to 20 repeats per pair. It simulated
endpoint-independent NAT, double NAT, symmetric NAT, port mapping, IPv6,
a strict firewall, a relaying federation device and network changes.
Home and mobile impairment profiles were simulated; some mobile inputs
were assumed. Handheld tests also used a real phone network and home Wi-Fi.

The tests did not cover real public Wi-Fi firewalls, other carriers, cell
tower roaming, a real Moonlight session or the R36T Max. The handhelds
could not run the IPv6 and network-change scenarios or traffic shaping.
The phone test window ended before double NAT, strict firewall, federation
relay and home-to-phone switch cases completed away from home.

A network change needs a reconnect, not migration. The lab gap was about
7.1 seconds. A home relay needs IPv6 or TCP port mapping to be reachable
from another network. This chunk does not implement TCP port mapping. Its
home relay is reachable across networks only through global IPv6. Private
LAN relay listeners also work on the same network. A UDP port mapping does
not make a TCP relay reachable. Symmetric NAT on both sides needs a reachable
relaying device. Only one configured public Nostr relay was live:
relay.primal.net. relay.nostr.band was dead.

The runtime watches local interfaces, addresses and default routes. It also
refreshes the endpoint and reruns STUN after an established connection is lost
and two reconnect attempts to that peer fail, but only when no incoming or
outgoing connection on the endpoint is healthy. The admission check fences new
connections during refresh. The independent TCP relay keeps its clients.
Refreshes are at least 60 seconds apart, with a 60, 120, 240, then 300-second
backoff until a connection succeeds. Normal peer retry backoff remains in force.
The trigger logs once at info level, without addresses.

An upstream NAT remap with an unchanged local interface (cell roaming) was not
covered by any test. Hardware testing gates this case where it can. If another
connection is healthy, the runtime does not refresh: a stale public address may
remain until that connection is lost or a local network change occurs. This
trade-off avoids interrupting a healthy peer to repair another route.

A weak phone link delivered 4.1 Mbit/s at 5 Mbit/s offered, with 18% loss.
QUIC adds rate control above Moonlight's own rate control. These numbers
do not prove that a real Moonlight session works well.

UPnP opens a mapping only on the local router. Behind carrier-grade NAT,
inbound traffic still fails unless the ISP runs PCP and the router bridges
UPnP to it (RFC 6970). Iroh's documentation calls n0's public relays
"for development and testing", says they are rate-limited, and recommends
against production workloads. Korri does not use them.

The embedded relay tries TCP 443 on each approved interface. If that port is
occupied or not permitted, it binds an ephemeral port and advertises only the
actual listener. No capability or service permission is added. Korrid most likely
runs without `CAP_NET_BIND_SERVICE`, so real devices will usually use an ephemeral
port. A strict firewall that allows only TCP 80 and 443 will then block the relay.
Granting that capability is follow-up work and needs the owner's approval.
The Hardware test stage must check which port each device binds.

## Local stream forwards

While a nearby link is up, its peer IP replaces the remembered Moonlight IP.
The address keeps its named control port, or uses 47989. This uses the same
nearby-link fact as the HTTP candidate, before the stream probe.

RPC reachability does not prove that Sunshine is reachable. Before choosing
a forward, korrid probes the existing `moonlight_address` control port. It uses
47989 unless the address names a port. The TCP probe sends no application bytes
and has the existing one-second connect timeout. A successful probe keeps the
existing address. A failed probe uses forwards only if a live iroh route exists.
Without one, the existing viewer path and its failure stay unchanged.

The viewing device can also run Sunshine. An IPv4 wildcard Sunshine listener
holds its TCP ports on every IPv4 loopback address. The local forwards therefore
bind only `[::1]`, with `IPV6_V6ONLY` set. The viewer receives `[::1]:47989`.
The port numbers stay unchanged, including those Sunshine reports in RTSP.
The selected remote session owns one forward lease. Relay-only connections do
not hold it. Changing the selected peer or ending the session releases it.

If any required IPv6 loopback port is busy, stream preparation fails with the
port named in the error. This is a device setup error, not the network failure
sentence. There is no IPv4 fallback. The Hardware test stage must prove that
Moonlight connects to `[::1]` and streams. That has not been measured. A viewer
in a network namespace is follow-up work if IPv6 loopback does not work.

## Sources

- Measured choice: `/tmp/korri-reach/pick.md` and `measure-report.md`.
- Spike: `spike/reach-iroh`, commit `6655fa19c`, `spikes/reach/iroh/`.
- Research audit: `/tmp/korri-reach/audit-report.md`, claims 6 and 7.
- [RFC 6970](https://www.rfc-editor.org/rfc/rfc6970.txt).
- [Iroh relay guidance](https://github.com/n0-computer/docs.iroh.computer/blob/main/add-a-relay.mdx).
