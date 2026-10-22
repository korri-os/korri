# Automatic identity waits for the end of setup

**Owner decision, 2026-10-22.**

A new device creates its automatic identity only when setup ends without a
join. Until then it has no owner. A device that joins during setup therefore
takes its first owner statement as a plain first binding: no identity switch,
no data-disposition question, and no lost pairings. The cost: nothing that
needs an owner, such as save sync or peers, works before setup ends.

Before this decision the local signer created the automatic identity at first
boot, so every later join was an identity switch.

## Considered options

- **First boot, as before.** Rejected: every join becomes an identity switch.
- **First boot, replaceable until setup ends.** Rejected: it needs a new
  exception to the rule that an owned device accepts statements only from the
  same owner.
