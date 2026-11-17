# An owned device joins by identity switch, keeping its data

**Owner decision, 2026-10-22.**

A device that already has its own automatic identity joins another person key
from Settings ("Join my other devices"). It uses the same join confirmation code and
Accept as setup (ADR 0070). Underneath it is an identity switch that transfers local data, behind
one confirmation. The person keeps saves and play history, and they start to
sync with the new owner's devices.

The switch revokes the Moonlight certificates korrid granted under the old
owner. Streaming between Korri devices re-pairs without a prompt (ADR 0024).

The switch keeps Moonlight clients that a person paired by PIN, such as a phone
or PC Moonlight app. They are not tied to the owner key, and clearing them would
break streaming the person set up outside Korri. The identity-switch
confirmation must say that only Korri's own device trust is reset.

## Considered options

- **Reset to setup, then join.** Rejected: local data is lost unless the person
  backs it up first.
- **Operator task only.** Rejected: every existing device would need hands-on
  work.
