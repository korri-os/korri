---
status: proposed
---

# Every release arrives by one install, in pieces from every source

**Owner decisions, 2026-11-20, design in progress.** A game that only a
peer holds can be played only by stream play today. The owner wants to play
it locally when that is possible, and the game should be copied to the
device. Today three separate mechanisms put game bytes on a device. Provider
acquisition (ADR 0016) and plugin artifacts (ADR 0012) share one serial queue
that runs in memory. Netplay's copy (ADR 0040) reads one file from one peer.
None of them can resume after an interruption.

## Decision

- **Install** is the one word for every way a release arrives on a device:
  from a provider, from an artifact, or from devices that hold the same
  release. Netplay's copy becomes an install. ADR 0040's "copied release"
  wording goes in a clean cut.
- One install manager in korrid runs every install. The design of the
  manager is part of this decision and is still open.
- An install takes pieces from every reachable install source at once,
  like a torrent, and checks each piece. It keeps verified pieces after an
  interruption and resumes when a source returns. The person can cancel it.
- An installed release stays in the device's library, as ADR 0040 already
  does for netplay, and the person removes it like any other game.
- The person chooses it. On a game that another device holds, "play here"
  is an action next to stream play; it is never picked for the person. The
  person can also install a game without playing it, and installs can wait
  in a queue.
- Play-here installs at once. If the predicted time is longer than one
  fixed threshold in korrid, Korri asks whether to stream meanwhile. The
  install goes on in the background, and the stream comes first.
- Stream play, netplay and a game that runs on the device come first.
  Installs slow down or pause for them on the device that plays and on the
  devices that serve pieces.
- Korri offers play-here only when this device has a runner for the game,
  enough free storage for the release, and a reachable install source.
  The device learns the release's size when the person presses the action,
  not from the catalog. Whether the device runs the game well is not yet
  decided (spike: no outside source rates games on ARM handhelds).
- The device owner's devices and family members' devices may install from
  any device of the federation without a prompt on that device (ADR 0077).
  Guests may not.
- File releases and folder releases are both in scope. A file release
  installed from a peer is the same release, because its bytes identify it.
  A folder release installed from a peer is a new folder release of the same
  game on this device. Only devices whose files match serve pieces of one
  folder install.
- The owner wants a sleeping holder woken to serve. korrid cannot wake a
  peer today.

## Considered options

- Copy only from the one device that holds the session, as netplay does.
  Rejected: the holder may sleep or leave, and one source is slower.
- A temporary copy that goes after the session. Rejected: every play copies
  again, and the game does not play offline.
- Korri picks the local route by itself when it can. Rejected: a silent copy
  of several GB surprises the person.
