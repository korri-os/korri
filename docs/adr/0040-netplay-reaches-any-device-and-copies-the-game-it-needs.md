# Netplay reaches any device and copies the game it needs

**Owner decision, 2026-10-26.** Design in progress. The transport, the
session model, route choice and saves are not decided.

Stream play works on the RG35XX SP and Pro, but those devices limit it. On
2026-10-26 the SP's CPU saturated while it ran the game and encoded. H.264
4:2:0 from its cedrus encoder made pixel art look soft at about 11 Mbit/s.
The stream dropped after about 16 s, and then Sunshine crashed. In netplay,
each device runs its own copy of the game, so no device has to encode video.

## Decision

- The first netplay slice must work between any two devices of the
  federation, wherever they are. It is not limited to one network.
- Netplay traffic uses the federation's own connections (ADR 0026, ADR 0036).
  So a third-party lobby or relay server, such as RetroArch's, is not used.
- Both devices need the same release, because a file release is identified by
  its content bytes. When the receiving device does not have it, Korri copies
  the release from the device that holds the session, then starts netplay.
- The copied release stays in the receiving device's library, as if the person
  had added it. The person can remove it like any other game.

## Considered options

- **Netplay only when the release is already there, otherwise stream play.**
  Rejected: a device without the game would always get the weaker route.
- **Netplay only, with no fallback.** Rejected for the same reason.
- **Keep the copy out of the library and delete it when space runs low.**
  Rejected: it adds a kind of content that is on a device but not in its
  library.
- **Delete the copy when the session ends.** Rejected: every netplay would copy
  the game again, and a large game would delay every start.

## Consequences

- Game content now moves between devices, and not only from providers.
- A kept game brings that game's records to the receiving device (ADR 0027).
- A large release, such as a disc image of several GB, can fill a small
  handheld and delays the first netplay with it.
