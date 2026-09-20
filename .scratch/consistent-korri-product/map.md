# One Korri product across devices

Label: wayfinder:map

## Destination

Settle the decisions needed for a buildable specification of the intended Korri product on every supported Linux device, including missing features. Define allowed hardware differences and the evidence that proves a fresh installation delivers that product.

## Notes

### Starting brief agreed with Simon

- "When I flash or add a new device it should always have the same product on it regardless of hardware quirks."
- RG353M has received the most attention, but is not a complete product specification. Use it as evidence, not the feature ceiling.
- Include intended features that current installations lack. This map is not limited to parity between existing implementations.
- Image sizing and variant/first-use readiness planning were subsequently skipped. No image count, size threshold, or offline-readiness promise was selected by that change.
- Choose obvious per-device emulator defaults from ROCKNIX's official wiki/website. Local benchmarking is not a prerequisite for this planning choice; supported-image acceptance remains separate.
- Users can add or remove plugins during onboarding in the proposed opinionated image. Offline readiness is not settled by this preference.
- Removing a bundled plugin means uninstalling it and reclaiming unused storage, not merely disabling it. Shared dependencies and retained rollback versions can still occupy space. The exact retention and reclamation behavior remains to be decided.

### How to work this map

This is planning. Resolve decisions, not implementation tickets. Use `/grilling` and `/domain-modeling` for each grilling ticket. Use the [product glossary](../../CONTEXT.md) when discussing supported and development images. Follow repository engineering and writing instructions. At completion, use `/to-spec`, then `/to-tickets`; do not treat these decision tickets as build slices.

Use the local Markdown tracker fallback from `/setup-matt-pocock-skills/issue-tracker-local.md`. No repository-specific skill tracker configuration was found when charting. Tickets live in `issues/`; their `Blocked by` lines define dependencies. Pick the first open, unblocked, unclaimed ticket by filename order. Claim before work. Append its resolution under `## Answer`, mark it resolved, and add a named link below. Resolve at most one non-research ticket per session. Do not duplicate the open-ticket list here.

Existing [repository rules](../../AGENTS.md) remain authoritative: Linux only, capability-based federation without fixed device roles, no builds on devices, and schema grounded in real producers or explicit user choices. Legacy is a read-only behavioral reference, not an automatic restoration list.

### Evidence boundaries

The initial review inspected source at `41af6030` and documents, not running devices. It did not run builds or hardware acceptance tests. Recheck source and deployment state before turning observations into claims about an installed device.

For device composition, read the six directories under `nix/devices/` and the shared modules they actually import. For plugin intent, start with the [September 15 brief](../../docs/briefs/2026-09-15-plugin-model-brief.md), which distinguishes agreed direction from unresolved recommendations. For delivery, inspect the [image workflow](../../.github/workflows/device-images.yml), [cache workflow](../../.github/workflows/nix-cache.yml), and [device build policy](../../nix/device-cache/README.md).

## Decisions so far

- [Define the intended Korri product](issues/01-define-intended-product.md#answer): approved the shared behavior scope and optional plugin choices; acquisition and saved-progress features are deferred.
- [Define hardware limits and product acceptance](issues/02-define-device-support.md#answer): required behavior gates supported status; optional hardware limits remain explicit; later releases use change-based physical retesting.
- [Choose the opinionated plugin selection](issues/03-choose-default-plugins.md#answer): bundle policy approved; per-device defaults use ROCKNIX guidance, without image sizing as a prerequisite.
- [Choose per-device emulator defaults from ROCKNIX](issues/10-validate-device-emulator-defaults.md#answer): selected six device lists from pinned platform tables, with explicit alternatives, inferred family mappings, and package gaps.

## Not yet specified

- Further implementation gaps exposed by the chosen emulators and hardware. Assess them against the agreed support policy; upstream selection is not local runtime verification.
- The detailed setup and recovery behavior needed for the selected plugins and real hardware limits. Existing identity, permission, and data contracts must ground those discussions.
- The operational transition from current device-specific installations to the selected product composition, including preservation of real user data. Its steps depend on the boundary chosen later in this map.
- Concrete acceptance scenarios and test fixtures for the final product contract. Do not substitute configuration equality for working behavior.

## Out of scope

- [Measure minimal and opinionated image sizes](issues/05-measure-image-size.md#scope-disposition) was skipped at Simon's request. No measurement is required to finish this map.
- [Choose image variants and first-use readiness](issues/06-choose-image-variants.md#scope-disposition) was skipped at Simon's request. Skipping it does not choose a variant count or establish offline readiness.
- Content acquisition and legacy acquisition-provider ports, cross-device game-save synchronization, and automatic restoration after shutdown are deferred beyond this effort. See [Define the intended Korri product](issues/01-define-intended-product.md#answer) for the approved boundary; ordinary saving and supported plugin save-state controls remain in scope.
- Implementing the product, fixing drivers, deploying software, or flashing devices during planning. Emulator selection is read-only research of ROCKNIX guidance and existing package producers, not a hardware test or authority to implement missing ports. Later release acceptance retains the separate device-change approvals and owner readiness requirements.
- Restoring Android, merging legacy wholesale, or inventing a speculative capability or configuration schema.
- Treating the source-review findings about Odin's older kiosk or the portal deploy signature bypass as authorization to patch or deploy either one. They remain evidence of drift for the product-boundary decision.
