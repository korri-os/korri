---
id: 01M2NK7ZYWTX1NWHDST133QRK2
slug: make-korrid-s-script-packages-tests-explain-a-missing-node-m
title: "Make korrid's script_packages tests explain a missing node_modules"
origin: parked
status: To Do
priority: medium
labels:
  - korrid
  - tests
  - developer-experience
created: 2026-09-16
source: se-work
---

# Make korrid's script_packages tests explain a missing node_modules

## Why it matters

A fresh git worktree fails 10 korrid tests with `Os { code: 2, kind: NotFound }` at script_packages.rs:72 and no indication of the cause. The tests read real npm packages from two gitignored directories, plugins/retroarch/node_modules and docs/research/retroarch-effect-quickjs-probe/node_modules, which only exist after a `bun install` nobody is told to run. The failure looks like a regression in the change under test, which is the expensive kind of wrong: I spent a verification cycle proving it was the environment and not my commits. Anyone landing plugin work in a new worktree pays the same toll.

## Acceptance Criteria

- [ ] A fresh worktree either passes the korrid suite or fails with a message naming the directory and the exact bun install command
- [ ] The two package roots the tests need are discoverable without reading script_packages.rs
- [ ] No test silently reads a gitignored directory and reports only a bare NotFound

## Related

- `services/korrid/tests/script_packages.rs`
- `plugins/retroarch/bun.lock`
- `docs/research/retroarch-effect-quickjs-probe/bun.lock`

## Notes

Verified on branch feat/plugin-clean-cut-finish: 10 failures in a fresh worktree, 5 after installing plugins/retroarch, 0 after installing the probe directory too. Both roots are covered by .gitignore line 1 (node_modules/).
