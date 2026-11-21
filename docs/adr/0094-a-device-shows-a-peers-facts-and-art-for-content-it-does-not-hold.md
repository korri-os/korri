---
status: accepted
---

# A device shows a peer's facts and art for content it does not hold

**Owner decisions, 2026-11-20.** These decisions settle the questions that
[ADR 0067](0067-a-device-uses-its-own-metadata-first-and-a-peers-when-it-has-none.md)
left open, which are how a peer's fact reaches a device and which peer wins.
The decisions apply that ADR to the art of
[ADR 0091](0091-art-comes-in-layers-of-sources-and-surfaces-choose-measured-pictures.md).

## What exists today

Read on main at `bd53e732d`.

- The surface model builds a peer's game with no art, no system and no cover
  (`services/korrid/src/surface/model.rs` :230, "A peer's copy has no cover
  here"). Only `Entry::LocalGame` fills `media` and `coverArtUrl`.
- The surface host already reads a stored image through
  `/game-assets-object` (`services/korrid/src/lib.rs` :5830). The doc comment
  of that route calls it "the local twin of the peer's object routes
  (`saves/peer/object_route.rs`)".
- An art pass covers only the games the device holds (ADR 0032).

## Decision

### What a peer supplies

- The slice covers every fact in ADR 0067, not only art.
- Example facts are the system, the public ids (ADR 0090), the title, and the
  stored pictures, logos and icons with their measured sizes.
- The device's own fact still wins, one fact at a time (ADR 0067).

### How art reaches the device

- korrid reads a peer's image when a surface first shows it. korrid then
  caches the image on the device, keyed by content hash.
- Later views read the local copy. The copy still shows when the peer is
  offline.
- The cache is bounded by a size cap, 512 MB by default on each device. When
  the cap is reached, korrid removes the image that was shown least recently.
  The number is a starting value. No measurement backs it.
- The surface still talks only to its own korrid. That korrid fetches from
  the peer over peer RPC (AGENTS.md).

### Which peer wins

- **Art:** korrid pools the candidates of every reachable peer that holds the
  game. The surface picks by measured shape and resolution, the same rule that
  ADR 0091 gives for local art plugins. No peer has priority.
- **Single-value facts** (system, title): the peer whose copy the card
  launches supplies the fact. Facts from different peers are not mixed on one
  card.

### Who fetches art

- An art pass stays limited to the games the device holds. A device does not
  ask its own art plugins about a game that only a peer holds.
- Each game is then fetched once, by its holder, with the holder's key.

### Art stays private

- Art moves only between devices of one owner's federation. It never passes
  through `@korri` infrastructure (ADR 0091, ADR 0042).

### Play history is out of scope

- Play history belongs to the person, not to a device: "The play history
  should be an abstraction away from the hardware."
- It is its own piece of work, designed together with save sync. This slice
  does not change it.

## Unresolved

Do not invent these shapes. Extract them from a real producer or consumer, or
ask the owner.

- The peer RPC operation that lists a peer's facts and media for a game, and
  its wire names.
- The cache's location, its index format, and how it reports its size.
- How a device notices that a peer's art changed: by content hash on each
  list, or by some other signal.
- How the cap is set on a device. It may be a setting, or a fixed value for
  the first version.

## Considered options

- **Copy all of a peer's art when the devices sync.** Rejected. A device that
  joins would download every image of every peer.
- **Read the art each time, with no cache.** Rejected. The art would be
  missing whenever the peer is offline.
- **Use the art of the holding peer only.** Rejected. When the holding peer
  has no art, the card shows the poster even though another peer has art.
- **Fetch art for every visible game, including peer games.** Rejected. It
  makes duplicate API calls with the person's key.
- **Share play history in this slice.** Moved to its own piece of work. Play
  history is person data, not a content fact.

## Consequences

- A device that only streams, such as a phone, shows art and systems for
  every peer game whose holder has fetched them.
- To list a peer game, korrid asks every peer that holds it. This costs more
  requests than asking one peer.
- A game whose holder has no art plugin or key shows the procedural poster on
  every device.
