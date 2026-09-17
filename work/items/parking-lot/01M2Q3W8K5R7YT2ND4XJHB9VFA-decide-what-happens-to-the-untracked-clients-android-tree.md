---
id: 01M2Q3W8K5R7YT2ND4XJHB9VFA
slug: decide-what-happens-to-the-untracked-clients-android-tree
title: Decide what happens to the untracked clients/android tree on disk
origin: parked
status: To Do
priority: medium
labels:
  - android
  - housekeeping
created: 2026-09-16
source: se-work
---

# Decide what happens to the untracked clients/android tree on disk

## Why it matters

`241de7f8` removed the Android client from the repository, but `clients/android/`
still sits in the working tree as untracked `app/`, `build/`, and `signer-test/`
directories. Every `git status` in this checkout reports it, which trains readers
to ignore untracked output, and the next person to run a wide `git add` can commit
a deleted platform back by accident.

## Acceptance Criteria

- [ ] `git status` on a clean main reports no `clients/android/` entry
- [ ] Anything worth keeping from the tree is captured somewhere durable before it is deleted, or the item records that nothing was
- [ ] The decision names the tag `android-final` as the recovery path for the deleted source

## Related

- `docs/briefs/2026-09-16-android-removal-scope.md`
- Commit `241de7f8`
- Tag `android-final`

## Notes

Only build leftovers and a signer test were observed: `clients/android/{app,build,signer-test}`.
Deleting them is the expected outcome; the item exists so the deletion is a decision
rather than a surprise.
