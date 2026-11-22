# iroh owns how devices reach each other

**Owner decision, 2026-11-22.** This supersedes ADR 0026's rule against
third-party relays and its Nostr signalling for reach, and the reach parts of
ADR 0089. ADR 0089's rule that korrid names no network stays.

The owner said: "I don't want Korri involved in any of this. How can we remove
any custom code and just let Tailscale or iroh or a home network just manage
themselves, and we don't do anything except what those protocols already do."
Asked to choose, the owner chose iroh's own discovery and n0's public relays
over keeping Korri free of third-party servers.

## Decision

- Each device runs one iroh endpoint whose identity is its device key. To reach
  a peer, korrid dials the peer's key and nothing else. iroh finds the peer and
  chooses and changes paths itself: the home network, a private network such
  as Tailscale, IPv6, hole punching or a relay.
- korrid uses iroh's own discovery and n0's public relays, as iroh ships them.
- Everything a peer exchanges with another, including a stream, travels on that
  one connection. When a path changes, the connection moves; nothing above iroh
  notices.
- korrid keeps no reach logic of its own: no Nostr reach signalling or offers,
  no relay listener on devices, no STUN list, no local discovery of its own, no
  route ordering, and no rebuilding of forwards after a path change. When iroh
  reports that the connection is gone, korrid says so once and stops.
- Nostr stays for what ADR 0074 lists apart from reach: records for devices that
  were off, and vouches and removals.

With the owner's approved addition, korrid caches iroh's last-seen
`EndpointAddr` values for roster peers and restores them through iroh's
`MemoryLookup` alongside n0 and mDNS. A new connection can then use a still
reachable cached address when n0 discovery is down. This disposable cache
holds at most 16 opaque addresses per peer; iroh still chooses every path.

## Considered options

- **iroh with no relay and no discovery service.** No third-party servers, but
  remote play would work only through a private network on both devices.
- **Keep Korri's custom reach (ADR 0026, 0089).** No third-party servers, but it
  is the code that got the home IP banned by public relays and left streams in
  a recovery loop when a path changed.

## Consequences

- n0's discovery and relay servers see when a person's devices are online and
  can carry encrypted traffic when no direct path works. Korri depends on n0
  keeping them free and running.
- The custom reach code, its tests and its issues (#270, #313, #324, #339, #596)
  are removed or closed, not fixed.
