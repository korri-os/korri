---
status: accepted
---

# A device uses its own metadata first, and a peer's when it has none

**Owner decision, 2026-11-13**, given after the first look at Velvet on usu
(a Galaxy Z Fold8 that only streams from peers). Verbatim:

> The client is always the winner if it has the information needed. So in this
> case, since there's no art, of course we would fall back to the remotes for
> their art. And that goes for pretty much any metadata across the board for
> every piece of content.

When a device shows content, it uses its own metadata for each fact it holds.
When it lacks a fact, it uses the fact that a peer holding the content has.
This covers art and every other piece of metadata, such as the system, for all
content, not only games. We chose this over "a device shows only what it holds
itself" (the rule today: `services/korrid/src/surface/model.rs` builds a peer's
game with no art, no system and no cover, "A peer's copy has no cover here").
That rule leaves a device that only streams, such as a phone, with nothing to
show.

## Consequences

- The local fact wins for each fact separately, not for the whole record. A
  device with its own title but no art shows its own title and a peer's art.
- The surface still talks only to its own korrid. That korrid gets the peer's
  fact over the peer RPC (AGENTS.md: "The shell's brain is always the korrid on
  its own device"). Plugins and secrets stay on the devices that hold them.
- Open: how to choose when two peers disagree, and whether a peer's fact is
  copied to the device or read each time. This decision does not settle either.
