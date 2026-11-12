/**
 * Korri's settings, device actions and Wi-Fi as rows: what each row says,
 * and what Confirm, left and right mean on it. One description, read by the
 * view (which draws the row) and by the update (which answers the press).
 *
 * The rules are Korri's (the treaty's SurfaceSettingInteraction):
 *
 *   action         Confirm runs it; one that cannot be undone, or that Korri
 *                  gives a confirmation for, is asked about first
 *   choice         left and right step through the choices, round the ends,
 *                  sent at once; Confirm steps forward. Marked and stepped
 *                  from `selected`, never by matching a label. A choice with
 *                  nothing chosen (Add setting) moves a candidate on Velvet's
 *                  side and Confirm sends it: stepping would choose for the
 *                  person.
 *   number         left and right step it inside its range; Confirm types it
 *   text           Confirm types it
 *   sensitiveText  Confirm types it, as dots; Korri never publishes the
 *                  secret, so the draft starts empty; `clearLabel` is a key
 *   none           a fact: it takes focus so it can be read whole
 */
import type {
  SurfaceAction,
  SurfaceModel as Model,
  SurfaceNetworkConnection,
  SurfaceSettingItem,
  SurfaceSettingsStatus,
  SurfaceWifiAttempt,
  SurfaceWifiNetwork,
} from "../../../../../contracts/generated/surface"
import { NONE, send, seq, type Act, type AskSpec, type Entry, type NumberRange, type RowSpec, type State } from "./types"

export const WIFI_SSID_MAX = 32
export const WIFI_PASSWORD_MAX = 63

/** A question before something that cannot be undone. The way out comes first and holds focus. */
export function question(title: string, lines: readonly string[], confirmLabel: string, act: Act): AskSpec {
  return { title, lines, buttons: [{ label: "Cancel", act: NONE }, { label: confirmLabel, danger: true, act }] }
}

/** One of Korri's actions as a row. An inert one is dim and says why. */
export function actionRow(id: string, action: SurfaceAction, run: Act): RowSpec {
  const note = action.description === undefined ? {} : { note: action.description }
  if (!action.enabled) return { id, kind: "row", label: action.label, ...note, dim: true, rests: true }
  return {
    id, kind: "row", label: action.label, ...note,
    ...(action.destructive === true ? { danger: true } : {}),
    press: action.destructive === true ? { t: "ask", ask: question(action.label, action.description === undefined ? [] : [action.description], action.label, run) } : run,
  }
}

export function choiceIndex(item: SurfaceSettingItem): number {
  if (item.interaction?.kind !== "choice") return -1
  const selected = item.interaction.selected
  return item.interaction.choices.findIndex(choice => choice.value === selected)
}

/** A choice Korri publishes with none of its values chosen. */
export const isPick = (item: SurfaceSettingItem): boolean =>
  item.interaction?.kind === "choice" && item.interaction.choices.length > 0 && choiceIndex(item) < 0

export function numberRange(range: { readonly min?: number; readonly max?: number }): string | undefined {
  if (range.min !== undefined && range.max !== undefined) return `${range.min} to ${range.max}`
  if (range.min !== undefined) return `At least ${range.min}`
  if (range.max !== undefined) return `At most ${range.max}`
  return undefined
}

export function numberFits(draft: string, range: NumberRange): boolean {
  const text = draft.trim()
  if (!/^-?\d*\.?\d+$/.test(text)) return false
  const value = Number(text)
  if (range.step === 1 && !Number.isInteger(value)) return false
  return (range.min === undefined || value >= range.min) && (range.max === undefined || value <= range.max)
}

/** One step of a number, held inside its range; undefined when it cannot move. */
export function numberStep(interaction: { readonly value?: number; readonly min?: number; readonly max?: number; readonly step?: number }, direction: -1 | 1): number | undefined {
  const step = interaction.step ?? 1
  const from = interaction.value ?? (direction > 0 ? interaction.min ?? 0 : interaction.max ?? 0)
  const next = interaction.value === undefined ? from : from + direction * step
  const held = Math.min(interaction.max ?? Infinity, Math.max(interaction.min ?? -Infinity, next))
  return held === interaction.value ? undefined : Number(held.toFixed(6))
}

const change = (settingId: string, value: string): Act => send({ _tag: "ChangeSetting", settingId, value })

/** One setting of Korri's as rows: the row, and under it Korri's problem with it and a way to put that away. */
export function settingRows(item: SurfaceSettingItem, status: SurfaceSettingsStatus, state: State, prefix = "s:"): RowSpec[] {
  const id = `${prefix}${item.id}`
  const saving = status._tag === "Saving" && status.settingId === item.id
  const notes = [item.description, item.source, item.scope === "device" ? "Stays on this device." : undefined].filter((part): part is string => part !== undefined && part !== "")
  const base = { id, kind: "row" as const, label: item.label, ...(item.value === undefined ? {} : { value: item.value }), ...(notes.length === 0 ? {} : { note: notes.join(" ") }), ...(saving ? { mark: "busy" as const } : {}) }
  const interaction = item.interaction
  let row: RowSpec
  if (interaction === undefined) row = { ...base, rests: true }
  else {
    switch (interaction.kind) {
      case "action": {
        const run = send({ _tag: "RunAction", actionId: interaction.actionId })
        const ask = interaction.confirmation !== undefined
          ? question(interaction.confirmation.title, [interaction.confirmation.message], interaction.confirmation.confirmLabel, run)
          : interaction.destructive === true ? question(item.label, item.description === undefined ? [] : [item.description], item.label, run) : undefined
        row = { ...base, ...(interaction.destructive === true ? { danger: true } : {}), press: saving ? NONE : ask === undefined ? run : { t: "ask", ask } }
        break
      }
      case "choice": {
        const count = interaction.choices.length
        if (count === 0) {
          row = { ...base, rests: true }
          break
        }
        if (isPick(item)) {
          const at = state.candidate?.rowId === id ? state.candidate.at : 0
          const candidate = interaction.choices[at]
          row = {
            ...base, value: candidate?.label ?? "", mark: saving ? "busy" : "step",
            press: saving || candidate === undefined ? NONE : seq(change(item.id, candidate.value), { t: "candidate", rowId: "", at: 0 }),
            step: direction => ({ t: "candidate", rowId: id, at: (at + direction + count) % count }),
          }
          break
        }
        const at = choiceIndex(item)
        const stepTo = (direction: -1 | 1): Act => {
          if (saving) return NONE
          const next = (at + direction + count) % count
          const choice = interaction.choices[next]
          return choice === undefined || next === at ? NONE : change(item.id, choice.value)
        }
        row = { ...base, value: interaction.choices[at]?.label ?? item.value ?? "", mark: saving ? "busy" : "step", press: stepTo(1), step: stepTo }
        break
      }
      case "number": {
        const range = numberRange(interaction)
        const entry: Entry = {
          e: "setting", settingId: item.id, label: range === undefined ? item.label : `${item.label} (${range})`, secret: false, maxLength: 24, placeholder: range ?? "Number",
          ...(interaction.clearable === true && interaction.value !== undefined ? { clearLabel: "Inherit" } : {}),
          numeric: {
            ...(interaction.min === undefined ? {} : { min: interaction.min }), ...(interaction.max === undefined ? {} : { max: interaction.max }),
            ...(interaction.step === undefined ? {} : { step: interaction.step }), clearable: interaction.clearable === true,
          },
        }
        row = {
          ...base, mark: saving ? "busy" : "step",
          press: saving ? NONE : { t: "type", entry, draft: interaction.value === undefined ? "" : String(interaction.value) },
          step: direction => {
            const next = saving ? undefined : numberStep(interaction, direction)
            return next === undefined ? NONE : change(item.id, String(next))
          },
        }
        break
      }
      case "text":
        row = {
          ...base, mark: saving ? "busy" : "more",
          press: saving ? NONE : {
            t: "type", draft: item.value ?? "",
            entry: {
              e: "setting", settingId: item.id, label: item.label, secret: false,
              ...(interaction.maxLength === undefined ? {} : { maxLength: interaction.maxLength }),
              ...(interaction.placeholder === undefined ? {} : { placeholder: interaction.placeholder }),
              ...(interaction.allowEmpty === true ? { allowEmpty: true } : {}),
            },
          },
        }
        break
      case "sensitiveText":
        row = {
          ...base, mark: saving ? "busy" : "lock",
          // A secret's value is Korri's word about it ("Configured"), never the secret: the draft starts empty.
          press: saving ? NONE : {
            t: "type", draft: "",
            entry: {
              e: "setting", settingId: item.id, label: item.label, secret: true,
              ...(interaction.maxLength === undefined ? {} : { maxLength: interaction.maxLength }),
              ...(interaction.clearLabel === undefined ? {} : { clearLabel: interaction.clearLabel }),
              ...(interaction.placeholder === undefined ? {} : { placeholder: interaction.placeholder }),
            },
          },
        }
        break
    }
  }
  if (status._tag === "Problem" && status.settingId === item.id) {
    return [row, { id: `${id}:problem`, kind: "problem", label: status.message }, { id: `${id}:dismiss`, kind: "row", label: "Dismiss", press: send({ _tag: "DismissSettingsProblem" }) }]
  }
  return [row]
}

// ---- Wi-Fi ----

export function connectionWords(connection: SurfaceNetworkConnection): string {
  switch (connection._tag) {
    case "Offline":
      return "Offline"
    case "Wired":
      return "Wired"
    case "Wifi":
      return connection.ssid
  }
}

export const barsOf = (strengthPercent: number | undefined): number =>
  strengthPercent === undefined ? -1 : strengthPercent >= 75 ? 4 : strengthPercent >= 50 ? 3 : strengthPercent >= 25 ? 2 : strengthPercent > 0 ? 1 : 0

export function scannedNetworks(model: Pick<Model, "network">): readonly SurfaceWifiNetwork[] {
  const wifi = model.network?.wifi
  return wifi?._tag === "Available" ? wifi.networks : []
}

export function lastAttempt(model: Pick<Model, "network">): SurfaceWifiAttempt {
  const wifi = model.network?.wifi
  return wifi?._tag === "Available" ? wifi.attempt : { _tag: "Idle" }
}

export function sameAttempt(a: SurfaceWifiAttempt, b: SurfaceWifiAttempt): boolean {
  switch (a._tag) {
    case "Idle":
      return b._tag === "Idle"
    case "Connecting":
      return b._tag === "Connecting" && a.ssid === b.ssid
    case "Failed":
      return b._tag === "Failed" && a.ssid === b.ssid && a.message === b.message && a.passwordRejected === b.passwordRejected
  }
}

/** A fresh failure with the key rejected: the keyboard to type it again, empty. */
export function rejectedEntry(previous: SurfaceWifiAttempt, model: Pick<Model, "network">): Entry | undefined {
  const attempt = lastAttempt(model)
  if (attempt._tag !== "Failed" || !attempt.passwordRejected || sameAttempt(attempt, previous)) return undefined
  const network = scannedNetworks(model).find(candidate => candidate.ssid === attempt.ssid)
  return network === undefined ? { e: "hiddenPassword", ssid: attempt.ssid } : { e: "wifiPassword", networkId: network.id, ssid: network.ssid }
}

/**
 * The Wi-Fi rows: Scan, the networks strongest first, and a hidden network.
 * A known or open network joins at once; a secured one Korri holds no key
 * for asks its password first. The network in use, or any network while
 * Korri is joining one, asks nothing.
 */
export function wifiRows(model: Pick<Model, "network">): RowSpec[] {
  const network = model.network
  if (network === undefined) return []
  const wifi = network.wifi
  if (wifi._tag === "NoAdapter") return [{ id: "w:none", kind: "note", label: "This device has no Wi-Fi adapter." }]
  const attempt = wifi.attempt
  const joining = attempt._tag === "Connecting"
  const on = network.connection._tag === "Wifi" ? network.connection.ssid : undefined
  const networks = [...wifi.networks].sort((a, b) => b.strengthPercent - a.strengthPercent)
  const rows: RowSpec[] = [{
    id: "w:scan", kind: "row", label: "Scan", value: wifi.scanning ? "Scanning…" : networks.length === 1 ? "1 network" : `${networks.length} networks`,
    ...(wifi.scanning ? { mark: "busy" as const } : {}), press: wifi.scanning ? NONE : send({ _tag: "ScanWifi" }),
  }]
  for (const item of networks) {
    const id = `w:net:${item.id}`
    const value = item.ssid === on ? "Connected" : joining && attempt.ssid === item.ssid ? "Connecting…" : item.known ? "Saved" : item.secured ? "Needs a password" : "Open"
    const press: Act = item.ssid === on || joining ? NONE
      : item.secured && !item.known ? { t: "type", entry: { e: "wifiPassword", networkId: item.id, ssid: item.ssid } }
        : send({ _tag: "ConnectWifi", networkId: item.id })
    rows.push({
      id, kind: "row", label: item.ssid, value, bars: barsOf(item.strengthPercent),
      mark: item.ssid === on ? "check" : joining && attempt.ssid === item.ssid ? "busy" : item.secured && !item.known ? "lock" : "",
      press,
    })
    if (attempt._tag === "Failed" && attempt.ssid === item.ssid) rows.push({ id: `${id}:problem`, kind: "problem", label: attempt.message })
  }
  const listed = (ssid: string) => networks.some(item => item.ssid === ssid)
  rows.push({
    id: "w:hidden", kind: "row", label: "Hidden network", value: joining && !listed(attempt.ssid) ? "Connecting…" : "Join by name",
    mark: joining && !listed(attempt.ssid) ? "busy" : "more", press: joining ? NONE : { t: "type", entry: { e: "hiddenSsid" } },
  })
  if (attempt._tag === "Failed" && !listed(attempt.ssid)) rows.push({ id: "w:hidden:problem", kind: "problem", label: attempt.message })
  return rows
}

/** The connection as a sentence for a page's sub line. */
export function connectionLine(connection: SurfaceNetworkConnection): string {
  switch (connection._tag) {
    case "Offline":
      return "This device is offline."
    case "Wired":
      return "Connected by wire."
    case "Wifi":
      return connection.strengthPercent === undefined ? `Connected to ${connection.ssid}.` : `Connected to ${connection.ssid}, ${connection.strengthPercent}% signal.`
  }
}
