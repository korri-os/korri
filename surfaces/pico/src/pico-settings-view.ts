import type {
  SurfaceModel,
  SurfaceSettingItem,
} from "@contracts/surface/korri-surface"
import { PICO_FONTS, type PicoFontId } from "./pico-fonts"
import { picoConnectionLabel, picoNetworksFrom, type PicoNetworkView } from "./pico-wifi-view"

/**
 * One settings row as the screen draws it.
 *
 * The treaty's interaction kinds collapse to what Pico can actually offer from
 * a d-pad and two buttons. `choice` becomes a cycler that sends the next value
 * on confirm — the same control legacy drew as `‹ VALUE ›`. `text` and
 * `sensitiveText` both become `text`, opened in Pico's editor: `sensitive`
 * marks a secret, which starts empty and is masked, and whose `value` is only
 * Korri's word about the saved one ("Configured"), never the secret.
 *
 * `step` is a cycler for a long list: it shows only the current choice, as
 * `◀ CHOICE ▶`, because ten face names side by side do not fit a row. A
 * Korri choice with more than PICO_SEGMENTS_MAX values is one too: Player
 * seats offers 255.
 *
 * `network` is one Wi-Fi network from Korri's scan; pressing it joins it.
 */
export type PicoSettingControl =
  | { readonly kind: "fact" }
  | { readonly kind: "action"; readonly actionId: string; readonly destructive: boolean; readonly confirmation?: PicoConfirmation }
  | { readonly kind: "cycle"; readonly next: string; readonly options: readonly string[]; readonly current: number }
  | { readonly kind: "step"; readonly next: string; readonly current: string }
  | {
      readonly kind: "text"
      readonly sensitive: boolean
      readonly placeholder?: string
      readonly maxLength?: number
      /** Korri's words for clearing a saved secret, only when there is one to clear. */
      readonly clearLabel?: string
    }
  | { readonly kind: "network"; readonly actionId: string; readonly network: PicoNetworkView }

export interface PicoConfirmation {
  readonly title: string
  readonly message: string
  readonly confirmLabel: string
  /** Korri marked the action destructive: the question warns (red). Otherwise
   * it only asks (yellow), as a review before turning something on does. */
  readonly destructive?: boolean
}

export interface PicoSettingRowView {
  readonly id: string
  readonly label: string
  readonly value?: string
  readonly description?: string
  /** Where the value in force comes from, in Korri's words ("from Device (default)"). */
  readonly source?: string
  /** Korri says the value stays on this device. */
  readonly deviceOnly?: boolean
  readonly control: PicoSettingControl
  /** What Korri says about this row right now. A `problem` is a settings
   * failure the player dismisses; `failed` is Korri's record of the last
   * attempt, which stays until the next one. */
  readonly state: "idle" | "saving" | { readonly problem: string } | { readonly failed: string }
}

/** A row Pico's text editor can open. */
export type PicoTextSettingRowView = PicoSettingRowView & {
  readonly control: Extract<PicoSettingControl, { readonly kind: "text" }>
}

/** The text setting `settingId` and the title of its group, when Korri publishes one. */
export function picoTextSettingFrom(
  settings: PicoSettingsView,
  settingId: string,
): { readonly group: string; readonly row: PicoTextSettingRowView } | undefined {
  for (const group of settings.groups) {
    for (const row of group.rows) {
      if (row.id === settingId && row.control.kind === "text") {
        return { group: group.title, row: row as PicoTextSettingRowView }
      }
    }
  }
  return undefined
}

export interface PicoSettingsGroupView {
  readonly title: string
  readonly rows: readonly PicoSettingRowView[]
}

export const PICO_IDENTITY_BACKUP_ACTION = "identity:backup"
export const PICO_IDENTITY_SWITCH_LOCAL_ACTION = "identity:switch:local"
export const PICO_IDENTITY_SWITCH_NIP46_ACTION = "identity:switch:nip46"
export const picoIdentityRetiredExportAction = (key: string) => `identity:retired:export:${key}`
export const picoIdentityRetiredDeleteAction = (key: string) => `identity:retired:delete:${key}`
/** The retired key a retired-key action names, when `action` is one for `operation`. */
export function picoIdentityRetiredKey(action: string | null, operation: "export" | "delete"): string | undefined {
  const prefix = `identity:retired:${operation}:`
  return action?.startsWith(prefix) === true ? action.slice(prefix.length) : undefined
}

export interface PicoSettingsView {
  readonly groups: readonly PicoSettingsGroupView[]
  readonly buildLabel?: string
}

/** The id of Pico's own face row. Pico handles a change to it; Korri never sees it. */
export const PICO_FONT_SETTING = "pico:font"

/** A choice with more values than this shows only the current one (`step`). */
export const PICO_SEGMENTS_MAX = 4

/** Pico's Wi-Fi rows. Pico handles them through the Wi-Fi host commands. */
export const PICO_WIFI_SCAN_ACTION = "pico:wifi:scan"
export const PICO_WIFI_HIDDEN_ACTION = "pico:wifi:hidden"
const WIFI_NETWORK_PREFIX = "pico:wifi:network:"
export const picoWifiNetworkAction = (networkId: string) => `${WIFI_NETWORK_PREFIX}${networkId}`
/** The network a Wi-Fi row's action names, when it names one. */
export function picoWifiNetworkId(actionId: string): string | undefined {
  return actionId.startsWith(WIFI_NETWORK_PREFIX) ? actionId.slice(WIFI_NETWORK_PREFIX.length) : undefined
}

/**
 * Korri's network as a settings group: the connection and the last attempt,
 * a scan, every network the scan found, and a hidden network by name. Absent
 * when Korri cannot manage this device's network. There is no "forget": the
 * treaty has no command for it.
 */
function wifiGroup(model: SurfaceModel): PicoSettingsGroupView | undefined {
  const network = model.network
  if (network === undefined) return undefined
  const wifi = network.wifi
  const attempt = wifi._tag === "Available" ? wifi.attempt : undefined
  const connection: PicoSettingRowView = {
    id: "pico:wifi:connection",
    label: "Connection",
    value: attempt?._tag === "Connecting" ? `Joining ${attempt.ssid}…` : picoConnectionLabel(network.connection),
    control: { kind: "fact" },
    state: attempt?._tag === "Failed" ? { failed: attempt.message } : "idle",
  }
  if (wifi._tag === "NoAdapter") {
    return {
      title: "WI-FI",
      rows: [connection, { id: "pico:wifi:adapter", label: "Wi-Fi", value: "None on this device", control: { kind: "fact" }, state: "idle" }],
    }
  }
  const networks = picoNetworksFrom(model)
  return {
    title: "WI-FI",
    rows: [
      connection,
      {
        id: PICO_WIFI_SCAN_ACTION,
        label: "Scan for networks",
        value: wifi.scanning ? "Scanning…" : `${networks.length} found`,
        control: { kind: "action", actionId: PICO_WIFI_SCAN_ACTION, destructive: false },
        state: "idle",
      },
      ...networks.map((item): PicoSettingRowView => ({
        id: picoWifiNetworkAction(item.id),
        label: item.ssid,
        control: { kind: "network", actionId: picoWifiNetworkAction(item.id), network: item },
        state: "idle",
      })),
      {
        id: PICO_WIFI_HIDDEN_ACTION,
        label: "Hidden network",
        description: "Join a network that does not show its name.",
        control: { kind: "action", actionId: PICO_WIFI_HIDDEN_ACTION, destructive: false },
        state: "idle",
      },
    ],
  }
}

/**
 * Pico's own settings: things only this surface has, kept by Pico on this
 * device. Listed after everything Korri published, under Pico's name, so they
 * never read as Korri's.
 */
function picoGroup(font: PicoFontId): PicoSettingsGroupView {
  const at = Math.max(0, PICO_FONTS.findIndex((candidate) => candidate.id === font))
  const current = PICO_FONTS[at]
  const next = PICO_FONTS[(at + 1) % PICO_FONTS.length]
  return {
    title: "PICO",
    rows: [{
      id: PICO_FONT_SETTING,
      label: "Font",
      description: "Kept on this device. Changes at once.",
      control: { kind: "step", next: next?.id ?? font, current: (current?.name ?? font).toUpperCase() },
      state: "idle",
    }],
  }
}

/**
 * The settings screen's view. `pico` carries Pico's own preferences; without
 * it the view holds only what Korri published, as a preview of Korri's part
 * shows it.
 */
export function picoSettingsViewFromModel(
  model: SurfaceModel,
  pico?: { readonly font: PicoFontId },
): PicoSettingsView {
  const identity = model.identityManagement
  const identityRows: PicoSettingRowView[] = identity ? [
    ...(identity.localBackupAvailable ? [{
      id: PICO_IDENTITY_BACKUP_ACTION,
      label: "Back up current identity",
      description: "Encrypted NIP-49 text and QR code",
      control: { kind: "action" as const, actionId: PICO_IDENTITY_BACKUP_ACTION, destructive: false },
      state: "idle" as const,
    }] : []),
    {
      id: PICO_IDENTITY_SWITCH_LOCAL_ACTION,
      label: "Switch from backup",
      control: { kind: "action" as const, actionId: PICO_IDENTITY_SWITCH_LOCAL_ACTION, destructive: true },
      state: "idle" as const,
    },
    {
      id: PICO_IDENTITY_SWITCH_NIP46_ACTION,
      label: "Switch to NIP-46",
      control: { kind: "action" as const, actionId: PICO_IDENTITY_SWITCH_NIP46_ACTION, destructive: true },
      state: "idle" as const,
    },
    ...identity.retiredPublicKeys.flatMap(publicKey => {
      const short = `${publicKey.slice(0, 8)}…${publicKey.slice(-8)}`
      return [{
        id: picoIdentityRetiredExportAction(publicKey),
        label: `Back up retired ${short}`,
        control: { kind: "action" as const, actionId: picoIdentityRetiredExportAction(publicKey), destructive: false },
        state: "idle" as const,
      }, {
        id: picoIdentityRetiredDeleteAction(publicKey),
        label: `Delete retired ${short}`,
        control: { kind: "action" as const, actionId: picoIdentityRetiredDeleteAction(publicKey), destructive: true },
        state: "idle" as const,
      }]
    }),
  ] : []
  const wifi = wifiGroup(model)
  return {
    groups: [
      ...model.settings.map((group) => ({
        title: group.title.toUpperCase(),
        rows: group.items.map((item) => picoSettingRowFrom(item, model)),
      })),
      ...(wifi === undefined ? [] : [wifi]),
      ...(identityRows.length ? [{ title: "IDENTITY", rows: identityRows }] : []),
      ...(pico === undefined ? [] : [picoGroup(pico.font)]),
    ],
    ...(model.buildLabel === undefined ? {} : { buildLabel: model.buildLabel }),
  }
}

/** One Korri setting as a row, with what Korri says about it now. */
export function picoSettingRowFrom(item: SurfaceSettingItem, model: SurfaceModel): PicoSettingRowView {
  const status = model.settingsStatus
  const state =
    status._tag === "Saving" && status.settingId === item.id
      ? "saving"
      : status._tag === "Problem" && status.settingId === item.id
        ? { problem: status.message }
        : "idle"
  return {
    id: item.id,
    label: item.label,
    ...(item.value === undefined ? {} : { value: item.value }),
    ...(item.description === undefined ? {} : { description: item.description }),
    ...(item.source === undefined ? {} : { source: item.source }),
    ...(item.scope === "device" ? { deviceOnly: true } : {}),
    control: controlFor(item),
    state,
  }
}

function controlFor(item: SurfaceSettingItem): PicoSettingControl {
  const interaction = item.interaction
  if (interaction === undefined) return { kind: "fact" }
  switch (interaction.kind) {
    case "action":
      return {
        kind: "action",
        actionId: interaction.actionId,
        destructive: interaction.destructive === true,
        ...(interaction.confirmation === undefined ? {} : {
          confirmation: { ...interaction.confirmation, destructive: interaction.destructive === true },
        }),
      }
    case "choice": {
      const labels = interaction.choices.map((choice) => choice.label)
      // Korri marks the choice in force by value when labels may repeat.
      const current = Math.max(0, interaction.selected === undefined
        ? interaction.choices.findIndex((choice) => choice.label === item.value)
        : interaction.choices.findIndex((choice) => choice.value === interaction.selected))
      const nextChoice = interaction.choices[(current + 1) % interaction.choices.length]
      if (labels.length > PICO_SEGMENTS_MAX) {
        return { kind: "step", next: nextChoice?.value ?? "", current: labels[current] ?? "" }
      }
      return {
        kind: "cycle",
        next: nextChoice?.value ?? "",
        options: labels,
        current,
      }
    }
    case "text":
      return {
        kind: "text",
        sensitive: false,
        ...(interaction.placeholder === undefined ? {} : { placeholder: interaction.placeholder }),
        ...(interaction.maxLength === undefined ? {} : { maxLength: interaction.maxLength }),
      }
    case "sensitiveText":
      return {
        kind: "text",
        sensitive: true,
        ...(interaction.placeholder === undefined ? {} : { placeholder: interaction.placeholder }),
        ...(interaction.maxLength === undefined ? {} : { maxLength: interaction.maxLength }),
        ...(interaction.clearLabel === undefined ? {} : { clearLabel: interaction.clearLabel }),
      }
  }
}
