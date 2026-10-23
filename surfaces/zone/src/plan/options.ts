/**
 * What the OPTIONS page shows, in order: its groups, their rows, and what A,
 * left and right mean on each row. One pure function of the model slices
 * the page reads, so the state model (which moves the cursor and answers a
 * press) and the page (which draws the rows) can never disagree about which
 * rows exist.
 *
 *     every SurfaceSettingGroup, in Korri's order, a DISMISS row under the
 *       one Korri has a problem with
 *     SYSTEM: SurfaceModel.actions as action rows, then LOOK (Zone's own
 *       choice of look, MODERN / BLEND / RETRO, never sent to Korri), then
 *       BUILD, Korri's build string read-only (absent when there is none of
 *       the three)
 *     WI-FI: the connection (its SSID whole as its description), then NO
 *       WI-FI ADAPTER, or SCAN, the networks
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
import { ZONE_LOOK_WORDS, ZONE_LOOKS, type ZoneLook } from "./look"

/**
 * The parts of Korri's model the OPTIONS page reads, and Zone's own look,
 * which the LOOK row shows (absent: no LOOK row).
 */
export type ZoneOptionsSlices = Pick<SurfaceModel, "settings" | "settingsStatus" | "actions" | "network" | "buildLabel"> & {
  readonly look?: ZoneLook
}

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
      /** Korri's hint for an empty draft. */
      readonly placeholder?: string
      /** A number: DONE sends only a number Korri's range takes. */
      readonly numeric?: ZoneNumberRange
    }
  | { readonly _tag: "Run"; readonly actionId: string; readonly confirm?: ZoneOptionsConfirm }
  | { readonly _tag: "DismissProblem" }
  | { readonly _tag: "Scan" }
  | { readonly _tag: "Join"; readonly networkId: string }
  | { readonly _tag: "JoinHidden" }
  /** A on LOOK steps Zone's look forward, as right does; Korri is never told. */
  | { readonly _tag: "Look" }

/** What the NO / YES dialog asks before an action fires. */
export interface ZoneOptionsConfirm {
  readonly headline: string
  /** The headline is Korri's confirmation title (a sentence of Korri's), not a row's label. */
  readonly korriHeadline?: boolean
  readonly lines: readonly string[]
  readonly confirmLabel: string
}

/**
 * What a Wi-Fi row's value draws besides its words: the connection as the
 * foot's badge, or a network's bars. The item's value is still the words,
 * which name it for assistive tech.
 */
export type ZoneOptionsSignal =
  | { readonly _tag: "Connection"; readonly connection: SurfaceNetworkConnection }
  | { readonly _tag: "Network"; readonly strengthPercent: number; readonly words: readonly string[] }

export interface ZoneOptionsRow {
  /** Korri's setting id, or a `zone:` id for a row Zone makes. */
  readonly id: string
  /** The group's key, for the cursor's jump to SYSTEM. */
  readonly group: string
  /** What ZoneOptionRow draws. */
  readonly item: SurfaceSettingItem
  readonly press: ZoneOptionsPress
  /** A Wi-Fi row's picture of its signal. */
  readonly signal?: ZoneOptionsSignal
  /** Korri cannot do this now: the row is dimmed and its description says why. */
  readonly disabled?: boolean
  /** Which of the row's words are facts Korri publishes, kept in their own case. */
  readonly keepCase?: { readonly label?: boolean; readonly value?: boolean }
  /** The description is Zone's own word (CONNECTING), not Korri's sentence. */
  readonly ownDescription?: boolean
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
/** SYSTEM's read-only BUILD row: Korri's build string, whole (the foot cuts it). */
export const ZONE_SYSTEM_BUILD = "zone:system-build"
/** SYSTEM's LOOK row: Zone's own choice, kept on the device. */
export const ZONE_SYSTEM_LOOK = "zone:system-look"
export const ZONE_WIFI_ADAPTER = "zone:wifi:adapter"
export const ZONE_WIFI_SCAN = "zone:wifi:scan"
export const ZONE_WIFI_HIDDEN = "zone:wifi:hidden"
export const zoneSystemRowId = (actionId: string) => `zone:system:${actionId}`
export const zoneNetworkRowId = (networkId: string) => `zone:wifi:network:${networkId}`

/**
 * The connection as the WI-FI group's first row names it. The row draws it
 * as ZoneSignal (the foot's badge, with its bars); these are its words.
 */
export function zoneConnectionValue(connection: SurfaceNetworkConnection): string {
  switch (connection._tag) {
    case "Offline":
      return "OFFLINE"
    case "Wired":
      return "WIRED"
    case "Wifi":
      return connection.strengthPercent === undefined
        ? connection.ssid
        : `${connection.ssid} ${connection.strengthPercent}%`
  }
}

/**
 * A network's words, one set shared by OPTIONS and first boot: LOCKED or
 * OPEN for its security, then SAVED when Korri knows its key. Zone's own
 * words, so they are set in caps; the strength before them is Korri's.
 */
export function zoneWifiWords(network: SurfaceWifiNetwork): readonly string[] {
  return [network.secured ? "LOCKED" : "OPEN", ...(network.known ? ["SAVED"] : [])]
}

/**
 * A listed network's value: its strength, then its words ("72% LOCKED
 * SAVED"). The one function OPTIONS and first boot name a network with;
 * OPTIONS draws bars before it.
 */
export function zoneNetworkValue(network: SurfaceWifiNetwork): string {
  return [`${network.strengthPercent}%`, ...zoneWifiWords(network)].join(" ")
}

/**
 * A Korri label that reads as a path ("/run/media/mini/roms", "~/roms"):
 * a fact, not a label, so it keeps its case. Any other label of Korri's is
 * set in caps like Zone's own.
 */
export const isPathLabel = (label: string): boolean => /^[/~]/.test(label)

/** A Korri number's range and step, and whether it may be cleared. */
export interface ZoneNumberRange {
  readonly min?: number
  readonly max?: number
  readonly step?: number
  readonly clearable: boolean
}

/** A number's range in Zone's words: "0 TO 10", "AT LEAST 0". Absent: none. */
export function zoneNumberRange(range: { readonly min?: number; readonly max?: number }): string | undefined {
  if (range.min !== undefined && range.max !== undefined) return `${range.min} to ${range.max}`
  if (range.min !== undefined) return `At least ${range.min}`
  if (range.max !== undefined) return `At most ${range.max}`
  return undefined
}

/** Whether a typed draft is a number the range takes. */
export function zoneNumberFits(draft: string, range: ZoneNumberRange): boolean {
  const text = draft.trim()
  if (!/^-?\d*\.?\d+$/.test(text)) return false
  const value = Number(text)
  if (range.step === 1 && !Number.isInteger(value)) return false
  return (range.min === undefined || value >= range.min) && (range.max === undefined || value <= range.max)
}

/**
 * Left or right on a number: one step that way, held inside the range. A
 * number with no value of its own starts from the range's end it moves
 * away from, or 0. Undefined when it cannot move.
 */
export function zoneNumberStep(
  interaction: { readonly value?: number; readonly min?: number; readonly max?: number; readonly step?: number },
  direction: -1 | 1,
): number | undefined {
  const step = interaction.step ?? 1
  const from = interaction.value ?? (direction > 0 ? interaction.min ?? 0 : interaction.max ?? 0)
  const next = interaction.value === undefined ? from : from + direction * step
  const held = Math.min(interaction.max ?? Infinity, Math.max(interaction.min ?? -Infinity, next))
  return held === interaction.value ? undefined : Number(held.toFixed(6))
}

/**
 * A choice Korri publishes with none of its values chosen (Add setting):
 * left and right move a candidate on Zone's side, and A sends it, because
 * stepping would choose for the person.
 */
export const isPick = (item: SurfaceSettingItem): boolean =>
  item.interaction?.kind === "choice" && item.interaction.choices.length > 0 && choiceIndex(item) < 0

/**
 * Where a choice row stands among its choices, by Korri's `selected` value
 * (two choices may share a label); -1 when it names none.
 */
export function choiceIndex(item: SurfaceSettingItem): number {
  if (item.interaction?.kind !== "choice") return -1
  const selected = item.interaction.selected
  return item.interaction.choices.findIndex(choice => choice.value === selected)
}

function settingPress(item: SurfaceSettingItem): ZoneOptionsPress {
  const interaction = item.interaction
  if (interaction === undefined) return { _tag: "Nothing" }
  switch (interaction.kind) {
    case "choice":
      return interaction.choices.length === 0 ? { _tag: "Nothing" } : { _tag: "Step", settingId: item.id }
    case "number": {
      const range = zoneNumberRange(interaction)
      return {
        _tag: "Edit",
        settingId: item.id,
        // The keyboard's title carries the range, so the person sees it while typing.
        label: range === undefined ? item.label : `${item.label} (${range})`,
        draft: interaction.value === undefined ? "" : String(interaction.value),
        secret: false,
        maxLength: 24,
        placeholder: range ?? "Number",
        ...(interaction.clearable === true && interaction.value !== undefined ? { clearLabel: "Inherit" } : {}),
        numeric: {
          ...(interaction.min === undefined ? {} : { min: interaction.min }),
          ...(interaction.max === undefined ? {} : { max: interaction.max }),
          ...(interaction.step === undefined ? {} : { step: interaction.step }),
          clearable: interaction.clearable === true,
        },
      }
    }
    case "text":
      return {
        _tag: "Edit",
        settingId: item.id,
        label: item.label,
        draft: item.value ?? "",
        secret: false,
        ...(interaction.maxLength === undefined ? {} : { maxLength: interaction.maxLength }),
        ...(interaction.placeholder === undefined ? {} : { placeholder: interaction.placeholder }),
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
        ...(interaction.placeholder === undefined ? {} : { placeholder: interaction.placeholder }),
      }
    case "action": {
      const confirm: ZoneOptionsConfirm | undefined = interaction.confirmation !== undefined
        ? {
            headline: interaction.confirmation.title,
            korriHeadline: true,
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
      // Korri's value is a fact and keeps its case; its label does when it is a path.
      const row: ZoneOptionsRow = {
        id: item.id,
        group: key,
        item,
        press: settingPress(item),
        keepCase: { value: true, ...(isPathLabel(item.label) ? { label: true } : {}) },
      }
      return status._tag === "Problem" && status.settingId === item.id
        ? [row, { id: ZONE_OPTIONS_DISMISS, group: key, item: DISMISS_ITEM, press: { _tag: "DismissProblem" } }]
        : [row]
    })
    return { key, title: group.title, rows, status, verbatim: false }
  })
}

/**
 * A device action as an action row. An inert one is the same row disabled:
 * dimmed, A does nothing, and Korri's description says why when focused.
 */
function systemRow(action: SurfaceAction): ZoneOptionsRow {
  const id = zoneSystemRowId(action.id)
  const description = action.description === undefined ? {} : { description: action.description }
  if (!action.enabled) {
    return {
      id,
      group: ZONE_OPTIONS_SYSTEM,
      item: { id, label: action.label, value: "Go", ...description, interaction: { kind: "action", actionId: action.id } },
      press: { _tag: "Nothing" },
      disabled: true,
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

/**
 * Korri's build string as a read-only row. The foot draws it last and cuts
 * it first ("korri 0.4.1 · …" at 640x480); here it stands whole, or cut by
 * the row and whole on the focused row's own line. A fact: its own case.
 */
function buildRow(build: string): ZoneOptionsRow {
  return {
    id: ZONE_SYSTEM_BUILD,
    group: ZONE_OPTIONS_SYSTEM,
    item: { id: ZONE_SYSTEM_BUILD, label: "Build", value: build },
    press: { _tag: "Nothing" },
    keepCase: { value: true },
  }
}

/**
 * Zone's look as a choice row: its three words, in the axis' order, the
 * value the look in use. A choice so it steps on left and right (the
 * portal's `korri-semantic-direction`) and on A; its press is Zone's own.
 */
function lookRow(look: ZoneLook): ZoneOptionsRow {
  return {
    id: ZONE_SYSTEM_LOOK,
    group: ZONE_OPTIONS_SYSTEM,
    item: {
      id: ZONE_SYSTEM_LOOK,
      label: "Look",
      value: ZONE_LOOK_WORDS[look],
      interaction: { kind: "choice", choices: ZONE_LOOKS.map(id => ({ value: id, label: ZONE_LOOK_WORDS[id] })), selected: look },
    },
    press: { _tag: "Look" },
  }
}

function systemGroup(slices: ZoneOptionsSlices): ZoneOptionsGroup[] {
  const build = slices.buildLabel === undefined || slices.buildLabel === "" ? [] : [buildRow(slices.buildLabel)]
  const look = slices.look === undefined ? [] : [lookRow(slices.look)]
  if (slices.actions.length === 0 && build.length === 0 && look.length === 0) return []
  return [{
    key: ZONE_OPTIONS_SYSTEM,
    title: "System",
    rows: [...slices.actions.map(systemRow), ...look, ...build],
    status: { _tag: "Idle" },
    verbatim: false,
  }]
}

const wifiRow = (id: string, item: Omit<SurfaceSettingItem, "id">, press: ZoneOptionsPress): ZoneOptionsRow =>
  ({ id, group: ZONE_OPTIONS_WIFI, item: { id, ...item }, press })

function wifiGroup(slices: ZoneOptionsSlices): ZoneOptionsGroup[] {
  const network = slices.network
  if (network === undefined) return []
  // The SSID whole under the focused row: the foot cuts it first and drops
  // it on a narrow screen, and the row's badge may cut it too.
  const ssid = network.connection._tag === "Wifi" ? { description: network.connection.ssid } : {}
  const connection: ZoneOptionsRow = {
    ...wifiRow(
      ZONE_WIFI_CONNECTION,
      { label: "CONNECTION", value: zoneConnectionValue(network.connection), ...ssid },
      { _tag: "Nothing" },
    ),
    signal: { _tag: "Connection", connection: network.connection },
  }
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
  const networks = [...wifi.networks].sort((a, b) => b.strengthPercent - a.strengthPercent)
  const attempt = wifi.attempt
  const networkRows = networks.map((item): ZoneOptionsRow => ({
    // An SSID is a name Korri publishes: it keeps its case.
    keepCase: { label: true },
    ...(attempt._tag === "Connecting" && attempt.ssid === item.ssid ? { ownDescription: true } : {}),
    ...wifiRow(
      zoneNetworkRowId(item.id),
      {
        label: item.ssid,
        value: zoneNetworkValue(item),
        ...(attempt._tag === "Connecting" && attempt.ssid === item.ssid ? { description: "CONNECTING" } : {}),
        interaction: { kind: "action", actionId: zoneNetworkRowId(item.id) },
      },
      { _tag: "Join", networkId: item.id },
    ),
    signal: { _tag: "Network", strengthPercent: item.strengthPercent, words: zoneWifiWords(item) },
  }))
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
      readonly placeholder?: string
      readonly numeric?: ZoneNumberRange
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
  /** Korri's hint, dimmed on the slab while the draft is empty. */
  readonly placeholder?: string
  readonly numeric?: ZoneNumberRange
} {
  switch (entry._tag) {
    case "Setting":
      return {
        title: entry.label,
        secret: entry.secret,
        ...(entry.numeric === undefined ? {} : { numeric: entry.numeric }),
        ...(entry.maxLength === undefined ? {} : { maxLength: entry.maxLength }),
        ...(entry.clearLabel === undefined ? {} : { clearLabel: entry.clearLabel }),
        ...(entry.placeholder === undefined ? {} : { placeholder: entry.placeholder }),
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
  /** A pick row's candidate, by row: the choice A would send. */
  readonly pick?: { readonly rowId: string; readonly at: number } | undefined
  readonly focused?: string | undefined
  readonly keyboard?: ZoneOptionsKeyboard | undefined
  readonly dialog?: ZoneOptionsDialog | undefined
}
