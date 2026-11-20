---
status: accepted
---

# Art comes in layers of sources, and surfaces choose measured pictures

**Owner decisions, 2026-11-20.** This amends
[ADR 0032](0032-steamgriddb-artwork-comes-from-a-plugin-not-from-korrid.md)
(its "roles for Velvet" amendment) and applies
[ADR 0067](0067-a-device-uses-its-own-metadata-first-and-a-peers-when-it-has-none.md)
to art. [ADR 0090](0090-system-plugins-clean-titles-and-art-matches-only-on-equal-clean-titles.md)
records how each source matches a game.

## What exists today

Read on main at `8c4fd1a17`.

- korrid asks every enabled art plugin in plugin id order
  (`art_plugins`, `services/korrid/src/acquisition/artwork.rs` :296–300).
  For each role, it takes the plugin's first candidate (:623). A plugin is
  not asked about a game that already holds every role (ADR 0032, "roles for
  Velvet").
- korrid stores four fixed roles: `tile`, `poster`, `hero`, `logo`
  (`GameAssetRole`, `game_assets.rs` :29–37), one image for each
  `<playable id>:<role>`. Each role has its own size rule, for example
  `hero` from 2:1 to 4:1 (ADR 0032, "roles for Velvet").
- The surface treaty sends `SurfaceMedia { role, url, width, height,
  color? }` with `SurfaceMediaRole` one of `poster`, `hero`, `logo`
  (`contracts/surface/src/lib.rs` :232–257). So the width and height of each
  image already reach the surface.
- Velvet draws its own card for a game with no art
  (`clients/shell/packages/velvet/test/browse.test.ts` :298, "no art, no
  hole").
- `@korri:steamgriddb` takes each person's own key, and Korri ships no key
  (ADR 0032 sections 3 and 7, ADR 0034).

## Decision

### Layers

- With more than one art plugin, each role or picture comes from the first
  plugin that has a candidate for it, in plugin id order. This is today's
  behaviour in `artwork.rs`.
- Tiered or scored combination of sources is future work.
- The layers for v1, first to last:
  1. SteamGridDB, with each person's own key (`@korri:steamgriddb`).
  2. IGDB, as `@korri:igdb` in `korri-os/plugins`, with each person's own
     Twitch credentials. Korri ships no key. This follows the SteamGridDB
     precedent of 2026-10-23 (ADR 0032).
  3. A peer's art, under ADR 0067: a device uses its own art, and a peer's
     art only when it has none.
  4. Velvet's procedural poster, which the surface draws itself.
- Peers may share fetched art inside the owner's own federation. Art never
  passes through `@korri` infrastructure.

### Measured media

- The surface contract moves from fixed roles (`tile`, `poster`, `hero`,
  `logo`) to measured facts (width, height, ratio) plus three broad kinds:
  logo, picture and icon.
- A surface chooses a picture by its ratio and its fidelity.
- Box art is allowed as a picture. The owner prefers the re-imagined
  SteamGridDB style as first class.
- SteamGridDB heroes, about 3.1:1, are the standard for Velvet's stage.
  The stage must not crop them heavily.
- The stage composition is still under owner review
  (`docs/briefs/2026-11-20-velvet-hero-stage-direction.md`, on another
  branch). This ADR does not record a composition.

## Unresolved

The owner has not chosen these shapes. Do not invent them; extract them from
a real producer or ask.

- **Layer order against plugin id order.** Plugin ids sort as text, so
  `@korri:igdb` comes before `@korri:steamgriddb`. Plugin id order would make
  IGDB the first layer, not the second. ibuki's SteamGridDB plugin is
  `@simonwjackson:steamgriddb`, which also sorts after `@korri:igdb`. The
  owner must say which rule wins until plugin management lets the person set
  the order.
- The treaty shape for kinds and measured facts: the names on the wire,
  whether the ratio is sent or derived from `width` and `height`, and what
  replaces `SurfaceMediaRole`.
- What fidelity is and how korrid measures it.
- What "first class" means for the re-imagined style: an order among
  candidates, a fact on the wire, or a surface preference.
- What replaces `GameAssetRole`, the per-role size rules and the
  `<playable id>:<role>` assignment key in storage, and what happens to
  `tile` and `LocalGame.coverAssetId`.
- How many pictures one game may hold, and what a layer gives "per picture".
- Which source produces icons. No producer exists today.
- How much cropping is "heavy".
- `@korri:igdb`'s hosts and secrets. ADR 0034 stores one key that korrid adds
  as a bearer token. IGDB's own documentation (not read for this ADR)
  describes a Twitch client id plus an app access token, and query requests
  by `POST`. ADR 0016 allows only `GET`. Read IGDB's API before the plugin is
  designed.
- How a peer's art reaches a device: copied or read each time. ADR 0067
  leaves this open, and also how to choose when two peers disagree.

## Considered options

- **Scored or tiered combination across sources** now. The owner deferred it
  to future work.
- **A plugin declares its layer or priority.** Rejected, as in ADR 0090:
  priority belongs to the person.
- **Ship a Korri key for IGDB or SteamGridDB.** Rejected: each person uses
  their own credentials (ADR 0032 section 7).

## Consequences

- A surface no longer depends on a role name to find a hero. It reads the
  measured shape, so a source that labels art differently still fits.
- The move off fixed roles is a treaty change, so it follows AGENTS.md: one
  clean cut, with no alias for the old roles. Whether stored assignments
  need a one-off migration depends on the unresolved storage shape.
- IGDB's terms of use were not read for this ADR. Read them before
  `@korri:igdb` is published, as ADR 0032 section 7 did for SteamGridDB.
  ADR 0042 still applies: `@korri` publishes only content licensed for
  redistribution, and the plugin itself fetches art only for one person's
  devices.
