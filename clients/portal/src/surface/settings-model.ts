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

/* Every fact here comes from korrid. The permission, streaming-host, and
 * system-information facts came from the shell, which no longer exists. */
export interface DeviceFacts {
  readonly version?: string
  readonly settings?: SettingsSnapshot
  readonly localGameCount?: number
  readonly discovery?: DiscoverySnapshot
}

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

const onOff = [
  { value: "true", label: "On" },
  { value: "false", label: "Off" },
] as const

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
  const groups = [
    group("Device", [
      facts.settings === undefined
        ? undefined
        : {
            id: "device-name",
            label: "Name",
            value: facts.settings.deviceName ?? "Unnamed",
            interaction: {
              kind: "text" as const,
              placeholder: "This device",
              maxLength: 64,
            },
          },
    ]),
    group("Metadata", [
      facts.settings === undefined
        ? undefined
        : {
            id: "steamgriddb-credential",
            label: "SteamGridDB API key",
            value: secretStatusLabel(facts.settings.steamGridDbCredential),
            description: "Used only by korrid for metadata and cover art lookup",
            interaction: {
              kind: "sensitiveText" as const,
              placeholder: "Paste API key",
              maxLength: 256,
              ...(facts.settings.steamGridDbCredential === SecretSettingStatus.Configured
                ? { clearLabel: "Clear saved key" }
                : {}),
            },
          },
    ]),
    group(
      "Plugins",
      facts.settings?.plugins.map(plugin => ({
        id: plugin.id,
        label: plugin.title,
        value: plugin.enabled ? "On" : "Off",
        interaction: { kind: "choice" as const, choices: onOff },
      })) ?? [],
    ),
    group("Games", [
      facts.localGameCount === undefined
        ? undefined
        : {
            id: "local-games",
            label: "On this device",
            value: countLabel(facts.localGameCount, "game"),
            description: "Declared in catalog/games.yaml",
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
            value: countLabel(facts.discovery.locations.length, "folder"),
            interaction: {
              kind: "action" as const,
              actionId: "game-folder-rescan",
            },
          },
      ...(facts.discovery?.locations.map(location => ({
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
