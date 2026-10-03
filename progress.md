# Progress

## Status
In Progress

## Tasks

- Tiny5 packaging worker: resource generator and corresponding-source fileset now include the vendored glyph table and full OFL notice.
- Source-media gate now requires byte-exact Tiny5 inputs in both recipe locations and the full OFL in the generated runtime font notice.
- Packaging documentation now distinguishes Tiny5 game glyphs from retained ProggyVector debugger/uncovered glyphs. Architecture and extracted-source rebuild checks remain pending.

## Files Changed

- Packaging: `plugins/zquest-classic/public-assets.nix`; `nix/zquest-source-release.nix`, `zquest-source-release.py`, `zquest-source-archive-check.py`.
- Notices/docs: `plugins/zquest-classic/ASSET-LICENSES.md`, `PUBLIC-CHANGES.txt`, `README.md`, `public-player-notes.md`.

## Notes

- Supplied `context.md` and `plan.md` were absent in `.worktree/zquest-tiny5`; the explicit delegated task and actual producer/consumer files defined this worker's scope.
- No device access, architecture builds, publication or commits by the packaging worker.
- Packaging static checks passed: Python AST, Nix parse, reviewed Tiny5 hashes, fileset/generator wiring, Ruff, nixfmt and `git diff --check`. Final checks must verify regenerated resource bytes and both extracted native recipes.
