# Choose the opinionated plugin selection

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: resolved
Blocked by: 01, 02

## Question

What default-bundle policy should each device model's opinionated image use, with exact emulator lists validated separately?

Use the intended-product decision and the hardware-limit decision. Inventory existing plugins and their actual behavior before presenting a proposed selection. Include the independently released plugin repository where needed, but do not equate every legacy plugin with a required default.

Read the [plugin-model brief](../../../docs/briefs/2026-09-15-plugin-model-brief.md), the existing [plugin installation decisions](../../../docs/briefs/2026-09-08-linux-plugin-installation-brief.md), [current plugin sources](../../../plugins/), and [RP Mini V2 game integration](../../../nix/devices/rpminiv2/game-plugins.nix). Distinguish source code, published packages, installed receipts, and verified running behavior.

Simon approved narrowing this ticket to bundle policy. Resolve that policy here; exact per-device emulator lists belong to [Validate per-device emulator defaults](10-validate-device-emulator-defaults.md). Keep owner choices and missing integrations explicit. Preserve plugins as removable plugins; including one in an image does not silently make it a core service. Record unavailable or unfinished packages explicitly. Image count and offline readiness remain decisions to settle using the later size evidence.

## Comments

[Plugin-selection evidence](../evidence/plugin-selection.md) distinguishes source declarations, missing ports, architecture restrictions, and unverified publication/installation. The PPSSPP source/identity conflict must be resolved if PSP is selected. No device builds or physical acceptance were performed for this inventory.

### Curated emulator defaults

Simon selected curated defaults: bundle a recommended emulator selection with a preferred runner for each included system. Users can install and select alternatives later, rather than receiving multiple alternatives for every system by default. This keeps initial choices bounded, at the cost of some games needing a different runner.

### Per-device selection

Simon clarified that each supported device model has its own default emulator list because the selection depends on device specifications. There is no single universal list of emulated systems to approve. Apply the curated-defaults choice within each device's selection. Shared Korri product behavior remains consistent; differences in bundled emulators do not authorize omitting required setup, management, or session behavior.

Specifications guide candidate selection, while the approved support policy still requires verified applicable behavior. Do not label missing drivers or an unfinished port as a hardware limit. This decision chooses neither an automatic selection mechanism nor a new device/configuration schema.

### Emulator selection quality

Simon selected dependable defaults. Validate representative games on the target device before including a runner in its default list; borderline or inconsistent options remain optional additions. This is not a guarantee that every game in an emulated system's library works. Candidate lists can be proposed during planning, but specifications and package declarations alone do not establish dependable performance.

### Remote-access plugins

Simon selected neither SSH nor Tailscale for default preinstallation. Offer both as optional additions where supported. This is a package-selection decision; enabling remote access, granting permissions, and joining a tailnet remain explicit owner actions. It does not remove separate recovery access or change the self-service product requirement.

### Legacy content providers

Simon is not ready to port itch.io, Community Game Catalog, or PortMaster from legacy. None was selected for the current default bundle. Simon then explicitly deferred content acquisition from this effort while retaining it as future product intent. The [product scope](01-define-intended-product.md#content-acquisition) records that amendment. These provider ports do not block the current bundle decision. Plugin installation and updates remain in scope; acquiring game content is a separate behavior.

### Streaming client

Simon selected Moonlight preinstalled as a plugin for the current effort, where streamed playback is supported, with opt-in installation intended soon afterward. It remains a removable plugin now. Keep the client as a plugin in both stages; the later change is its default selection, not a second integration. No date or release boundary for that default change was selected.

This is an intended bundle decision, not a claim that the current image already contains a working Moonlight plugin. Complete and verify the missing Linux integration before claiming supported streamed play. Preinstallation does not authorize an automatic connection or grant access to another device. Streamed play remains in the current product scope.

## Answer

Simon explicitly approved closing this ticket as the bundle-policy decision and tracking exact per-device emulator lists in a separate validation task. The agreements above define the policy: curated, dependable defaults per device; Moonlight preinstalled as a removable plugin for now; SSH and Tailscale optional rather than preinstalled; game-content acquisition deferred.

The later move to optional Moonlight installation changes its default selection, not its plugin boundary. No release date was selected. A missing port is still missing work, not a completed plugin merely because this policy selects it.

[Validate per-device emulator defaults](10-validate-device-emulator-defaults.md) owns the exact package lists and their evidence. [Measure minimal and opinionated image sizes](05-measure-image-size.md) must wait for those lists and the removable-plugin packaging decision before claiming final comparisons. No universal system list, complete per-device bundle, image size, or supported device has been certified here.

The cost of this split is that final size comparisons remain pending. It lets the shared bundle/lifecycle design proceed without presenting hardware guesses as verified defaults. This resolution does not authorize a new configuration schema, implementation, publication, or device change.
