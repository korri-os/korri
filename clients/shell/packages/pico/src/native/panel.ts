/**
 * Settings and a game's launch settings: coloured spines over a card of rows
 * (PicoPanelScreen, PicoSettingsPanel), and their questions: a confirmation,
 * a list to pick from, the text editor, a Wi-Fi password and the identity
 * dialog. Ports of PicoSettings, PicoLaunchSettings, PicoTextEditor and
 * PicoIdentityDialog.
 */
import type { SurfaceIdentityManagement } from "../model"
import { picoLaunchScope, picoLaunchShown, type PicoLaunchView } from "../pico-launch-view"
import {
  PICO_IDENTITY_BACKUP_ACTION,
  PICO_IDENTITY_SWITCH_LOCAL_ACTION,
  PICO_IDENTITY_SWITCH_NIP46_ACTION,
  picoIdentityRetiredKey,
  picoNumberFits,
  picoNumberRange,
  picoSettingsViewFromModel,
  picoTextSettingFrom,
  type PicoConfirmation,
  type PicoSettingRowView,
  type PicoSettingsView,
} from "../pico-settings-view"
import type { PicoTextDraft } from "../pico-text-draft"
import { picoWifiEntryView } from "../pico-wifi-view"
import { IDENTITY_FORM, type IdentityForm, type Settings, type SettingsQuestion } from "../state/navigation"
import { qrPath } from "./qr"
import type { Target } from "./focus"
import { frame } from "./frame"
import { keyboard, type KeyPress } from "./keyboard"
import { A, B, button, type ButtonView, card, type CardView, chip, type ChipView, mark, NO_CARD } from "./kit"
import { cardTargets } from "./catalog"
import { type Act, type Ctx, local, nav, type Screen, type Typing } from "./types"

export interface SegmentView { readonly label: string; readonly on: boolean }

export interface RowView {
  /** Korri's group caption above this row (launch settings), or "". */
  readonly section: string
  readonly label: string
  readonly value: string
  readonly description: string
  readonly source: string
  readonly deviceOnly: boolean
  /** fact, action, cycle, step, pick, text, network */
  readonly kind: string
  readonly focused: boolean
  readonly focusId?: string
  readonly warn: boolean
  /** idle, saving, problem, failed */
  readonly state: string
  readonly stateText: string
  readonly segments: readonly SegmentView[]
  readonly bars: number
  readonly locked: boolean
  readonly chips: readonly ChipView[]
  /** Plates and small lines above this row in its list: main.slint places
   * and scrolls rows by them. */
  readonly platesBefore: number
  readonly linesBefore: number
}

/** Small lines a row takes besides its plate: its caption, its source, its
 * description and Korri's word about it. */
export const rowLines = (row: RowView): number =>
  (row.section === "" ? 0 : 1) + (row.source === "" && !row.deviceOnly ? 0 : 1) + (row.description === "" ? 0 : 1) + (row.stateText === "" || row.state === "saving" ? 0 : 1)

/** Number the rows' places in their list. */
export function stacked(rows: readonly RowView[]): RowView[] {
  let lines = 0
  return rows.map((row, i) => {
    const placed = { ...row, platesBefore: i, linesBefore: lines }
    lines += rowLines(row)
    return placed
  })
}

/** The spines' colours, in turn (PicoPanelScreen). */
export const SPINE_TONES = ["blue", "pink", "orange", "yellow", "lime", "peach"] as const


export function rowView(row: PicoSettingRowView, focused: boolean, focusId?: string): RowView {
  const control = row.control
  const state = row.state
  const value = control.kind === "step" ? control.current
    : control.kind === "pick" ? "Choose a setting"
    : control.kind === "text" && control.sensitive ? (row.value ?? "")
    : control.kind === "network" ? (control.network.connected ? "CONNECTED" : "")
    : row.value ?? ""
  return {
    section: row.section ?? "",
    label: row.label,
    value,
    description: row.description ?? "",
    source: row.source ?? "",
    deviceOnly: row.deviceOnly === true,
    kind: control.kind,
    focused,
    ...(focusId === undefined ? {} : { focusId }),
    warn: control.kind === "action" && control.destructive,
    state: state === "idle" ? "idle" : state === "saving" ? "saving" : "problem" in state ? "problem" : "failed",
    stateText: typeof state === "object" ? ("problem" in state ? state.problem : state.failed) : state === "saving" ? "SAVING" : "",
    segments: control.kind === "cycle" ? control.options.map((option, i) => ({ label: option.label.toUpperCase(), on: i === control.current })) : [],
    bars: control.kind === "network" ? control.network.bars : 0,
    locked: control.kind === "network" && control.network.secured,
    chips: [],
    platesBefore: 0,
    linesBefore: 0,
  }
}

/** What a press on a row means (PicoSettings' `activate`). */
export function rowAct(row: PicoSettingRowView): Act | undefined {
  const control = row.control
  switch (control.kind) {
    case "fact":
      return undefined
    case "cycle":
    case "step":
      return [nav({ _tag: "ChangedSetting", settingId: row.id, value: control.next })]
    case "action":
      return control.confirmation !== undefined
        ? [nav({ _tag: "AskedSettingConfirmation", actionId: control.actionId, confirmation: control.confirmation })]
        : [nav({ _tag: "PressedSettingAction", actionId: control.actionId })]
    case "text":
      return [nav({ _tag: "OpenedEditor", settingId: row.id })]
    case "pick":
      return [nav({ _tag: "OpenedPicker", settingId: row.id })]
    case "network":
      return [nav({ _tag: "PressedSettingAction", actionId: control.actionId })]
  }
}

interface PanelParts {
  readonly spines: readonly ButtonView[]
  readonly spineTone: number
  readonly beside: boolean
  readonly rows: readonly RowView[]
  readonly focusedRow: number
  readonly targets: Target<Act>[]
}

/** Spines and rows. `top` leaves room on the focus plan for tabs above. */
function panelParts(
  settings: PicoSettingsView,
  group: number,
  focused: string | undefined,
  beside: boolean,
  top: number,
): PanelParts {
  const targets: Target<Act>[] = []
  const at = Math.max(0, Math.min(group, settings.groups.length - 1))
  const spines = settings.groups.map((shown, i) => {
    const id = `spine:${i}`
    targets.push({
      id,
      region: "spines", box: beside ? { x: 0, y: top + i * 10, w: 18, h: 8 } : { x: i * 20, y: 0, w: 18, h: 8 },
      act: [nav({ _tag: "ChoseSettingsGroup", group: i })],
      // Moving onto a spine shows its rows, as focusing a tab did.
      enter: [nav({ _tag: "ChoseSettingsGroup", group: i })],
    })
    return { label: shown.title, detail: "", ...mark(id, focused), disabled: false, warn: false, lit: i === at }
  })
  const rows = (settings.groups[at]?.rows ?? []).map((row, i) => {
    const id = `row:${row.id}`
    const act = rowAct(row)
    targets.push({
      id,
      region: "rows", box: beside ? { x: 20, y: top + i * 10, w: 80, h: 8 } : { x: 0, y: top + 10 + i * 10, w: 100, h: 8 },
      ...(act === undefined ? {} : { act }),
    })
    return rowView(row, focused === id, id)
  })
  return {
    spines,
    spineTone: at,
    beside,
    rows: stacked(rows),
    focusedRow: Math.max(0, rows.findIndex(row => row.focused)),
    targets,
  }
}

const panelView = (parts: PanelParts) => ({
  spines: parts.spines,
  spineTone: parts.spineTone,
  beside: parts.beside,
  rows: parts.rows,
  focusedRow: parts.focusedRow,
})

const EMPTY_PANEL = {
  spines: [] as ButtonView[],
  spineTone: 0,
  beside: false,
  rows: [] as RowView[],
  focusedRow: 0,
  scopes: [] as ButtonView[],
  notes: [] as string[],
  message: "",
}

/** A confirmation Korri attached to an action: red when destructive, else
 * yellow; a message of several lines is a report, each line a stop. */
function confirmationCard(confirmation: PicoConfirmation, focused: string | undefined) {
  const lines = confirmation.message.split("\n").filter(line => line.trim() !== "")
  const report = lines.length > 1
  return cardTargets(
    card({
      tone: confirmation.destructive === true ? "warn" : "ask",
      kicker: confirmation.destructive === true ? "ARE YOU SURE?" : "CHECK FIRST",
      title: confirmation.title,
      message: report ? "" : confirmation.message,
      lines: report ? lines.map(text => ({ text, focused: false })) : [],
      actions: [button(confirmation.confirmLabel.toUpperCase(), { warn: confirmation.destructive === true }), button("CANCEL")],
      inline: report,
    }),
    ["confirm", "cancel"],
    [[nav({ _tag: "ConfirmedSettingAction" })], [nav({ _tag: "CancelledSettingAction" })]],
    focused,
    report ? lines.length : 0,
  )
}

export interface EditorView {
  readonly group: string
  /** "NOW usu": the value Korri holds, while it is not a secret. */
  readonly now: string
  readonly note: string
  readonly prompt: string
  readonly text: string
  readonly placeholder: string
  readonly count: string
  readonly keys: ReturnType<typeof keyboard>["keys"]
  readonly keyRows: number
  readonly actions: readonly ButtonView[]
  readonly alone: ButtonView
  readonly hasAlone: boolean
  readonly problem: string
}

/** The keyboard's presses as messages to the program that owns the draft. */
export const typeInto = (press: KeyPress): Act =>
  press._tag === "Type" ? [nav({ _tag: "TypedText", character: press.character })]
    : press._tag === "Backspace" ? [nav({ _tag: "PressedTextBackspace" })]
    : press._tag === "Clear" ? [nav({ _tag: "ClearedText" })]
    : press._tag === "Capitals" ? [nav({ _tag: "ToggledCapitals" })]
    : [nav({ _tag: "ToggledSymbols" })]

/** The text editor's body (PicoTextEditor): field, keyboard and answers. */
export function editorParts(
  options: {
    readonly group: string
    readonly note?: string
    readonly prompt?: string
    readonly now?: string
    readonly draft: PicoTextDraft
    readonly secret: boolean
    readonly shown?: boolean
    readonly placeholder?: string
    readonly maxLength?: number
    readonly actions: readonly { readonly label: string; readonly disabled?: boolean; readonly act: Act }[]
    readonly alone?: { readonly label: string; readonly act: Act }
    readonly problem?: string
    readonly charset?: "search" | "text"
  },
  focused: string | undefined,
  press: (press: KeyPress) => Act = typeInto,
): { readonly editor: EditorView; readonly targets: Target<Act>[] } {
  const keys = keyboard(options.charset ?? "text", options.draft, focused, 30, press)
  const targets: Target<Act>[] = [...keys.targets]
  const actions = options.actions.map((action, i) => {
    const id = `answer:${i}`
    if (action.disabled !== true) targets.push({ id, region: "answers", box: { x: i * 50, y: 90, w: 48, h: 8 }, act: action.act })
    return button(action.label, { disabled: action.disabled === true, ...mark(id, focused) })
  })
  if (options.alone !== undefined) targets.push({ id: "alone", region: "answers", box: { x: 0, y: 100, w: 100, h: 8 }, act: options.alone.act })
  const text = options.draft.text
  return {
    editor: {
      group: options.group,
      now: options.now === undefined || options.now === "" ? "" : `NOW ${options.now}`,
      note: options.note ?? "",
      prompt: options.prompt ?? "",
      text: options.secret && options.shown !== true ? "•".repeat([...text].length) : text,
      placeholder: options.placeholder ?? "",
      count: options.maxLength === undefined ? "" : `${[...text].length}/${options.maxLength}`,
      keys: keys.keys,
      keyRows: keys.rows,
      actions,
      alone: button(options.alone?.label ?? "", { warn: true, ...mark("alone", focused) }),
      hasAlone: options.alone !== undefined,
      problem: options.problem ?? "",
    },
    targets,
  }
}

export const NO_EDITOR: EditorView = {
  group: "", now: "", note: "", prompt: "", text: "", placeholder: "", count: "", keys: [], keyRows: 0, actions: [],
  alone: button(""), hasAlone: false, problem: "",
}

function editorNote(row: PicoSettingRowView): string {
  const numeric = row.control.kind === "text" ? row.control.numeric : undefined
  const range = numeric === undefined ? undefined : picoNumberRange(numeric)
  return [row.description, range === undefined ? undefined : `${range}.`].filter(part => part !== undefined).join(" ")
}

/** A question over Settings or launch settings, when one is open. */
function question(
  ctx: Ctx,
  settings: PicoSettingsView,
  layer: Settings,
  title: string,
): { readonly screen: Partial<Screen> & { readonly view: Record<string, unknown> }; readonly key: string } | undefined {
  const focused = ctx.s.focus
  const asking = layer.question
  const clock = ctx.korri.clockLabel ?? ""
  if (asking._tag === "ConfirmingAction") {
    const placed = confirmationCard(asking.confirmation, focused)
    return {
      key: `confirm:${asking.actionId}`,
      screen: { targets: placed.targets, fallback: placed.card.lines.length > 0 ? "line:0" : "cancel", view: { card: placed.card } },
    }
  }
  if (asking._tag === "PickingChoice") {
    const row = settings.groups.flatMap(group => group.rows).find(candidate => candidate.id === asking.settingId)
    if (row?.control.kind !== "pick") return undefined
    const options = row.control.options
    const placed = cardTargets(
      card({ tone: "ask", kicker: "CHOOSE ONE", title: row.label, message: row.description ?? "", actions: [...options.map(option => button(option.label)), button("CANCEL")] }),
      [...options.map(option => `pick:${option.value}`), "cancel"],
      [...options.map(option => [nav({ _tag: "PickedChoice", value: option.value })]), [nav({ _tag: "CancelledPicker" })]],
      focused,
    )
    return { key: `pick:${asking.settingId}`, screen: { targets: placed.targets, view: { card: placed.card } } }
  }
  if (asking._tag === "EditingText" || asking._tag === "ConfirmingClear") {
    const edited = picoTextSettingFrom(settings, asking.settingId)
    if (edited === undefined) return undefined
    const row = edited.row
    const control = row.control
    if (asking._tag === "ConfirmingClear") {
      const placed = cardTargets(
        card({ tone: "warn", kicker: "ARE YOU SURE?", title: `${(control.clearLabel ?? "Clear").toUpperCase()}?`, message: `${row.label} goes back to having no value.`, actions: [button((control.clearLabel ?? "Clear").toUpperCase(), { warn: true }), button("CANCEL")] }),
        ["confirm", "cancel"],
        [[nav({ _tag: "ConfirmedClear" })], [nav({ _tag: "CancelledClear" })]],
        focused,
      )
      return { key: `clear:${row.id}`, screen: { targets: placed.targets, fallback: "cancel", view: { card: placed.card } } }
    }
    const disabled = (asking.draft.text.trim() === "" && control.allowEmpty !== true)
      || (control.numeric !== undefined && !picoNumberFits(asking.draft.text, control.numeric))
    const range = control.numeric === undefined ? undefined : picoNumberRange(control.numeric)
    const parts = editorParts({
      group: edited.group,
      note: editorNote(row),
      prompt: range === undefined ? "" : `A number, ${range.toLowerCase()}`,
      draft: asking.draft,
      ...(control.numeric === undefined && !control.sensitive && row.value !== undefined ? { now: row.value } : {}),
      secret: control.sensitive,
      ...(control.placeholder === undefined ? {} : { placeholder: control.placeholder }),
      ...(control.maxLength === undefined ? {} : { maxLength: control.maxLength }),
      actions: [
        { label: row.state === "saving" ? "SAVING" : "SAVE", disabled: disabled || row.state === "saving", act: [nav({ _tag: "SavedText" })] },
        { label: "CANCEL", act: [nav({ _tag: "ClosedEditor" })] },
      ],
      ...(control.clearLabel === undefined ? {} : { alone: { label: control.clearLabel.toUpperCase(), act: [nav({ _tag: "AskedClear" })] } }),
      ...(typeof row.state === "object" && "problem" in row.state ? { problem: row.state.problem } : {}),
    }, focused)
    return {
      key: `edit:${row.id}`,
      screen: {
        targets: parts.targets,
        fallback: "key:0:0",
        view: {
          page: "editor",
          editor: parts.editor,
          frame: frame({ place: title, label: row.label.toUpperCase(), clock, hints: [A("SELECT"), B("CANCEL")] }),
        },
      },
    }
  }
  if (asking._tag === "Wifi") {
    const entry = picoWifiEntryView(asking.entry)
    const parts = editorParts({
      group: "WI-FI",
      note: entry.note ?? "",
      prompt: entry.prompt,
      draft: entry.draft,
      secret: entry.secret,
      shown: entry.shown,
      maxLength: entry.maxLength,
      actions: [
        { label: entry.submitLabel, disabled: !entry.ready, act: [nav({ _tag: "SubmittedWifi" })] },
        { label: "CANCEL", act: [nav({ _tag: "ClosedEditor" })] },
        ...(entry.secret ? [{ label: entry.shown ? "HIDE" : "SHOW", act: [nav({ _tag: "ToggledShown" })] }] : []),
      ],
    }, focused)
    return {
      key: `wifi:${entry.place}:${entry.prompt}`,
      screen: {
        targets: parts.targets,
        fallback: "key:0:0",
        view: { page: "editor", editor: parts.editor, frame: frame({ place: title, label: entry.place, clock, hints: [A("SELECT"), B("CANCEL")] }) },
      },
    }
  }
  return undefined
}

export function settingsScreen(ctx: Ctx, layer: Settings): Screen {
  const { s, korri, model, plan } = ctx
  const focused = s.focus
  const settings = picoSettingsViewFromModel(model, { font: s.font })
  if (layer.question._tag === "Identity") return identityScreen(ctx, layer.question)
  const asked = question(ctx, settings, layer, "SETTINGS")
  const clock = korri.clockLabel ?? ""
  const base = frame({ label: "SETTINGS", clock, readout: settings.buildLabel ?? "", hints: [A("SELECT"), B("BACK")] })
  if (asked !== undefined && asked.screen.view["page"] === "editor") {
    return { key: `settings:${asked.key}`, targets: asked.screen.targets ?? [], ...(asked.screen.fallback === undefined ? {} : { fallback: asked.screen.fallback }), view: { ...asked.screen.view, card: NO_CARD } }
  }
  if (asked !== undefined) {
    return {
      key: `settings:${asked.key}`,
      targets: asked.screen.targets ?? [],
      ...(asked.screen.fallback === undefined ? {} : { fallback: asked.screen.fallback }),
      view: { page: "settings", frame: base, panel: { ...EMPTY_PANEL }, ...asked.screen.view },
    }
  }
  const parts = panelParts(settings, layer.group, focused, plan.spinesBeside, 0)
  const problem = model.settingsStatus._tag === "Problem" ? model.settingsStatus : undefined
  const empty = settings.groups.length === 0
  return {
    key: "settings",
    targets: parts.targets,
    active: { spines: `spine:${parts.spineTone}` },
    fallback: `spine:${parts.spineTone}`,
    view: {
      page: "settings",
      frame: base,
      card: empty
        ? card({ tone: "tell", kicker: "NOTHING TO SET", message: "Korri has no settings to show on this device." })
        : NO_CARD,
      panel: { ...panelView(parts), scopes: [], notes: [], message: problem === undefined ? "" : problem.message },
    },
  }
}

// ---- Launch settings ----

export function launchScreen(ctx: Ctx, launch: PicoLaunchView, layer: Settings): Screen {
  const { s, korri, model, plan } = ctx
  const focused = s.focus
  const clock = korri.clockLabel ?? ""
  const title = launch.gameTitle ?? "LAUNCH SETTINGS"
  const shown = picoLaunchShown(launch, layer.scope)
  const asked = question(ctx, launch.settings, layer, "LAUNCH SETTINGS")
  const reload = launch.actions.find(action => action.id.endsWith(":reload") || action.label.toUpperCase() === "RELOAD")
  const leadAct: Act | undefined = reload === undefined || !reload.enabled ? undefined : [nav({ _tag: "PressedLaunchAction", actionId: reload.id })]
  const base = frame({
    place: launch.gameTitle ?? "",
    label: "LAUNCH SETTINGS",
    clock,
    hints: [A("SELECT"), B("BACK")],
    hasLead: reload !== undefined,
    leadKind: "plain",
    lead: button((reload?.label ?? "RELOAD").toUpperCase(), { ...mark("lead", focused), disabled: reload?.enabled !== true }),
  })
  void model
  if (asked !== undefined && asked.screen.view["page"] === "editor") {
    return { key: `launch:${asked.key}`, targets: asked.screen.targets ?? [], ...(asked.screen.fallback === undefined ? {} : { fallback: asked.screen.fallback }), view: { ...asked.screen.view, card: NO_CARD } }
  }
  if (asked !== undefined) {
    return {
      key: `launch:${asked.key}`,
      targets: asked.screen.targets ?? [],
      ...(asked.screen.fallback === undefined ? {} : { fallback: asked.screen.fallback }),
      view: { page: "settings", frame: base, panel: { ...EMPTY_PANEL }, ...asked.screen.view },
    }
  }
  if (launch.phase === "Conflict" || launch.phase === "Error" || launch.phase === "Loading") {
    const answers = launch.actions.filter(action => action.enabled)
    const placed = cardTargets(
      launch.phase === "Loading"
        ? card({ tone: "tell", kicker: "READING", title, message: "Korri is reading this game's launch settings." })
        : card({
            tone: launch.phase === "Conflict" ? "ask" : "warn",
            kicker: launch.phase === "Conflict" ? "CHANGED ELSEWHERE" : "COULD NOT READ",
            title,
            message: launch.message ?? "",
            actions: answers.map(action => button(action.label.toUpperCase())),
          }),
      answers.map(action => `launch:${action.id}`),
      answers.map(action => [nav({ _tag: "PressedLaunchAction", actionId: action.id })]),
      focused,
    )
    return {
      key: `launch:${launch.phase}`,
      targets: placed.targets,
      view: { page: "settings", frame: { ...base, hasLead: false }, panel: { ...EMPTY_PANEL }, card: placed.card },
    }
  }
  const beside = plan.spinesBeside
  const parts = panelParts(shown, layer.group, focused, beside, 20)
  const current = launch.layers[Math.max(0, Math.min(layer.group, launch.layers.length - 1))]
  const scopeAt = picoLaunchScope(current, layer.scope)
  const scopes = (current?.scopes ?? []).map((scope, i) => {
    const id = `scope:${i}`
    parts.targets.push({
      id,
      region: "scopes", box: beside ? { x: 20 + i * 10, y: 10, w: 9, h: 8 } : { x: i * 10, y: 10, w: 9, h: 8 },
      act: [nav({ _tag: "ChoseLaunchScope", scope: i })],
      enter: [nav({ _tag: "ChoseLaunchScope", scope: i })],
    })
    return button(scope.label.toUpperCase(), { lit: i === scopeAt, ...mark(id, focused) })
  })
  if (leadAct !== undefined) parts.targets.push({ id: "lead", region: "floor", box: { x: 0, y: 1000, w: 10, h: 8 }, act: leadAct })
  return {
    key: "launch",
    targets: parts.targets,
    active: { spines: `spine:${parts.spineTone}`, scopes: `scope:${scopeAt}` },
    fallback: `row:${shown.groups[parts.spineTone]?.rows[0]?.id ?? ""}`,
    view: {
      page: "settings",
      frame: base,
      card: NO_CARD,
      panel: { ...panelView(parts), scopes, notes: launch.notes, message: "" },
    },
  }
}

// ---- The identity dialog ----

export interface IdentityView {
  readonly title: string
  readonly problem: string
  readonly note: string
  readonly qr: string
  readonly qrSize: number
  readonly secret: string
  readonly rows: readonly ButtonView[]
}

export const NO_IDENTITY: IdentityView = { title: "", problem: "", note: "", qr: "", qrSize: 0, secret: "", rows: [] }

const mask = (text: string) => "•".repeat([...text].length)

/**
 * The identity dialog (PicoIdentityDialog): its fields open Pico's keyboard,
 * the records choice cycles, the statement ticks, and the answer sends what
 * the form holds.
 */
export function identityScreen(ctx: Ctx, asking: Extract<SettingsQuestion, { _tag: "Identity" }>): Screen {
  const { s, korri, model } = ctx
  const focused = s.focus
  const clock = korri.clockLabel ?? ""
  if (s.typing !== undefined) return typingScreen(ctx, s.typing, "SETTINGS")
  const action = asking.actionId
  const form: IdentityForm = asking.form ?? IDENTITY_FORM
  const identity: SurfaceIdentityManagement | undefined = model.identityManagement
  const status = identity?.status
  const working = status?._tag === "Working"
  const exportKey = picoIdentityRetiredKey(action, "export")
  const deleteKey = picoIdentityRetiredKey(action, "delete")
  const switching = action === PICO_IDENTITY_SWITCH_LOCAL_ACTION || action === PICO_IDENTITY_SWITCH_NIP46_ACTION
  const title = action === PICO_IDENTITY_BACKUP_ACTION ? "BACK UP IDENTITY"
    : action === PICO_IDENTITY_SWITCH_LOCAL_ACTION ? "SWITCH FROM BACKUP"
    : action === PICO_IDENTITY_SWITCH_NIP46_ACTION ? "SWITCH TO NIP-46"
    : exportKey !== undefined ? "BACK UP RETIRED KEY"
    : "DELETE RETIRED KEY"
  const rows: { readonly id: string; readonly view: ButtonView; readonly act?: Act }[] = []
  const field = (id: Typing["field"], label: string, value: string, secret: boolean) => rows.push({
    id,
    view: button(label, { detail: value === "" ? "—" : secret ? mask(value) : value.length > 18 ? `${value.slice(0, 18)}…` : value }),
    act: [local(state => ({ ...state, typing: { field: id, label, draft: { text: value, capitals: false, symbols: false }, secret } }))],
  })
  let note = ""
  let qr = ""
  let qrSize = 0
  let secret = ""
  if (status?._tag === "BackupReady") {
    note = "Korri cannot recover a lost key. If this backup is not exported before device loss, the identity is unrecoverable. Store it outside the device."
    const drawn = qrPath(status.encryptedSecret)
    qr = drawn?.path ?? ""
    qrSize = drawn?.size ?? 0
    secret = status.encryptedSecret
    rows.push({ id: "done", view: button("DONE"), act: [nav({ _tag: "ClosedIdentity" })] })
  } else if (status?._tag === "Switched") {
    note = "Korri is restarting with the new identity."
    rows.push({ id: "close", view: button("CLOSE"), act: [nav({ _tag: "ClosedIdentity" })] })
  } else {
    if (action === PICO_IDENTITY_BACKUP_ACTION || exportKey !== undefined) {
      field("identity:password", "Password", form.password, true)
      rows.push({
        id: "submit",
        view: button(working && status?._tag === "Working" ? status.operation : "CREATE BACKUP", { disabled: working || form.password.trim() === "" }),
        act: [nav({ _tag: "SubmittedIdentity" })],
      })
    }
    if (action === PICO_IDENTITY_SWITCH_LOCAL_ACTION) {
      field("identity:secret", "Encrypted backup", form.secret, false)
      field("identity:password", "Password", form.password, true)
    }
    if (action === PICO_IDENTITY_SWITCH_NIP46_ACTION) field("identity:bunkerUri", "Bunker URI", form.bunkerUri, false)
    if (switching) {
      rows.push({
        id: "disposition",
        view: button("Local records", { detail: form.disposition === "transfer" ? "◀ Transfer ▶" : "◀ Delete ▶" }),
        act: [nav({ _tag: "ChoseIdentityDisposition", disposition: form.disposition === "transfer" ? "delete" : "transfer" })],
      })
      rows.push({
        id: "confirmed",
        view: button(`${form.confirmed ? "[X]" : "[ ]"} I understand trust from the old key is lost`),
        act: [nav({ _tag: "CheckedIdentityConfirmation", confirmed: !form.confirmed })],
      })
      note = "Save files and save states stay under the plugin account."
      const incomplete = !form.confirmed || (action === PICO_IDENTITY_SWITCH_LOCAL_ACTION ? form.secret.trim() === "" || form.password.trim() === "" : form.bunkerUri.trim() === "")
      rows.push({
        id: "submit",
        view: button(working && status?._tag === "Working" ? status.operation : "SWITCH IDENTITY", { warn: true, disabled: working || incomplete }),
        act: [nav({ _tag: "SubmittedIdentity" })],
      })
    }
    if (deleteKey !== undefined) {
      note = "Delete this private key only after its encrypted backup is stored elsewhere."
      rows.push({
        id: "confirmed",
        view: button(`${form.confirmed ? "[X]" : "[ ]"} I have stored its backup`),
        act: [nav({ _tag: "CheckedIdentityConfirmation", confirmed: !form.confirmed })],
      })
      rows.push({ id: "submit", view: button("DELETE KEY", { warn: true, disabled: working || !form.confirmed }), act: [nav({ _tag: "SubmittedIdentity" })] })
    }
    rows.push({ id: "close", view: button("CLOSE"), act: [nav({ _tag: "ClosedIdentity" })] })
  }
  const targets: Target<Act>[] = rows
    .filter(row => !row.view.disabled)
    .map((row, i) => ({ id: row.id, region: "list", box: { x: 0, y: i * 10, w: 100, h: 8 }, ...(row.act === undefined ? {} : { act: row.act }) }))
  return {
    key: `identity:${action}:${status?._tag ?? "Idle"}`,
    targets,
    fallback: rows[0]?.id,
    view: {
      page: "identity",
      frame: frame({ place: "SETTINGS", label: title, clock, hints: [A("SELECT"), B("CLOSE")] }),
      card: NO_CARD,
      identity: {
        title,
        problem: status?._tag === "Problem" ? status.message : "",
        note,
        qr,
        qrSize,
        secret,
        rows: rows.map(row => ({ ...row.view, ...mark(row.id, focused) })),
      },
    },
  }
}

/** Pico's keyboard over a form field (identity, a backup password). */
export function typingScreen(ctx: Ctx, typing: Typing, place: string): Screen {
  const focused = ctx.s.focus
  const edit = (change: (draft: PicoTextDraft) => PicoTextDraft): Act =>
    [local(state => state.typing === undefined ? state : { ...state, typing: { ...state.typing, draft: change(state.typing.draft) } })]
  const parts = editorParts({
    group: typing.label.toUpperCase(),
    draft: typing.draft,
    secret: typing.secret,
    actions: [
      { label: "OK", act: [...commitTyping(typing), local(state => ({ ...state, typing: undefined }))] },
      { label: "CANCEL", act: [local(state => ({ ...state, typing: undefined }))] },
    ],
  }, focused, press =>
    press._tag === "Type" ? edit(draft => ({ ...draft, text: draft.text + press.character }))
      : press._tag === "Backspace" ? edit(draft => ({ ...draft, text: [...draft.text].slice(0, -1).join("") }))
      : press._tag === "Clear" ? edit(draft => ({ ...draft, text: "" }))
      : press._tag === "Capitals" ? edit(draft => ({ ...draft, capitals: !draft.capitals }))
      : edit(draft => ({ ...draft, symbols: !draft.symbols })))
  return {
    key: `typing:${typing.field}`,
    targets: parts.targets,
    fallback: "key:0:0",
    view: {
      page: "editor",
      card: NO_CARD,
      editor: parts.editor,
      frame: frame({ place, label: typing.label.toUpperCase(), clock: ctx.korri.clockLabel ?? "", hints: [A("SELECT"), B("CANCEL")] }),
    },
  }
}

/** OK on a form field: the text goes to the program that owns the form. */
function commitTyping(typing: Typing): Act {
  const value = typing.draft.text
  switch (typing.field) {
    case "identity:password":
      return [nav({ _tag: "EditedIdentity", field: "password", value })]
    case "identity:secret":
      return [nav({ _tag: "EditedIdentity", field: "secret", value })]
    case "identity:bunkerUri":
      return [nav({ _tag: "EditedIdentity", field: "bunkerUri", value })]
    case "backup:password":
      return [{ to: "setup", message: { _tag: "EditedBackupPassword", password: value } }]
  }
}
