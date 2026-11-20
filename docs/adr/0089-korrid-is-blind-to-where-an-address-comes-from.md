# korrid is blind to where an address comes from

**Owner decision, 2026-11-19.** This amends ADR 0041 and changes how ADR 0026's
relays are used.

The owner said: "I cannot couple our code base to anything like Tailscale or
ZeroTier. However, I want to support plugins for all of that." and "I need to
be able to do everything without Tailscale. The addition of something like
Tailscale just gives me those extra benefits for free."

## Decision

- korrid treats every address on every interface the same. It contains no
  check for a network's name, interface name or address range. Tailscale,
  ZeroTier and similar networks reach Korri only as addresses on an interface
  that their plugin brings up.
- All addresses go into one pool. korrid races the candidates and uses the
  fastest working path, and it moves to another path when that one fails.
  Nothing is preferred by name or by declaration; an overlay path wins only by
  measurement.
- Nostr relays stay, as the fallback. Korri must do everything with no private
  network installed. Relays carry first contact, joins, reach to peers with no
  working known address, and records (ADR 0027, 0069).
- A private network's plugin ships in every product image, idle until the
  person chooses it in setup or in settings. It signs in with the native
  program's own login link, shown as a QR code that the person approves on a
  phone. Korri never sees a password or key (ADR 0034). When the sign-in
  expires, the plugin reports that it needs sign-in again.
- Every account sign-in, Steam's and a private network's, uses one
  plugin-neutral sign-in card in the surface treaty. Each service keeps its own
  QR code; the card carries no service name in its type. The QR codes show the
  service's logo in the middle and use a designed, not plain, module style.
- Devices on one local network find each other with no server, on every
  network they join. An announcement carries a rotating identifier that only
  the person's own devices can recognise, so strangers on the network cannot
  follow a device from place to place. Addresses found this way join the same
  pool and expire soon, because a roaming device's local address is short-lived.
  Each pair of the person's devices derives the identifier from their device
  keys, and it changes every few minutes, so no new shared secret exists.
- A device tries a peer's last known addresses even after the stored record
  expires: fresh addresses first, then expired ones, then the relays. A wrong
  address fails the device-key check.
- While two devices are connected, the connection keeps its own paths current.
  korrid stops refreshing that peer through the relays until the connection
  ends.
- The private network is offered in an optional setup step after Wi-Fi and in
  settings, which show its sign-in state.
- Tailscale is the first such plugin. ZeroTier and others follow only after
  Tailscale is proven: the RP Mini V2, with the plugin, on a phone hotspot
  reaches aka at home. aka's Tailscale comes from its own system configuration;
  korrid uses it the same way, which is the point.

## Considered options

- **A plugin declares "prefer my paths".** Rejected for now: it needs a new
  declaration schema, and measurement already lets a fast private network win.
- **Stop using relays on a device that has a private network.** Rejected: joins,
  peers without that network and records for an offline device still need them.

## Consequences

- The Tailscale checks in `services/korrid/src/federation/interfaces.rs`,
  `federation/reach/relay.rs` and `federation/reach/controller.rs` are deleted.
  Devices that already run Tailscale outside Korri gain those paths with no
  plugin.
- Container bridges, work VPNs and other unrelated addresses also become
  candidates. A wrong address fails the device-key check, so the cost is wasted
  attempts, not a wrong peer.
