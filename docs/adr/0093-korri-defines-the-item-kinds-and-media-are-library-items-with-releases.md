---
status: accepted
---

# Korri defines the item kinds, and media are library items with releases

**Owner decisions, 2026-11-20 (issue #563, map #559).**

## What exists today

Read on main at `f49d498db`.

- Every catalog entry is a game. `Game` (`services/korrid/src/lib.rs`) has a
  `system` and a `GameSource`; `LibraryItemPayload`
  (`services/korrid/src/config/library.rs`) holds `releases`, each with an
  optional `system` and a `LibraryIdentity` whose variants include `Provider`.
- The surface contract carries only `SurfaceGame`
  (`contracts/surface/src/lib.rs`). There is no item kind anywhere.
- Plugins declare systems. Korri knows no system of its own.

## Decision

1. **Library item** is the shared parent. A library item has one **item
   kind**: game, film, episode, track or audiobook.
2. **Korri defines the item kinds.** Plugins provide items of these kinds and
   cannot add one. Systems stay plugin-declared and belong to games only.
3. **Episode is one kind.** Its parent is a series or a podcast. Anything
   that differs between the two reads the parent, not the kind. Series,
   podcast, album and artist are groupings; issue #564 decides what they hold
   and whether they play.
4. **Media items have releases.** One version on a media server (for example
   one Jellyfin `MediaSource`) is a remote release, identified by its
   provider's reference. Downloading it for offline play is an install
   (ADR 0092).
5. **Each release states its needs.** A release says whether it is video or
   audio only, and its formats. A device chooses among the releases it can
   play. The item kind does not fix the needs, so a video podcast episode can
   also play as audio.

## Why

- Surfaces, progress and playback must know what a film or a track is.
  Plugin-declared kinds would force Korri to define fixed presentation shapes
  anyway, which is a built-in set under another name.
- "Library item" extends a name that main already uses
  (`LibraryItemPayload`). "Content" already means bytes (release, artifact).
- Releases and install let offline download reuse the install model instead
  of adding a second one.
- Release-level needs fit the federation rule (content declares what it
  needs) and the facts: Jellyfin has several `MediaSources` per item, ossicle
  episodes carry `audioPath` and `videoPath`, and the devices differ by codec
  (`docs/research/media-decode-and-player.md` on `research/media-decode-and-player`).

## Costs

- A sixth item kind (comics, e-books) needs a change to Korri, not a plugin.
- Games have systems and media kinds do not, so the model is not uniform.
- Every media provider must report formats per release.
- A remote release has no bytes, so release identity for media rests on the
  provider's reference. Two servers that hold the same film give two releases
  until something links them; how they are linked is not decided here.

## Not decided here

The field names and storage of library items, kinds and release needs. The
schema guard in `AGENTS.md` applies: each field comes from a real producer
(the Jellyfin, pyxis and ossicle contracts) when the contract tickets decide it.
