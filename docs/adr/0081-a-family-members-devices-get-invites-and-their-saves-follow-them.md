# A family member's devices get invites, and their saves follow them

**Owner decision, 2026-11-17.** Issue #178.

The devices that a family member owns are treated like the owner's own
devices for stream play: they get invites with no code. A guest's device
needs a code and an Accept (ADR 0080). This is the difference a lasting place
makes.

A family member's save history copies to the family member's own devices,
as records spread between devices that meet (ADR 0027). A child who plays on
the family TV and later gets a handheld keeps their progress.

## Considered options

- **Family members also join a stream with a code.** One flow for everyone
  who is not the owner. Rejected: a partner would type a code every evening.
- **Family saves stay on the owner's devices.** Simpler. Rejected: progress
  would be stuck there.

## Consequences

- The owner's devices must accept devices that the family member's person
  key vouched for (ADR 0069), not only the owner's own devices.
- Records now cross from one person's devices to another person's devices.
  Until now they spread only among one person's devices.
