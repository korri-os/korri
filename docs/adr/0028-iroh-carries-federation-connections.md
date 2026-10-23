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
7.1 seconds. A home relay is reachable from another network only through
IPv6 or port mapping. Symmetric NAT on both sides needs a reachable
relaying device. Only one configured public Nostr relay was live:
relay.primal.net. relay.nostr.band was dead.

A weak phone link delivered 4.1 Mbit/s at 5 Mbit/s offered, with 18% loss.
QUIC adds rate control above Moonlight's own rate control. These numbers
do not prove that a real Moonlight session works well.

UPnP opens a mapping only on the local router. Behind carrier-grade NAT,
inbound traffic still fails unless the ISP runs PCP and the router bridges
UPnP to it (RFC 6970). Iroh's documentation calls n0's public relays
"for development and testing", says they are rate-limited, and recommends
against production workloads. Korri does not use them.

## Sources

- Measured choice: `/tmp/korri-reach/pick.md` and `measure-report.md`.
- Spike: `spike/reach-iroh`, commit `6655fa19c`, `spikes/reach/iroh/`.
- Research audit: `/tmp/korri-reach/audit-report.md`, claims 6 and 7.
- [RFC 6970](https://www.rfc-editor.org/rfc/rfc6970.txt).
- [Iroh relay guidance](https://github.com/n0-computer/docs.iroh.computer/blob/main/add-a-relay.mdx).
