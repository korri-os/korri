---
id: 01M1N4375QCH4JF8FC93KTR570
slug: decompose-shift-behind-the-pico-gates-and-lift-them-to-surfa
title: Decompose Shift behind the Pico gates and lift them to surfaces/*
origin: parked
status: To Do
priority: medium
labels:
  - surface
  - shift
  - design-system
  - gates
created: 2026-09-04
source: se-work
context:
  cwd: /home/simonwjackson/code/sandbox/korri
  branch: feat/pico-surface-first-slice
  repo: korri
  invoked_by: se-work
---

# Decompose Shift behind the Pico gates and lift them to surfaces/*

## Why it matters

surfaces/pico now carries decomposition and authoring gates that hold the rules both design skills demand, but their scope is one surface because Shift would be red today: zero *.part.tsx files, a 449-line shift-design-parts.ts registry tagged across the tree, 159 className sites with repeated literals (shift-sheet-control-description appears 8 times), and pages/ files that mix layout with private components. Leaving the gates surface-local means the rules are enforced where they were cheapest to satisfy and absent where the real product ships. The work is Shift's decomposition, not a test move: run the gate against surfaces/shift, take the failure list as the inventory, extract downward, then change the walk root to surfaces/*.

## Acceptance Criteria

- [ ] The gate files walk surfaces/* rather than a single surface
- [ ] Every component under surfaces/shift/src/ui and pages has a sibling part; page and template layers are present
- [ ] shift-design-parts.ts is gone and no className literal or class selector is defined twice
- [ ] shift-check and pico-check both pass

## Related

- `surfaces/pico/test/decomposition-gate.test.ts`
- `surfaces/pico/test/authoring-gate.test.ts`
- `surfaces/shift/src/shift-design-parts.ts`
