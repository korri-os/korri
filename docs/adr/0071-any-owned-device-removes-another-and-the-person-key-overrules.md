# Any owned device removes another, and the person key overrules

**Owner decision, 2026-11-16.**

Any device the person owns can remove another owned device. The person key
can undo a removal. When a device is removed, the devices it vouched for stay
owned.

The reason: when a device is stolen, the person must be able to remove it at
once from whatever device is in their hand. Reaching the cold person key
(ADR 0069) every time would make that slow.

A removal is the owner statement with status "revoked", signed by the
removing device's key. It lists the devices that the removed device had
vouched for, as far as the remover knows. Any other vouch from the removed
device is invalid. A device sets its own timestamps, so without that list a
removed device could sign a vouch dated before its removal.

The person key undoes a removal by signing a newer owner statement with status
"owned" for that device. It overrides removals signed by devices. A removal
signed by the person key stays final, as today (owner decisions,
2026-11-16).

## Considered options

- **Only the person key removes.** Safe from a thief. Rejected: it needs the
  cold key even to retire an old device.
- **Removal also removes the devices it vouched for**, or lists them for
  review. Rejected by the owner as more than the common case needs.
- **Accept that a removed device can back-date vouches.** Rejected: a thief
  would keep a way to add devices.
- **A separate undo record that names the removal.** Rejected: one more record
  kind, where a newer owner statement does the same.

## Consequences

- A thief who holds an owned device can remove the person's other devices
  until the person uses the person key to undo it.
- A device that a thief vouched for stays owned when the stolen device is
  removed, if the remover knew of it. The person must remove it on its own.
- A vouch that the remover had not seen yet becomes invalid. That device must
  join again.
- Today a revocation is final whoever signs it. That rule changes for
  removals signed by devices.
