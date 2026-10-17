# A source is paired with exactly one system or system group

Discovery scans only sources: folders a person pairs explicitly with one system or one system group. The paired system's layouts and evidence decide what a game is inside that folder; a folder's name never decides a system. Paired folders that overlap are refused, and neither is scanned until the overlap is removed.

This reverses the 2026-10-02 requirement "a game is not bound to a system folder". Matching by extension across every folder turned 2,293 build and README files on a Mini V2 into catalog entries, and many systems (Atari 2600, PC Engine, WonderSwan, PC games) carry no reliable signature at all. The pairing is the evidence those systems lack.

## Considered Options

- Content rules only, in any folder: cannot identify headerless systems or PC games, and lets generic extensions (`.o`, `.md`) catalog junk.
- Several systems per folder: keeps mixed folders working, but brings back clashes between unrelated systems. System groups cover the related-system case instead.
- The inner folder wins on overlap: works, but makes the owner of a file depend on nesting rather than on one explicit pairing.
