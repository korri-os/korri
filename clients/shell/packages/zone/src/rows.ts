/**
 * The list pages as one shape: OPTIONS, LAUNCH SETTINGS, RUN WITH, GET
 * GAMES, GAME OPTS. and first boot's lists are all Zone's sky band of rows
 * (ZoneOptionGroup, ZoneOptionRow, ZoneActionRow, ZoneWarningLines). Each
 * builder here reads Zone's own plan (src/plan/) and model (src/state/) and
 * returns the rows main.slint draws, top to bottom, in Zone's words.
 *
 * Every row carries every field: the shell keeps a field a view leaves out
 * (src/view.rs), so a row that once was red would stay red.
 */
import type { SurfaceAction, SurfaceModel, SurfaceSettingItem } from "../../../../../contracts/generated/surface"
import { launchCursor, launchPlan, launchRowFit, wasWords, type ZoneLaunchOpen } from "./plan/launch-settings"
import { focusedRow, isPick, optionsPlan, type ZoneOptionsPlan, type ZoneOptionsRow, type ZoneOptionsSlices } from "./plan/options"
import { runWithPlan, zoneSavedRowId, type ZoneRunWithOpen } from "./plan/options-run-with"
import {
  zoneStoreActionRow,
  zoneStoreEntryRow,
  zoneStorePlayRow,
  zoneStoreProviderRow,
  zoneStoreProviderTitle,
  zoneStoreSearch,
  ZONE_STORE_SEARCH_ROW,
} from "./plan/store"
import type { ZoneScreen } from "./plan/screen"
import { optionsLayer, type ZoneOptions } from "./state/options"
import type { ZoneSurfaceState } from "./state/surface"

export interface Btn {
  readonly label: string
  readonly focused: boolean
  readonly lit: boolean
  readonly disabled: boolean
  readonly danger: boolean
  /** Its line in its row of buttons (src/focus.ts sets it from the plan). */
  readonly line: number
  /** Its focus target's id, "" when it is none: what a tap on it reports (ADR 0055). */
  readonly focusId: string
}

export interface Tag {
  readonly label: string
  /** system, here, remote, continue, download, open, saved */
  readonly kind: string
  /** Korri's name: its own case, in the look's face for Korri's words. */
  readonly korri: boolean
}

export interface Row {
  /** title, caption, item, note, problem, text, buttons, tags, word */
  readonly kind: string
  readonly label: string
  /** The label is a fact Korri publishes: its own case. */
  readonly labelFact: boolean
  readonly value: string
  readonly valueFact: boolean
  readonly focused: boolean
  readonly disabled: boolean
  /** The value is a destructive verb: red caps. */
  readonly danger: boolean
  /** A choice: the red triangles flank the value while focused. */
  readonly choice: boolean
  /** Korri is saving this row: the red caret blinks at the value's end. */
  readonly saving: boolean
  /** The value is changed and not kept yet (LAUNCH SETTINGS): it is drawn in yellow. */
  readonly changed: boolean
  /** The earlier value's words beside the value ("was 4x"); "" when none stand there. */
  readonly was: string
  /** Wi-Fi bars lit, 0 to 4; -1 for none. */
  readonly bars: number
  readonly buttons: readonly Btn[]
  readonly tags: readonly Tag[]
  /** A row of buttons holds the cursor on one of its buttons: the list keeps it in view. */
  readonly holds: boolean
  /** Its focus target's id, "" when it is none: what a tap on it reports (ADR 0055).
   * Every row sends it, so a row that stops being a target never keeps an old one. */
  readonly focusId: string
}

const NONE: Row = {
  kind: "item",
  label: "",
  labelFact: false,
  value: "",
  valueFact: false,
  focused: false,
  disabled: false,
  danger: false,
  choice: false,
  saving: false,
  changed: false,
  was: "",
  bars: -1,
  buttons: [],
  tags: [],
  holds: false,
  focusId: "",
}

export const row = (fields: Partial<Row>): Row => ({ ...NONE, ...fields })
export const btn = (label: string, fields: Partial<Btn> = {}): Btn =>
  ({ label, focused: false, lit: false, disabled: false, danger: false, line: 0, focusId: "", ...fields })
export const tag = (label: string, kind: string, korri = false): Tag => ({ label, kind, korri })

/** ZoneBars' rule: one bar a quarter, never none while connected. */
export const barsOf = (strength: number | undefined, connected = true): number =>
  !connected ? 0 : strength === undefined ? 1 : Math.max(1, Math.min(4, Math.round(strength / 25)))

/** A setting's value as the row shows it (a pick shows its candidate). */
function valueOf(entry: ZoneOptionsRow, options: ZoneOptions): string {
  const item: SurfaceSettingItem = entry.item
  const interaction = item.interaction
  if (interaction?.kind === "choice" && isPick(item)) {
    const at = options.pick?.rowId === entry.id ? options.pick.at : 0
    return interaction.choices[at]?.label ?? ""
  }
  if (interaction?.kind === "choice") {
    return interaction.choices.find(choice => choice.value === interaction.selected)?.label ?? item.value ?? ""
  }
  if (interaction?.kind === "number") return interaction.value === undefined ? (item.value ?? "") : String(interaction.value)
  return item.value ?? ""
}

/**
 * OPTIONS' plan as rows: a group's title, its rows, the focused row's
 * description (always a disabled row's), Korri's problem under its row.
 */
export function optionRows(plan: ZoneOptionsPlan, options: ZoneOptions): Row[] {
  const rows: Row[] = []
  // A group key (SYSTEM, after the system button) stands for its first row.
  const lit = focusedRow(plan, options.focused)?.id
  for (const group of plan.groups) {
    rows.push(row({ kind: "title", label: group.title }))
    for (const entry of group.rows) {
      const focused = lit === entry.id && options.keyboard === undefined && options.dialog === undefined
      const interaction = entry.item.interaction
      const saving = group.status._tag === "Saving" && group.status.settingId === entry.id
      const signal = entry.signal
      const bars = signal === undefined ? -1
        : signal._tag === "Network" ? barsOf(signal.strengthPercent)
        : signal.connection._tag === "Wifi" ? barsOf(signal.connection.strengthPercent)
        : signal.connection._tag === "Wired" ? 4 : 0
      rows.push(row({
        kind: "item",
        label: entry.item.label,
        labelFact: entry.keepCase?.label === true || group.verbatim,
        value: valueOf(entry, options),
        // The connection's SSID is Korri's name: its own case (ZoneSignal draws it as a fact).
        valueFact: entry.keepCase?.value === true || (entry.signal?._tag === "Connection" && entry.signal.connection._tag === "Wifi"),
        focused,
        focusId: `row:${entry.id}`,
        disabled: entry.disabled === true,
        danger: interaction?.kind === "action" && interaction.destructive === true,
        choice: interaction?.kind === "choice" || interaction?.kind === "number",
        saving,
        bars,
      }))
      const source = entry.item.source === undefined ? "" : entry.item.source
      const scope = entry.item.scope === "device" ? "Device only" : ""
      const note = entry.item.description ?? ""
      if ((focused || entry.disabled === true) && (note !== "" || source !== "" || scope !== "")) {
        rows.push(row({ kind: "note", label: [note, source, scope].filter(word => word !== "").join(" · "), labelFact: entry.ownDescription !== true }))
      }
      if (group.status._tag === "Problem" && group.status.settingId === entry.id) {
        rows.push(row({ kind: "problem", label: group.status.message, labelFact: true }))
      }
    }
  }
  return rows
}

export function optionsRows(state: ZoneSurfaceState, slices: ZoneOptionsSlices): Row[] {
  return optionRows(optionsPlan(slices), state.options)
}

/** Korri's actions as one row of buttons; `focused` names the action the cursor is on. */
export function actionButtons(actions: readonly SurfaceAction[], focused: string | undefined, lit?: string): Row {
  return row({
    kind: "buttons",
    // The row holds the cursor when one of its buttons does: the list keeps it in view.
    holds: actions.some(action => action.id === focused),
    buttons: actions.map(action => btn(action.label, {
      focusId: `button:${action.id}`,
      focused: action.id === focused,
      lit: action.id === lit,
      disabled: !action.enabled,
      danger: action.destructive === true,
    })),
  })
}

/** A sentence as a note says it: closed by a full stop unless it ends one already. */
const sentence = (words: string): string => (/[.!?…]$/.test(words) ? words : `${words}.`)

/** The first letter as a capital. */
const capital = (words: string): string => words.charAt(0).toUpperCase() + words.slice(1)

export interface LaunchShape {
  /** The list, top to bottom: Korri's notes and message, the groups' rows, Korri's actions. */
  readonly rows: Row[]
  /** The keep row, under the list and always in view; `shown` only while a row is changed. */
  readonly keep: { readonly shown: boolean; readonly row: Row; readonly note: string }
}

/**
 * LAUNCH SETTINGS (ADR 0066): one list, each setting once under its
 * section, then the keep row. A changed row's value is yellow, with its
 * earlier value beside it when the plan finds room and in its note when
 * not. A row Korri gives no way to change is dimmed. Only the focused row
 * shows its note: the words the row had to cut, the earlier value, Korri's
 * description (why a row cannot change), and where the value stays.
 */
export function launchRows(state: ZoneSurfaceState, korri: SurfaceModel, launch: ZoneLaunchOpen, screen: ZoneScreen): LaunchShape {
  const plan = launchPlan(launch, korri.settingsStatus)
  const options = state.launch
  const layered = optionsLayer(options) !== "none"
  const cursor = launchCursor(plan, options.focused, state.launchAction)
  const lit = !layered && cursor.zone !== "none" && cursor.zone !== "actions" ? cursor.rowId : undefined
  const rows: Row[] = []
  for (const note of launch.notes) rows.push(row({ kind: "text", label: note, labelFact: true }))
  if (launch._tag === "Conflict" || launch._tag === "Error") rows.push(row({ kind: "problem", label: launch.message, labelFact: true }))
  const buttons = () => {
    if (plan.actions.length === 0) return
    // The button the row lights: the one the cursor is on, else the one it would enter on while no row holds it.
    const held = cursor.zone === "actions" ? cursor.actionId : undefined
    rows.push(actionButtons(plan.actions, held))
  }
  if (plan.actionsFirst) buttons()
  if (launch._tag === "Loading") rows.push(row({ kind: "word", label: "Loading" }))
  // Ready with no row is no fault: Korri has nothing to set for this game.
  if (launch._tag === "Ready" && plan.list.length === 0) rows.push(row({ kind: "word", label: "No settings" }))
  for (const group of plan.groups) {
    rows.push(row({ kind: "caption", label: group.title }))
    for (const entry of group.rows) {
      const item = entry.item
      const interaction = item.interaction
      const focused = lit === entry.id
      const labelFact = entry.keepCase?.label === true
      const value = valueOf(entry, options)
      const was = plan.was.get(entry.id)
      const fixed = interaction === undefined
      const choice = interaction?.kind === "choice" || interaction?.kind === "number"
      const fit = launchRowFit(item.label, labelFact, value, was, choice, screen)
      rows.push(row({
        kind: "item",
        label: item.label,
        labelFact,
        value,
        valueFact: true,
        focused,
        focusId: `row:${entry.id}`,
        disabled: fixed,
        danger: interaction?.kind === "action" && interaction.destructive === true,
        choice,
        saving: group.status._tag === "Saving" && group.status.settingId === entry.id,
        changed: was !== undefined,
        was: was !== undefined && fit.wasBeside ? wasWords(was) : "",
      }))
      if (focused) {
        const parts = [
          ...(fit.labelCut ? [item.label] : []),
          ...(fit.valueCut ? [value] : []),
          ...(was !== undefined && !fit.wasBeside ? [capital(wasWords(was))] : []),
          ...(item.description === undefined || item.description === "" ? [] : [item.description]),
          ...(item.source === undefined || item.source === "" ? [] : [item.source]),
        ]
        if (parts.length > 0) rows.push(row({ kind: "note", label: parts.map(sentence).join(" "), labelFact: true }))
      }
      if (group.status._tag === "Problem" && group.status.settingId === entry.id) {
        rows.push(row({ kind: "problem", label: group.status.message, labelFact: true }))
      }
    }
  }
  if (!plan.actionsFirst) buttons()
  const keep = plan.keep
  if (keep === undefined) return { rows, keep: { shown: false, row: row({ kind: "item" }), note: "" } }
  const focused = lit === keep.id
  const value = valueOf(keep, options)
  const fit = launchRowFit(keep.item.label, false, value, undefined, keep.item.interaction?.kind === "choice", screen)
  const parts = [
    ...(fit.labelCut ? [keep.item.label] : []),
    ...(fit.valueCut ? [value] : []),
    ...(keep.item.description === undefined || keep.item.description === "" ? [] : [keep.item.description]),
    ...(keep.item.source === undefined || keep.item.source === "" ? [] : [keep.item.source]),
  ]
  return {
    rows,
    keep: {
      shown: true,
      row: row({
        kind: "item",
        label: keep.item.label,
        value,
        valueFact: true,
        focused,
        focusId: `row:${keep.id}`,
        choice: keep.item.interaction?.kind === "choice",
        saving: korri.settingsStatus._tag === "Saving" && korri.settingsStatus.settingId === keep.id,
        holds: focused,
      }),
      note: focused ? parts.map(sentence).join(" ") : "",
    },
  }
}

/** RUN WITH: warnings, Korri's message, a row per route with its buttons, the choice's own buttons. */
export function runWithRows(state: ZoneSurfaceState, choice: ZoneRunWithOpen): Row[] {
  const plan = runWithPlan(choice)
  const focused = state.runWith.focused ?? plan.ids[0]
  const rows: Row[] = []
  for (const warning of choice.warnings) rows.push(row({ kind: "problem", label: warning, labelFact: true }))
  if (choice.message !== "") rows.push(row({ kind: "text", label: choice.message, labelFact: true }))
  for (const orphan of plan.orphans) {
    rows.push(row({ kind: "route", tags: [tag("Saved", "here"), tag(orphan.label, "system", true)], label: orphan.runnerId, labelFact: true, value: "Unavailable", focused: focused === zoneSavedRowId(orphan.label), focusId: `row:${zoneSavedRowId(orphan.label)}` }))
  }
  for (const entry of plan.routes) {
    const saved = entry.saved.length === 0 ? [] : [tag("Saved", "here"), ...entry.saved.map(label => tag(label, "system", true))]
    rows.push(row({ kind: "route", tags: saved, label: entry.route.program, labelFact: true, value: entry.route.runnerBuild, valueFact: true, focused: focused === entry.id, focusId: `row:${entry.id}` }))
    if (focused === entry.id) rows.push(row({ kind: "note", label: entry.route.runnerBuild, labelFact: true }))
    for (const warning of entry.route.warnings) rows.push(row({ kind: "problem", label: warning, labelFact: true }))
    if (entry.route.actions.length > 0) rows.push(actionButtons(entry.route.actions, focused))
  }
  if (choice.actions.length > 0) rows.push(actionButtons(choice.actions, focused))
  return rows
}

/** GET GAMES as rows, the ids the cursor walks (with the drawn row each stands in), and Korri's dialog. */
export interface StoreShape {
  readonly rows: Row[]
  readonly ids: readonly { readonly id: string; readonly row: number }[]
  /** Korri's question or trouble: its action ids in order, or none. */
  readonly dialog: readonly string[]
  readonly dialogHeadline: string
  readonly dialogKorri: boolean
  readonly dialogLine: string
  readonly dialogActions: readonly SurfaceAction[]
  readonly title: string
  readonly tags: readonly Tag[]
  readonly well: boolean
}

export function storeRows(state: ZoneSurfaceState, korri: SurfaceModel): StoreShape {
  const store = korri.store
  const model = state.store
  const rows: Row[] = []
  const ids: { id: string; row: number }[] = []
  let dialog: { headline: string; korri: boolean; line: string; actions: readonly SurfaceAction[] } | undefined
  let title = ""
  const tags: Tag[] = []
  let well = false
  const focusRow = (id: string) => model.keyboard === undefined && (model.focused === id || (model.focused === undefined && first === id))
  let first: string | undefined
  if (store !== undefined) {
    switch (model.page) {
      case "providers": {
        const providers = store.providers
        if (providers._tag === "Loading") rows.push(row({ kind: "word", label: "Loading" }))
        if (providers._tag === "Problem") dialog = { headline: "Providers", korri: false, line: providers.message, actions: providers.actions }
        if (providers._tag === "Ready") {
          first = providers.providers[0] === undefined ? undefined : zoneStoreProviderRow(providers.providers[0].id)
          if (providers.providers.length === 0) rows.push(row({ kind: "word", label: "No providers" }))
          else {
            rows.push(row({ kind: "title", label: "Providers" }))
            for (const provider of providers.providers) {
              const id = zoneStoreProviderRow(provider.id)
              ids.push({ id, row: rows.length })
              rows.push(row({ kind: "item", label: provider.title, labelFact: true, value: "Open", focused: focusRow(id), focusId: `row:${id}` }))
            }
          }
        }
        break
      }
      case "results": {
        title = zoneStoreProviderTitle(store, model.provider) ?? ""
        const search = zoneStoreSearch(store, model.provider)
        first = ZONE_STORE_SEARCH_ROW
        rows.push(row({ kind: "title", label: "Search" }))
        ids.push({ id: ZONE_STORE_SEARCH_ROW, row: rows.length })
        rows.push(row({ kind: "item", label: "Search for", value: search._tag === "Idle" ? "" : search.query, valueFact: true, focused: focusRow(ZONE_STORE_SEARCH_ROW), focusId: `row:${ZONE_STORE_SEARCH_ROW}` }))
        if (search._tag === "Searching") rows.push(row({ kind: "word", label: "Searching" }))
        if (search._tag === "Problem") dialog = { headline: "Search", korri: false, line: search.message, actions: search.actions }
        if (search._tag === "Ready") {
          if (search.entries.length === 0) rows.push(row({ kind: "word", label: "Nothing found" }))
          else {
            rows.push(row({ kind: "title", label: "Results" }))
            for (const entry of search.entries) {
              const id = zoneStoreEntryRow(entry.id)
              ids.push({ id, row: rows.length })
              rows.push(row({ kind: "item", label: entry.title, labelFact: true, value: entry.system ?? "", valueFact: true, focused: focusRow(id), focusId: `row:${id}` }))
            }
          }
        }
        break
      }
      case "entry": {
        const details = store.details
        if (details._tag === "Closed") {
          rows.push(row({ kind: "word", label: "Loading" }))
          break
        }
        title = details.entry.title
        well = true
        if (details.entry.system !== undefined) tags.push(tag(details.entry.system, "system", true))
        if (details._tag === "Loading") rows.push(row({ kind: "word", label: "Loading" }))
        if (details._tag === "Problem") dialog = { headline: details.entry.title, korri: true, line: details.message, actions: details.actions }
        if (details._tag !== "Ready") break
        if (details.description !== undefined) rows.push(row({ kind: "text", label: details.description, labelFact: true }))
        const get = details.get
        const buttons = (entries: readonly { readonly id: string; readonly label: string; readonly disabled: boolean }[]) => {
          first ??= entries.find(entry => !entry.disabled)?.id
          const at = rows.length
          const lit = entries.find(entry => !entry.disabled)?.id
          for (const entry of entries) ids.push({ id: entry.id, row: at })
          rows.push(row({
            kind: "buttons",
            holds: entries.some(entry => focusRow(entry.id)),
            buttons: entries.map(entry => btn(entry.label, { focused: focusRow(entry.id), lit: entry.id === lit, disabled: entry.disabled, focusId: `button:${entry.id}` })),
          }))
        }
        switch (get._tag) {
          case "Offer":
            if (get.note !== undefined) rows.push(row({ kind: "text", label: get.note, labelFact: true }))
            buttons(get.actions.map(action => ({ id: zoneStoreActionRow(action.id), label: action.label, disabled: !action.enabled })))
            break
          case "Unavailable":
            tags.push(tag("Not available", "remote"))
            rows.push(row({ kind: "text", label: get.reason, labelFact: true }))
            break
          case "Confirm":
            dialog = { headline: get.kicker, korri: true, line: get.detail ?? "", actions: get.actions }
            break
          case "Problem":
            dialog = { headline: get.kicker, korri: true, line: get.reason, actions: get.actions }
            break
          case "Working":
            tags.push(get.queued === true ? tag("Queued", "continue") : tag("Download", "download"))
            break
          case "InLibrary":
            if (get.detail === undefined) tags.push(tag("In library", "system"))
            else rows.push(row({ kind: "text", label: get.detail, labelFact: true }))
            if (get.gameId !== undefined) buttons([{ id: zoneStorePlayRow(get.gameId), label: "Play", disabled: false }])
            break
        }
      }
    }
  }
  return {
    rows,
    ids,
    dialog: dialog === undefined ? [] : dialog.actions.map(action => action.id),
    dialogHeadline: dialog?.headline ?? "",
    dialogKorri: dialog?.korri ?? false,
    dialogLine: dialog?.line ?? "",
    dialogActions: dialog?.actions ?? [],
    title,
    tags,
    well,
  }
}
