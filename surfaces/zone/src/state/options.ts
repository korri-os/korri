/**
 * The OPTIONS page as one value, and the one function that changes it.
 *
 *     focused    the row the cursor is on: a row id from src/plan/options.ts,
 *                or ZONE_OPTIONS_SYSTEM for "the SYSTEM group's first row"
 *     at         that row's index, so the cursor stays put when Korri
 *                removes the row under it
 *     keyboard   the keyboard over the page, with what it is typing
 *     dialog     the NO / YES dialog over the page, before an action fires
 *     attempt    the last Wi-Fi attempt Korri published, so a fresh rejected
 *                key can reopen the keyboard once and only once
 *     pick       the candidate on a choice Korri publishes with nothing
 *                chosen (Add setting): left and right move it, A sends it
 *
 * Korri's SurfaceModel is not stored here. Korri owns it and replaces it
 * whole; `update` receives it beside this value and reads the rows from
 * `optionsPlan`, exactly as the page draws them.
 *
 * Direction and confirmation never arrive as messages: the host moves DOM
 * focus and clicks, and the page reports `Focused…` and `Pressed…`. Back and
 * menu arrive from the host's buttons through the wire stage.
 */
import type { SurfaceModel, SurfaceWifiAttempt } from "@contracts/surface/korri-surface"
import {
  choiceIndex,
  entryHeading,
  isPick,
  zoneNumberFits,
  zoneNumberStep,
  focusedRow,
  optionsPlan,
  ZONE_OPTIONS_SYSTEM,
  zoneNetworkRowId,
  ZONE_WIFI_HIDDEN,
  type ZoneOptionsDialog,
  type ZoneOptionsEntry,
  type ZoneOptionsKey,
  type ZoneOptionsKeyboard,
  type ZoneOptionsPlan,
  type ZoneOptionsSlices,
} from "../plan/options"
import { ask, stay, type ZoneRequest, type ZoneStep } from "./requests"
import { joinHidden, lastAttempt, pickNetwork, rejectedEntry, sameAttempt, scan, submitWifi, type WifiNext } from "./wifi"

export interface ZoneOptions {
  readonly focused: string | undefined
  readonly at: number
  readonly keyboard: ZoneOptionsKeyboard | undefined
  readonly dialog: ZoneOptionsDialog | undefined
  readonly attempt: SurfaceWifiAttempt
  readonly pick: { readonly rowId: string; readonly at: number } | undefined
}

export type ZoneOptionsMessage =
  /** Korri published a new model: the cursor and the layers are settled against it. */
  | { readonly _tag: "KorriPublished" }
  /** The host's B. Closes the keyboard or the dialog without acting. */
  | { readonly _tag: "PressedBack" }
  /** The host's menu button. Commits the keyboard's draft, as DONE does. */
  | { readonly _tag: "PressedMenu" }
  | { readonly _tag: "FocusedRow"; readonly rowId: string }
  /** A on a row. */
  | { readonly _tag: "PressedRow"; readonly rowId: string }
  /** Left or right on a choice row. */
  | { readonly _tag: "SteppedRow"; readonly rowId: string; readonly direction: -1 | 1 }
  | { readonly _tag: "FocusedKey"; readonly keyId: string }
  | { readonly _tag: "PressedKey"; readonly key: ZoneOptionsKey }
  /** The clear key beside a secret's keyboard: Korri's clearLabel. */
  | { readonly _tag: "PressedClear" }
  | { readonly _tag: "FocusedDialogButton"; readonly index: 0 | 1 }
  | { readonly _tag: "PressedDialogButton"; readonly index: 0 | 1 }

export const OPTIONS_START: ZoneOptions = {
  focused: undefined,
  at: 0,
  keyboard: undefined,
  dialog: undefined,
  attempt: { _tag: "Idle" },
  pick: undefined,
}

/** The start, with Korri's current attempt already seen: a failure from
 * before the page opened does not raise the keyboard. */
export const optionsStart = (korri: ZoneOptionsSlices): ZoneOptions => ({ ...OPTIONS_START, attempt: lastAttempt(korri) })

/** What is over the page, for the wire stage's back and layer bookkeeping. */
export function optionsLayer(state: ZoneOptions): "keyboard" | "dialog" | "none" {
  return state.keyboard !== undefined ? "keyboard" : state.dialog !== undefined ? "dialog" : "none"
}

/**
 * The `system` button lands on the SYSTEM group's first row. Pure, and
 * needs no model: the group's key is resolved to its first row when the page
 * draws or the next message arrives. A keyboard or dialog stays open and the
 * cursor stays where it is: `system` acts on the page, never through a layer.
 */
export function focusSystemGroup(state: ZoneOptions): ZoneOptions {
  return optionsLayer(state) === "none" ? { ...state, focused: ZONE_OPTIONS_SYSTEM } : state
}

export function update(state: ZoneOptions, message: ZoneOptionsMessage, korri: ZoneOptionsSlices): ZoneStep<ZoneOptions> {
  const plan = optionsPlan(korri)
  const result = step(settleFocus(state, plan), message, korri, plan)
  return result.model === state && result.requests.length === 0 ? stay(state) : result
}

function step(state: ZoneOptions, message: ZoneOptionsMessage, korri: ZoneOptionsSlices, plan: ZoneOptionsPlan): ZoneStep<ZoneOptions> {
  switch (message._tag) {
    case "KorriPublished":
      return stay(published(state, korri, plan))
    case "PressedBack":
      if (state.keyboard !== undefined) return stay({ ...state, keyboard: undefined })
      if (state.dialog !== undefined) return stay({ ...state, dialog: undefined })
      return stay(state)
    case "PressedMenu":
      return state.keyboard === undefined ? stay(state) : commit(state, state.keyboard)
    case "FocusedRow": {
      const at = plan.rows.findIndex(row => row.id === message.rowId)
      return at < 0 || state.keyboard !== undefined || state.dialog !== undefined
        ? stay(state) : stay({ ...state, focused: message.rowId, at })
    }
    case "PressedRow":
      return state.keyboard !== undefined || state.dialog !== undefined ? stay(state) : press(state, message.rowId, korri, plan)
    case "SteppedRow":
      return state.keyboard !== undefined || state.dialog !== undefined
        ? stay(state) : stepChoice(state, message.rowId, message.direction, korri, plan)
    case "FocusedKey":
      return state.keyboard === undefined ? stay(state)
        : stay({ ...state, keyboard: { ...state.keyboard, focusedKey: message.keyId } })
    case "PressedKey":
      return state.keyboard === undefined ? stay(state) : key(state, state.keyboard, message.key)
    case "PressedClear": {
      const entry = state.keyboard?.entry
      // A secret Korri can clear, or a number Korri lets clear (its INHERIT).
      return entry?._tag === "Setting" && (entry.secret || entry.numeric?.clearable === true) && entry.clearLabel !== undefined
        ? ask({ ...state, keyboard: undefined }, { _tag: "ChangeSetting", settingId: entry.settingId, value: "" })
        : stay(state)
    }
    case "FocusedDialogButton":
      return state.dialog === undefined ? stay(state) : stay({ ...state, dialog: { ...state.dialog, focused: message.index } })
    case "PressedDialogButton": {
      const dialog = state.dialog
      if (dialog === undefined) return stay(state)
      return message.index === 1
        ? ask({ ...state, dialog: undefined }, { _tag: "RunAction", actionId: dialog.actionId })
        : stay({ ...state, dialog: undefined })
    }
  }
}

/** A group key stands for its first row; a row Korri removed gives way to
 * the row now at its place. */
function settleFocus(state: ZoneOptions, plan: ZoneOptionsPlan): ZoneOptions {
  if (state.focused === undefined) return state
  const row = focusedRow(plan, state.focused)
  if (row !== undefined) {
    const at = plan.rows.indexOf(row)
    return row.id === state.focused && at === state.at ? state : { ...state, focused: row.id, at }
  }
  const fallback = plan.rows[Math.min(state.at, plan.rows.length - 1)]
  return fallback === undefined
    ? { ...state, focused: undefined, at: 0 }
    : { ...state, focused: fallback.id, at: Math.min(state.at, plan.rows.length - 1) }
}

function published(state: ZoneOptions, korri: ZoneOptionsSlices, plan: ZoneOptionsPlan): ZoneOptions {
  const attempt = lastAttempt(korri)
  const reopen = state.keyboard === undefined ? rejectedEntry(state.attempt, korri) : undefined
  let next: ZoneOptions = sameAttempt(state.attempt, attempt) ? state : { ...state, attempt }
  if (reopen !== undefined) {
    const rowId = reopen._tag === "WifiPassword" ? zoneNetworkRowId(reopen.networkId) : ZONE_WIFI_HIDDEN
    const at = plan.rows.findIndex(row => row.id === rowId)
    next = { ...next, keyboard: openKeyboard(reopen, ""), dialog: undefined, ...(at < 0 ? {} : { focused: rowId, at }) }
  }
  // A setting's keyboard closes when Korri stops offering the text it edits.
  const entry = next.keyboard?.entry
  if (entry?._tag === "Setting") {
    const kind = plan.rows.find(row => row.id === entry.settingId)?.item.interaction?.kind
    if (kind !== "text" && kind !== "sensitiveText" && kind !== "number") next = { ...next, keyboard: undefined }
  }
  // A dialog closes when the action it asks about is gone.
  const dialog = next.dialog
  if (dialog !== undefined && !plan.rows.some(row => row.press._tag === "Run" && row.press.actionId === dialog.actionId)) {
    next = { ...next, dialog: undefined }
  }
  return next
}

const openKeyboard = (entry: ZoneOptionsEntry, draft: string): ZoneOptionsKeyboard =>
  ({ entry, draft, caret: draft.length, layer: "letters" })

function wifi(state: ZoneOptions, next: WifiNext): ZoneStep<ZoneOptions> {
  switch (next._tag) {
    case "Nothing":
      return stay(state)
    case "Send":
      return ask({ ...state, keyboard: undefined }, next.request)
    case "Type":
      return stay({ ...state, keyboard: openKeyboard(next.entry, "") })
  }
}

function press(state: ZoneOptions, rowId: string, korri: ZoneOptionsSlices, plan: ZoneOptionsPlan): ZoneStep<ZoneOptions> {
  const at = plan.rows.findIndex(row => row.id === rowId)
  const row = plan.rows[at]
  if (row === undefined) return stay(state)
  const here: ZoneOptions = { ...state, focused: row.id, at }
  const action = row.press
  switch (action._tag) {
    case "Nothing":
      return stay(here)
    case "Step": {
      // A pick sends its candidate; any other choice steps forward.
      if (!isPick(row.item)) return stepChoice(here, rowId, 1, korri, plan)
      const interaction = row.item.interaction
      const at = state.pick?.rowId === rowId ? state.pick.at : 0
      const choice = interaction?.kind === "choice" ? interaction.choices[at] : undefined
      if (choice === undefined || saving(korri, row.id)) return stay(here)
      return ask({ ...here, pick: undefined }, { _tag: "ChangeSetting", settingId: row.id, value: choice.value })
    }
    case "Edit": {
      if (saving(korri, row.id)) return stay(here)
      const entry: ZoneOptionsEntry = {
        _tag: "Setting",
        settingId: action.settingId,
        label: action.label,
        secret: action.secret,
        ...(action.maxLength === undefined ? {} : { maxLength: action.maxLength }),
        ...(action.clearLabel === undefined ? {} : { clearLabel: action.clearLabel }),
        ...(action.placeholder === undefined ? {} : { placeholder: action.placeholder }),
        ...(action.numeric === undefined ? {} : { numeric: action.numeric }),
      }
      return stay({ ...here, keyboard: openKeyboard(entry, action.draft) })
    }
    case "Run":
      if (saving(korri, row.id)) return stay(here)
      return action.confirm === undefined
        ? ask(here, { _tag: "RunAction", actionId: action.actionId })
        : stay({ ...here, dialog: { actionId: action.actionId, confirm: action.confirm, focused: 0 } })
    case "DismissProblem":
      return ask(here, { _tag: "DismissSettingsProblem" })
    case "Scan":
      return wifi(here, scan(korri))
    case "Join":
      return wifi(here, pickNetwork(korri, action.networkId))
    case "JoinHidden":
      return wifi(here, joinHidden())
    // Zone's own look: the surface steps it (state/look.ts); nothing goes to Korri.
    case "Look":
      return stay(here)
  }
}

const saving = (korri: ZoneOptionsSlices, settingId: string) =>
  korri.settingsStatus._tag === "Saving" && korri.settingsStatus.settingId === settingId

/** Left or right on a choice: the next choice in that direction, round the
 * ends, sent at once. A value Korri's choices do not name starts from the
 * first (right) or the last (left). */
function stepChoice(
  state: ZoneOptions,
  rowId: string,
  direction: -1 | 1,
  korri: ZoneOptionsSlices,
  plan: ZoneOptionsPlan,
): ZoneStep<ZoneOptions> {
  const row = plan.rows.find(candidate => candidate.id === rowId)
  const interaction = row?.item.interaction
  if (row !== undefined && interaction?.kind === "number") {
    if (saving(korri, row.id)) return stay(state)
    const next = zoneNumberStep(interaction, direction)
    return next === undefined ? stay(state) : ask(state, { _tag: "ChangeSetting", settingId: row.id, value: String(next) })
  }
  if (row === undefined || interaction?.kind !== "choice" || interaction.choices.length === 0) return stay(state)
  // A pick moves its candidate on Zone's side; nothing goes to Korri.
  if (isPick(row.item)) {
    const count = interaction.choices.length
    const from = state.pick?.rowId === rowId ? state.pick.at : 0
    return stay({ ...state, pick: { rowId, at: (from + direction + count) % count } })
  }
  // LOOK is Zone's own choice, stepped by the surface, never sent to Korri.
  if (row.press._tag === "Look") return stay(state)
  if (saving(korri, row.id)) return stay(state)
  const count = interaction.choices.length
  const at = choiceIndex(row.item)
  const next = at < 0 ? (direction > 0 ? 0 : count - 1) : (at + direction + count) % count
  const choice = interaction.choices[next]
  if (choice === undefined || next === at) return stay(state)
  return ask(state, { _tag: "ChangeSetting", settingId: row.id, value: choice.value })
}

function key(state: ZoneOptions, keyboard: ZoneOptionsKeyboard, pressed: ZoneOptionsKey): ZoneStep<ZoneOptions> {
  const { draft, caret } = keyboard
  const limit = entryHeading(keyboard.entry).maxLength
  switch (pressed.kind) {
    case "char":
    case "space": {
      if (limit !== undefined && draft.length >= limit) return stay(state)
      const char = pressed.kind === "space" ? " " : pressed.char
      return stay({
        ...state,
        keyboard: { ...keyboard, draft: draft.slice(0, caret) + char + draft.slice(caret), caret: caret + 1 },
      })
    }
    case "delete":
      return caret === 0 ? stay(state)
        : stay({ ...state, keyboard: { ...keyboard, draft: draft.slice(0, caret - 1) + draft.slice(caret), caret: caret - 1 } })
    case "layer":
      return keyboard.layer === pressed.layer ? stay(state) : stay({ ...state, keyboard: { ...keyboard, layer: pressed.layer } })
    case "done":
      return commit(state, keyboard)
  }
}

/** DONE or menu: what the draft is for decides what goes. */
function commit(state: ZoneOptions, keyboard: ZoneOptionsKeyboard): ZoneStep<ZoneOptions> {
  const entry = keyboard.entry
  if (entry._tag !== "Setting") return wifi(state, submitWifi(entry, keyboard.draft))
  // An empty secret says nothing: clearing one is its own key.
  if (entry.secret && keyboard.draft === "") return stay({ ...state, keyboard: undefined })
  // A number: empty clears it when Korri lets it; anything else must fit the range.
  if (entry.numeric !== undefined) {
    if (keyboard.draft.trim() === "") {
      return entry.numeric.clearable
        ? ask({ ...state, keyboard: undefined }, { _tag: "ChangeSetting", settingId: entry.settingId, value: "" })
        : stay({ ...state, keyboard: undefined })
    }
    if (!zoneNumberFits(keyboard.draft, entry.numeric)) return stay(state)
  }
  const request: ZoneRequest = { _tag: "ChangeSetting", settingId: entry.settingId, value: keyboard.draft }
  return ask({ ...state, keyboard: undefined }, request)
}
