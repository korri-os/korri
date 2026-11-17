# A family member plays, installs and streams, but does not run the devices

**Owner decision, 2026-11-17.** Issue #178.

A family member is a person with their own person key and a lasting place
among another person's devices, such as a partner or a child. Korri has three
tiers: owner, family and guest (ADR 0076). The owner chose "family" as the
least formal clear word. A close friend can be family too.

On the owner's devices a family member can:

- Play the installed games with their own save history.
- Keep their own profiles and launch settings for a game.
- Play on a stream and start games on the owner's devices from their own
  devices, near or far.
- Install and remove games and plugins.

Only the owner changes device settings, such as Wi-Fi and displays, and only
the owner admits other people (ADR 0075).

A family member keeps their place until any of the owner's devices removes
them, or until they leave by themselves. There is no end date. After a
removal, the family member's save history stays on the owner's devices until
the owner deletes it, so a removal by mistake loses nothing. A removal takes
effect at once: a removed family member who plays on the owner's stream
loses their seat (owner decisions, 2026-11-17).

A family member does not need a Korri device. An owned device can create one,
with their own recovery words (ADR 0079).

## Considered options

- **Family members play only and cannot install.** Issue #6 names code that
  a stranger installs as the main risk to the home network. Rejected by the
  owner: family is trusted, and asking the owner for every install is
  friction.
- **Two tiers with fixed rights and a separate duration.** Rejected: a short
  visit is a guest, so family needs no end date of its own.
- **Names considered for the tier:** member, household, regular, resident,
  crew, local, housemate, friend. The owner found them too formal or too
  generic.

## Consequences

- A family member can bring code onto the owner's devices. Plugin signature
  and permission checks still apply to every install. Who approves a plugin
  or a publisher that a family member installs is not decided.
- Korri needs a way to make a person key on a device for someone who owns no
  device.
- The removal of a family member is not a device removal (CONTEXT.md,
  Removal). Whether the person key can undo it is not decided. How a device
  that was offline learns of it is not decided.
- Launch settings have no place for a person today, except a profile, and only a device of the
  same owner can change a streamed game's settings (ADR 0066). Where a family
  member's own launch settings live is not decided.
- "Family" already means a runner family. "Family" names the tier, "family
  member" names one person, and "runner family" names runners.
- The code's `household` pass tier is the old name for this tier.
- A family member sees the owner's whole library. Limits per person, for
  example for a child, are a later design (owner decision, 2026-11-17).
