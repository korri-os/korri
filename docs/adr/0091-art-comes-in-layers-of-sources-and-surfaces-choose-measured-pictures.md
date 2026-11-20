---
status: accepted
---

# Art comes in layers of sources, and surfaces choose measured pictures

**Owner decisions, 2026-11-20.** This amends
[ADR 0032](0032-steamgriddb-artwork-comes-from-a-plugin-not-from-korrid.md)
(its "roles for Velvet" amendment) and applies
[ADR 0067](0067-a-device-uses-its-own-metadata-first-and-a-peers-when-it-has-none.md)
to art. [ADR 0090](0090-system-plugins-clean-titles-and-art-matches-only-on-equal-clean-titles.md)
records how each source matches a game. Later on 2026-11-20 the owner
replaced the plugin-id-first-wins rule for art plugins (section "Layers"),
and chose the Velvet stage's picture and composition (section "The Velvet
stage").

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

- korrid keeps the candidates from every enabled art plugin. No art plugin
  wins by its place in plugin id order.
- The surface picks among the candidates by their measured shape and
  resolution.
- There is no priority among art plugins: a plugin cannot declare one, and
  a person cannot set one.
- Why: plugin ids sort as text, so `@korri:igdb` comes before
  `@korri:steamgriddb`. Plugin id order would make IGDB's box art beat the
  re-imagined SteamGridDB grids that the owner prefers.
- This replaces today's behaviour in `artwork.rs` (first plugin in id order,
  first candidate of each role).
- Tiered or scored combination of sources is future work.
- The layers for v1, first to last:
  1. The device's own art plugins, all together and with no order between
     them:
     - SteamGridDB, with each person's own key (`@korri:steamgriddb`).
     - IGDB, as `@korri:igdb` in `korri-os/plugins`, with each person's own
       Twitch credentials. Korri ships no key. This follows the SteamGridDB
       precedent of 2026-10-23 (ADR 0032).
  2. A peer's art, under ADR 0067: a device uses its own art, and a peer's
     art only when it has none.
  3. Velvet's procedural poster, which the surface draws itself.
- Peers may share fetched art inside the owner's own federation. Art never
  passes through `@korri` infrastructure.

### Measured media

- The surface contract moves from fixed roles (`tile`, `poster`, `hero`,
  `logo`) to measured facts (width, height, ratio) plus three broad kinds:
  logo, picture and icon.
- A surface chooses a picture by its measured shape and its resolution.
- Box art is allowed as a picture. The owner prefers the re-imagined
  SteamGridDB style as first class.
- The stage must not crop a picture heavily. Section "The Velvet stage"
  records which picture Velvet's stage prefers and how it shows a wide
  hero.

### The Velvet stage

Owner decision, later on 2026-11-20. This replaces this ADR's first text,
which made SteamGridDB heroes (about 3.1:1) the standard for the stage.

- The Velvet stage prefers a picture near 16:9, for example IGDB artworks
  or fanart. That picture fills the stage with little or no crop.
- When only a wide hero exists (SteamGridDB, about 3.1:1), Velvet uses
  composition A, the Band, from
  `docs/briefs/2026-11-20-velvet-hero-stage-direction.md`. The Band shows
  the whole hero at the window's full width from the top edge, and the
  stage light fills the rest of the stage.
- The owner's quality bar: "the design can't be shitty".
- The Band ships only after the owner reviews it on real art and on real
  devices.
- Before it ships, the Band must solve the weaknesses that the brief names:
  - A pale hero makes a white bar at the top of the stage (Chrono Trigger
    in the brief).
  - A dark logo is hard to read on the dark stage. The stored logo colour
    is not reliable: on Hades it reads `#020101` for a red and white logo
    (brief, "Open").
  - Tall windows, and the Stage pane alone, show plain space under the
    words.
  - The Band is less immersive than a full-bleed picture: on a 16:9
    window the hero takes 57 % of the height.
- Evidence: in the brief's review deck, the owner chose "Today (cut to
  fill)" on the pick frame at 02:48Z, and "A Band" at 03:41Z. Then, in
  chat, the owner learned that 16:9 art exists, and chose the Band for
  games whose only picture is a wide hero.
- This decision does not choose how korrid finds a 16:9 picture, which
  plugin produces it, or how the surface tells "near 16:9" from "wide".
  Those stay in "Unresolved".

## Unresolved

The owner has not chosen these shapes. Do not invent them; extract them from
a real producer or ask.

- How korrid stores the extra candidates: the image bytes, or references
  that it fetches later.
- Whether korrid keeps every candidate of a plugin or only some of them
  (today it keeps the first of each role, `artwork.rs` :623–624).
- The treaty shape for kinds and measured facts: the names on the wire,
  whether the ratio is sent or derived from `width` and `height`, and what
  replaces `SurfaceMediaRole`.
- What resolution means beyond the `width` and `height` that already reach
  the surface, and whether korrid measures more than that. This text first
  said "fidelity".
- What "first class" means for the re-imagined style: a fact on the wire or
  a surface preference. It is not a source priority (section "Layers").
- What replaces `GameAssetRole`, the per-role size rules and the
  `<playable id>:<role>` assignment key in storage, and what happens to
  `tile` and `LocalGame.coverAssetId`.
- How many pictures one game may hold, and what a layer gives "per picture".
- Which source produces icons. No producer exists today.
- How much cropping is "heavy", and how near to 16:9 a picture must be to
  fill the stage with "little or no crop".
- Which source gives Velvet's stage its 16:9 picture. IGDB artworks and
  fanart are the owner's examples. No producer of either exists today, and
  this ADR does not name a fanart source.
- How the Band solves each weakness in section "The Velvet stage". The
  brief proposes a rule for dark logos that needs #422's colour extractor;
  the owner has not accepted it.
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
- **First plugin in plugin id order wins**, today's behaviour. Rejected:
  `@korri:igdb` sorts before `@korri:steamgriddb`, so box art would beat the
  re-imagined grids.
- **A plugin declares its layer or priority.** Rejected, as in ADR 0090.
- **A person sets an order among art plugins.** Rejected: the surface picks
  by measured shape and resolution, so no order among art plugins is kept.
- **Cut a wide hero to fill the stage**, today's behaviour
  (`VelvetStage.slint`, `fit = max(W / w, H / h)`). The owner chose it on
  the pick frame in the deck at 02:48Z, then chose the Band for wide-only
  heroes. A 16:9 picture now gives the filled stage with little or no crop.
- **B, the Bleed, and C, Beside**, from the brief. Not chosen: the owner
  chose A, the Band.
- **Ship a Korri key for IGDB or SteamGridDB.** Rejected: each person uses
  their own credentials (ADR 0032 section 7).

## Consequences

- korrid must ask every enabled art plugin about a game. Today it skips a
  plugin for a game that already holds every role (ADR 0032, "roles for
  Velvet"; `artwork.rs` :510–511 and :620). That skip no longer fits, and
  each plugin now costs requests to its site for every game (inferred).
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
