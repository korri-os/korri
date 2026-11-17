# A guest's device plays on a stream after a code and one Accept

**Owner decision, 2026-11-17.** Issue #178.

A friend's own Korri device is not one of the owner's devices and is not a
family member's, so it gets no invite to the owner's stream. To play, the
friend enters a 10-symbol code that the device holding the session shows.
Then the person at that device presses Accept once. The friend's device
holds a controller seat as a guest (ADR 0076). It holds it until the session
ends or the friend disconnects (owner decision, 2026-11-17).

The code and Accept are the same as for a join (ADR 0070). They work near or
far, the key exchange on the code is a PAKE, and they trust no network. For the owner's own
devices, the person at the holding device approves nothing. A guest is
different, because the device is someone else's.

## Considered options

- **Nearby friends' devices see the invite and play with one Accept.** Less
  typing. Rejected: "nearby" trusts a network, which ADR 0070 rejects.

## Consequences

- A guest on a stream can play and watch only, as stream play already
  allows. It cannot end or change the session. Today a pass with
  `stream.launch` can end a session and call `SessionControlInvoke`,
  `SessionLeave` and `SessionResume` (`services/korrid/src/authorization.rs`),
  so the code must change.
- A guest's seat ends when the friend disconnects. Whether a dropped
  connection also ends it is not decided. That is an exception to
  seat reservation, which keeps a seat while a controller is disconnected.
- One code per visit. A family member's devices need no code (ADR 0081).
