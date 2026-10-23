---
status: accepted
---

# The device korrid can launch on its peers

**Owner decision, 2026-10-22 (stream-play contract O1).** The korrid on a
product device lists its peers' games and can launch a game on a peer. This
follows ADR 0017: one device korrid runs the local portal's complete path,
not a second brain process on the device.

## Problem

Product devices run `KORRID_MODE = "host"`. The native peer launch path,
`UpstreamRegistry::prepare_stream`, and the injected `LinuxStreamViewer`
were composed only in brain mode. A product device could hold a session
but could not select one on another device.

## Decision

- Compose the host-mode peer registry from the same federation directory
  and credentials used by its authenticated peer listener, as brain mode
  does with `with_federation`.
- The local portal's catalog includes local games and peers' games. A peer
  catalog request still returns only the holding device's own games. This
  prevents a recursive federation fan-out and keeps source identity intact.
- A local portal request with an explicit peer selector uses the existing
  native `prepare_stream` path. It refuses while this device holds a live
  local session. Peer calls to prepare and status still address this
  device's own holder, not its selected viewer session.
- Only an explicitly selected remote session is this device's session.
  An idle device must not adopt a peer's live session merely by polling it.
- The registry takes an injected `LinuxStreamViewer`. Product composition
  uses the native Moonlight adapter and certificate-provision broker; tests
  use `TestLinuxStreamViewer`. The adapter reads occupied local seats from
  inputd. No PIN or direct portal-to-peer RPC is added.
- Local control follows that same selected route. The device that started
  a far session can control it. A device playing on its stream controls
  only its viewer; the far holder retains session authority.

## Considered options

- **A second brain on each device.** Two processes would split the local
  portal's authority and require a second session selection. ADR 0017
  already rejected this split for discovery and the store.
- **A new peer launcher.** This would duplicate native transport, exact
  launch identity, certificate provisioning, compensation and viewer
  lifecycle already present in `UpstreamRegistry`.
- **Defer.** A product device could not play on another device's stream.

## Consequences

- Local catalog reads now make bounded parallel peer calls. An unavailable
  peer contributes a catalog failure; it does not erase local games.
- Peer display labels are not unique identities. The existing label-based
  launch selector refuses ambiguous labels rather than guessing.
- Session selection remains process-local, as in the existing native path.
  This decision does not introduce a new persistent selected-route schema.
- The viewer and game share the native launch-unit machinery, but only
  viewer units may read Moonlight's client key directory. Default game
  units hide it, as they already hide Sunshine's private state.
