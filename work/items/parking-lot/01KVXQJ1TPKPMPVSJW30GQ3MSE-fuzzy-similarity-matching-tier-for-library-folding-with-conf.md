---
id: 01KVXQJ1TPKPMPVSJW30GQ3MSE
slug: fuzzy-similarity-matching-tier-for-library-folding-with-conf
title: Fuzzy/similarity matching tier for library folding with confidence scoring
origin: parked
status: To Do
priority: medium
labels:
  - federation
  - library
  - dedup
  - matching
created: 2026-06-24
source: se-challenge-plan
---

# Main applicability

Imported from `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62:work/items/parking-lot/01KVXQJ1TPKPMPVSJW30GQ3MSE-fuzzy-similarity-matching-tier-for-library-folding-with-conf.md`.
Source inspection used main `04d300680184e4db6a58e86514284c9b342159c4`. No new runtime test or device acceptance was performed.

## Why keep this

Keep this separate from exact folding. clients/portal/src/launchables/fold-games.ts folds only a shared hash or provider identity. Differently dumped copies without a shared identity remain separate.

## Scope on main

First demonstrate a real pair that exact folding cannot join. Preserve exact folding as the first stage. Thresholds, automatic merge policy, and durable curation schema remain unresolved; this import chooses none of them.

This is parked work, not an approved implementation plan. `AGENTS.md`, current contracts, and current producer data govern future work. Historical schemas, paths, APIs, safety settings, and completed boxes below do not establish current main behavior.

## Cost and remaining acceptance

A false merge can hide distinct games. Similar titles alone are not sufficient evidence, and curation adds user work.

## Legacy record

The original body follows unchanged. Its progress and acceptance refer to legacy. Retrieve related files from the same fixed commit, not from current main.


# Fuzzy/similarity matching tier for library folding with confidence scoring

## Why it matters

Exact-identifier folding (hash or native id) cannot match copies that lack a shared identifier — e.g. the same game dumped slightly differently, or named differently across hosts. A second matching tier is needed that uses system + title + available metadata as weighted evidence to produce a confidence score, auto-accepts merges above a high threshold, and surfaces lower-confidence candidates to the user for curation. This is the natural follow-on once exact-identity folding ships, and keeping it separate protects v1 from false-positive merges.

## Acceptance Criteria

- [ ] A scoring function combines system, title, and available metadata into a confidence score for two candidate releases
- [ ] Matches above a defined high-confidence threshold auto-fold without user action
- [ ] Matches in a middle band are surfaced non-intrusively for user accept/reject curation
- [ ] Matches below the band are left as separate items
- [ ] User curation decisions are durable and survive rescans and peer reconnections
- [ ] Exact-identifier folding (hash/native id) remains the first tier and is unaffected

## Related

- `work/items/active/01KVVMYE5SFC4H8X5H0EBY7WG3-federated-single-file-folding/plan.md`
- `docs/research/game-library-entity-resolution-deduplication.md`
