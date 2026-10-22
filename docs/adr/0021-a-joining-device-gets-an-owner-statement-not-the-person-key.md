# A joining device gets an owner statement, not the person key

**Owner decision, 2026-10-22.**

When a device joins a person's devices, a key holder signs the new device's
first owner statement. The person key is not copied to the new device. A stolen
or lost handheld then holds no person key, so revoking its device key is a
complete response. The cost: only key holders can approve a join. A person who
loses every key holder and protected nothing cannot add devices to that person
key again.

The device that created the automatic identity is the first key holder. The
person can make another device a key holder on purpose: in Settings on a key
holder, confirmed on both devices, sent over the encrypted join channel. Each
extra key holder is one more copy of the person key, chosen knowingly. Only key
holders show a join request.

## Considered options

- **Copy the person key on approval.** Rejected: every approved device could act
  as the person until an identity switch on every device. This is how the Odin 2
  Portal received its key on 2026-10-22, through a backup restore.
- **Ask at each approval.** Rejected: it puts a security choice in front of
  every person at every join.
