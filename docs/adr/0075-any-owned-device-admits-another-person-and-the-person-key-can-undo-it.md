# Any owned device admits another person, and the person key can undo it

**Owner decision, 2026-11-17.** Issue #178. Design in progress: the form of
an admission is not decided.

Any device the person owns can make another person family (ADR 0077). The
person key does not sign each admission. It stays cold (ADR 0069), and it can
undo an admission, as it undoes a removal (ADR 0071). A guest needs no
admission (ADR 0076).

The reason is the same as for a join. Someone becomes family, and the person
has only the device in their hand. Needing the cold person key whenever
someone becomes family would make the feature unused.

A person with their own Korri device becomes family with the same code and
Accept as a join (ADR 0070). Their device enters a 10-symbol code that the
owner's device shows. The owner then accepts them as family. A person with no
device is created on the owner's device (ADR 0079).

Family admissions are public on relays, like vouches and removals
(ADR 0069), with keys and dates only. A person can be family on any number of
owners' devices (owner decisions, 2026-11-17).

## Considered options

- **Only the person key admits.** Safe from a thief. Rejected: it needs the
  recovery words or a key holder whenever someone becomes family.
- **Each device admits people only for itself.** Like a console. Rejected:
  an admission on one device would mean nothing on the person's other
  devices, and Korri is one federation of them.

## Consequences

- The existing person pass design, in which the person key signs every pass
  (`docs/research/korrid-identity-protocol.md`), no longer fits.
- A thief who holds an owned device can admit strangers until the person
  removes that device.
- Anyone who knows the owner's public key can see how many family keys the
  owner has added, and when.
- A family member's save history follows them to every owner's devices where
  they are family (owner decision, 2026-11-17). How the saves reach a second
  owner's devices is not decided.
- ADR 0069 names two uses of the person key: recovery and undoing a removal.
  This ADR adds a third: undoing an admission.
