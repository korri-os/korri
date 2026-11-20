---
status: accepted
---

# System plugins clean titles, and art matches only on equal clean titles

**Owner decisions, 2026-11-20 (issue #465).** This amends
[ADR 0032](0032-steamgriddb-artwork-comes-from-a-plugin-not-from-korrid.md)
sections 1 and 4.

## What exists today

Read on main at `8c4fd1a17`.

- Discovery stores the raw file name as the title. The default layout
  `**/{title}.{ext}` (`services/korrid/src/plugin.rs` :729–731) always
  captures a title. So `services/korrid/src/discovery/identify/mod.rs`
  :445–449 never calls `title::fallback_title`, and a title such as
  `Castlevania - Symphony of the Night (USA)` reaches the library unchanged.
  This is the root cause of #465.
- `fallback_title` (`discovery/title.rs` :4) strips `(…)` and `[…]` groups
  and turns `_`, `-` and `.` into spaces. It does not invert articles.
- The library keeps one title per game: `LibraryItemPayload.title`
  (`config/library.rs` :34), from legacy `library-item.ts` :246.
- The art handler gets only the title: `PluginGameAssetCandidatesInput
  { title, responses }` (`acquisition/artwork.rs` :49–52).

The evidence for the match rule is a throwaway eval on 2026-11-20, kept
outside the repository in `/tmp/match-eval/results.md`. It searched
SteamGridDB for 121 real titles from ibuki, rpminiv2 and aka; 75 of them have
a SteamGridDB game.

| Strategy | Correct | Wrong |
|---|---|---|
| exact on the stored title (today) | 40 | 4 |
| tag strip, article inversion, then exact | 59 | 4 |
| the same, then normalized-equal | 59 | 4 |
| the same, then fuzzy at 90 | 61 | 6 |
| tag strip, then a SteamGridDB store-type guard | 59 | 1 |

Fuzzy matching takes siblings. At 90 it scores
`Legend of Zelda, The - Oracle of Ages (USA)` against
`The Legend of Zelda: Oracle of Seasons` at 90.1, `Wario Land 4` against
`Wario Land 3` at 91.7, and `Super Mario 64` against `Super Mario 64 DS` at
90.3.

## Decision

### Title cleaning

- A system plugin may contribute a title-cleaning handler. It is pure: a raw
  title goes in, a clean title comes out. It may look things up in its own
  data, such as MAME short names or PS1 serials.
- Discovery gives the plugin's handler the raw captured title first.
- korrid's generic baseline runs only when no plugin answers. The baseline
  is the No-Intro and GoodTools convention: strip the tags and invert the
  article (`Legend of Zelda, The` becomes `The Legend of Zelda`). Today's
  `fallback_title` does the tag strip only.
- When more than one plugin can answer, korrid asks them in plugin id order
  and the first answer wins.
- A plugin never declares a priority. Priority belongs to the person. Until
  plugin management exists, the only manual control is to enable or disable
  a plugin.

### Storage

- The library keeps legacy's single title field. Discovery stores the clean
  title in it. No second title field is added.
- The raw file name stays recoverable from the release's location in
  `device.yaml`: `locations.<hash>[].path` for a file release
  (`Location::File { path }`, `config/mod.rs` :552–554).
- A one-off operational migration cleans the titles of the existing records
  when the cut is deployed (AGENTS.md: one clean cut, no runtime migration).

### Matching

- An art source matches a game only when the clean title and the source's
  name are equal after normalization. Normalization folds accents and case,
  drops punctuation, and treats a trailing `Version` as optional.
- No art source uses a fuzzy match.
- korrid passes the game's system to the art handler, in addition to the
  title.
- Each plugin guards against games with the same name by its own source's
  data. It breaks a tie by system, then by release year when it knows the
  year. When the tie remains, the game gets no art from that source.
- A person cannot fix a match by hand yet. That comes with plugin
  management.

## Unresolved

The owner has not chosen these shapes. Do not invent them; extract them from
a real producer or ask.

- The title-cleaning handler's operation name, input and output, and whether
  it may use the request turns of ADR 0016.
- How "can answer" is decided: for example, whether only the plugins that
  declare the game's system are asked.
- Whether title cleaning also applies to titles that come from acquisition
  or declared games, not from discovery.
- The field that carries the system in `PluginGameAssetCandidatesInput`,
  and the system's form (a system id or something else).
- Where a plugin gets a release year. korrid does not pass one today.
- The migration's script and its check. For a file-set release the raw names
  are in `FileSet { root, files }` (`config/mod.rs` :562–576), not in a
  single `path`.

## Considered options

- **Fuzzy matching at a threshold.** Rejected. At 90 it gives 2 more correct
  matches and 2 more wrong ones than equal clean titles. The wrong ones are
  siblings in a series.
- **A plugin declares its own priority.** Rejected for ever: an author
  cannot know what the person wants. This follows the same line as
  [ADR 0089](0089-korrid-is-blind-to-where-an-address-comes-from.md), which
  rejected a plugin's "prefer my paths".
- **Keep the raw title and clean it only for the art lookup.** Rejected:
  the owner kept legacy's single title field, and the raw name is already
  in `device.yaml`.

## Consequences

- ADR 0032 section 1 said the title is "the only game fact" the handler gets.
  That is no longer true: the handler also gets the system.
- ADR 0032 section 4 ("Titles keep their `fallback_title` form") still holds
  for art: no art source rewrites a title. Titles now change once, by
  cleaning, at discovery and in the migration.
- A clean title changes the record, so it changes the release fingerprint
  that keys art attempts and assignments (ADR 0032, #464 amendment: "a
  title or release-list edit invalidates the assignment"). After the
  migration, every cleaned game hides its stored art until the next art
  pass fetches it again.
- The eval measured SteamGridDB only. Equal clean titles still miss 16 of
  the 75 games that SteamGridDB has. The 31 acquired ibuki titles matched
  only 2 of 4 correctly with any strategy, so their titles need the plugins'
  own data.
