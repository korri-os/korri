---
status: accepted
---

# Every release arrives by one install, in pieces from every source

**Owner decisions, 2026-11-20. Work: #551 to #556; waking a device: #557.** A game that only a
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
  must hash a folder release again after it changes. Measured in #552 with
  the real iroh-blobs store: a generated file of 4,700,000,000 bytes, not a
  real release, hashed in place, gives an outboard (the kept BLAKE3 tree)
  of 18,359,360 bytes, about 0.39% of the file. The size depends only on
  the file's length: (⌈length / 16 KiB⌉ − 1) × 64 bytes.
- Where an installed release lives on disk is netplay's release-key layout
  (ADR 0040): `cache/<release-key hex>/` with the content inside, the same
  folder for every install source. Decided in #551. The producer is the
  netplay copy (`placement::install_cache`, owner decision netplay
  q27, 2026-10-26), and the folder name is ADR 0030's "ready folder name"
  (`placement::ready_name`). The provider layout
  (`cache/<plugin>/<provider>/<hex>/`, `placement::provider_cache`) needs a
  provider, and an install from devices has none: ADR 0040 chose the
  release-key layout for that reason ("so it needs no provider"). One
  release from two providers also becomes one folder. Provider installs now
  publish there. The provider cache keeps only the work of a provider job:
  its `.downloads` staging and the `.tmp-*` folder that the reader's sandbox
  unit writes (ADR 0030, Change 1d). Real data moves once, by the operator,
  when the cut is deployed (see "One-off migration: provider installs").
- Velvet is the first surface to show play-here and the list of installs.
- A Steam-engine game (ADR 0083) has two kinds of install source: Valve,
  through the Steam engine, and devices whose folder of the game matches.
  The install takes exactly the files in Valve's manifest for that build.
  A device serves only when its files match that build. Files that a game
  wrote into its own folder stay on their device. The Wine prefix
  (`steamapps/compatdata/<appid>`) and the appmanifest are outside the
  game's folder and are not part of the install. Steam-engine games are
  today's only folder releases. Library-folder folder releases wait for
  the PC entry-point picker.
- When this device has no runner for the game but a plugin offers one,
  Korri asks to install the plugin first, with the usual permission
  approval, as for netplay (ADR 0040).
- The work lands in this order: the install manager with today's provider
  and artifact sources, resumable and kept across restarts; devices as
  sources through iroh-blobs, for file releases; play-here and the list of
  installs on Velvet; netplay's copy moved onto the manager; folder
  releases; the priority and battery rules.
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

## One-off migration: provider installs

The #551 cut moves the ready folders of provider installs. On each device
that ran a provider download before the cut, the operator runs this once,
as the `korrid` user, with korrid and its two sockets stopped
(`systemctl stop korrid-cli.socket korrid-control.socket korrid.service`):
a client that connects to a socket starts korrid again within seconds.
`<provider cache>` is each
`cache/<plugin short>/<provider name>/` folder of an acquisition provider
(ADR 0016). Do not run it on an ADR 0012 artifact folder: an artifact
version can also be a long hex name.

```sh
cd /var/lib/korri/cache
for ready in <provider cache>/*; do
  name=$(basename "$ready")
  case ${#name} in 40|64) ;; *) continue ;; esac
  if [ -e "$name" ]; then chmod -R u+w "$ready" && rm -rf "$ready"
  else chmod u+w "$ready" && mv "$ready" "$name" && chmod u-w "$name"; fi
done
```

A ready folder is read-only (`placement::freeze_and_sync` clears its write
bits when an install is published).
Moving a directory to another parent rewrites its `..` entry, which needs
write access to the directory itself, so the move opens the folder and
closes it again. Found on ibuki on 2026-11-24: a plain `mv` failed with
"Permission denied".

A folder that already exists under its release key holds the same bytes,
because the name is their identity, so the copy in the provider cache goes.
A device that has no provider cache needs nothing.

## Considered options

- Copy only from the one device that holds the session, as netplay does.
  Rejected: the holder may sleep or leave, and one source is slower.
- A temporary copy that goes after the session. Rejected: every play copies
  again, and the game does not play offline.
- Korri picks the local route by itself when it can. Rejected: a silent copy
  of several GB surprises the person.
