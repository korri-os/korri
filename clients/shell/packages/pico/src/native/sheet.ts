/**
 * A tall blue card over the whole screen with a list in it (PicoGameOverlay):
 * the pause menu over a running game, and the runner picker. Items are rows
 * to press, notes under them, section captions, headings and facts.
 */
import type { SurfaceRunnerChoice } from "../model"
import type { PicoOverlayControlView, PicoOverlayView } from "../pico-overlay-view"
import { picoRangeLabel, picoStatusStrip } from "../pico-overlay-view"
import type { PicoOverlayState } from "../state/overlay"
import { rangeValue } from "../state/overlay"
import type { Target } from "./focus"
import { A, B, LR, type HintView, card, type CardView, button, NO_CARD } from "./kit"
import { cardTargets } from "./catalog"
import { type Act, nav, type Screen } from "./types"

export interface SheetItem {
  /** row, note, section, heading, fact, text */
  readonly kind: string
  readonly label: string
  readonly value: string
  readonly focused: boolean
  readonly focusId?: string
  readonly disabled: boolean
  readonly warn: boolean
  /** A range: cells lit of ten, and arrows either side. */
  readonly range: boolean
  readonly cells: number
  /** The value cycles: arrows either side. */
  readonly arrows: boolean
  /** Rows, small lines and headings above this item: main.slint places and
   * scrolls items by them. */
  readonly rowsBefore: number
  readonly linesBefore: number
  readonly headsBefore: number
}

export interface SheetView {
  readonly kicker: string
  readonly title: string
  readonly items: readonly SheetItem[]
  readonly focusedItem: number
  readonly hints: readonly HintView[]
}

const item = (kind: string, label: string, options: Partial<SheetItem> = {}): SheetItem => ({
  kind, label, value: "", focused: false, disabled: false, warn: false, range: false, cells: 0, arrows: false,
  rowsBefore: 0, linesBefore: 0, headsBefore: 0, ...options,
})

function finish(items: SheetItem[], focused: string | undefined, ids: readonly (string | undefined)[]): { readonly items: SheetItem[]; readonly focusedItem: number } {
  let rows = 0
  let lines = 0
  let heads = 0
  const marked = items.map((shown, i) => {
    const id = ids[i]
    const placed = { ...shown, ...(id === undefined ? { focused: false } : { focused: id === focused, focusId: id }), rowsBefore: rows, linesBefore: lines, headsBefore: heads }
    if (shown.kind === "row") rows += 1
    else if (shown.kind === "heading") heads += 1
    else lines += 1
    return placed
  })
  return { items: marked, focusedItem: Math.max(0, marked.findIndex(shown => shown.focused)) }
}

// ---- The gameplay overlay (PicoOverlay) ----

type OverlayDispatch = { readonly to: "overlay"; readonly message: import("../state/overlay").PicoOverlayMessage }
const overlay = (message: OverlayDispatch["message"]): OverlayDispatch => ({ to: "overlay", message })

export function overlayScreen(view: PicoOverlayView, state: PicoOverlayState, focused: string | undefined): Screen {
  // The status strip stands with every state of the overlay; it is no focus target.
  const strip = picoStatusStrip(view)
  if (state.asking !== undefined) {
    const control = state.asking
    const placed = cardTargets(
      card({
        tone: "warn",
        kicker: "ARE YOU SURE?",
        title: `${control.label.toUpperCase()}?`,
        message: control.description ?? "",
        actions: [button(control.label.toUpperCase(), { warn: true }), button("CANCEL")],
      }),
      ["confirm", "cancel"],
      [[overlay({ _tag: "ConfirmedControl" })], [overlay({ _tag: "CancelledControl" })]],
      focused,
    )
    return {
      key: `overlay:ask:${control.id}`,
      targets: placed.targets,
      fallback: "cancel",
      view: { card: placed.card, sheet: emptySheet([A("SELECT"), B("BACK")]), strip },
    }
  }
  if (view.problem !== undefined) {
    const problem = view.problem
    const placed = cardTargets(
      card({ tone: "warn", kicker: problem.kicker, title: view.title, message: problem.reason, actions: problem.canRetry ? [button("RETRY"), button("RESUME")] : [button("RESUME")] }),
      problem.canRetry ? ["retry", "resume"] : ["resume"],
      problem.canRetry ? [[overlay({ _tag: "PressedRetry" })], [overlay({ _tag: "PressedBack" })]] : [[overlay({ _tag: "PressedBack" })]],
      focused,
    )
    return { key: "overlay:problem", targets: placed.targets, view: { card: placed.card, sheet: emptySheet([A("SELECT"), B("RESUME")]), strip } }
  }
  const items: SheetItem[] = []
  const ids: (string | undefined)[] = []
  const targets: Target<Act>[] = []
  const add = (shown: SheetItem, id?: string) => {
    items.push(shown)
    ids.push(id)
  }
  if (view.streamPlayers !== undefined) add(item("text", `Playing: ${view.streamPlayers.join(", ")}`))
  const control = (shown: PicoOverlayControlView) => {
    const id = `control:${shown.id}`
    const range = shown.range
    const value = range === undefined ? undefined : rangeValue(state, shown)
    const cells = range === undefined || value === undefined || range.max <= range.min ? 0 : Math.round(((value - range.min) / (range.max - range.min)) * 10)
    add(item("row", shown.label, {
      value: range !== undefined && value !== undefined ? picoRangeLabel(value, range) : shown.stateLabel ?? "",
      disabled: !shown.enabled,
      warn: shown.destructive,
      range: range !== undefined,
      cells,
      arrows: shown.stateLabel !== undefined && range === undefined,
    }), id)
    if (shown.enabled) {
      const left: Act = range === undefined ? [] : [overlay({ _tag: "SteppedRange", control: shown, request: { way: -1, releaseExpected: false } })]
      const right: Act = range === undefined ? [] : [overlay({ _tag: "SteppedRange", control: shown, request: { way: 1, releaseExpected: false } })]
      targets.push({
        id,
        region: "list", box: { x: 0, y: targets.length * 10, w: 100, h: 8 },
        // A range moves with left and right; confirm does nothing on it.
        ...(range === undefined ? { act: [overlay(shown.destructive ? { _tag: "AskedControl", control: shown } : { _tag: "InvokedControl", control: shown })] } : {}),
        ...(range === undefined ? {} : { horizontal: { left, right } }),
      })
    }
    const note = shown.enabled ? shown.description : shown.disabledReason ?? shown.description
    if (note !== undefined && note !== "") add(item("note", note))
  }
  view.controls.forEach(control)
  for (const group of view.groups) {
    add(item("section", group.label))
    group.controls.forEach(control)
  }
  const done = finish(items, focused, ids)
  const ranging = targets.some(target => target.id === focused && target.horizontal !== undefined)
  return {
    key: "overlay",
    targets,
    view: {
      card: NO_CARD,
      sheet: {
        kicker: "PAUSED",
        title: view.title,
        ...done,
        hints: [...(ranging ? [LR("ADJUST")] : []), A("SELECT"), B("RESUME")],
      },
      strip,
    },
  }
}

function emptySheet(hints: readonly HintView[]): SheetView {
  return { kicker: "", title: "", items: [], focusedItem: 0, hints }
}

// ---- The runner picker (PicoRunnerPicker) ----

export function runnerScreen(choice: Exclude<SurfaceRunnerChoice, { _tag: "Closed" }>, focused: string | undefined): Screen {
  const items: SheetItem[] = []
  const ids: (string | undefined)[] = []
  const targets: Target<Act>[] = []
  const add = (shown: SheetItem, id?: string) => {
    items.push(shown)
    ids.push(id)
  }
  const row = (action: { readonly id: string; readonly label: string; readonly enabled: boolean }) => {
    const id = `action:${action.id}`
    add(item("row", action.label, { disabled: !action.enabled }), id)
    if (action.enabled) targets.push({ id, region: "list", box: { x: 0, y: targets.length * 10, w: 100, h: 8 }, act: [nav({ _tag: "ChoseRunnerAction", actionId: action.id })] })
  }
  add(item("text", choice.message))
  for (const saved of choice.saved) add(item("text", `${saved.label}: ${saved.runnerId}`))
  for (const warning of new Set(choice.warnings)) add(item("text", warning))
  for (const route of choice.routes) {
    add(item("heading", route.runnerId))
    add(item("fact", "System", { value: route.systemId }))
    add(item("fact", "Family", { value: route.familyId ?? "None" }))
    add(item("fact", "Runner build", { value: route.runnerBuild }))
    add(item("fact", "Program", { value: route.program }))
    for (const warning of new Set(route.warnings)) add(item("text", warning))
    route.actions.forEach(row)
  }
  choice.actions.forEach(row)
  const done = finish(items, focused, ids)
  return {
    key: `runner:${choice._tag}`,
    targets,
    view: {
      page: "runner",
      card: NO_CARD as CardView,
      sheet: { kicker: "RUNTIMES", title: choice.gameTitle, ...done, hints: [A("SELECT"), B("CANCEL")] },
    },
  }
}
