# Choose the opinionated plugin selection

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: claimed
Blocked by: 01, 02

## Question

Which plugins should an opinionated image include by default, and which choices must users make during onboarding?

Use the intended-product decision and the hardware-limit decision. Inventory existing plugins and their actual behavior before presenting a proposed selection. Include the independently released plugin repository where needed, but do not equate every legacy plugin with a required default.

Read the [plugin-model brief](../../../docs/briefs/2026-09-15-plugin-model-brief.md), the existing [plugin installation decisions](../../../docs/briefs/2026-09-08-linux-plugin-installation-brief.md), [current plugin sources](../../../plugins/), and [RP Mini V2 game integration](../../../nix/devices/rpminiv2/game-plugins.nix). Distinguish source code, published packages, installed receipts, and verified running behavior.

Resolve a concrete proposed bundle with Simon, including any deliberate hardware-dependent availability and the owner choices that prevent first use. Preserve plugins as removable plugins; including one in an image does not silently make it a core service. Record unavailable or unfinished packages explicitly. Image count and offline readiness remain decisions to settle using the later size evidence.

## Comments

[Plugin-selection evidence](../evidence/plugin-selection.md) distinguishes source declarations, missing ports, architecture restrictions, and unverified publication/installation. The PPSSPP source/identity conflict must be resolved if PSP is selected. No device builds or physical acceptance were performed for this inventory.

### Curated emulator defaults

Simon selected curated defaults: bundle a recommended emulator selection with a preferred runner for each included system. Users can install and select alternatives later, rather than receiving multiple alternatives for every system by default. This keeps initial choices bounded, at the cost of some games needing a different runner.

No exact system list, runner list, additional plugin selection, or image size has been approved by this answer. This ticket remains claimed, not resolved.
