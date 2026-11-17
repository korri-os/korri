# A member plays, installs and streams, but does not run the devices

**Owner decision, 2026-11-17.** Issue #178. The name "member" is not final.

A member is a person with their own person key and a lasting place among
another person's devices, such as a partner or a child. Korri has three
tiers: owner, member and guest (ADR 0076).

On the owner's devices a member can:

- play the installed games with their own save history;
- keep their own profiles and launch settings for a game;
- play on a stream and start games on the owner's devices from the member's
  own devices, near or far;
- install and remove games and plugins.

Only the owner changes device settings, such as Wi-Fi, displays and library
folders, and only the owner admits other people (ADR 0075).

A member does not need a Korri device. An owned device can create a member,
with the member's own recovery words (ADR 0072).

## Considered options

- **Members play only and cannot install.** Issue #6 names code that a
  stranger installs as the main risk to the home network. Rejected by the
  owner: a member is trusted, and asking the owner for every install is
  friction.
- **Two tiers with fixed rights and a separate duration.** Rejected: a
  short visit is a guest, so a member needs no end date of its own.

## Consequences

- A member can bring code onto the owner's devices. Plugin signature and
  permission checks still apply to every install.
- Korri needs a way to make a person key on a device for someone who owns no
  device.
