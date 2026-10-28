# Devices reach each other without a relay server

**Owner decision, 2026-10-22.** Design in progress. How the connection is built
and which library carries it are not decided.

The devices of one federation must reach each other from any network with
internet access, with no dependency on Tailscale and no server that Korri or the
person hosts to carry their traffic.

## Decision

- Devices find each other and arrange a connection through public Nostr relays.
  They learn their public address from public STUN servers. Both kinds of
  server only see that devices are active; they never carry the connection.
- Devices try every method that needs nothing from the person: IPv6, then
  automatic router port mapping (UPnP, NAT-PMP or PCP), then hole punching.
  When all of them fail, any device of the
  same federation that is online and reachable at that moment may relay the
  encrypted traffic. No device is ever designated for this; any device can be
  offline at any time. A device relays only while it is on external power and
  an unmetered network.
- When two devices are together and cannot reach each other, one device starts
  a nearby link, a private Wi-Fi network, and the other joins it on its own.
  This works with no router and no internet. Bluetooth or Wi-Fi may help the
  devices find each other. Bluetooth carries traffic only where its bandwidth
  is enough; a Moonlight stream needs several megabits per second, so the
  stream always goes over Wi-Fi.
- When a device already has Tailscale or a similar VPN, Korri uses it as one
  more method. Korri never requires one.
- Korri never asks the person to change their router, such as forwarding a port
  by hand.
- No third-party relay carries traffic, including n0's public relays and
  Tailscale's DERP. ADR 0041 changes this for devices that have the optional
  Tailscale plugin. Korri accepts that some pairs of networks cannot connect,
  and it says so clearly when that happens.
- Small records, such as settings and the person name, are also kept
  encrypted on several public Nostr relays, so they arrive when no other device
  is on.
- Korri hosts a service of its own only when that service is free to run and
  durable, so that it keeps working with no one paying for it or maintaining
  it. The Korri-run fallback Nostr relay that the device-join design proposed
  (ADR 0025 on the `docs/device-join-design` branch) is dropped: devices use
  public relays only.
- The connection library is chosen from a spike that builds the hardest case,
  two devices away from home, with the strongest candidates.

## Considered options

- Tailscale or a hosted relay like DERP reaches every network. It was rejected
  because it couples Korri to an outside service, or to a server in the middle.
- Asking for a port forward makes a home device reachable almost everywhere. It
  was rejected because the person must never configure their router.

## Consequences

- Two devices that are both away from home, with no reachable device of the
  federation, are the weakest case. The owner says it is a common one.
- Coverage is lower than Tailscale's. Research put direct hole punching at
  about 70% to 90% of network pairs (unaudited). Symmetric NAT on mobile
  carriers is the common failure.
- For stream play across the internet, the connection must also carry
  Sunshine's and Moonlight's traffic, not only Korri's own messages.
