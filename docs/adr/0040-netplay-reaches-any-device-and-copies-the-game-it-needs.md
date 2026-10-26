# Netplay reaches any device and copies the game it needs

**Owner decision, 2026-10-26.** Design in progress.

Stream play works on the RG35XX SP and Pro, but those devices limit it. On
2026-10-26 the SP's CPU saturated while it ran the game and encoded. H.264
4:2:0 from its cedrus encoder made pixel art look soft at about 11 Mbit/s.
The stream dropped after about 16 s, and then Sunshine crashed. In netplay,
each device runs its own copy of the game, so no device has to encode video.

## Decision

- The first netplay slice must work between any two devices of the
  federation, wherever they are. It is not limited to one network.
- Netplay traffic uses the federation's own connections (ADR 0026, ADR 0036).
  So a third-party lobby or relay server, such as RetroArch's, is not used.
  korrid forwards RetroArch's netplay port through the iroh tunnel it already
  uses for a stream viewer. RetroArch itself stays unchanged.
- The first slice takes one receiving device per session. Several receiving
  devices, each choosing its own route, is the goal and is in issue #275.
- Both devices need the same release, because a file release is identified by
  its content bytes. When the receiving device does not have it, Korri copies
  the release from the device that holds the session, then starts netplay.
- The copied release stays in the receiving device's library, as if the person
  had added it. The person can remove it like any other game. It is stored
  like a downloaded game (ADR 0016): in korrid's cache under its release key
  (ADR 0030), checked against that key, and added to the library through the
  records of ADR 0013.
- Netplay can open during play. The device that holds the session does not
  have to start the game in a netplay mode. RetroArch can start hosting while
  a game runs, and a device that joins gets the current state.
- In the first slice, a runner can offer netplay when it is a RetroArch core
  whose libretro core info states `savestate_features = "deterministic"`,
  such as Snes9x 2010. Korri keeps no hand-written list of cores.
- A game that no runner on the receiving device can play in netplay, such as a
  large PC game, always uses stream play.
- When both routes are possible, the person on the receiving device chooses
  between stream play, which starts at once, and netplay, which may first wait
  for the copy. This choice is a first version. The experience is to be
  designed again after the feature works.
- The device that holds the session controls it, as in stream play. It hosts
  netplay, and only it can end, freeze, load a state or reset. The receiving
  device can play and leave. When the holder ends the session, netplay ends
  and the receiving device's game ends with it. When the receiving device
  leaves, the holder plays on.
- The receiving device's controllers take the first free controller seat in
  the holder's session, from the same pool as stream play. korrid makes
  RetroArch's input port match that seat.
- Korri exposes no RetroArch netplay settings in this slice. RetroArch's
  defaults hold, and Korri controls only hosting, joining and the password.
- Only the holder captures a save snapshot from a netplay session. The
  receiving device starts from the holder's state and captures none, so
  netplay never makes a split.

## Considered options

- **Netplay only when the release is already there, otherwise stream play.**
  Rejected: a device without the game would always get the weaker route.
- **Netplay only, with no fallback.** Rejected for the same reason.
- **Keep the copy out of the library and delete it when space runs low.**
  Rejected: it adds a kind of content that is on a device but not in its
  library.
- **Delete the copy when the session ends.** Rejected: every netplay would copy
  the game again, and a large game would delay every start.
- **Korri chooses the route automatically.** Not chosen for now: stream play
  starts at once and fits large games, and netplay looks better but can wait
  for a copy. The trade-off depends on the game and the person.
- **Either device controls the session for everyone.** Rejected: RetroArch
  gives the netplay host authority, and two people could act at the same time.
- **The receiving device plays on alone when the holder goes.** Rejected: the
  session would split into two games, with saves that split too.
- **A Korri netplay protocol that korrid carries.** Rejected: it would rebuild
  RetroArch's rollback and resync.
- **RetroArch chooses the player.** Rejected: netplay players would fall
  outside Korri's seats and seat reservations.
- **Copy the release into a library folder.** Rejected: the device may have no
  library folder for that system, and a layout rule would have to be invented.
- **Only the proof game's core.** Rejected: other cores would wait, although
  upstream states that they are safe.
- **Every RetroArch core.** Rejected: a core without deterministic savestates
  drifts, and the person learns it only during play.
- **Both devices capture snapshots.** Rejected: two snapshots from the same
  start would make a split for the person to resolve after many sessions.
- **The receiving device keeps a private copy of its saves.** Rejected: it adds
  a kind of save that is outside the save history.
- **Stream play first, then a switch to netplay in the same session.** Not
  chosen for now: it moves seats, input and the picture between routes during
  play, and nothing in Korri does that today.

## Consequences

- Lag depends on the path iroh finds. A path relayed through another device
  of the federation adds delay.
- If the holder drops off the network, the receiving device loses the game,
  although it runs the game itself.
- A core that states deterministic savestates can still drift. Only one core
  is proved by the first slice.
- A savestate the receiving person makes during netplay is lost.
- Game content now moves between devices, and not only from providers.
- A kept game brings that game's records to the receiving device (ADR 0027).
- A large release, such as a disc image of several GB, can fill a small
  handheld and delays the first netplay with it.

## Proof of the first slice

The RG35XX SP holds a session of *Teenage Mutant Ninja Turtles IV - Turtles
in Time*. The RG35XX Pro is on a different network, such as a phone hotspot,
and starts without the release. The Pro receives the invite, chooses netplay,
gets the copy, and its controller drives player 2. The SP then ends the
session, and the Pro's game ends with it.
