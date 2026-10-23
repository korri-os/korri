# Iroh carries federation connections

**Owner decision, 2026-10-23.** Use iroh 1.4.1 for ADR 0026.

## Decision

Korri disables iroh's defaults. Public Nostr relays carry the signal.
Configured public STUN servers find addresses. Only a device of the same
federation may relay encrypted traffic. Korri uses no n0 relay or discovery.

The measured spike favoured iroh. In the build-machine lab, it connected in
349 of 349 attempts without symmetric NAT or a strict firewall. WebRTC
connected in 337 of 350. With symmetric NAT or a strict firewall and no
relaying device, iroh connected in 11 of 210 attempts. WebRTC connected in
none. A federation device rescued symmetric NAT for both. It rescued the
UDP-blocked case for iroh through TCP. The spike's UDP-only TURN did not.

The lab offered 20 Mbit/s on the home profile. Iroh delivered 17.9 Mbit/s with
11% loss. WebRTC delivered 1.9 Mbit/s with 90% loss. At 10 and 5 Mbit/s,
iroh delivered 9.98 and 4.98 Mbit/s with under 0.3% loss. Iroh answered
about 240 peer calls per run alongside the stream. WebRTC answered one.
Its SCTP scheduler put unordered packets before ordered calls.
These numbers measure the spikes, not every implementation of WebRTC.

The iroh spike passed 18 of 18 byte checks for Moonlight's ports. The tests
did not run a real Moonlight session. The lab had no paired client.
The aarch64 spike binary was 19.4 MB, against 7.7 MB for WebRTC. Iroh had
338 Rust dependencies, against 256. Both use MIT or Apache-2.0 licences.
Iroh already has port mapping and an embedded Rust relay. WebRTC still
needed port mapping, TCP or TLS TURN, and a scheduling fix.

## Evidence and limits

The lab made 1,120 runs, with 5 to 20 repeats per pair. It simulated
endpoint-independent NAT, double NAT, symmetric NAT, port mapping, IPv6,
a strict firewall, a relaying federation device and network changes.
The lab simulated home and mobile impairment profiles. It assumed some
mobile inputs. Handheld tests also used a real phone network and home Wi-Fi.

The tests did not cover real public Wi-Fi firewalls, other carriers, cell
tower roaming, a real Moonlight session or the R36T Max. The handhelds
could not run the IPv6 and network-change scenarios or traffic shaping.
The handheld tests did not finish double NAT, strict firewall, federation
relay or home-to-phone switch cases away from home. The phone window ended first.

A network change needs a reconnect, not migration. The lab gap was about
7.1 seconds. A home relay needs IPv6 or TCP port mapping so that peers can
reach it from another network. This chunk does not implement TCP port mapping.
Other networks can reach its home relay only through global IPv6. Private
LAN relay listeners also work on the same network. A UDP port mapping does
not make a TCP relay reachable. Symmetric NAT on both sides needs a reachable
relaying device. Only one configured public Nostr relay was live:
relay.primal.net. relay.nostr.band was dead.

The runtime watches local interfaces, addresses and default routes. If it loses
an established connection, it waits for two reconnect attempts to fail. It
refreshes the endpoint and reruns STUN only when no incoming or outgoing
connection on the endpoint is healthy. The admission check stops new connections
during refresh. The independent TCP relay keeps its clients connected.
The runtime separates refreshes by at least 60 seconds. It uses a 60, 120, 240,
then 300-second backoff until a connection succeeds. Normal peer retry backoff
remains in force. Korrid logs the trigger once at info level, without addresses.

The tests did not cover an upstream NAT remap with an unchanged local interface.
Cell roaming can cause this case. Hardware testing must gate it where it can.
If another connection is healthy, the runtime does not refresh. A stale public
address may remain until the runtime loses that connection or a local network
change occurs. This trade-off keeps a healthy peer connected while another
route fails.

The lab offered 5 Mbit/s on a weak phone link. The link delivered 4.1 Mbit/s
with 18% loss. QUIC adds rate control above Moonlight's own rate control.
These numbers do not prove that a real Moonlight session works well.

UPnP opens a mapping only on the local router. Behind carrier-grade NAT,
inbound traffic still fails unless the ISP runs PCP and the router bridges
UPnP to it (RFC 6970). Iroh's documentation calls n0's public relays
"for development and testing" and recommends against production workloads.
n0 limits their rate. Korri does not use them.

The embedded relay tries TCP 443 on each allowed interface. If another listener
occupies the port or the OS refuses the bind, the relay binds an ephemeral port.
It advertises only the actual listener. Korri adds no capability or service
permission. Korrid most likely runs without `CAP_NET_BIND_SERVICE`.
Real devices will usually use an ephemeral port. A strict firewall that allows only
TCP 80 and 443 will then block the relay. The owner must approve any later
change that grants that capability. The Hardware test stage must check which
port each device binds.

Korrid refreshes relay answers independently of peer reachability. A failed
Nostr publication or an unanswered offer does not prove that a peer is
unreachable. Before it sets failure state, korrid sends one signed SessionStatus
read over the existing iroh route. The read has a five-second bound. A valid
signed reply keeps the peer Ready.

## Local stream forwards

While a nearby link is up, korrid uses its peer IP instead of the remembered
Moonlight IP. Korrid keeps its named control port, or uses 47989. It reads the
same nearby-link fact for the HTTP candidate before it probes the stream.

An RPC reply does not prove that Sunshine can accept a connection. Before it
chooses a forward, korrid probes the existing `moonlight_address` control port.
It uses 47989 unless the address names a port. The TCP probe sends no application
bytes. It has the existing one-second connect timeout. A successful probe keeps
the existing address. After a failed probe, korrid uses forwards only if a live
iroh route exists. Without one, it keeps the existing viewer path and its failure.

The viewing device can also run Sunshine. An IPv4 wildcard Sunshine listener
holds its TCP ports on every IPv4 loopback address. The local forwards therefore
bind only `[::1]`, with `IPV6_V6ONLY` set. The viewer receives `[::1]:47989`.
The port numbers stay unchanged, including those Sunshine reports in RTSP.
The selected remote session owns one forward lease. Relay-only connections do
not hold it. A peer selection change or session end releases it.

If another listener holds a required IPv6 loopback port, korrid refuses stream
preparation. The error names that port. This is a device setup error, not the
network failure sentence. Korrid provides no IPv4 fallback. The Hardware test
stage must prove that Moonlight connects to `[::1]` and streams. The tests have
not measured this path. A later change can put the viewer in a network namespace
if IPv6 loopback does not work.

## Sources

- Measured choice: `/tmp/korri-reach/pick.md` and `measure-report.md`.
- Spike: `spike/reach-iroh`, commit `6655fa19c`, `spikes/reach/iroh/`.
- Research audit: `/tmp/korri-reach/audit-report.md`, claims 6 and 7.
- [RFC 6970](https://www.rfc-editor.org/rfc/rfc6970.txt).
- [Iroh relay guidance](https://github.com/n0-computer/docs.iroh.computer/blob/main/add-a-relay.mdx).
