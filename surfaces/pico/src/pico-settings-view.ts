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
 *
 * `pick` is a Korri choice with none of its values chosen yet, such as Add
 * setting: a press opens the list to choose from, because stepping would
 * choose for the player.
 *
 * A Korri `number` opens the editor too, holding the number set there. The
 * row says the range. When Korri lets the number be cleared, the editor
 * offers INHERIT, which sends "" (the treaty's clear).
 */
export type PicoSettingControl =
  | { readonly kind: "fact" }
  | { readonly kind: "action"; readonly actionId: string; readonly destructive: boolean; readonly confirmation?: PicoConfirmation }
  | {
      readonly kind: "cycle"
      readonly next: string
      readonly options: readonly { readonly value: string; readonly label: string }[]
      readonly current: number
    }
  | { readonly kind: "step"; readonly next: string; readonly current: string }
  | { readonly kind: "pick"; readonly options: readonly { readonly value: string; readonly label: string }[] }
  | {
      readonly kind: "text"
      readonly sensitive: boolean
      readonly placeholder?: string
      readonly maxLength?: number
      /** Korri's words for clearing a saved secret, or Pico's INHERIT for a
       * number Korri lets clear, only when there is something to clear. */
      readonly clearLabel?: string
      /** What the editor opens with, when it is not the row's value. */
      readonly initial?: string
      /** A number: the editor checks the typing against Korri's range. */
      readonly numeric?: { readonly min?: number; readonly max?: number; readonly step?: number }
      /** Korri takes an empty text as a value: SAVE stays on with nothing typed. */
      readonly allowEmpty?: boolean
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
  /** The caption of Korri's group this row opens, when it opens one and the
   * screen names groups (a launch settings place does). */
  readonly section?: string
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
/** So does a choice whose labels together are longer than this many
 * characters: side by side they would squeeze the row's own label. */
export const PICO_SEGMENTS_CHARS = 24

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

/** A number's range in words: "0 to 10", "at least 0". Absent: no range. */
export function picoNumberRange(range: { readonly min?: number; readonly max?: number }): string | undefined {
  if (range.min !== undefined && range.max !== undefined) return `${range.min} to ${range.max}`
  if (range.min !== undefined) return `At least ${range.min}`
  if (range.max !== undefined) return `At most ${range.max}`
  return undefined
}

/** Whether typed text is a number Korri's range takes. */
export function picoNumberFits(text: string, range: { readonly min?: number; readonly max?: number; readonly step?: number }): boolean {
  const trimmed = text.trim()
  if (trimmed === "" || !/^-?\d*\.?\d+$/.test(trimmed)) return false
  const value = Number(trimmed)
  if (range.step === 1 && !Number.isInteger(value)) return false
  return (range.min === undefined || value >= range.min) && (range.max === undefined || value <= range.max)
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
      const choices = interaction.choices
      // Korri marks the choice in force by value: labels may repeat.
      const found = choices.findIndex((choice) => choice.value === interaction.selected)
      // Nothing chosen yet: the player chooses from the list.
      if (found < 0) return { kind: "pick", options: choices.map(({ value, label }) => ({ value, label })) }
      const current = found
      const nextChoice = choices[(current + 1) % choices.length]
      const width = choices.reduce((sum, choice) => sum + choice.label.length + 1, 0)
      if (choices.length > PICO_SEGMENTS_MAX || width > PICO_SEGMENTS_CHARS) {
        return { kind: "step", next: nextChoice?.value ?? "", current: choices[current]?.label ?? "" }
      }
      return {
        kind: "cycle",
        next: nextChoice?.value ?? "",
        options: choices.map(({ value, label }) => ({ value, label })),
        current,
      }
    }
    case "number":
      return {
        kind: "text",
        sensitive: false,
        initial: interaction.value === undefined ? "" : String(interaction.value),
        numeric: {
          ...(interaction.min === undefined ? {} : { min: interaction.min }),
          ...(interaction.max === undefined ? {} : { max: interaction.max }),
          ...(interaction.step === undefined ? {} : { step: interaction.step }),
        },
        placeholder: picoNumberRange(interaction) ?? "Number",
        maxLength: 24,
        ...(interaction.clearable === true && interaction.value !== undefined ? { clearLabel: "Inherit" } : {}),
      }
    case "text":
      return {
        kind: "text",
        sensitive: false,
        ...(interaction.placeholder === undefined ? {} : { placeholder: interaction.placeholder }),
        ...(interaction.maxLength === undefined ? {} : { maxLength: interaction.maxLength }),
        ...(interaction.allowEmpty === true ? { allowEmpty: true } : {}),
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
