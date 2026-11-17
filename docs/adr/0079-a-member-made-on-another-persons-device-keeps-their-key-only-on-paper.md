# A member made on another person's device keeps their key only on paper

**Owner decision, 2026-11-17.** Issue #178.

When an owned device creates a member who has no Korri device (ADR 0077),
Korri makes the member's person key from new recovery words (ADR 0072). It
shows the words and their QR code once, for paper. No device keeps the
member's private key. The member uses the words on their first own device.

The owner's devices never need to sign as the member, because the person
playing is picked with no proof (ADR 0078). So the key matters only when the
member later sets up a device of their own.

## Considered options

- **The local signer on the creating device keeps the member's key.**
  Nothing to lose. Rejected: a thief of the owner's device would then hold
  the member's key too.

## Consequences

- If the member loses the paper, they get a new person key. Their save
  history stays with the old key.
