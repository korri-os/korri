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
- One install manager in korrid runs every install. One install runs at a
  time, from a queue that the person can reorder. Play-here and netplay go
  to the front of the queue. Pieces of that one install come from many
  sources in parallel.
- A provider is one more install source. It serves byte ranges when its
  server allows them, and devices serve the rest.
- Pieces are made and checked with iroh-blobs: each holder computes a
  BLAKE3 tree once and keeps it, and every piece is checked as it arrives.
  The finished install is also checked against its release key. iroh-blobs
  0.105 needs iroh 1.x, so it fits korrid's iroh 1.4.1. A holder hashes a
  release in place, without copying it into a store. A file that changes
  while it is served fails the check on the receiving device, so a holder
  must hash a folder release again after it changes.
- Where an installed release lives on disk is not decided. The first slice
  of the install manager decides it from one of the two existing layouts:
  netplay's release-key layout (ADR 0040) or the provider layout (ADR 0016).
- Velvet is the first surface to show play-here and the list of installs.
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
  fixed threshold in korrid (2 minutes to start), Korri asks whether to
  stream meanwhile. The install goes on in the background, and the stream
  comes first.
- Stream play, netplay and a game that runs on the device come first.
  Installs slow down or pause for them on the device that plays and on the
  devices that serve pieces. A device on battery serves fewer pieces and
  stops below a fixed charge (20% to start).
- Korri offers play-here only when this device has a runner for the game,
  enough free storage for the release, and a reachable install source.
  The device learns the release's size when the person presses the action,
  not from the catalog. Korri does not judge whether the device runs the
  game well. A spike found no outside source that rates games on ARM
  handhelds, and Korri measures no game performance today.
- The device owner's devices and family members' devices may install from
  any device of the federation without a prompt on that device (ADR 0077).
  Guests may not.
- File releases and folder releases are both in scope. A file release
  installed from a peer is the same release, because its bytes identify it.
  A folder release installed from a peer is a new folder release of the same
  game on this device. Only devices whose files match serve pieces of one
  folder install.
- Devices that are awake or in light sleep serve pieces. A device in deep
  sleep does not. The owner wants a sleeping holder woken to serve, but
  korrid cannot wake a peer today, so waking is separate work.

## Considered options

- Copy only from the one device that holds the session, as netplay does.
  Rejected: the holder may sleep or leave, and one source is slower.
- A temporary copy that goes after the session. Rejected: every play copies
  again, and the game does not play offline.
- Korri picks the local route by itself when it can. Rejected: a silent copy
  of several GB surprises the person.
