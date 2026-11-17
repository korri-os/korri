# A guest's device joins a stream with a code and one Accept

**Owner decision, 2026-11-17.** Issue #178.

A friend's own Korri device is not one of the owner's devices and is not a
family member's, so it gets no invite to the owner's stream. To play, the friend
enters a 7-symbol code that the device holding the session shows. Then the
person at that device presses Accept once. The friend's device holds a controller seat as a
guest (ADR 0076) until the session ends or the friend disconnects.

The flow is the join flow (ADR 0070): it works near or far, the code is a
PAKE, and it trusts no network. Under the same-owner rule, player 1 sees who
joined but approves nothing. A guest is different, because the device is
someone else's.

## Considered options

- **Nearby friends' devices see the invite and join with one Accept.** Less
  typing. Rejected: "nearby" trusts a network, which ADR 0070 rejects.

## Consequences

- A guest on a stream can play and watch only, as stream play already
  allows. It cannot end or change the session.
- One code per visit.
