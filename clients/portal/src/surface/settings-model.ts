/** Device facts and the narrow settings Korri can honestly change today. */
import type {
  SurfaceSettingGroup,
  SurfaceSettingItem,
} from "@contracts/surface/korri-surface"
import {
  SecretSettingStatus,
  type DiscoverySnapshot,
  type SettingsSnapshot,
} from "@contracts/generated/korrid"
import { pluginSettingGroups, type PluginFacts } from "./plugin-repositories"

/* Every fact here comes from korrid. The permission, streaming-host, and
 * system-information facts came from the shell, which no longer exists. */
export interface DeviceFacts {
  readonly version?: string
  readonly settings?: SettingsSnapshot
  readonly localGameCount?: number
  readonly discovery?: DiscoverySnapshot
  /** Absent until Korri has read setup progress. */
  readonly onboardingNeeded?: boolean
  /** Plugin repositories and the approval path. Absent before the portal
   * has them (and in tests that do not need them); then no plugin group shows. */
  readonly plugins?: PluginFacts
  /** This device's launch settings, built by the launch settings controller.
   * Absent until read, and when korrid cannot state them. */
  readonly launchSettings?: SurfaceSettingGroup
}

/** Setting-local action that puts the device back into first-boot setup. */
export const SETUP_RESTART_ACTION_ID = "setup-restart"

const countLabel = (count: number, noun: string): string =>
  count === 1 ? `1 ${noun}` : `${count} ${noun}s`

const group = (
  title: string,
  items: readonly (SurfaceSettingItem | undefined)[],
): SurfaceSettingGroup | undefined => {
  const present = items.filter((item): item is SurfaceSettingItem =>
    Boolean(item),
  )
  return present.length > 0 ? { title, items: present } : undefined
}

// The device-only setting accepts the existing positive-u8 seat range.
const playerCounts = Array.from({ length: 255 }, (_, index) => {
  const value = String(index + 1)
  return { value, label: value }
})

const secretStatusLabel = (status: SecretSettingStatus): string =>
  status === SecretSettingStatus.Configured ? "Configured" : "Not configured"

const discoveryStateLabel = (snapshot: DiscoverySnapshot | undefined): string => {
  if (snapshot === undefined) return "Not set up"
  switch (snapshot.state._tag) {
    case "Scanning":
      return "Scanning…"
    case "Enriching":
      return "Adding details…"
    case "Problem":
      return "Needs attention"
    case "Idle":
      return "Ready"
  }
  const exhaustive: never = snapshot.state
  return exhaustive
}

export function settingsFrom(
  facts: DeviceFacts,
): readonly SurfaceSettingGroup[] {
  const canEdit = (id: string) => facts.settings?.editableSettingIds.includes(id) ?? false
  const groups = [
    group("Device", [
      facts.settings === undefined
        ? undefined
        : {
            id: "device-name",
            label: "Name",
            // No name, no value: a surface's editor then starts empty and shows
            // the placeholder, instead of seeding a word nobody chose.
            ...(facts.settings.deviceName == null ? {} : { value: facts.settings.deviceName }),
            ...(canEdit("device-name") ? {
              interaction: {
                kind: "text" as const,
                placeholder: "This device",
                maxLength: 64,
              },
            } : {}),
          },
      // Offered only once setup has finished; during setup the row would be a
      // no-op, and before the first read Korri cannot say which it is.
      facts.onboardingNeeded !== false
        ? undefined
        : {
            id: SETUP_RESTART_ACTION_ID,
            label: "Run setup again",
            description: "Games, Wi-Fi and identity stay.",
            interaction: {
              kind: "action" as const,
              actionId: SETUP_RESTART_ACTION_ID,
              confirmation: {
                title: "Run setup again?",
                message:
                  "Korri will show first-boot setup now. Games, Wi-Fi and identity stay.",
                confirmLabel: "Run setup",
              },
            },
          },
    ]),
    group("Controllers", [
      facts.settings === undefined
        ? undefined
        : {
            id: "host.preferences.playerCount",
            label: "Player seats",
            value: String(facts.settings.playerCount),
            description: "Shared by physical and remote controllers. Stop the game before changing.",
            ...(canEdit("host.preferences.playerCount") ? {
              interaction: { kind: "choice" as const, choices: playerCounts, selected: String(facts.settings.playerCount) },
            } : {}),
          },
    ]),
    facts.launchSettings,
    group("Metadata", [
      facts.settings === undefined
        ? undefined
        : {
            id: "steamgriddb-credential",
            label: "SteamGridDB API key",
            value: secretStatusLabel(facts.settings.steamGridDbCredential),
            description: "Used only by korrid for metadata and cover art lookup",
            ...(canEdit("steamgriddb-credential") ? {
              interaction: {
                kind: "sensitiveText" as const,
                placeholder: "Paste API key",
                maxLength: 256,
                ...(facts.settings.steamGridDbCredential === SecretSettingStatus.Configured
                  ? { clearLabel: "Clear saved key" }
                  : {}),
              },
            } : {}),
          },
    ]),
    // Plugins: one group per repository, built from korrid's repository
    // facts. Toggles go through setPluginEnabled, never updateSetting.
    ...(facts.plugins === undefined ? [] : pluginSettingGroups(facts.plugins, facts.settings?.plugins)),
    group("Games", [
      facts.localGameCount === undefined
        ? undefined
        : {
            id: "local-games",
            label: "On this device",
            value: countLabel(facts.localGameCount, "game"),
            description: "Declared in catalog/library.yaml",
          },
      facts.discovery === undefined
        ? undefined
        : {
            id: "game-discovery-status",
            label: "Folder scan",
            value: discoveryStateLabel(facts.discovery),
            description:
              facts.discovery.diagnostics[0]?.message ??
              "Games appear as soon as scanning finds them",
          },
      facts.discovery === undefined
        ? undefined
        : {
            id: "game-folder-rescan",
            label: "Rescan game folders",
            value: countLabel(facts.discovery.sources.length, "folder"),
            interaction: {
              kind: "action" as const,
              actionId: "game-folder-rescan",
            },
          },
      ...(facts.discovery?.sources.map(location => ({
        id: `game-folder:${location.id}`,
        label: location.label,
        value: "Registered",
        interaction: {
          kind: "action" as const,
          actionId: `game-folder-remove:${location.id}`,
          destructive: true,
          confirmation: {
            title: "Remove game folder?",
            message:
              "Korri will remove games it added from this folder. Edited or hand-authored games stay.",
            confirmLabel: "Remove folder",
          },
        },
      })) ?? []),
    ]),
    group("System information", [
      facts.version === undefined
        ? undefined
        : { id: "korrid-version", label: "korrid", value: facts.version },
    ]),
  ]

  return groups.filter((entry): entry is SurfaceSettingGroup => Boolean(entry))
}
