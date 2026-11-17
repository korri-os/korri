# The person key is a Korri-only key made from recovery words

**Owner decision, 2026-11-16.**

A person key is used only for Korri. It is made from the person's 12 recovery
words (ADR 0072). It is not the person's social Nostr identity, and Korri
does not ask a phone signer such as Amber for it.

The reasons: the public device list (ADR 0069) is then not tied to the
person's public identity. A stolen Korri key cannot post as the person.
Because the key guards only entertainment devices, Korri can keep recovery
simple.

## Considered options

- **The person's social Nostr key.** One identity everywhere. Rejected: a
  random key cannot become 12 words, the device list would point at the
  person's public identity, and recovery would have to stay strict.

## Consequences

- Korri cannot read the person's name or picture from a Nostr profile. The
  person types a name at setup.
- Later social features, such as seeing friends by their Nostr key, need a
  way to connect the two keys.
- The owner's current devices move to a new person key once.
- They move by a one-off identity switch on each device that keeps its data
  (ADR 0023): the old key approves each move once in Amber, then Korri stops
  using it (owner decision, 2026-11-16). This is an operation, not code that stays.
