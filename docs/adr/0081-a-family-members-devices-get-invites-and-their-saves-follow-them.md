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

- The owner's devices must accept the family member's devices, whose
  ownership comes from the family member's own vouches (ADR 0069).
- ADR 0027 spreads records between "devices of the federation". A family
  member's devices are now part of that federation for the family member's
  own records. Which other records they receive is not decided.
