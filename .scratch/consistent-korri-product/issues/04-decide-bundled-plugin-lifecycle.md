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

### Software retention on uninstall

Simon selected **release both versions on uninstall**. Retain current-plus-previous rollback for installed plugins. Successful uninstall releases the plugin's current and previous software selections, without reserving either copy for offline reinstallation. Shared files still needed elsewhere remain. Saves, settings, credentials, and identity are not deleted by this choice.

The accepted cost is that reinstallation can require a download. There is no guaranteed offline undo after uninstall. This choice does not bypass failed native cleanup or authorize deletion of another consumer's files.

### Storage reclamation timing

Simon selected **reclaim eligible space before uninstall finishes**. Korri must automatically complete storage cleanup before reporting uninstall complete. Releasing software references alone is not completion. Report cleanup failures rather than claiming space was freed.

Only unused software is eligible. Preserve user data and files still needed by other plugins or retained system versions. This decision does not authorize deleting system rollback versions. The accepted cost is that uninstall can take longer and can remain incomplete if cleanup fails.

This is a product requirement, not verified runtime behavior. The inspected removal path does not yet perform storage reclamation.

### Remaining discussion

System-image references, future default-bundle updates, and interruption/retry behavior remain to be settled. The next question concerns newly added defaults on existing installations. The recommendation is to keep the owner's plugin selection unchanged and offer new defaults as optional installations. That update-policy recommendation is not approved.

The ticket remains claimed; no implementation or device operation is authorized.
