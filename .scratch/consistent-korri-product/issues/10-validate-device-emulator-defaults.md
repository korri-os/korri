# Choose per-device emulator defaults from ROCKNIX

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:research
Type: research
Status: resolved
Blocked by: 03

## Question

Which obvious emulator defaults should each current Korri device model use, based on ROCKNIX's official wiki and website?

### Scope amendment

Simon replaced the previous hardware-validation prerequisite with documented selection: choose obvious defaults using ROCKNIX's published lists. Image-size measurement and image-variant/first-use readiness planning were also explicitly skipped. Do not require benchmarks, test games, image builds, or installed-device inspection to resolve this selection.

Use the existing six board compositions as the target set: RG353M, Odin 2 Portal, RG DS, R36T Max, RP Mini V2, and RG35XXSP. Preserve the approved policy of curated defaults per device rather than bundling every alternative. This is delegated selection judgment, not certification of a supported image.

### Sources and selection

Read primary ROCKNIX system/emulator lists and device-family documentation. Distinguish explicit upstream defaults from supported alternatives and Korri's own selections. A first-listed emulator is not automatically a documented default. Where the exact model has no official page, state the gap and label any use of a related SoC family or conservative common set as an inference.

Map selected names to real Korri plugin producers where they exist. Keep standalone and libretro implementations distinct, especially the current PPSSPP identity conflict. List missing packages or integration as implementation gaps; do not silently replace the selected emulator, waive architecture restrictions, or invent new plugin identities/configuration schema.

The [existing plugin inventory](../evidence/plugin-selection.md) and [hardware evidence](../evidence/hardware-acceptance.md) provide local context. ROCKNIX's published guidance is the selection basis. This task does not import ROCKNIX installation procedures, firmware operations, recovery policy, or performance promises into Korri.

### Completion

Record the chosen per-device defaults with source links and explicit qualifications. Link larger research and selection tables as assets. No physical test or package publication is claimed by this answer. The separately approved supported-image acceptance rules still apply when implementing and releasing Korri.

## Answer

Used Simon's delegated judgment to select the defaults in [ROCKNIX-based emulator defaults](../evidence/rocknix-recommendations.md). The record contains the exact six device lists, their common retro set, standalone-versus-libretro choices, local producer names where available, and missing native-package/integration work.

The parent fetched the full official device pages and their linked generated platform tables, pinned to ROCKNIX distribution `7f1b3abece2c7d263cebd16b5a2ba4268d6ddaa5`. Explicit upstream `(default)` markers are distinguished from Korri's selected alternatives. RG DS uses the RK3566 table as an explicitly labeled analogy; R36T Max uses the documented RK3326 family, not an assertion that it is an R36S.

The selection is complete for planning. It is not an installed package manifest, a supported-device certification, a redistribution license, or a performance result. Existing plugin producers are identified without inventing new identities for missing wrappers. Standalone PPSSPP is selected rather than also bundling the generated libretro plugin with the same identity.

Image measurement and image-variant/first-use readiness planning remain skipped. No benchmarking or physical-device operation was required to resolve this choice. The accepted cost is uncertainty about Korri integration and performance until later implementation/release acceptance.
