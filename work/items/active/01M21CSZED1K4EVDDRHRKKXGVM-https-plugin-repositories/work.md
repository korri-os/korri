---
id: 01M21CSZED1K4EVDDRHRKKXGVM
title: HTTPS plugin repositories
status: active
created: 2026-09-08
source: direct
---

# HTTPS plugin repositories

Add CLI-first plugin repository publication, source management, explicit source selection, and integration with the existing administrator installer. The official source is owner-curated. Users can add other HTTPS catalogs. Trust follows the HTTPS location without a separate catalog-signing key. Updates remain tied to the selected repository.

The implementation plan is in `plan.md`. Product decisions are in `docs/briefs/2026-09-08-linux-plugin-installation-brief.md`. The installer implemented in `d175f7d6` is the baseline, not the older portable-service proposal.

External publication requires the owner to provide the official catalog location and approve the repository/release actions. No physical deployment or paid service is authorized.

## Current status

The CLI implementation is complete locally. The work item remains active for first-publication acceptance; no official URL or live registry is configured yet.

- Repository source management, HTTPS catalog reads, explicit selection, source-bound approval/update/switch, and offline lifecycle are implemented.
- The build-side publisher produces complete content-addressed Nix cache archives. Real Tailscale conversion, empty-store import, NAR hash/size rejection, and the existing Nix integrity checks pass without a repository signing-key workflow.
- The maintained cold-host VM passes HTTPS installation, cross-source approval rejection, authenticated local Headscale traffic, failed updates, forced-crash recovery, source removal, retained data and cleanup-failure refusal. No physical device or owner's tailnet was used.
- Review corrections cover literal URL fetching, one normalized HTTPS URL policy, rejection of tar extension headers before buffering, and exact-ID GitHub draft preparation. Regression tests cover each correction.
- The owner-triggered GitHub workflow prepares both Linux architectures and can prepare an explicitly approved draft. It never publishes. Offline workflow tests and actionlint pass.
- ARM package expressions are verified by evaluation, not execution. A live ARM build and the first public download remain operational acceptance tasks.

Before publication, supply the actual catalog URL, review the destination/tag/environment and immutable-release settings, and authorize the external actions. Keep old development receipts untouched until an explicit format-cut decision is made. No runtime receipt migration or signature bypass is provided.
