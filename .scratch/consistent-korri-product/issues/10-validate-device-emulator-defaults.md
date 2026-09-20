# Validate per-device emulator defaults

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:task
Type: task
Status: open
Blocked by: 03

## Question

What exact emulator package list meets the approved default-selection policy for each selected device model?

Simon explicitly separated this validation task from [Choose the opinionated plugin selection](03-choose-default-plugins.md#answer). It supplies the concrete lists required for final image-size comparisons. The policy is already chosen: curated, dependable defaults per device, with alternatives available as optional additions. This task does not choose a universal emulated-system list.

### Evidence and scope

Start with [plugin-selection evidence](../evidence/plugin-selection.md), [hardware acceptance evidence](../evidence/hardware-acceptance.md), and the actual device and plugin producers. The current repository has six board compositions; their existence does not certify any of them as supported. State which models the validation covers and keep untested models explicit rather than silently inheriting another model's list.

[RG353M GBA acceptance](../../../docs/acceptance/rg353m-gba-launch-2026-09-07.md) provides historical mGBA gameplay evidence and its limits. [RP Mini V2 library access](../../../nix/devices/rpminiv2/game-plugins.nix) names content directories, not an installed or validated runner list. These are different kinds of evidence.

The independently released plugin catalogue identifies candidate packages and architecture exclusions. Check the current source revision before using it. Resolve the standalone-versus-libretro PPSSPP source choice if PSP is selected; their current shared identity does not make their behavior or settings interchangeable. Preserve legitimate package/platform exclusions.

### Validation work

1. Derive a bounded candidate list from each device's documented specifications, real package availability, and existing observed routes. Label hypotheses separately from measured behavior.
2. Use exact prebuilt packages on the target architecture. Record producer revision and output identity so the later size comparison measures the selected software.
3. Validate representative owner-supplied games, actual input, audio, saving, session return/end, and sustained play under relevant power and thermal conditions. Use existing criteria where available; seek an explicit judgment where dependable performance cannot be established. Do not invent a universal frame-rate or temperature threshold.
4. Record the selected preferred runners and the tested conditions. State the evidence for exclusions, alternatives, and remaining gaps. Selecting a runner does not claim every game for its emulated system works.

Read-only inspection can run AFK. Any owner-assisted or timed physical test is HITL and requires fresh explicit readiness. Installation, state changes, or media writes need their existing separate approval. Never build or dispatch builds from a device, bypass signatures, or alter protected firmware. Preserve real user data and a working recovery route.

If testing requires missing product integration, a driver fix, or an unavailable delivery path, record the prerequisite and stop that part. This task does not authorize implementing missing ports to force validation to pass. Do not substitute catalogue declarations, inferred specifications, or source-only reports for physical acceptance.

### Completion

Record the exact per-device lists and their validation evidence under `## Answer`, linking larger reports rather than pasting them. Obtain Simon's choice for any remaining preference-dependent runner trade-off. Unvalidated or partially available sets must remain identified as such and cannot supply a final whole-image comparison by implication.

This task verifies emulator selection, not the entire supported-image product. Missing Moonlight integration or other image components remain separate requirements under the approved scope. Publishing images, choosing image variants, and declaring a complete supported release are outside this task.
