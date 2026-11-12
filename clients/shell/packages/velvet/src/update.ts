/**
 * Velvet's one update: what a message means, by what holds the screen.
 *
 * Layers, top first: the curtain (Korri's status, Velvet's question,
 * Korri's identity work), the keyboard, a sheet, then the screen. A press
 * goes to the top layer and nowhere else; everything under it is inert.
 *
 *     Pressed    directions move focus by the plan (src/focus.ts); Confirm
 *                performs the focused thing's Act; Back closes one level, or
 *                runs the action Korri marks for it (contract 7.2); options
 *                opens the sort sheet in a set and a game's actions in the
 *                detail; menu opens the house menu; system opens Settings
 *     Activated  a tap on a drawn target (ADR 0055): focus goes there and
 *                Confirm follows; a game in a list opens on the second tap
 *     Dragged    a finger dragged a set's rows: they scroll, focus stays
 *     Touched    touch is in use: the focus edge hides until the next press
 *     Tick       the stage drifts after two seconds of rest; a question
 *                Korri did not answer is asked again; a notice runs out
 *     Resized    the window: a new plan
 *
 * After every message `settle` brings the state up to Korri's model: the
 * home's lists, the set's window, each panel's focus, and FocusGame when
 * the focused game changed.
 */
import type { SurfaceButton, SurfaceMessage as Message, SurfaceModel as Model } from "../../../../../contracts/generated/surface"
import { detailButtons, detailGame, dragHome, dragSet, focusedGame, openSet, pressDetail, pressHome, pressSet, setSorts, setWindow, settleSet, tapHome, tapSet, type Step } from "./browse"
import { curtainLayout, curtainOf, type CurtainSpec } from "./curtain"
import { keyAt, keyGrid, keyInPlace, keyMove } from "./keys"
import { homeOf, LISTS_START, refresh, settleLists, shelvesOf, staleLists, tickLists } from "./lists"
import { clearEntry, commitEntry, entryHeading, onboardingSteps, screenPanel, sheetPanel, shown } from "./pages"
import { plan, type Plan } from "./plan"
import { lastAttempt, rejectedEntry, sameAttempt } from "./settings"
import type { Act, ButtonSpec, Direction, KeyboardState, PanelSpec, PanelState, Request, RowSpec, State } from "./types"

export const MOTION_KEY = "velvet.motion"
/** The tap target that is the Back button (ADR 0055: a touch screen has none). */
export const BACK_TARGET = "back"
/** The stage drifts after this long without a press. */
export const DRIFT_AFTER_MS = 2000
/** Ticks an invite's notice shows. */
const NOTICE_TICKS = 8

export function start(model: Model, preferences: Readonly<Record<string, string>>): State {
  const state: State = {
    size: { width: 1280, height: 720 },
    still: preferences[MOTION_KEY] === "reduced",
    touched: false,
    drift: false,
    lists: LISTS_START,
    depth: 0,
    onMenu: false,
    rowAt: 1,
    shelf: "",
    columns: {},
    pages: [],
    panels: {},
    scope: 0,
    curtain: { key: "", at: 0, line: 0 },
    onboarding: { step: 0, started: false, finishing: false },
    identity: { f: "idle" },
    overlay: { moving: {}, leaving: false },
    attempt: lastAttempt(model),
    reported: null,
    // Invites that stand when Velvet starts were announced before it.
    invitesSeen: (model.invites ?? []).map(invite => invite.id),
    wasBrowsing: model.status._tag === "Browsing",
  }
  return state
}

// ---- Panels: focus ----

/**
 * Whether a row takes the focus: a row or a range that does something when
 * pressed or stepped, or that only holds the focus so its note can be read.
 * The one rule for both sides: `update` moves the focus by it and `view`
 * marks the targets by it, so what is drawn as a target is what a press
 * can reach.
 */
export const takesFocus = (row: RowSpec): boolean => (row.kind === "row" || row.kind === "range") && (row.press !== undefined || row.step !== undefined || row.rests === true)

/** Where a panel's focus is: on the kept row or button, else by its place, else where the panel opens. */
export function panelFocus(panel: PanelSpec, kept: PanelState | undefined): PanelState {
  const rows = panel.rows.filter(takesFocus)
  const ids = [...rows.map(row => row.id), ...panel.buttons.map(button => button.id)]
  if (ids.length === 0) return { focus: "", at: 0 }
  if (kept !== undefined && ids.includes(kept.focus)) {
    const at = rows.findIndex(row => row.id === kept.focus)
    return at < 0 ? kept : at === kept.at ? kept : { focus: kept.focus, at }
  }
  if (kept !== undefined && kept.focus !== "") {
    // Korri took the row away: the row now at its place.
    const row = rows[Math.min(kept.at, rows.length - 1)]
    if (row !== undefined) return { focus: row.id, at: Math.min(kept.at, rows.length - 1) }
  }
  const first = panel.first !== undefined && ids.includes(panel.first) ? panel.first : ids[0]!
  return { focus: first, at: Math.max(0, rows.findIndex(row => row.id === first)) }
}

/** Whether a panel's buttons stand one under the other. */
export const panelStacks = (p: Plan): boolean => p.stack

function panelPress(state: State, panel: PanelSpec, button: SurfaceButton, p: Plan): Step {
  const rows = panel.rows.filter(takesFocus)
  const kept = panelFocus(panel, state.panels[panel.key])
  const put = (next: PanelState): Step => ({ state: { ...state, panels: { ...state.panels, [panel.key]: next }, candidate: next.focus === state.candidate?.rowId ? state.candidate : undefined } })
  if (button === "back") return { state, act: panel.back }
  if (button === "menu") return panel.menu === undefined ? { state } : { state, act: panel.menu }
  if (button === "options" || button === "system") return { state }
  const rowAt = rows.findIndex(row => row.id === kept.focus)
  if (rowAt >= 0) {
    const row = rows[rowAt]!
    switch (button) {
      case "up":
        return rowAt > 0 ? put({ focus: rows[rowAt - 1]!.id, at: rowAt - 1 }) : { state }
      case "down":
        if (rowAt < rows.length - 1) return put({ focus: rows[rowAt + 1]!.id, at: rowAt + 1 })
        // Into the buttons: the first one that can be pressed (a dimmed Next waits beside Skip).
        return panel.buttons.length > 0 ? put({ focus: (panel.buttons.find(candidate => candidate.dim !== true) ?? panel.buttons[0]!).id, at: rowAt }) : { state }
      case "left":
      case "right":
        return row.step === undefined ? { state } : { state, act: row.step(button === "left" ? -1 : 1) }
      case "confirm":
        return row.press === undefined ? { state } : { state, act: row.press }
    }
  }
  const at = panel.buttons.findIndex(candidate => candidate.id === kept.focus)
  const focused: ButtonSpec | undefined = panel.buttons[at]
  if (focused === undefined) return { state }
  const stacked = panelStacks(p)
  const along = stacked ? (button === "up" ? -1 : button === "down" ? 1 : 0) : button === "left" ? -1 : button === "right" ? 1 : 0
  if (button === "confirm") return { state, act: focused.press }
  if (along !== 0) {
    const next = panel.buttons[at + along]
    if (next !== undefined) return put({ focus: next.id, at: kept.at })
  }
  // Up from the buttons: back to the row focus was on.
  if (button === "up" && rows.length > 0 && (!stacked || at === 0)) {
    const back = Math.min(kept.at, rows.length - 1)
    return put({ focus: rows[back]!.id, at: back })
  }
  return { state }
}

function panelTap(state: State, panel: PanelSpec, target: string, at: number | undefined, released: boolean): Step | undefined {
  const row = panel.rows.find(candidate => candidate.id === target && takesFocus(candidate))
  if (row !== undefined) {
    const next: State = { ...state, panels: { ...state.panels, [panel.key]: { focus: row.id, at: panel.rows.filter(takesFocus).indexOf(row) } } }
    if (at !== undefined && row.slide !== undefined) return { state: next, act: row.slide(at, released) }
    return row.press === undefined ? { state: next } : { state: next, act: row.press }
  }
  const button = panel.buttons.find(candidate => candidate.id === target)
  if (button === undefined) return undefined
  const kept = panelFocus(panel, state.panels[panel.key])
  return { state: { ...state, panels: { ...state.panels, [panel.key]: { focus: button.id, at: kept.at } } }, act: button.press }
}

// ---- Acts ----

function openKeyboard(entry: KeyboardState["entry"], draft: string): KeyboardState {
  return { entry, draft, caret: draft.length, layer: "letters", key: draft === "" ? "k:q" : "k:done" }
}

/** Perform an act: the one place a press becomes a change or a request. */
export function perform(state: State, act: Act | undefined, model: Model, out: Request[]): State {
  if (act === undefined) return state
  switch (act.t) {
    case "none":
      return state
    case "send":
      out.push(...act.requests)
      return state
    case "open": {
      const key = pageKey(act.page)
      const { [key]: _dropped, ...panels } = state.panels
      return { ...state, pages: [...state.pages, act.page], panels, keyboard: undefined }
    }
    case "close":
      if (state.sheet !== undefined) return { ...state, sheet: undefined }
      return state.pages.length > 0 ? { ...state, pages: state.pages.slice(0, -1) } : state
    case "home":
      return { ...state, pages: [], sheet: undefined }
    case "type":
      return { ...state, keyboard: openKeyboard(act.entry, act.draft ?? "") }
    case "ask":
      return act.ask.buttons.length === 0 ? { ...state, ask: undefined } : { ...state, ask: { spec: act.ask, at: 0 } }
    case "sheet": {
      const { [`sheet:${act.sheet}`]: _dropped, ...panels } = state.panels
      return { ...state, sheet: { kind: act.sheet }, panels }
    }
    case "set":
      out.push(openSet(act.set))
      // The set takes the window: a list's question that is out is asked again later.
      return { ...state, set: act.set, depth: 2, lists: state.lists.asked === undefined ? state.lists : { ...state.lists, asked: undefined } }
    case "sort": {
      if (state.set === undefined) return state
      const { systemId: _was, gameId: _game, ...rest } = state.set
      const systemId = act.systemId === undefined ? state.set.systemId : act.systemId ?? undefined
      const next = { ...rest, order: act.order ?? state.set.order, ...(systemId === undefined ? {} : { systemId }), at: 0, first: 0, low: 0, moving: true, pill: state.set.pill }
      out.push(openSet(next))
      return { ...state, set: next }
    }
    case "detail":
      return { ...state, detail: { game: act.game, at: 0 } }
    case "lobby": {
      const shelves = shelvesOf(state.lists, model)
      return shelves.length === 0 ? state : { ...state, depth: 2, shelf: shelves.some(shelf => shelf.key === state.shelf) ? state.shelf : shelves[0]!.key }
    }
    case "candidate":
      return act.rowId === "" ? { ...state, candidate: undefined } : { ...state, candidate: { rowId: act.rowId, at: act.at } }
    case "scope":
      return { ...state, scope: act.at }
    case "motion":
      out.push({ _tag: "RememberPreference", key: MOTION_KEY, value: act.still ? "reduced" : "full" })
      return { ...state, still: act.still }
    case "onboarding":
      return { ...state, onboarding: { step: act.step, started: act.started ?? state.onboarding.started, finishing: act.finishing ?? false } }
    case "identity":
      return { ...state, identity: act.flow }
    case "moving":
      return { ...state, overlay: { ...state.overlay, moving: { ...state.overlay.moving, [act.controlId]: act.value } } }
    case "leaving":
      return { ...state, overlay: { ...state.overlay, leaving: true } }
    case "seq": {
      let next = state
      for (const inner of act.acts) next = perform(next, inner, model, out)
      return next
    }
  }
}

export const pageKey = (page: State["pages"][number]): string => (page.p === "provider" ? `page:provider:${page.id}` : `page:${page.p}`)

// ---- The curtain ----

function curtainPress(state: State, spec: CurtainSpec, button: SurfaceButton, p: Plan): Step {
  if (button === "back") return spec.back === "swallow" ? { state } : { state, act: spec.back }
  if (button === "menu" || button === "options" || button === "system") return { state }
  const layout = curtainLayout(spec, p)
  const at = Math.max(0, Math.min(state.curtain.at, spec.buttons.length - 1))
  if (button === "confirm") {
    const focused = spec.buttons[at]
    return focused === undefined || focused.dim === true ? { state } : { state, act: focused.act }
  }
  const along = layout.stack ? (button === "up" ? -1 : button === "down" ? 1 : 0) : button === "left" ? -1 : button === "right" ? 1 : 0
  if (along !== 0 && spec.buttons[at + along] !== undefined) return { state: { ...state, curtain: { ...state.curtain, at: at + along } } }
  // A press no button took moves the words, when there are more than the window shows.
  if (button === "up" || button === "down") {
    const most = Math.max(0, layout.lines.length - layout.room)
    const line = Math.max(0, Math.min(most, state.curtain.line + (button === "up" ? -1 : 1)))
    if (line !== state.curtain.line) return { state: { ...state, curtain: { ...state.curtain, line } } }
  }
  return { state }
}

// ---- The keyboard ----

function keyboardPress(state: State, keyboard: KeyboardState, button: SurfaceButton, p: Plan): Step {
  const heading = entryHeading(keyboard.entry)
  const grid = keyGrid(keyboard.layer, heading.clearLabel)
  if (button === "back") return { state: { ...state, keyboard: undefined } }
  if (button === "menu") return commit(state, keyboard)
  if (button === "options" || button === "system") return { state }
  if (button !== "confirm") {
    const next = keyMove(grid, keyboard.key, button as Direction, p.keyW, p.keyH, p.keyGap)
    return next === keyboard.key ? { state } : { state: { ...state, keyboard: { ...keyboard, key: next } } }
  }
  return pressKey(state, keyboard, keyAt(grid, keyboard.key).id)
}

function pressKey(state: State, keyboard: KeyboardState, id: string): Step {
  const heading = entryHeading(keyboard.entry)
  const grid = keyGrid(keyboard.layer, heading.clearLabel)
  const key = grid.find(candidate => candidate.id === id)
  if (key === undefined) return { state }
  const lit = { ...keyboard, key: key.id }
  const { draft } = keyboard
  switch (key.key.kind) {
    case "char":
    case "space": {
      if (heading.maxLength !== undefined && [...draft].length >= heading.maxLength) return { state: { ...state, keyboard: lit } }
      const next = draft + (key.key.kind === "space" ? " " : key.key.char)
      return { state: { ...state, keyboard: { ...lit, draft: next, caret: next.length } } }
    }
    case "delete": {
      const next = [...draft].slice(0, -1).join("")
      return { state: { ...state, keyboard: { ...lit, draft: next, caret: next.length } } }
    }
    case "layer": {
      const to = keyGrid(key.key.layer, heading.clearLabel)
      return { state: { ...state, keyboard: { ...keyboard, layer: key.key.layer, key: keyInPlace(key, to) } } }
    }
    case "clear": {
      const act = clearEntry(keyboard.entry)
      return act === undefined ? { state: { ...state, keyboard: lit } } : { state: { ...state, keyboard: undefined }, act }
    }
    case "done":
      return commit(state, lit)
  }
}

/** Done, or the menu button: the draft goes where its entry says. With nothing valid to send, the keyboard stays. */
function commit(state: State, keyboard: KeyboardState): Step {
  const act = commitEntry(keyboard.entry, keyboard.draft)
  return act === undefined ? { state: { ...state, keyboard } } : { state: { ...state, keyboard: undefined }, act }
}

// ---- Presses ----

type Top = "curtain" | "keyboard" | "sheet" | "screen"

/** The layer that holds focus. */
export function topLayer(state: State, model: Model): Top {
  if (curtainOf(state, model) !== undefined) return "curtain"
  if (state.keyboard !== undefined) return "keyboard"
  if (state.sheet !== undefined) return "sheet"
  return "screen"
}

function pressed(state: State, button: SurfaceButton, model: Model, out: Request[]): State {
  const p = plan(state.size.width, state.size.height)
  const done = (step: Step): State => perform(step.state, step.act, model, out)
  const curtain = curtainOf(state, model)
  if (curtain !== undefined) return done(curtainPress(state, curtain, button, p))
  if (state.keyboard !== undefined) return done(keyboardPress(state, state.keyboard, button, p))
  if (state.sheet !== undefined) {
    // The button that opened a sheet closes it too.
    if ((button === "options" && state.sheet.kind !== "menu") || (button === "menu" && state.sheet.kind === "menu")) return { ...state, sheet: undefined }
    return done(panelPress(state, sheetPanel(state.sheet.kind, state, model), button, p))
  }
  const what = shown(state, model)
  const panel = screenPanel(state, model)
  if (panel !== undefined) {
    // Korri was asked to close the overlay: the focus still moves, and nothing is asked twice.
    if (what === "overlay" && state.overlay.leaving && button !== "up" && button !== "down") return state
    if (button === "system" && what === "page" && state.pages[state.pages.length - 1]?.p !== "settings") return perform(state, { t: "open", page: { p: "settings" } }, model, out)
    return done(panelPress(state, panel, button, p))
  }
  // The library.
  if (button === "menu") return perform(state, { t: "sheet", sheet: "menu" }, model, out)
  if (button === "system") return perform(state, { t: "open", page: { p: "settings" } }, model, out)
  if (what === "detail") {
    if (button === "options") {
      const game = detailGame(state, model)
      return game !== undefined && detailButtons(game, model).some(candidate => candidate.id === "d:more") ? perform(state, { t: "sheet", sheet: "actions" }, model, out) : state
    }
    return done(pressDetail(state, button, model, p))
  }
  if (what === "set" && state.set !== undefined) {
    if (button === "options") return setSorts(state.set) ? perform(state, { t: "sheet", sheet: "sort" }, model, out) : state
    return done(pressSet(state, state.set, button, model, p))
  }
  if (button === "options") return state
  if (homeOf(state.lists, model.publishedAt).pick === undefined) return done(emptyPress(state, button, model))
  return done(pressHome(state, button, model))
}

// ---- The library with no pick: empty, loading, or unread ----

export interface EmptyButton {
  readonly id: string
  readonly label: string
  readonly icon: string
  readonly act: Act
}

/** What an empty library offers: Find games, then Open settings; a library Korri could not read offers Try again. */
export function emptyButtons(model: Model): readonly EmptyButton[] {
  const settings: EmptyButton = { id: "e:settings", label: "Open settings", icon: "gear", act: { t: "open", page: { p: "settings" } } }
  if (model.catalog._tag === "Error") return [{ id: "e:reload", label: "Try again", icon: "again", act: { t: "send", requests: [{ _tag: "Reload" }] } }, settings]
  if (model.catalog._tag === "Loading") return []
  return [
    ...(model.store === undefined ? [] : [{ id: "e:find", label: "Find games", icon: "plus", act: { t: "seq", acts: [{ t: "send", requests: [{ _tag: "OpenStore" }] }, { t: "open", page: { p: "store" } }] } } as EmptyButton]),
    settings,
  ]
}

export const emptyAt = (state: State, model: Model): number => Math.max(0, Math.min(state.panels.empty?.at ?? 0, emptyButtons(model).length - 1))

function emptyPress(state: State, button: SurfaceButton, model: Model): Step {
  const buttons = emptyButtons(model)
  const p = plan(state.size.width, state.size.height)
  if (state.onMenu || buttons.length === 0) {
    if (button === "confirm") return { state: { ...state, onMenu: true }, act: { t: "sheet", sheet: "menu" } }
    if ((button === "down" || button === "back") && buttons.length > 0) return { state: { ...state, onMenu: false } }
    return { state }
  }
  const at = emptyAt(state, model)
  if (button === "confirm") return { state, act: buttons[at]!.act }
  if (button === "up" && (!p.stack || at === 0)) return { state: { ...state, onMenu: true } }
  const along = p.stack ? (button === "up" ? -1 : button === "down" ? 1 : 0) : button === "left" ? -1 : button === "right" ? 1 : 0
  if (along !== 0 && buttons[at + along] !== undefined) return { state: { ...state, panels: { ...state.panels, empty: { focus: buttons[at + along]!.id, at: at + along } } } }
  return { state }
}

// ---- Taps ----

function activated(state: State, target: string, at: number | undefined, released: boolean, model: Model, out: Request[]): State {
  const p = plan(state.size.width, state.size.height)
  const done = (step: Step | undefined): State => (step === undefined ? state : perform(step.state, step.act, model, out))
  const curtain = curtainOf(state, model)
  // The way back for a finger: the chip, a tap beside a sheet or over the keyboard's field.
  if (target === BACK_TARGET) return curtain !== undefined ? state : pressed(state, "back", model, out)
  if (curtain !== undefined) {
    if (!target.startsWith("x:")) return state
    const index = Number(target.slice(2))
    const button = curtain.buttons[index]
    if (button === undefined || button.dim === true) return state
    return perform({ ...state, curtain: { ...state.curtain, at: index } }, button.act, model, out)
  }
  if (state.keyboard !== undefined) return target.startsWith("k:") ? done(pressKey(state, state.keyboard, target)) : state
  if (state.sheet !== undefined) return done(panelTap(state, sheetPanel(state.sheet.kind, state, model), target, at, released))
  const what = shown(state, model)
  const panel = screenPanel(state, model)
  if (panel !== undefined) return what === "overlay" && state.overlay.leaving ? state : done(panelTap(state, panel, target, at, released))
  if (what === "detail") {
    const game = detailGame(state, model)
    const index = game === undefined ? -1 : detailButtons(game, model).findIndex(button => button.id === target)
    if (index < 0 || state.detail === undefined) return state
    return done(pressDetail({ ...state, detail: { ...state.detail, at: index } }, "confirm", model, p))
  }
  if (what === "set" && state.set !== undefined) return done(tapSet(state, state.set, target, model))
  if (target === "menu") return perform({ ...state, onMenu: true }, { t: "sheet", sheet: "menu" }, model, out)
  if (homeOf(state.lists, model.publishedAt).pick === undefined) {
    const index = emptyButtons(model).findIndex(button => button.id === target)
    return index < 0 ? state : perform({ ...state, onMenu: false, panels: { ...state.panels, empty: { focus: target, at: index } } }, emptyButtons(model)[index]!.act, model, out)
  }
  return done(tapHome(state, target, model))
}

// ---- Korri published ----

function published(state: State, model: Model): State {
  let next = state
  // A fresh failure with the key rejected reopens the keyboard for that network, once.
  const attempt = lastAttempt(model)
  if (!sameAttempt(state.attempt, attempt)) {
    const again = next.keyboard === undefined ? rejectedEntry(state.attempt, model) : undefined
    next = { ...next, attempt, ...(again === undefined ? {} : { keyboard: openKeyboard(again, "") }) }
  }
  // A setting's keyboard closes when Korri stops offering the text it edits.
  const entry = next.keyboard?.entry
  if (entry?.e === "setting") {
    const places = model.launchSettings !== undefined && model.launchSettings._tag !== "Closed" ? model.launchSettings.places.flatMap(place => place.groups) : []
    const kind = [...model.settings, ...places].flatMap(group => group.items).find(item => item.id === entry.settingId)?.interaction?.kind
    if (kind !== "text" && kind !== "sensitiveText" && kind !== "number") next = { ...next, keyboard: undefined }
  }
  if (model.onboarding === undefined && (next.onboarding.step !== 0 || next.onboarding.finishing)) next = { ...next, onboarding: { step: 0, started: false, finishing: false } }
  if (model.onboarding !== undefined) {
    const most = onboardingSteps(model).length - 1
    if (next.onboarding.step > most) next = { ...next, onboarding: { ...next.onboarding, step: most } }
  }
  // The overlay starts afresh each time Korri presents it; a range Korri caught up with stops moving.
  if (model.presentation.kind !== "gameplay-overlay") {
    if (next.overlay.leaving || Object.keys(next.overlay.moving).length > 0 || next.panels.overlay !== undefined) {
      const { overlay: _dropped, ...panels } = next.panels
      next = { ...next, overlay: { moving: {}, leaving: false }, panels }
    }
  } else {
    const controls = [...model.presentation.controls, ...model.presentation.groups.flatMap(group => group.controls)]
    const moving = Object.fromEntries(Object.entries(next.overlay.moving).filter(([id, value]) => {
      const control = controls.find(candidate => candidate.id === id)
      return control !== undefined && control.interaction.kind === "range" && control.interaction.value !== value
    }))
    if (Object.keys(moving).length !== Object.keys(next.overlay.moving).length) next = { ...next, overlay: { ...next.overlay, moving } }
  }
  // A page whose subject Korri no longer publishes closes.
  const pages = next.pages.filter(page => !(page.p === "identity" && model.identityManagement === undefined) && !((page.p === "store" || page.p === "provider" || page.p === "entry") && model.store === undefined) && !(page.p === "wifi" && model.network === undefined))
  if (pages.length !== next.pages.length) next = { ...next, pages }
  if (model.launchSettings === undefined || model.launchSettings._tag === "Closed") {
    if (next.scope !== 0) next = { ...next, scope: 0 }
  }
  // A new invite is announced once, by a notice that takes no focus.
  const invites = model.invites ?? []
  const fresh = invites.find(invite => !next.invitesSeen.includes(invite.id))
  if (fresh !== undefined) next = { ...next, invitesSeen: invites.map(invite => invite.id), notice: { id: fresh.id, left: NOTICE_TICKS } }
  else if (next.invitesSeen.some(id => !invites.some(invite => invite.id === id))) next = { ...next, invitesSeen: invites.map(invite => invite.id) }
  if (next.notice !== undefined && !invites.some(invite => invite.id === next.notice!.id)) next = { ...next, notice: undefined }
  // Back from a game: what was played changed, so the home's lists are read again.
  const browsing = model.status._tag === "Browsing"
  if (browsing !== next.wasBrowsing) next = { ...next, wasBrowsing: browsing, lists: browsing ? staleLists(next.lists) : next.lists }
  return next
}

// ---- Settle ----

function settle(state: State, model: Model, out: Request[], tick: boolean): State {
  let next = state
  const p = plan(next.size.width, next.size.height)
  // The set's window; else the home's lists.
  if (next.set !== undefined) {
    const settled = settleSet(next.set, model.catalog, p)
    if (settled.request !== undefined) out.push(settled.request)
    else if (tick && setWindow(next.set, model.catalog) === undefined && model.catalog._tag === "Ready") out.push(openSet(next.set))
    if (settled.set !== next.set) next = { ...next, set: settled.set }
    // The games the home keeps are as the set's window has them now.
    if (model.catalog._tag === "Ready") {
      const fresh = refresh(next.lists, model.catalog.window)
      if (fresh.lists !== next.lists) next = { ...next, lists: fresh.lists }
    }
  } else if (model.presentation.kind === "catalog") {
    const ticked = tick ? tickLists(next.lists) : { lists: next.lists }
    const settled = "request" in ticked && ticked.request !== undefined ? ticked : settleLists(ticked.lists, model, true)
    if (settled.request !== undefined) out.push(settled.request)
    if (settled.lists !== next.lists) next = { ...next, lists: settled.lists }
  }
  // The home's cursor stays on what is there.
  const shelves = shelvesOf(next.lists, model)
  if (next.depth === 2 && shelves.length === 0) next = { ...next, depth: 0 }
  if (next.depth === 2 && shelves.length > 0 && !shelves.some(shelf => shelf.key === next.shelf)) next = { ...next, shelf: shelves[0]!.key }
  if (homeOf(next.lists, model.publishedAt).pick === undefined && next.depth !== 0) next = { ...next, depth: 0 }
  // The curtain: a new one puts focus on its first button.
  const curtain = curtainOf(next, model)
  const key = curtain?.key ?? ""
  if (key !== next.curtain.key) next = { ...next, curtain: { key, at: curtain?.first ?? 0, line: 0 } }
  // Each panel's focus, held on a row that exists.
  for (const panel of [screenPanel(next, model), next.sheet === undefined ? undefined : sheetPanel(next.sheet.kind, next, model)]) {
    if (panel === undefined) continue
    const kept = next.panels[panel.key]
    const focus = panelFocus(panel, kept)
    if (kept === undefined || kept.focus !== focus.focus || kept.at !== focus.at) next = { ...next, panels: { ...next.panels, [panel.key]: focus } }
  }
  // The focused game, for Korri's actions on it.
  const what = shown(next, model)
  if (what === "home" || what === "set" || what === "detail") {
    const focused = focusedGame(next, model)?.id ?? null
    if (focused !== next.reported) {
      out.push(focused === null ? { _tag: "FocusGame" } : { _tag: "FocusGame", gameId: focused })
      next = { ...next, reported: focused }
    }
  }
  return next
}

export function update(state: State, message: Message, model: Model): { state: State; requests: Request[] } {
  const out: Request[] = []
  let next = state
  let tick = false
  switch (message._tag) {
    case "Resized": {
      const size = { width: Math.max(1, message.width), height: Math.max(1, message.height) }
      if (size.width !== state.size.width || size.height !== state.size.height) next = { ...next, size }
      break
    }
    case "Tick": {
      tick = true
      const drift = message.idleMs >= DRIFT_AFTER_MS && !next.still
      if (drift !== next.drift) next = { ...next, drift }
      if (next.notice !== undefined) next = { ...next, notice: next.notice.left <= 1 ? undefined : { ...next.notice, left: next.notice.left - 1 } }
      break
    }
    case "KorriPublished":
      next = published(next, model)
      break
    case "PreferenceStored":
      break
    case "Pressed":
      // A button brings every scrolled shelf back to its focus (ADR 0055).
      if (next.touched || next.drift || next.scrolled !== undefined) next = { ...next, touched: false, drift: false, scrolled: undefined }
      next = pressed(next, message.button, model, out)
      break
    case "Activated":
      if (!next.touched || next.drift) next = { ...next, touched: true, drift: false }
      next = activated(next, message.target, message.at, message.released === true, model, out)
      break
    case "Dragged":
      if (!Number.isInteger(message.by) || message.by === 0) break
      if (!next.touched || next.drift) next = { ...next, touched: true, drift: false }
      if (message.region === "grid" && next.set !== undefined && topLayer(next, model) === "screen" && shown(next, model) === "set") {
        const set = dragSet(next.set, message.by, model, plan(next.size.width, next.size.height))
        if (set !== next.set) next = { ...next, set }
      } else if ((message.region === "row" || message.region.startsWith("shelf:")) && topLayer(next, model) === "screen" && shown(next, model) === "home") {
        next = dragHome(next, message.region, message.by, model)
      }
      break
    case "Touched":
      if (!next.touched) next = { ...next, touched: true }
      break
  }
  next = settle(next, model, out, tick)
  return { state: next, requests: out }
}
