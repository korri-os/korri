# Any owned device vouches for a join, and the person key stays cold

**Owner decision, 2026-11-16.** Supersedes ADR 0021. Design in progress: the
form of a vouch, recovery and removal are not decided.

Any device the person already owns can approve a join. It vouches for the new
device with its own device key. The person key no longer signs each join. It
stays cold, and the person uses it only to recover and to remove devices.

The owner's reason: "If you and I, as the developer and product owner, are
having trouble with this scenario, the standard user is going to face problems
as well." Under ADR 0021 the phone was the arbiter for every device. A join
needed the phone, Amber, and a relay that both could reach. Joins failed in
ways the owner could not see (issue #434, D12).

## Considered options

- **Keep the phone as the only key holder and fix the experience.** Least
  work. Rejected: it does not federate, and the phone stays a single point of
  failure.
- **A threshold owner key (FROST, for example 2 of 3 devices).** Stronger.
  Rejected: complex, the Nostr tooling for it is young, and the owner wants
  nothing that he has to maintain.

## Consequences

- A stolen owned device can admit more devices until the person removes it.
  Removal must therefore also deal with the devices that it vouched for.
- Peers must accept a device whose ownership comes from a vouch, not only from
  a statement that the person key signed.
- Joins no longer need a phone signer. Amber stays a way to hold the cold
  person key.
