/**
 * What the OPTIONS page shows, in order: its groups, their rows, and what A,
 * left and right mean on each row. One pure function of the model slices
 * the page reads, so the state model (which moves the cursor and answers a
 * press) and the page (which draws the rows) can never disagree about which
 * rows exist.
 *
 *     every SurfaceSettingGroup, in Korri's order, a DISMISS row under the
 *       one Korri has a problem with
 *     SYSTEM: SurfaceModel.actions as action rows (absent when there are none)
 *     WI-FI: the connection, then NO WI-FI ADAPTER, or SCAN, the networks
 *       strongest first and HIDDEN NETWORK (absent when Korri cannot manage
 *       the network)
 *
 * Every row is drawn by ZoneOptionRow, which takes a string value, so the
 * rows Zone makes for itself are SurfaceSettingItems too. Their ids start
 * with `zone:` so they never meet an id Korri publishes. Zone's own words
 * are written in caps here because the WI-FI group keeps the case of the
 * SSIDs it lists.
 *
 * The page's own state lives in src/state/options.ts. Its keyboard and
 * dialog types are declared here, beside the rows, because the page draws
 * them and may not import src/state.
 */
import type {
  SurfaceAction,
  SurfaceModel,
  SurfaceNetworkConnection,
  SurfaceSettingItem,
  SurfaceSettingsStatus,
  SurfaceWifiNetwork,
} from "@contracts/surface/korri-surface"

/** The parts of Korri's model the OPTIONS page reads. */
export type ZoneOptionsSlices = Pick<SurfaceModel, "settings" | "settingsStatus" | "actions" | "network">

/** What A does on a row. */
export type ZoneOptionsPress =
  | { readonly _tag: "Nothing" }
  /** A on a choice steps it forward, as right does. */
  | { readonly _tag: "Step"; readonly settingId: string }
  | {
      readonly _tag: "Edit"
      readonly settingId: string
      readonly label: string
      readonly draft: string
      readonly secret: boolean
      readonly maxLength?: number
      readonly clearLabel?: string
    }
  | { readonly _tag: "Run"; readonly actionId: string; readonly confirm?: ZoneOptionsConfirm }
  | { readonly _tag: "DismissProblem" }
  | { readonly _tag: "Scan" }
  | { readonly _tag: "Join"; readonly networkId: string }
  | { readonly _tag: "JoinHidden" }

/** What the NO / YES dialog asks before an action fires. */
export interface ZoneOptionsConfirm {
  readonly headline: string
  readonly lines: readonly string[]
  readonly confirmLabel: string
}

export interface ZoneOptionsRow {
  /** Korri's setting id, or a `zone:` id for a row Zone makes. */
  readonly id: string
  /** The group's key, for the cursor's jump to SYSTEM. */
  readonly group: string
  /** What ZoneOptionRow draws. */
  readonly item: SurfaceSettingItem
  readonly press: ZoneOptionsPress
}

export interface ZoneOptionsGroup {
  readonly key: string
  readonly title: string
  readonly rows: readonly ZoneOptionsRow[]
  /** Korri's settings status, or the Wi-Fi attempt said the same way. */
  readonly status: SurfaceSettingsStatus
  /** A row's label is a name Korri publishes (an SSID): keep its case. */
  readonly verbatim: boolean
}

export interface ZoneOptionsPlan {
  readonly groups: readonly ZoneOptionsGroup[]
  /** Every row, top to bottom across the groups: the order up and down walk. */
  readonly rows: readonly ZoneOptionsRow[]
}

/** The group keys Zone makes. A settings group's key is its index. */
export const ZONE_OPTIONS_SYSTEM = "zone:system"
export const ZONE_OPTIONS_WIFI = "zone:wifi"

/** Row ids Zone makes. */
export const ZONE_OPTIONS_DISMISS = "zone:dismiss"
export const ZONE_WIFI_CONNECTION = "zone:wifi:connection"
export const ZONE_WIFI_ADAPTER = "zone:wifi:adapter"
export const ZONE_WIFI_SCAN = "zone:wifi:scan"
export const ZONE_WIFI_HIDDEN = "zone:wifi:hidden"
export const zoneSystemRowId = (actionId: string) => `zone:system:${actionId}`
export const zoneNetworkRowId = (networkId: string) => `zone:wifi:network:${networkId}`

/** Signal bars as glyphs: one lit per quarter, never none, as ZoneSignal counts them. */
const BAR_GLYPHS = ["▂", "▄", "▆", "█"] as const
const BAR_OFF = "▁"

export function zoneBars(strengthPercent: number | undefined): string {
  const lit = strengthPercent === undefined ? 1
    : Math.max(1, Math.min(BAR_GLYPHS.length, Math.round(strengthPercent / 25)))
  return BAR_GLYPHS.map((glyph, index) => (index < lit ? glyph : BAR_OFF)).join("")
}

/** The connection as the WI-FI group's first row reads it. */
export function zoneConnectionValue(connection: SurfaceNetworkConnection): string {
  switch (connection._tag) {
    case "Offline":
      return "OFFLINE"
    case "Wired":
      return "WIRED"
    case "Wifi":
      return connection.strengthPercent === undefined
        ? `${connection.ssid} ${zoneBars(undefined)}`
        : `${connection.ssid} ${zoneBars(connection.strengthPercent)} ${connection.strengthPercent}%`
  }
}

/** A listed network's value: bars, then LOCK and KNOWN when they hold, or JOINED for the one in use. */
export function zoneNetworkValue(network: SurfaceWifiNetwork, joined: boolean): string {
  return [
    zoneBars(network.strengthPercent),
    ...(network.secured ? ["LOCK"] : []),
    ...(network.known ? ["KNOWN"] : []),
    ...(joined ? ["JOINED"] : []),
  ].join(" ")
}

/** Where a choice row stands among its choices; -1 when Korri's value names none. */
export function choiceIndex(item: SurfaceSettingItem): number {
  if (item.interaction?.kind !== "choice") return -1
  return item.interaction.choices.findIndex(choice => choice.label === item.value || choice.value === item.value)
}

function settingPress(item: SurfaceSettingItem): ZoneOptionsPress {
  const interaction = item.interaction
  if (interaction === undefined) return { _tag: "Nothing" }
  switch (interaction.kind) {
    case "choice":
      return interaction.choices.length === 0 ? { _tag: "Nothing" } : { _tag: "Step", settingId: item.id }
    case "text":
      return {
        _tag: "Edit",
        settingId: item.id,
        label: item.label,
        draft: item.value ?? "",
        secret: false,
        ...(interaction.maxLength === undefined ? {} : { maxLength: interaction.maxLength }),
      }
    case "sensitiveText":
      // A secret's value is Korri's word about it ("Configured"), never the
      // secret, so the draft starts empty.
      return {
        _tag: "Edit",
        settingId: item.id,
        label: item.label,
        draft: "",
        secret: true,
        ...(interaction.maxLength === undefined ? {} : { maxLength: interaction.maxLength }),
        ...(interaction.clearLabel === undefined ? {} : { clearLabel: interaction.clearLabel }),
      }
    case "action": {
      const confirm: ZoneOptionsConfirm | undefined = interaction.confirmation !== undefined
        ? {
            headline: interaction.confirmation.title,
            lines: [interaction.confirmation.message],
            confirmLabel: interaction.confirmation.confirmLabel,
          }
        : interaction.destructive === true
          ? { headline: item.label, lines: [], confirmLabel: "Yes" }
          : undefined
      return confirm === undefined
        ? { _tag: "Run", actionId: interaction.actionId }
        : { _tag: "Run", actionId: interaction.actionId, confirm }
    }
  }
}

const DISMISS_ITEM: SurfaceSettingItem = {
  id: ZONE_OPTIONS_DISMISS,
  label: "Dismiss",
  value: "OK",
  interaction: { kind: "action", actionId: ZONE_OPTIONS_DISMISS },
}

function settingsGroups(slices: ZoneOptionsSlices): ZoneOptionsGroup[] {
  const status = slices.settingsStatus
  return slices.settings.map((group, index) => {
    const key = `setting:${index}`
    const rows = group.items.flatMap((item): ZoneOptionsRow[] => {
      const row: ZoneOptionsRow = { id: item.id, group: key, item, press: settingPress(item) }
      return status._tag === "Problem" && status.settingId === item.id
        ? [row, { id: ZONE_OPTIONS_DISMISS, group: key, item: DISMISS_ITEM, press: { _tag: "DismissProblem" } }]
        : [row]
    })
    return { key, title: group.title, rows, status, verbatim: false }
  })
}

/** A device action as an action row. An inert one is drawn read-only and says so. */
function systemRow(action: SurfaceAction): ZoneOptionsRow {
  const id = zoneSystemRowId(action.id)
  const description = action.description === undefined ? {} : { description: action.description }
  if (!action.enabled) {
    return {
      id,
      group: ZONE_OPTIONS_SYSTEM,
      item: { id, label: action.label, value: "Unavailable", ...description },
      press: { _tag: "Nothing" },
    }
  }
  const destructive = action.destructive === true
  return {
    id,
    group: ZONE_OPTIONS_SYSTEM,
    item: {
      id,
      label: action.label,
      value: "Go",
      ...description,
      interaction: { kind: "action", actionId: action.id, ...(destructive ? { destructive } : {}) },
    },
    press: destructive
      ? {
          _tag: "Run",
          actionId: action.id,
          confirm: {
            headline: action.label,
            lines: action.description === undefined ? [] : [action.description],
            confirmLabel: "Yes",
          },
        }
      : { _tag: "Run", actionId: action.id },
  }
}

function systemGroup(slices: ZoneOptionsSlices): ZoneOptionsGroup[] {
  if (slices.actions.length === 0) return []
  return [{
    key: ZONE_OPTIONS_SYSTEM,
    title: "System",
    rows: slices.actions.map(systemRow),
    status: { _tag: "Idle" },
    verbatim: false,
  }]
}

const wifiRow = (id: string, item: Omit<SurfaceSettingItem, "id">, press: ZoneOptionsPress): ZoneOptionsRow =>
  ({ id, group: ZONE_OPTIONS_WIFI, item: { id, ...item }, press })

function wifiGroup(slices: ZoneOptionsSlices): ZoneOptionsGroup[] {
  const network = slices.network
  if (network === undefined) return []
  const connection = wifiRow(
    ZONE_WIFI_CONNECTION,
    { label: "CONNECTION", value: zoneConnectionValue(network.connection) },
    { _tag: "Nothing" },
  )
  const wifi = network.wifi
  if (wifi._tag === "NoAdapter") {
    return [{
      key: ZONE_OPTIONS_WIFI,
      title: "WI-FI",
      rows: [connection, wifiRow(ZONE_WIFI_ADAPTER, { label: "NO WI-FI ADAPTER" }, { _tag: "Nothing" })],
      status: { _tag: "Idle" },
      verbatim: true,
    }]
  }
  const inUse = network.connection._tag === "Wifi" ? network.connection.ssid : undefined
  const networks = [...wifi.networks].sort((a, b) => b.strengthPercent - a.strengthPercent)
  const attempt = wifi.attempt
  const networkRows = networks.map(item => wifiRow(
    zoneNetworkRowId(item.id),
    {
      label: item.ssid,
      value: zoneNetworkValue(item, item.ssid === inUse),
      ...(attempt._tag === "Connecting" && attempt.ssid === item.ssid ? { description: "CONNECTING" } : {}),
      interaction: { kind: "action", actionId: zoneNetworkRowId(item.id) },
    },
    { _tag: "Join", networkId: item.id },
  ))
  const rows: ZoneOptionsRow[] = [
    connection,
    wifiRow(
      ZONE_WIFI_SCAN,
      {
        label: "SCAN",
        value: wifi.scanning ? "SCANNING…" : `${networks.length} FOUND`,
        interaction: { kind: "action", actionId: ZONE_WIFI_SCAN },
      },
      { _tag: "Scan" },
    ),
    ...networkRows,
    wifiRow(
      ZONE_WIFI_HIDDEN,
      { label: "HIDDEN NETWORK", value: "JOIN", interaction: { kind: "action", actionId: ZONE_WIFI_HIDDEN } },
      { _tag: "JoinHidden" },
    ),
  ]
  // The attempt is said on its network's row, as a settings status is: the
  // caret while Korri joins, the message in red when it could not. A network
  // that is not listed (a hidden one) is said on HIDDEN NETWORK.
  const rowFor = (ssid: string) => {
    const listed = networks.find(item => item.ssid === ssid)
    return listed === undefined ? ZONE_WIFI_HIDDEN : zoneNetworkRowId(listed.id)
  }
  const status: SurfaceSettingsStatus = attempt._tag === "Connecting"
    ? { _tag: "Saving", settingId: rowFor(attempt.ssid) }
    : attempt._tag === "Failed"
      ? { _tag: "Problem", settingId: rowFor(attempt.ssid), message: attempt.message }
      : { _tag: "Idle" }
  return [{ key: ZONE_OPTIONS_WIFI, title: "WI-FI", rows, status, verbatim: true }]
}

/** The page's groups and rows, top to bottom. */
export function optionsPlan(slices: ZoneOptionsSlices): ZoneOptionsPlan {
  const groups = [...settingsGroups(slices), ...systemGroup(slices), ...wifiGroup(slices)]
  return { groups, rows: groups.flatMap(group => group.rows) }
}

/**
 * The row the cursor is on. `focused` may name a group (SYSTEM, after the
 * `system` button): that is its first row, or the first row of the page when
 * the group is absent. A row id the plan no longer has is no row.
 */
export function focusedRow(plan: ZoneOptionsPlan, focused: string | undefined): ZoneOptionsRow | undefined {
  if (focused === undefined) return undefined
  if (focused === ZONE_OPTIONS_SYSTEM) {
    return plan.rows.find(row => row.group === ZONE_OPTIONS_SYSTEM) ?? plan.rows[0]
  }
  return plan.rows.find(row => row.id === focused)
}

// ---- The page's own layers, as the page draws them ----

/** The keyboard's layers and keys, as ZoneKeyboard names them. */
export type ZoneOptionsKeyLayer = "letters" | "shift" | "symbols" | "uri"
export type ZoneOptionsKey =
  | { readonly kind: "char"; readonly char: string }
  | { readonly kind: "space" }
  | { readonly kind: "delete" }
  | { readonly kind: "done" }
  | { readonly kind: "layer"; readonly layer: ZoneOptionsKeyLayer }

/** What the keyboard is typing, and for what. */
export type ZoneOptionsEntry =
  | {
      readonly _tag: "Setting"
      readonly settingId: string
      readonly label: string
      readonly secret: boolean
      readonly maxLength?: number
      readonly clearLabel?: string
    }
  | { readonly _tag: "WifiPassword"; readonly networkId: string; readonly ssid: string }
  | { readonly _tag: "HiddenSsid" }
  | { readonly _tag: "HiddenPassword"; readonly ssid: string }

export interface ZoneOptionsKeyboard {
  readonly entry: ZoneOptionsEntry
  readonly draft: string
  readonly caret: number
  readonly layer: ZoneOptionsKeyLayer
  /** The focused key's id, as ZoneKeyboard names it; absent until the cursor lands. */
  readonly focusedKey?: string
}

export interface ZoneOptionsDialog {
  readonly actionId: string
  readonly confirm: ZoneOptionsConfirm
  /** 0 is NO, 1 is the confirming button. NO is lit when the dialog opens. */
  readonly focused: 0 | 1
}

/** IEEE 802.11 names are at most 32 octets; a WPA passphrase at most 63 characters. */
export const WIFI_SSID_MAX = 32
export const WIFI_PASSWORD_MAX = 63

/** The keyboard's heading, its secrecy, its limit and its clear key. */
export function entryHeading(entry: ZoneOptionsEntry): {
  readonly title: string
  /** A name Korri publishes, kept in its own case. */
  readonly name?: string
  readonly secret: boolean
  readonly maxLength?: number
  readonly clearLabel?: string
} {
  switch (entry._tag) {
    case "Setting":
      return {
        title: entry.label,
        secret: entry.secret,
        ...(entry.maxLength === undefined ? {} : { maxLength: entry.maxLength }),
        ...(entry.clearLabel === undefined ? {} : { clearLabel: entry.clearLabel }),
      }
    case "WifiPassword":
      return { title: "Password", name: entry.ssid, secret: true, maxLength: WIFI_PASSWORD_MAX }
    case "HiddenSsid":
      return { title: "Network name", secret: false, maxLength: WIFI_SSID_MAX }
    case "HiddenPassword":
      return { title: "Password", name: entry.ssid, secret: true, maxLength: WIFI_PASSWORD_MAX }
  }
}

/** The page's own state, as the page reads it: src/state/options.ts's model
 * is one of these. */
export interface ZoneOptionsView {
  readonly focused?: string | undefined
  readonly keyboard?: ZoneOptionsKeyboard | undefined
  readonly dialog?: ZoneOptionsDialog | undefined
}
