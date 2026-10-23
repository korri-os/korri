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
- Devices connect directly by hole punching. When that fails, any device of the
  same federation that is online and reachable at that moment may relay the
  encrypted traffic. No device is ever designated for this; any device can be
  offline at any time. A device relays only while it is on external power and
  an unmetered network.
- Korri never asks the person to change their router, such as forwarding a port
  by hand.
- No third-party relay carries traffic, including n0's public relays and
  Tailscale's DERP. Korri accepts that some pairs of networks cannot connect,
  and it says so clearly when that happens.
- Small records, such as settings and the person name, are also kept
  encrypted on several public Nostr relays, so they arrive when no other device
  is on.
- Korri hosts a service of its own only when that service is free to run and
  durable, so that it keeps working with no one paying for it or maintaining
  it.

## Considered options

- Tailscale or a hosted relay like DERP reaches every network. It was rejected
  because it couples Korri to an outside service, or to a server in the middle.
- Asking for a port forward makes a home device reachable almost everywhere. It
  was rejected because the person must never configure their router.

## Consequences

- Coverage is lower than Tailscale's. Research put direct hole punching at
  about 70% to 90% of network pairs (unaudited). Symmetric NAT on mobile
  carriers is the common failure.
- For stream play across the internet, the connection must also carry
  Sunshine's and Moonlight's traffic, not only Korri's own messages.
