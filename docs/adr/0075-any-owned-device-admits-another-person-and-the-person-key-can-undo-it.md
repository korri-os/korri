# Any owned device admits another person, and the person key can undo it

**Owner decision, 2026-11-17.** Issue #178. Design in progress: what an
admission grants and how long it lasts are not decided.

Any device the person owns can let another person use the person's devices.
The person key does not sign each admission. It stays cold (ADR 0069), and it
can undo an admission, as it undoes a removal (ADR 0071).

The reason is the same as for a join. A friend arrives, and the person has
only the device in their hand. Needing the cold person key for every visit
would make the feature unused.

A person with their own Korri device becomes family by the join flow
(ADR 0070): their device enters a 7-symbol code that the owner's device
shows, and the owner accepts them as family. A person with no device is
created on the owner's device (ADR 0079).

Family admissions are public on relays, like vouches and removals
(ADR 0069), with keys and dates only. A person can be family on any number of
owners' devices (owner decisions, 2026-11-17).

## Considered options

- **Only the person key admits.** Safe from a thief. Rejected: it needs the
  recovery words or a key holder whenever a friend comes over.
- **Each device admits people only for itself.** Like a console. Rejected:
  an admission on one device would mean nothing on the person's other
  devices, and Korri is one federation of them.

## Consequences

- The existing person pass design, in which the person key signs every pass
  (`docs/research/korrid-identity-protocol.md`), no longer fits. The form of
  an admission is not decided.
- A thief who holds an owned device can admit strangers until the person
  removes that device.
- Anyone who knows the owner's person key can see how many family keys the
  owner has, and when each was added or removed.
- A family member's save history copies to every owner's devices where they
  are family (ADR 0081).
