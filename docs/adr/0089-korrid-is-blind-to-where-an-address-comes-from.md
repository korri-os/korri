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
- Tailscale is the first such plugin. ZeroTier and others follow only after
  Tailscale is proven: the RP Mini V2 on a phone hotspot reaches aka at home.

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
