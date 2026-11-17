# Any owned device removes another, and the person key overrules

**Owner decision, 2026-11-16.** Design in progress: how an overrule is
recorded is not decided.

Any device the person owns can remove another owned device. The person key
can undo a removal. When a device is removed, the devices it vouched for stay
owned.

The reason: when a device is stolen, the person must be able to remove it at
once from whatever device is in their hand. Reaching the cold person key
(ADR 0069) every time would make that slow.

## Considered options

- **Only the person key removes.** Safe from a thief. Rejected: it needs the
  cold key even to retire an old device.
- **Removal also removes the devices it vouched for**, or lists them for
  review. Rejected by the owner as more than the common case needs.

## Consequences

- A thief who holds an owned device can remove the person's other devices
  until the person uses the person key to undo it.
- A device that a thief vouched for stays owned when the stolen device is
  removed. The person must remove it on its own.
