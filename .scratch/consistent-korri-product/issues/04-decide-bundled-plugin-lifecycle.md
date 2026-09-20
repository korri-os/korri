# Decide how bundled plugins remain removable

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: claimed
Blocked by: 03

## Question

How should image delivery and plugin ownership support real uninstallation and storage reclamation for the selected bundled plugins?

Uninstallation rather than disablement is already chosen in the map's starting brief. Do not ask Simon to choose it again. The open decisions concern references retained by the system image, plugin selection, rollback, shared dependencies, and user data. Do not infer permission to delete saves, settings, identity, or authentication state from permission to remove a plugin.

Inspect the real [plugin host](../../../services/korrid/plugin-host/), [device cache policy](../../../nix/device-cache/README.md), and the system and runtime references created by current images. Read existing installation and plugin-model decisions before proposing changes. The September 15 brief rejects adding RetroArch to the base as a shortcut; distinguish bundling a removable plugin from baking its runtime into core.

Resolve the ownership and lifecycle decision with Simon, including the observable removal result and any retention delay. Ground proposed persisted state in existing contracts or explicit user decisions. A targeted probe can establish reference behavior if source inspection is insufficient; do not implement a production installer as part of this ticket.

## Comments

[Bundled plugin removal: source evidence](../evidence/plugin-removal.md) records the current lifecycle, image-composition gaps, and verification limits. Current-plus-previous rollback already exists in code. Successful removal releases both selections after cleanup, but does not itself collect unreferenced Nix-store files. Source inspection is not a live storage-reclamation test.

The first pending owner choice is whether uninstall retains any software copy for offline reinstallation. The recommendation is to retain current and previous only while installed, then release both on uninstall while preserving user data. This recommendation is not approved. Reclamation timing, system-image references, and future default-bundle updates remain to be settled. No resolution has been recorded.
