# A guest has no lasting place, and their saves last one visit

**Owner decision, 2026-11-17.** Issue #178.

People who use another person's devices come in two kinds. A guest is there
for now: a friend at a party, someone who plays too rarely to need their own
space, or a stranger at a device in a public place. A guest holds controller
seats for one visit and keeps nothing after it. The other kind has a lasting
place among the owner's devices (ADR 0077).

A guest needs no person key. When a guest plays at a device, their saves last
for the visit only, and Korri deletes them when the visit ends. A friend whose
own Korri device joins the owner's stream is also a guest: their key puts
their name on screen, but it does not give them a lasting place. A lasting
place is always a separate, explicit step.

## Considered options

- **Keep guest saves on that device under one shared guest.** Progress
  survives to the next visit. Rejected: every guest shares one save history,
  and old saves pile up on the device.
- **The guest plays as the device owner, with the owner's saves.** Today's
  behaviour, and no work. Rejected: a guest can overwrite the owner's
  progress.
- **Anyone with a key whom the owner lets in gets a lasting place.** Fewer
  kinds of person. Rejected: one stream invite would then give lasting access.

## Consequences

- A visit needs a clear end, because the saves go with it.
- A guest who wants to keep progress must get a lasting place before the
  visit ends.
