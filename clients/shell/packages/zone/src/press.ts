/**
 * The shell's presses in Zone's words. In the browser the portal turned a
 * direction into DOM focus (clients/portal/src/input/spatial-focus.ts) and a
 * confirm into a click, and Zone's pages reported `Focused…` and `Chose…`
 * facts. Here focus lives in the model: this module reads where the cursor
 * is and turns each press into the fact the page would have reported, then
 * Zone's own `update` (src/state/surface.ts) decides what it means.
 *
 * Focus moves by regions (contract 8.4, src/focus.ts): inside a region a
 * direction goes to the target the plan draws nearest that way
 * (src/plan/screen.ts `nearest`: tabs, rows of buttons that may wrap,
 * dialogs whose buttons may stack, the keys); leaving a region follows the
 * exits src/focus.ts declares. A list is a column: up and down walk its
 * rows, left and right step a choice. Back, menu, options and system are
 * Zone's own host buttons.
 */
import type { SurfaceButton, SurfaceModel } from "../../../../../contracts/generated/surface"
import { findKey, keyAt, keyGrid, type ZoneGridKey } from "./keys"
import { buttonRow, dialogButtons, nearest, tabsPlan, type ZoneBox, type ZoneScreen } from "./plan/screen"
import { entryHeading, optionsPlan, ZONE_OPTIONS_SYSTEM } from "./plan/options"
import { launchPlan, launchSlices, ZONE_LAUNCH_SCOPE_ROW } from "./plan/launch-settings"
import { runWithPlan } from "./plan/options-run-with"
import { storeRows } from "./rows"
import { launchChoices, playtime, type ZoneFindKey } from "./state/games"
import { dayOf, hoursAndMinutes, launchLabel } from "./view"
import { identityCardCount, retiredKeyAt } from "./state/identity"
import { bandGames, cardCount, topLayer, ZONE_TABS, type ZoneNavMessage } from "./state/navigation"
import { onboardingTargets, TARGET } from "./state/onboarding"
import { optionsLayer } from "./state/options"
import { overlayControls } from "./state/overlay"
import { problemButtons, statusButtonCount } from "./state/status"
import { withLook, zoneShown, type ZoneSurfaceMessage, type ZoneSurfaceState } from "./state/surface"

type Direction = "up" | "down" | "left" | "right"
type Send = (message: ZoneSurfaceMessage) => void

const isDirection = (button: SurfaceButton): button is Direction =>
  button === "up" || button === "down" || button === "left" || button === "right"

const clamp = (value: number, low: number, high: number): number => Math.min(Math.max(value, low), high)

/** Step `at` along `count` items, stopping at the ends. */
const along = (at: number, count: number, direction: -1 | 1): number => clamp(at + direction, 0, Math.max(0, count - 1))

/** The step a horizontal direction means. */
const sideways = (button: Direction): -1 | 1 => (button === "left" ? -1 : 1)

/**
 * Route one press. `state` is the state before it; `send` runs Zone's
 * update with each message in turn (a press can be one message or none).
 */
export function press(state: ZoneSurfaceState, button: SurfaceButton, korri: SurfaceModel, screen: ZoneScreen, send: Send): void {
  SCREEN = screen
  switch (button) {
    case "back":
      return send({ _tag: "PressedBack" })
    case "menu":
      return send({ _tag: "PressedMenu" })
    case "system":
      return send({ _tag: "PressedSystem" })
    case "options":
      return send({ _tag: "PressedOptions" })
    case "up":
    case "down":
    case "left":
    case "right":
    case "confirm":
      break
  }
  const nav = state.nav
  // The results tally holds every press (navigation's rule).
  if (nav.tally !== undefined && !nav.tally.leaving && zoneShown(state, korri) === "games") {
    return send({ _tag: "Games", message: { _tag: "PressedTally" } })
  }
  // Korri's status layer covers the page under Busy, Problem and Confirm.
  if (statusLayer(state, button, korri, send)) return
  switch (zoneShown(state, korri)) {
    case "paused":
      return paused(state, button, korri, send)
    case "join":
      return optionsPage(state, button, korri, send)
    case "onboarding":
      return onboarding(state, button, korri, send)
    case "runwith":
      return runWith(state, button, korri, send)
    case "launch":
      return launch(state, button, korri, send)
    case "store":
      return store(state, button, korri, send)
    case "games":
      return games(state, button, korri, send)
    case "options":
      return nav.zone === "tabs" ? tabs(state, button, korri, send) : optionsPage(state, button, korri, send)
    case "savedata":
      return nav.zone === "tabs" ? tabs(state, button, korri, send) : saveData(state, button, korri, send)
  }
}

// ---- The plan's geometry (src/plan/screen.ts) ----

/** The window's plan for the press under way (press() sets it first). */
let SCREEN: ZoneScreen

/** The target drawn nearest `direction` in a region of boxes; undefined when none. */
const step = (boxes: readonly ZoneBox[], at: number, direction: Direction): number | undefined => nearest(boxes, at, direction)

/** A dialog's buttons as ZoneDialog draws them. */
const dialogStep = (labels: readonly string[], at: number, direction: Direction) =>
  step(dialogButtons(labels, SCREEN).boxes, at, direction)

/** A row of buttons at a width, caps-1. */
const rowStep = (labels: readonly string[], width: number, at: number, direction: Direction) =>
  step(buttonRow(labels, width, 10 * SCREEN.a, SCREEN.a, SCREEN.look).boxes, at, direction)

/** The tabs as ZoneTabStrip places them; the page's tab is the banner. */
const TAB_LABELS = ["Games", "Options", "Save data"] as const
const tabStep = (active: number, at: number, direction: Direction) => step(tabsPlan(TAB_LABELS, active, SCREEN).boxes, at, direction)

/** The list's width on a page with the sky band (ZoneSkyBand). */
const skyList = (well: boolean) => SCREEN.width - 16 * SCREEN.a - (well ? 56 * SCREEN.a : 0)

/** The keys as ZoneKeyboard places them over the body. */
function keyBoxes(grid: readonly ZoneGridKey[]): ZoneBox[] {
  const a = SCREEN.a
  const height = SCREEN.height - SCREEN.topH
  const tight = height < 170 * a
  const pad = tight ? 4 * a : 7 * a
  const gap = tight ? 2 * a : 3 * a
  const slab = tight ? 18 * a : 24 * a
  const slant = tight ? 7 * a : 14 * a
  const panel = Math.min(SCREEN.width - 4 * a, 450 * a)
  const inner = panel - 2 * (pad + slant)
  const unit = Math.max(16 * a, Math.floor((inner - 11 * gap) / 12 / a) * a)
  const rows = Math.max(1, ...grid.map(key => key.row + 1))
  const keyH = Math.max(16 * a, Math.min(20 * a, Math.floor((height - 12 * a - 3 * pad - slab - (rows - 1) * gap - 4 * a) / rows / a) * a))
  return grid.map(key => ({ x: key.at * (unit + gap), y: key.row * (keyH + gap), w: key.units * unit + (key.units - 1) * gap, h: keyH }))
}

/** The status layer's buttons, when it holds the cursor. True when it took the press. */
function statusLayer(state: ZoneSurfaceState, button: Exclude<SurfaceButton, "back" | "menu" | "system" | "options">, korri: SurfaceModel, send: Send): boolean {
  const status = korri.status
  const holds = status._tag === "Busy" || status._tag === "Problem" || status._tag === "Confirm" || status._tag === "ChooseSave"
    || status._tag === "Paused" || status._tag === "Ended"
  if (!holds) return false
  const count = statusButtonCount(status)
  const focus = clamp(state.status.focus, 0, Math.max(0, count - 1))
  const message = (inner: Parameters<typeof statusMessage>[0]) => send({ _tag: "Status", message: inner })
  if (isDirection(button)) {
    // Busy's buttons stand in the strip (160 art px); the others in the dialog.
    const labels = statusLabels(status)
    const next = status._tag === "Busy" ? rowStep(labels, 160 * SCREEN.a, focus, button) : dialogStep(labels, focus, button)
    if (next !== undefined && count > 0) message({ _tag: "FocusedStatusButton", index: next })
    return true
  }
  if (button !== "confirm") return true
  switch (status._tag) {
    case "Busy": {
      const action = status.actions?.[focus]
      if (action !== undefined) message({ _tag: "ChoseBusyAction", actionId: action.id })
      return true
    }
    case "Problem": {
      const which = problemButtons(status)[focus]
      if (which !== undefined) message({ _tag: "ChoseProblemButton", button: which })
      return true
    }
    case "Confirm": {
      const action = status.actions[focus]
      if (action !== undefined) message({ _tag: "ChoseConfirmAction", actionId: action.id })
      return true
    }
    case "ChooseSave": {
      // One answer per side, then Korri's own actions; each is Korri's action id.
      const ids = [...status.sides.map(side => side.actionId), ...status.actions.map(action => action.id)]
      const id = ids[focus]
      if (id !== undefined) send({ _tag: "Status", message: { _tag: "ChoseConfirmAction", actionId: id } })
      return true
    }
    case "Paused": {
      const action = status.actions[focus]
      if (action !== undefined) message({ _tag: "ChosePausedAction", actionId: action.id })
      return true
    }
    case "Ended":
      message({ _tag: "ChoseEndedDismiss" })
      return true
  }
  return true
}

/** The status layer's button words, as src/view.ts statusView draws them. */
function statusLabels(status: SurfaceModel["status"]): string[] {
  switch (status._tag) {
    case "Busy":
      return (status.actions ?? []).map(action => action.label)
    case "Problem":
      return problemButtons(status).map(which => (which === "dismiss" ? "Dismiss" : "Retry"))
    case "Confirm":
    case "Paused":
      return status.actions.map(action => action.label)
    case "ChooseSave":
      return [...status.sides.map(side => `${side.device} · ${dayOf(side.endedAt)} · ${hoursAndMinutes(playtime(side.durationSeconds))}`), ...status.actions.map(action => action.label)]
    case "Ended":
      return ["Dismiss"]
    default:
      return []
  }
}

// A helper type for the status messages above.
function statusMessage(message: Extract<ZoneSurfaceMessage, { _tag: "Status" }>["message"]) {
  return message
}

/** The tabs on OPTIONS and SAVE DATA (GAMES has its own, beside FIND). */
function tabs(state: ZoneSurfaceState, button: Direction | "confirm", korri: SurfaceModel, send: Send): void {
  const nav = state.nav
  const at = ZONE_TABS.indexOf(nav.tabCursor)
  if (button === "confirm") return send({ _tag: "Games", message: { _tag: "ChoseTab", tab: nav.tabCursor } })
  const inside = tabStep(ZONE_TABS.indexOf(nav.tab), at, button)
  if (inside !== undefined) return send({ _tag: "Games", message: { _tag: "FocusedTab", tab: ZONE_TABS[inside]! } })
  if (button !== "down") return
  // SAVE DATA with Korri's backup shown: the slab under the tabs.
  if (nav.tab === "savedata" && korri.identityManagement?.status._tag === "BackupReady") {
    return send({ _tag: "SaveData", message: { _tag: "FocusedSecret" } })
  }
  switch (nav.tab) {
    case "games":
      return send({ _tag: "Games", message: { _tag: "FocusedCard", index: nav.card } })
    case "options": {
      const plan = optionsPlan(withLook(state, korri))
      const row = plan.rows.find(candidate => candidate.id === state.options.focused) ?? plan.rows[0]
      if (row !== undefined) send({ _tag: "Options", message: { _tag: "FocusedRow", rowId: row.id } })
      return
    }
    case "savedata":
      return send({ _tag: "SaveData", message: { _tag: "FocusedSaveCard", index: state.identity.card } })
  }
}

// ---- GAMES ----

function games(state: ZoneSurfaceState, button: Direction | "confirm", korri: SurfaceModel, send: Send): void {
  const nav = state.nav
  const go = (message: ZoneNavMessage) => send({ _tag: "Games", message })
  switch (topLayer(nav)) {
    case "keyboard":
      return keyboard(nav.find.layer, nav.find.key, undefined, button, id => go({ _tag: "FocusedKey", id }), key => {
        const found = findKey(key)
        if (found !== undefined) go({ _tag: "PressedKey", key: found })
      })
    case "gameopts":
      return gameOpts(state, button, korri, send)
    case "idle":
      // Any press wakes the attract (navigation drops the layer).
      return go({ _tag: "Woke" })
    case "none":
      break
    default:
      return
  }
  // Korri's catalog could not be read: RELOAD is the one button.
  if (korri.catalog._tag === "Error" && nav.zone !== "tabs") {
    if (button === "confirm") go({ _tag: "ChoseReload" })
    if (button === "up") go({ _tag: "FocusedTab", tab: nav.tab })
    return
  }
  const band = bandGames(nav, korri)
  const store = korri.store !== undefined
  const findShown = korri.catalog._tag === "Ready"
  switch (nav.zone) {
    case "tabs": {
      const at = ZONE_TABS.indexOf(nav.tabCursor)
      if (button === "confirm") return go({ _tag: "ChoseTab", tab: nav.tabCursor })
      const inside = tabStep(ZONE_TABS.indexOf(nav.tab), at, button)
      if (inside !== undefined) return go({ _tag: "FocusedTab", tab: ZONE_TABS[inside]! })
      if (button === "right" && findShown) return go({ _tag: "FocusedFind" })
      if (button === "down") {
        // The catalog could not be read: RELOAD stands under the tabs.
        if (korri.catalog._tag === "Error") return go({ _tag: "FocusedCard", index: nav.card })
        if (band.length > 0) return go({ _tag: "FocusedCard", index: nav.card })
        if (store) return go({ _tag: "FocusedGetGames" })
        if (findShown) return go({ _tag: "FocusedFind" })
      }
      return
    }
    case "find":
      if (button === "confirm") return go({ _tag: "ChoseFind" })
      if (button === "up") return go({ _tag: "FocusedTab", tab: nav.tab })
      if (button === "left") return store ? go({ _tag: "FocusedGetGames" }) : go({ _tag: "FocusedTab", tab: nav.tab })
      if (button === "down" && band.length > 0) return go({ _tag: "FocusedCard", index: nav.card })
      return
    case "get":
      if (button === "confirm") return go({ _tag: "ChoseGetGames" })
      if (button === "up") return go({ _tag: "FocusedTab", tab: nav.tab })
      if (button === "right" && findShown) return go({ _tag: "FocusedFind" })
      if (button === "down" && band.length > 0) return go({ _tag: "FocusedCard", index: nav.card })
      return
    case "cards": {
      if (band[nav.card] === undefined) {
        // A draft that matches nothing: NO GAMES, the cursor on FIND (or GET GAMES); it moves as from there.
        if (findShown) return games({ ...state, nav: { ...nav, zone: "find" } }, button, korri, send)
        if (store) return games({ ...state, nav: { ...nav, zone: "get" } }, button, korri, send)
        return
      }
      if (button === "left" || button === "right") {
        const next = along(nav.card, band.length, sideways(button))
        if (next !== nav.card) go({ _tag: "FocusedCard", index: next })
        return
      }
      if (button === "up") return store && nav.card === 0 ? go({ _tag: "FocusedGetGames" }) : go({ _tag: "FocusedTab", tab: nav.tab })
      if (button === "down") return go({ _tag: "FocusedAction", index: 0 })
      return go({ _tag: "ChoseCard", index: nav.card })
    }
    case "actions": {
      const game = band[nav.card]
      if (game === undefined) return
      const launches = launchChoices(game)
      if (isDirection(button)) {
        // The launch buttons, then OPTS., as the row draws them (it may wrap).
        const labels = [...launches.map(launchLabel), "Opts."]
        const next = rowStep(labels, SCREEN.width, Math.min(nav.action, labels.length - 1), button)
        if (next !== undefined) return go({ _tag: "FocusedAction", index: next })
        if (button === "up") return go({ _tag: "FocusedCard", index: nav.card })
        return
      }
      const launch = launches[nav.action]
      if (launch === undefined) return go({ _tag: "ChoseGameOpts" })
      return go(launch.locationId === undefined
        ? { _tag: "ChoseLaunch", gameId: game.id }
        : { _tag: "ChoseLaunch", gameId: game.id, locationId: launch.locationId })
    }
  }
}

/** GAME OPTS.: Korri's actions for the focused game (model.focusedGame, from FocusGame). */
function gameOpts(state: ZoneSurfaceState, button: Direction | "confirm", korri: SurfaceModel, send: Send): void {
  const nav = state.nav
  const go = (message: ZoneNavMessage) => send({ _tag: "Games", message })
  const game = bandGames(nav, korri)[nav.card]
  const actions = game !== undefined && korri.focusedGame?.gameId === game.id ? korri.focusedGame.actions : []
  if (nav.opts.asking !== undefined) {
    if (isDirection(button)) {
      const next = dialogStep(["No", "Yes"], nav.opts.answer, button)
      if (next !== undefined) go({ _tag: "FocusedAnswer", index: next })
      return
    }
    if (button === "confirm") go({ _tag: "AnsweredGameAction", yes: nav.opts.answer === 1 })
    return
  }
  if (button === "up" || button === "down") {
    const next = along(nav.opts.row, actions.length, button === "up" ? -1 : 1)
    if (next !== nav.opts.row) go({ _tag: "FocusedGameOpt", index: next })
    return
  }
  const action = actions[clamp(nav.opts.row, 0, Math.max(0, actions.length - 1))]
  if (button === "confirm" && action !== undefined && game !== undefined) {
    go({ _tag: "ChoseGameAction", gameId: game.id, actionId: action.id, enabled: action.enabled, destructive: action.destructive === true })
  }
}

// ---- Keyboards ----

/** One keyboard press: a direction moves the lit key, confirm presses it. */
function keyboard(
  layer: ZoneFindKey extends never ? never : Parameters<typeof keyGrid>[0],
  focused: string | undefined,
  clear: string | undefined,
  button: Direction | "confirm",
  focus: (id: string) => void,
  pressKey: (key: ZoneGridKey["key"]) => void,
): void {
  const grid = keyGrid(layer, clear)
  const from = keyAt(grid, focused)
  if (isDirection(button)) {
    const next = grid[step(keyBoxes(grid), grid.indexOf(from), button) ?? -1]
    if (next !== undefined && next.id !== focused) focus(next.id)
    else if (focused === undefined) focus(from.id)
    return
  }
  const key = grid.find(candidate => candidate.id === focused) ?? from
  pressKey(key.key)
}

// ---- OPTIONS (and first boot's join keyboard) ----

function optionsPage(state: ZoneSurfaceState, button: Direction | "confirm", korri: SurfaceModel, send: Send): void {
  const options = state.options
  const slices = withLook(state, korri)
  const message = (inner: Extract<ZoneSurfaceMessage, { _tag: "Options" }>["message"]) => send({ _tag: "Options", message: inner })
  rowsPage(state, options, optionsPlan(slices).rows, button, message, () => send({ _tag: "Games", message: { _tag: "FocusedTab", tab: state.nav.tab } }))
}

/** OPTIONS' rows, keyboard and dialog, shared with LAUNCH SETTINGS. */
function rowsPage(
  state: ZoneSurfaceState,
  options: ZoneSurfaceState["options"],
  rows: ReturnType<typeof optionsPlan>["rows"],
  button: Direction | "confirm",
  message: (inner: Extract<ZoneSurfaceMessage, { _tag: "Options" }>["message"]) => void,
  above: (() => void) | undefined,
): void {
  switch (optionsLayer(options)) {
    case "keyboard": {
      const board = options.keyboard!
      const heading = entryHeading(board.entry)
      return keyboard(board.layer, board.focusedKey, heading.clearLabel, button, keyId => message({ _tag: "FocusedKey", keyId }), key => {
        if (key.kind === "clear") return message({ _tag: "PressedClear" })
        message({ _tag: "PressedKey", key })
      })
    }
    case "dialog": {
      const dialog = options.dialog!
      if (isDirection(button)) {
        const next = dialogStep(["No", dialog.confirm.confirmLabel], dialog.focused, button)
        if (next !== undefined) message({ _tag: "FocusedDialogButton", index: next === 0 ? 0 : 1 })
        return
      }
      if (button === "confirm") message({ _tag: "PressedDialogButton", index: dialog.focused })
      return
    }
    case "none":
      break
  }
  // The system button leaves the cursor on the SYSTEM group: its first row.
  const focused = options.focused === ZONE_OPTIONS_SYSTEM ? (rows.find(row => row.group === ZONE_OPTIONS_SYSTEM) ?? rows[0])?.id : options.focused
  const at = rows.findIndex(row => row.id === focused)
  const row = rows[at]
  if (button === "up" || button === "down") {
    if (row === undefined) {
      if (button === "down" && rows[0] !== undefined) message({ _tag: "FocusedRow", rowId: rows[0].id })
      return
    }
    if (button === "up" && at === 0) return above?.()
    const next = rows[along(at, rows.length, button === "up" ? -1 : 1)]
    if (next !== undefined && next.id !== row.id) message({ _tag: "FocusedRow", rowId: next.id })
    return
  }
  if (row === undefined) return
  if (button === "confirm") return message({ _tag: "PressedRow", rowId: row.id })
  const interaction = row.item.interaction
  if (interaction?.kind === "choice" || interaction?.kind === "number") {
    message({ _tag: "SteppedRow", rowId: row.id, direction: sideways(button) })
  }
  void state
}

// ---- LAUNCH SETTINGS ----

function launch(state: ZoneSurfaceState, button: Direction | "confirm", korri: SurfaceModel, send: Send): void {
  const settings = korri.launchSettings
  if (settings === undefined || settings._tag === "Closed") return
  const plan = launchPlan(settings, korri.settingsStatus, state.launchScope)
  const slices = launchSlices(korri, state.launchScope)
  const rows = slices === undefined || !plan.rows ? [] : optionsPlan(slices).rows
  const actions = plan.actions
  const message = (inner: Extract<ZoneSurfaceMessage, { _tag: "Launch" }>["message"]) => send({ _tag: "Launch", message: inner })
  // On Korri's actions under the rows: left and right walk them, up goes back to the rows.
  if (rows.length === 0 && state.launchAction === undefined && optionsLayer(state.launch) === "none") {
    const first = actions.find(action => action.enabled)
    if (first !== undefined && button === "confirm") return send({ _tag: "LaunchAction", actionId: first.id })
    if (first !== undefined) send({ _tag: "FocusedLaunchAction", actionId: first.id })
    // The view already lights the first: a direction moves on from it.
    if (!isDirection(button) || first === undefined) return
    return launch({ ...state, launchAction: first.id }, button, korri, send)
  }
  const onAction = state.launchAction !== undefined && optionsLayer(state.launch) === "none"
  if (onAction) {
    const at = Math.max(0, actions.findIndex(action => action.id === state.launchAction))
    if (isDirection(button)) {
      // The buttons as the row draws them (it may wrap).
      const inside = rowStep(actions.map(action => action.label), skyList(false), at, button)
      if (inside !== undefined) return send({ _tag: "FocusedLaunchAction", actionId: actions[inside]!.id })
    }
    if (button === "left" || button === "right" || button === "down") return
    if (button === "up") {
      // The row the list holds, else its last.
      const back = rows.find(entry => entry.id === state.launch.focused) ?? rows.at(-1)
      if (back !== undefined) message({ _tag: "FocusedRow", rowId: back.id })
      return
    }
    if (button === "confirm") send({ _tag: "LaunchAction", actionId: state.launchAction! })
    return
  }
  const at = rows.findIndex(row => row.id === state.launch.focused)
  const toActions = () => {
    const first = actions.find(action => action.enabled) ?? actions[0]
    if (first !== undefined) send({ _tag: "FocusedLaunchAction", actionId: first.id })
  }
  if (optionsLayer(state.launch) === "none" && button === "down" && (rows.length === 0 || at === rows.length - 1)) return toActions()
  if (optionsLayer(state.launch) === "none" && rows.length === 0) return
  rowsPage(state, state.launch, rows, button, message, undefined)
  void ZONE_LAUNCH_SCOPE_ROW
}

// ---- RUN WITH ----

function runWith(state: ZoneSurfaceState, button: Direction | "confirm", korri: SurfaceModel, send: Send): void {
  const choice = korri.runnerChoice
  if (choice === undefined || choice._tag === "Closed") return
  const plan = runWithPlan(choice)
  const ids = plan.ids
  const at = Math.max(0, ids.indexOf(state.runWith.focused ?? ids[0] ?? ""))
  const focused = ids[at]
  const message = (inner: Extract<ZoneSurfaceMessage, { _tag: "RunWith" }>["message"]) => send({ _tag: "RunWith", message: inner })
  if (isDirection(button)) {
    // A route's buttons stand in one row under it, the choice's own in one row at the foot:
    // left and right walk a row, up and down walk the rows.
    const rowOf = (id: string) => runWithRow(choice, plan, id)
    const here = focused === undefined ? 0 : rowOf(focused)
    // Inside a row of buttons: the button drawn that way (the row may wrap).
    const same = ids.filter(id => rowOf(id) === here)
    const words = new Map([...plan.routes.flatMap(entry => entry.route.actions), ...choice.actions].map(action => [action.id, action.label]))
    const inside = same.length > 1 ? rowStep(same.map(id => words.get(id) ?? ""), skyList(false), same.indexOf(focused ?? ""), button) : undefined
    if (inside !== undefined) {
      if (same[inside] !== focused) message({ _tag: "FocusedRow", rowId: same[inside]! })
      return
    }
    if (button === "left" || button === "right") return
    const rows = [...new Set(ids.map(rowOf))]
    const target = rows[along(rows.indexOf(here), rows.length, button === "up" ? -1 : 1)]
    // No row that way: the cursor stays.
    if (target === here) return
    const next = ids.find(id => rowOf(id) === target)
    if (next !== undefined && next !== focused) message({ _tag: "FocusedRow", rowId: next })
    return
  }
  if (focused === undefined) return
  if (state.runWith.focused === undefined) message({ _tag: "FocusedRow", rowId: focused })
  if (plan.actions.has(focused)) message({ _tag: "PressedAction", actionId: focused })
}

/** Which drawn row an id stands in on RUN WITH: the route rows, each route's buttons, the foot's buttons. */
export function runWithRow(choice: Parameters<typeof runWithPlan>[0], plan: ReturnType<typeof runWithPlan>, id: string): number {
  let row = 0
  for (const orphan of plan.orphans) {
    if (id === `saved:${orphan.label}`) return row
    row++
  }
  for (const entry of plan.routes) {
    if (id === entry.id) return row
    row++
    if (entry.route.actions.length > 0) {
      if (entry.route.actions.some(action => action.id === id)) return row
      row++
    }
  }
  return choice.actions.some(action => action.id === id) ? row : -1
}

// ---- GET GAMES ----

function store(state: ZoneSurfaceState, button: Direction | "confirm", korri: SurfaceModel, send: Send): void {
  const model = state.store
  const message = (inner: Extract<ZoneSurfaceMessage, { _tag: "Store" }>["message"]) => send({ _tag: "Store", message: inner })
  if (model.keyboard !== undefined) {
    return keyboard(model.keyboard.layer, model.keyboard.key, undefined, button, id => message({ _tag: "FocusedKey", id }), key => {
      const found = findKey(key)
      if (found !== undefined) message({ _tag: "PressedKey", key: found })
    })
  }
  const shape = storeRows(state, korri)
  // Korri's question or trouble: its buttons in a row.
  if (shape.dialog.length > 0) {
    const at = clamp(model.dialog, 0, shape.dialog.length - 1)
    if (isDirection(button)) {
      const next = dialogStep(shape.dialogActions.map(action => action.label), at, button)
      if (next !== undefined) message({ _tag: "FocusedDialogButton", index: next })
      return
    }
    const action = shape.dialog[at]
    if (button === "confirm" && action !== undefined) message({ _tag: "PressedAction", actionId: action })
    return
  }
  const ids = shape.ids
  if (ids.length === 0) return
  // With nothing focused yet the view lights the first live row: the cursor starts there.
  const at = ids.findIndex(entry => entry.id === model.focused)
  const here = ids[at] ?? shape.rows.flatMap((drawn, index) => drawn.focused ? ids.filter(entry => entry.row === index) : [])[0] ?? ids[0]!
  if (button === "confirm") {
    if (at < 0) message({ _tag: "FocusedRow", rowId: here.id })
    return message({ _tag: "PressedRow", rowId: here.id })
  }
  // Inside a row of buttons: the button drawn that way (the row may wrap).
  const same = ids.filter(entry => entry.row === here.row)
  const words = shape.rows[here.row]?.buttons.map(drawn => drawn.label) ?? []
  const inside = same.length > 1 && words.length === same.length ? rowStep(words, skyList(shape.well), same.indexOf(here), button) : undefined
  if (inside !== undefined) {
    if (same[inside]!.id !== here.id) message({ _tag: "FocusedRow", rowId: same[inside]!.id })
    return
  }
  if (button === "left" || button === "right") return
  const rows = [...new Set(ids.map(entry => entry.row))]
  const target = rows[along(rows.indexOf(here.row), rows.length, button === "up" ? -1 : 1)]
  // No row that way: the cursor stays.
  if (target === here.row) return
  const next = ids.find(entry => entry.row === target)
  if (next !== undefined && next.id !== here.id) message({ _tag: "FocusedRow", rowId: next.id })
}

// ---- SAVE DATA ----

function saveData(state: ZoneSurfaceState, button: Direction | "confirm", korri: SurfaceModel, send: Send): void {
  const identity = korri.identityManagement
  const model = state.identity
  const flow = model.flow
  const message = (inner: Extract<ZoneSurfaceMessage, { _tag: "SaveData" }>["message"]) => send({ _tag: "SaveData", message: inner })
  const status = identity?.status._tag
  if (status === "Problem" || status === "Switched") {
    if (button === "confirm") message({ _tag: "DismissedStatus" })
    return
  }
  if (status === "Working") return
  switch (flow._tag) {
    case "password":
    case "secret":
    case "bunker": {
      const draft = flow.draft
      return keyboard(draft.layer, draft.focusedKey, undefined, button, key => message({ _tag: "FocusedKey", key }), key => {
        switch (key.kind) {
          case "char":
            return message({ _tag: "TypedChar", char: key.char })
          case "space":
            return message({ _tag: "TypedSpace" })
          case "delete":
            return message({ _tag: "DeletedChar" })
          case "layer":
            return message({ _tag: "SwitchedLayer", layer: key.layer })
          case "done":
            return message({ _tag: "PressedDone" })
          case "clear":
            return
        }
      })
    }
    case "disposition":
    case "trust":
    case "confirmDelete":
    case "confirmBackedUp":
      if (isDirection(button)) {
        const next = dialogStep(flow._tag === "disposition" ? ["Transfer", "Delete"] : ["No", "Yes"], flow.focus, button)
        if (next !== undefined) message({ _tag: "FocusedChoice", index: next === 0 ? 0 : 1 })
        return
      }
      if (button !== "confirm") return
      if (flow._tag === "disposition") return message({ _tag: "ChoseDisposition", disposition: flow.focus === 0 ? "transfer" : "delete" })
      return message({ _tag: "Answered", yes: flow.focus === 1 })
    case "retired":
      if (isDirection(button)) {
        const next = rowStep(["Back up", "Delete"], SCREEN.width, flow.action === "backup" ? 0 : 1, button)
        if (next !== undefined) return message({ _tag: "FocusedRetiredAction", action: next === 0 ? "backup" : "delete" })
        if (button === "up") return message({ _tag: "FocusedSaveCard", index: model.card })
        return
      }
      if (button === "confirm") message({ _tag: "ChoseRetiredAction", action: flow.action })
      return
    case "showSecret":
    case "idle":
      break
  }
  const backup = status === "BackupReady"
  // Korri's backup takes the band's room: the slab, then B brings the cards back.
  if (backup) {
    if (button === "up") return send({ _tag: "Games", message: { _tag: "FocusedTab", tab: state.nav.tab } })
    if (flow._tag !== "showSecret" || !flow.onSlab) message({ _tag: "FocusedSecret" })
    return
  }
  const count = identityCardCount(identity)
  if (button === "left" || button === "right") {
    const next = along(model.card, count, sideways(button))
    if (next !== model.card) message({ _tag: "FocusedSaveCard", index: next })
    return
  }
  if (button === "up") return send({ _tag: "Games", message: { _tag: "FocusedTab", tab: state.nav.tab } })
  if (button === "down") {
    if (retiredKeyAt(identity, model.card) !== undefined) message({ _tag: "FocusedRetiredAction", action: "backup" })
    return
  }
  message({ _tag: "ChoseSaveCard", index: model.card })
}

// ---- PAUSED ----

function paused(state: ZoneSurfaceState, button: Direction | "confirm", korri: SurfaceModel, send: Send): void {
  const overlay = korri.presentation
  if (overlay.kind !== "gameplay-overlay") return
  const model = state.overlay
  const message = (inner: Extract<ZoneSurfaceMessage, { _tag: "Paused" }>["message"]) => send({ _tag: "Paused", message: inner })
  if (model.asking !== undefined) {
    if (isDirection(button)) {
      const next = dialogStep(["No", "Yes"], model.answer, button)
      if (next !== undefined) message({ _tag: "FocusedAnswer", index: next === 0 ? 0 : 1 })
      return
    }
    if (button === "confirm") message({ _tag: "ChoseAnswer", yes: model.answer === 1 })
    return
  }
  const controls = overlayControls(overlay)
  const at = Math.max(0, controls.findIndex(control => control.id === model.focusedId))
  const control = controls[at]
  if (control === undefined) return
  if (button === "up" || button === "down") {
    const next = controls[along(at, controls.length, button === "up" ? -1 : 1)]
    if (next !== undefined && next.id !== model.focusedId) message({ _tag: "FocusedControl", controlId: next.id })
    return
  }
  if (button === "confirm") return message({ _tag: "PressedControl", controlId: control.id })
  message({ _tag: "SteppedControl", controlId: control.id, direction: sideways(button) })
}

// ---- First boot ----

function onboarding(state: ZoneSurfaceState, button: Direction | "confirm", korri: SurfaceModel, send: Send): void {
  const model = state.onboarding
  const message = (inner: Extract<ZoneSurfaceMessage, { _tag: "Onboarding" }>["message"]) => send({ _tag: "Onboarding", message: inner })
  if (model.entry !== undefined) {
    return keyboard(model.entry.layer, model.entry.focusedKey, undefined, button, id => message({ _tag: "FocusedKey", id }), key => {
      const found = findKey(key)
      if (found !== undefined) message({ _tag: "PressedKey", key: found })
    })
  }
  const targets = onboardingTargets(model, korri)
  const at = Math.max(0, targets.indexOf(model.focus))
  if (isDirection(button)) {
    // CONTROLLERS' seats stand in a row: left and right walk it; elsewhere the targets are a column.
    const seats = model.step === "controllers" && model.focus.startsWith("seat:")
    const sidewaysWalk = (model.step === "controllers" && seats) || model.step === "identity"
    const vertical = button === "up" || button === "down"
    if (!vertical && !sidewaysWalk) return
    if (vertical && model.step === "controllers" && seats && button === "up") return
    const step = vertical ? (button === "up" ? -1 : 1) : sideways(button)
    // The seats are a row of their own: left and right stop at its ends.
    const row = seats && !vertical ? targets.filter(target => target.startsWith("seat:")) : targets
    let next = row[along(Math.max(0, row.indexOf(model.focus)), row.length, step)]
    // Down from a seat goes to NEXT; up from NEXT to the first seat.
    if (vertical && model.step === "controllers" && seats) next = TARGET.next
    if (button === "up" && model.step === "controllers" && model.focus === TARGET.next) next = targets.find(target => target.startsWith("seat:")) ?? next
    if (next !== undefined && next !== model.focus) message({ _tag: "FocusedTarget", target: next })
    return
  }
  const focus = model.focus
  if (focus === TARGET.next) return message({ _tag: "ChoseNext" })
  if (focus === TARGET.finish) return message({ _tag: "ChoseFinish" })
  if (focus === TARGET.save) return message({ _tag: "ChoseStart" })
  if (focus === TARGET.scan) return message({ _tag: "ChoseScan" })
  if (focus === TARGET.skip) return message({ _tag: "ChoseSkip" })
  if (focus === TARGET.name) return message({ _tag: "OpenedName" })
  if (focus.startsWith("net:")) message({ _tag: "OnboardingJoin", networkId: focus.slice(4) })
  void cardCount
}
