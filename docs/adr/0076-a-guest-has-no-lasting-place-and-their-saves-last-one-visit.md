# A guest has no lasting place, and their saves last one visit

**Owner decision, 2026-11-17.** Issue #178.

People who use another person's devices come in two kinds. A guest is there
for now. It can be a friend at a party, someone who plays too rarely to need
their own space, or a stranger at a device in a public place. A guest holds
controller seats for one visit and keeps nothing after it. The other kind has
a lasting place among the owner's devices (ADR 0077).

A guest needs no person key. When a guest plays at a device, their saves last
for the visit only, and Korri deletes them when the visit ends. A friend's own
Korri device can play on the owner's stream (ADR 0080). That friend is also a
guest. Their key puts their name on screen, but it does not give them a
lasting place. A lasting place is always a separate, explicit step.

Anyone at a device can start a visit: "Guest" is always in the device's list
of people. A visit ends when someone picks another person on the device, or
when the device sleeps or shuts down (owner decisions, 2026-11-17).

## Considered options

- **Keep guest saves on that device under one shared guest.** Progress
  survives to the next visit. Rejected: every guest shares one save history,
  and old saves pile up on the device.
- **The guest plays as the device owner, with the owner's saves.** Today's
  behaviour, and no work. Rejected: a guest can overwrite the owner's
  progress.
- **End a visit only when someone picks "End visit".** No surprises.
  Rejected: people forget, and guest saves pile up.
- **Only an owner or a family member can start a visit.** Rejected: it does
  not work for a device in a public place with nobody there.
- **Anyone with a key whom the owner lets in gets a lasting place.** Fewer
  kinds of person. Rejected: one Accept on a stream would then give lasting
  access.

## Consequences

- A short sleep in the middle of a party ends the visit and deletes its
  saves. Light sleep and deep sleep keep the live session frozen. What
  happens to a guest's frozen game at that point is not decided.
- Saves are records, which devices keep until every device has them. Guest
  saves are an exception. Whether they spread to other devices during a
  visit is not decided.
- A guest can play any game that the device in front of them can play,
  including a game streamed from another of the owner's devices. A guest at
  the TV can start a game on the owner's PC. Physical access to a device is
  already an accepted risk, as for a console in a living room.
- A guest who wants to keep progress must become family before the visit
  ends. Then the visit's saves move into the new family member's save
  history (owner decision, 2026-11-17).
