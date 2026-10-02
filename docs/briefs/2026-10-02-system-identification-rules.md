# System identification rules

Status: direction chosen, schema not chosen, nothing implemented.

## User requirements

- A file extension never belongs exclusively to one system.
- A game is not bound to a system folder. Folder names never decide anything.
- Plugins have their say in how content is identified, for example lists of
  regex patterns.
- Korri is system-agnostic. Plugins define systems. Korri cannot know every
  system in advance. A first-party plugin can ship a large list of known
  systems.

## Decision

On 2026-10-02 the user chose to put identification rules on the **system
record**, not on each plugin's discovery claim. "What makes a file a Wii disc"
is written once. Dolphin, other Wii runners and recompilations share it.

## Current state on main

Checked at `f7ec9bf04`.

| Fact | Source |
| --- | --- |
| Systems are plugin-declared string IDs. Production korrid code has no built-in system list. | `services/korrid/src/plugin.rs` `SystemRecord` (`id`, `title`, `aliases`) |
| Discovery claims carry `extensions`, one `system` and `runners`. A claim registers only when its system and one of its runners are enabled. | `plugin.rs` `FileReleaseDiscoveryClaim`, registry construction |
| Claims for different systems on one file give `ClaimConflict` before hashing. | `discovery/scanner.rs` |
| Compound suffixes such as `.p8.png` match; the longest suffix wins within a claim, never between systems. | Commit `d6fdfbaae`, `file_release_discovery_claims_for_filename` |
| Runners may list exact whole-file hashes in `releases` instead of `systems`. | Commit `13a074832` |

Option A replaces `discovery.fileReleases`. Under the no-compatibility rule
this is one clean cut, including the `korri-plugins` generator
(`plugins/libretro/cores.nix`). The compound-suffix behavior must carry over.

## Proposed evidence ranking

Korri ranks evidence. Plugins supply rules but no priorities.

1. Exact whole-file hash.
2. Content rule: bytes at an offset, a file inside the disc image, or a regex
   over such a file.
3. File-name pattern. It matches the file name only, never folders.
4. Extension, only when exactly one system claims it.

Two systems matching at the same level give that one file a diagnostic. Korri
never guesses.

## Rule kinds and grounding

| Kind | Example | Grounding |
| --- | --- | --- |
| Exact hash | Skate 3 ISO | Runner `releases` on main |
| Bytes at offset | Wii `5d1c9ea3` at `0x18` | Dolphin `Volume.cpp`, RetroArch `task_database_cue.c` |
| File inside disc, with regex | PS2 `SYSTEM.CNF` matches `^BOOT2\s*=` | RetroAchievements identification docs, rcheevos |
| File exists inside disc | PSP `PSP_GAME/PARAM.SFO` | RetroAchievements identification docs |
| File-name pattern | PICO-8 `.p8.png` | Commit `d6fdfbaae`, legacy classifier |
| Extension | `.gba` | Existing claims |

Prior art: `docs/research/shared-extension-content-identification.md`.

## Decided: identify without a runner

On 2026-10-02 the user chose to identify content for every system with rules,
even when no enabled runner can play it. A Wii ISO shows as a Wii game on a
device without a Wii emulator. This follows the federation rule: content
declares what it needs, and devices supply the ability to play it. Cost: the
library shows games this device cannot launch, so the UI needs an
unavailable state. Today's rule that a claim needs an enabled runner does not
carry over.

## Decided: rules for one system ID combine

On 2026-10-02 the user chose to combine identification rules from every
enabled plugin that declares the same system ID. A catalog plugin can give
`xbox-360` a content rule, and the Skate 3 plugin can add its ISO hash to the
same system. Cost: one plugin's bad rule widens that system for every plugin.
The evidence ranking turns a clash between two systems into a diagnostic.
Which plugin's title wins is still open.

## Open questions

1. Decided above.
2. Decided above, except which title wins when declarations disagree.
3. Resolved on 2026-10-02 by the user's correction. A recompilation is a
   runner, like Dolphin. Runner `releases` only select routes. They do not
   identify a system, and the Skate 3 plugin declares no system. The ISO's
   system comes from system rules in other plugins. `linux_routes.rs` already
   matches hash runners without checking the release's system. Still open:
   what happens when no system rule identifies a file that a runner accepts
   by hash.
4. Field names and rule shapes.
5. Which container readers are in the first slice: CHD, `.cue` tracks,
   ISO 9660 file lookup.

## Superseded

Slice 1 (hash before deciding a disputed claim) was written against
`fileReleases` claims. Fold it into this work instead of building it first.

## Costs

- korrid gains bounded readers for offsets, ISO 9660, CHD and `.cue`.
- Regex rules add a direct dependency on the linear-time `regex` crate and
  need length limits.
- Shared system IDs are a convention. Aliases help only when declared.
- One plugin's bad rule affects files other plugins see. The ranking turns
  conflicts into diagnostics, not wrong labels.
