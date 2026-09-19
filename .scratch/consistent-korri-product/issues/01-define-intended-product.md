# Define the intended Korri product

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: claimed
Blocked by:

## Question

Which user-visible behaviors belong to the intended Korri product, which are optional plugin choices, and which are deliberately outside it?

Start with the map's agreed brief. Inventory actual current consumers, device configurations, retained legacy behavior, and prior product decisions before asking Simon to fill gaps. RG353M is a useful reference, not the definition of completeness. Existing code proves an implementation exists; it does not prove the behavior remains wanted.

Review the whole experience, including first use, content discovery and play, federation, plugin management, and the controls needed to operate and recover the device. These are investigation areas, not a preapproved feature list. Respect the existing rule that a device need not have a screen or any fixed role.

Start from [device compositions](../../../nix/devices/), [surface contracts](../../../contracts/surface/korri-surface.ts), the [plugin-model brief](../../../docs/briefs/2026-09-15-plugin-model-brief.md), and its precise legacy references. Check other product decisions when the observed behavior points to them. Treat stale READMEs and historical work items as historical evidence, not current requirements.

Resolve through a live exchange with Simon. Record the agreed behavior inventory, source pointers, and known gaps. Distinguish required behavior from the plugins proposed to provide it. Create further decision tickets for newly exposed uncertainties rather than selecting schemas or architecture to make the inventory concrete.

## Comments

Source inventory is available in [Product behavior inventory](../evidence/product-inventory.md). It separates current consumers, legacy implementations, placeholders, and recorded intent. No device behavior was verified, and no product classification has been approved from this inventory.

The first live question asks whether routine setup and management must work through Korri without SSH or configuration-file edits. The answer is pending. This ticket remains claimed, not resolved.
