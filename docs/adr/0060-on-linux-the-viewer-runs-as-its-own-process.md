---
status: accepted
---

# On Linux the viewer runs as its own process

**Owner decision, 2026-11-11.** Issue #391 replaces Moonlight Embedded with a
viewer that Korri owns. On Linux that viewer runs as its own process. It does
not run inside the shell or inside korrid. korrid gets the viewer's lifecycle
as typed events over a local socket.

A typed lifecycle needs a viewer that Korri owns. It does not need a shared
process.

## Considered options

- **Inside the shell process.** The shell uses Slint's software renderer and
  cannot draw decoded video under its own widgets. A decoder or packet-parser
  fault would stop the only screen. The C protocol core would run in the
  process that holds the korrid capability.
- **Inside korrid.** korrid has no window and holds the device keys.

## Consequences

- The gameplay overlay stays a second window on Linux, so korrid keeps its
  compositor focus handling.
- The typed socket replaces three sources of viewer state: systemd unit
  state, compositor process IDs and the status socket of Moonlight patch
  `0023`.
- This decision is for Linux. On Android the viewer runs inside the app.
