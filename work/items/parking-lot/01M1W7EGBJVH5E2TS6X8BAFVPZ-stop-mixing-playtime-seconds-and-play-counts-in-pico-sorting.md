---
id: 01M1W7EGBJVH5E2TS6X8BAFVPZ
slug: stop-mixing-playtime-seconds-and-play-counts-in-pico-sorting
title: Stop mixing playtime seconds and play counts in Pico sorting
origin: parked
status: To Do
priority: medium
labels:[]
created: 2026-09-06
source: user
---

# Stop mixing playtime seconds and play counts in Pico sorting

## Why it matters

MOST PLAYED currently compares totalPlaytimeSeconds ?? playCount per game, which compares unlike units when the catalog mixes available statistics. The Caliper layout/interaction review does not validate that ranking policy; it needs an explicit, honest ordering rule and mixed-metadata coverage.

## Acceptance Criteria

- [ ] Add a failing test using games with playtime-only, count-only, both and absent statistics.
- [ ] Choose and document a consistent metric or explicit grouping policy without converting counts into durations.
- [ ] Keep missing data distinguishable from zero and verify the visible order through Find.

## Related

- `surfaces/pico/src/pico-library-view.ts`
- `docs/acceptance/pico-caliper-2026-09-06.md`
