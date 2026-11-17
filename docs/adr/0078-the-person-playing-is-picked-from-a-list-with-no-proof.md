# The person playing is picked from a list, with no proof

**Owner decision, 2026-11-17.** Issue #178.

On a shared device, the person who plays is not always the device owner. A
family member says "it is me playing now" by picking their name from a list
on the device. Korri asks for no code and no confirmation on another device.

These are entertainment devices, and the owner wants this to be next to
invisible. If a child plays as a parent, the child's play goes into the
parent's save history. Earlier snapshots stay in that history and can be
restored (ADR 0014), so no progress is lost.

The person a session uses is the **lead**. When a device wakes, it shows its
list of people with the last lead already selected, so one press starts.
The list holds the device owner, every family member, and "Guest" (owner
decisions, 2026-11-17).

## Considered options

- **A device with one person logs in by itself.** The owner first wanted
  this. Rejected when he chose that "Guest" counts as a choice and that every
  family member is on every device: the list always has two or more people.
- **Play as the last lead and switch from the home screen.** Zero presses.
  Rejected: the next person can start a game as the wrong lead.
- **List only the family members who have played on that device.** Rejected:
  the owner wants every family member everywhere, with nothing to add.
- **Pick the name, then confirm on a device the family member owns.** Proof
  of who plays. Rejected: the family member would need their own device in
  hand every time.
- **Each controller belongs to a person.** Invisible when it works. Rejected:
  people swap controllers all the time.

## Consequences

- Anyone at the device can play as any family member, and as the owner.
- Every wake costs one press, even on a handheld only the owner uses.
- The lead and the device owner are now separate ideas on a device. With no
  proof of the lead, anyone at an owned device can act as the owner. How
  Korri keeps device settings and admissions to the owner (ADR 0077) is not
  decided.
- When several people play together on one device, the saves go to the
  lead. That matches stream play and netplay, where the holder's saves
  count. The other people's progress never enters their own histories.
