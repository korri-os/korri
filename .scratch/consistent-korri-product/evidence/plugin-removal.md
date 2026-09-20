# Bundled plugin removal: source evidence

Supports [Decide how bundled plugins remain removable](../issues/04-decide-bundled-plugin-lifecycle.md). Inspected on 2026-09-20 against source at `39328978`; the subsequent claim commit changes only the ticket status. This is evidence for a live decision, not its resolution.

## Existing agreements

Real uninstallation and reclamation of unused storage are already selected in the [map](../map.md). Shared dependencies and retained rollback versions can still occupy storage. Permission to uninstall is not permission to delete user data.

The [September 9 brief](../../../docs/briefs/2026-09-09-plugin-authoring-standard-brief.md), decision 7, approves current-plus-previous plugin builds and offline restore. The [September 15 brief](../../../docs/briefs/2026-09-15-plugin-model-brief.md) retains independent plugins and ordinary Nix dependencies. It rejects putting RetroArch in the base as a shortcut. It does not revoke plugin rollback.

## Implemented lifecycle

These are verified source facts. No test suite, VM, or physical-device operation ran during this inspection.

| Area | Source fact | Evidence |
|---|---|---|
| Retained software | The receipt holds one current and one previous selection. An update replaces previous with current. Rollback swaps exact package, provenance, and approval while preserving enabled/disabled intent. | [Selection implementation](../../../services/korrid/plugin-host/src/selection.rs), lines 10–77. |
| Storage references | Current and previous have separate Nix GC roots. A pending candidate has another root during a transaction. Two retained selections do not mean only two physical builds remain on disk. | [Selection implementation](../../../services/korrid/plugin-host/src/selection.rs), lines 81–133. |
| Ordinary removal | The host records removal intent before cleanup. After cleanup succeeds, it deletes the receipt and active, previous, and pending selection roots. It does not request deletion of private state. | [Host](../../../services/korrid/plugin-host/src/host.rs), lines 312–335 and 556–583; [selection removal](../../../services/korrid/plugin-host/src/selection.rs), lines 121–133. |
| Failed removal | Failed native cleanup prevents removal of the selections. The recorded removal intent prevents recovery from silently starting the plugin again. | [Host](../../../services/korrid/plugin-host/src/host.rs), lines 319–335 and 556–583; [unit cleanup](../../../services/korrid/plugin-host/src/unit.rs), lines 100–163. |
| Reclaiming bytes | The inspected removal path releases references but does not invoke Nix garbage collection. It establishes neither immediate reclaimed storage nor a collection deadline. | The host, selection, and unit paths above. |
| Explicit purge | `--purge` additionally requests systemd deletion of the plugin's state directory. This is not general deletion of game saves or credentials stored elsewhere. | [CLI](../../../services/korrid/plugin-host/src/main.rs), lines 118–119; [unit cleanup](../../../services/korrid/plugin-host/src/unit.rs), lines 156–191. |
| Rollback limits | Rollback does not import a package or consult a repository catalog. It still checks exact approvals and publisher authority. It does not restore a snapshot of mutable application data. | [Host rollback](../../../services/korrid/plugin-host/src/host.rs), lines 285–296; [selection](../../../services/korrid/plugin-host/src/selection.rs), lines 21–28 and 68–77. |

The implementation already releases both software selections on successful removal. Simon subsequently settled [software retention on uninstall](../issues/04-decide-bundled-plugin-lifecycle.md#software-retention-on-uninstall) and [storage reclamation timing](../issues/04-decide-bundled-plugin-lifecycle.md#storage-reclamation-timing). The ticket holds those decisions. Retention after uninstall is distinct from the approved rollback policy for installed plugins.

## Image and startup boundaries

| Inspected source | What it establishes | What it does not establish |
|---|---|---|
| [RG353M host integration](../../../nix/devices/rg353m/plugin-host.nix), lines 1–21. | Enables the generic host and publisher trust. | A preinstalled plugin list or bundled payloads. |
| [RP Mini game integration](../../../nix/devices/rpminiv2/game-plugins.nix), lines 1–51. | Imports the generic host and configures publisher trust. Its opening comment says the image carries receipts and packages. | The comment is not a producer. Neither this module nor the inspected shared composition demonstrates how those receipts and packages reach the image. |
| [Generic host module](../../../services/korrid/plugin-host/nixos-module.nix), lines 66–112. | Installs the host, creates its state/root directories, and runs `restore-all` at boot. | It does not declare a default-plugin inventory or regenerate installed selections from defaults. |
| [Image-time receipt command](../../../services/korrid/plugin-host/src/main.rs), lines 12–32 and 47–60. | `seed` prints an enabled receipt for an existing package, with no previous selection. | It does not write that receipt into an image or install the plugin. |
| [Shared SD image composition](../../../nix/formats/sd-card.nix), lines 9–21. | Uses upstream SD-image production and copies optional Wi-Fi credentials. | Its own population hook does not seed plugins. |
| [Boot recovery](../../../services/korrid/plugin-host/src/host.rs), lines 405–477. | Reads existing selections and removes stale selection roots where a receipt is absent. | It does not choose a new default set from an image inventory. |

**Inferred:** a successfully removed plugin stays removed across ordinary reboot under the inspected recovery path. System-update and reimaging behavior require separate evidence about preservation of state and image delivery. No running device or generated image was inspected.

A copied store payload and a payload retained by the system closure have different lifetimes. Releasing the plugin host's roots cannot release another consumer's references. An image composition that permanently retains removable plugins would prevent the promised reclamation. The current and retained system generations, shared dependencies, and other profiles must be accounted for before promising freed bytes. No image-size measurement is needed to decide that ownership rule.

## User data and verification limits

RP Mini's [game module](../../../nix/devices/rpminiv2/game-plugins.nix), lines 63–94, creates shared save, state, screenshot, and system directories outside the plugin host's selection directories. These are not disposable copies of plugin software. Ordinary removal does not ask systemd to purge plugin state; arbitrary native cleanup is not a general data-safety guarantee.

The [credential VM case](../../../services/korrid/plugin-host/vm-test.nix), lines 660–678, removes the plugin and then separately deletes its credential source. Uninstall does not by itself establish account logout, remote credential revocation, or deletion of every secret.

The [existing lifecycle validation record](../../../services/korrid/plugin-host/LIFECYCLE-VALIDATION.md) reports receipt/filesystem checks, while explicitly leaving the integrated current/previous VM gate unrun in that record. The current [VM test source](../../../services/korrid/plugin-host/vm-test.nix) includes offline rollback, removal/data retention, purge, and failed-cleanup cases. Reading those cases is not executing them.

## Decisions still needed

- Set the image/reference ownership needed to meet the chosen reclamation rule. Do not silently discard system recovery to free storage.
- Set the remaining policy for optional additions and removal of required plugins. The [required-plugin update decision](../issues/04-decide-bundled-plugin-lifecycle.md#required-plugins-during-updates) is recorded in the ticket. Do not invent selection records or a new configuration schema.
- Set interruption/retry behavior without undoing the approved data-preservation and failure-reporting rules. Package rollback is not application-data recovery.

The ticket remains claimed. No new product choice, schema, installer change, deployment, or deletion is authorized by this inventory.
