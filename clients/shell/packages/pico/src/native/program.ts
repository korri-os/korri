/**
 * Pico as a native surface package (contract 3.1): `init`, `update`, `view`
 * and `overlay` over Pico's own programs.
 *
 * The adapter is thin by design. Pico's rules stay in src/state: the
 * catalog's navigation (`update`), first-boot setup (`updateOnboarding`) and
 * the gameplay overlay (`updateOverlay`). This file
 *
 *  - turns the shell's buttons into Pico's messages: Back, Options, Menu and
 *    System as Pico's host buttons; up, down, left and right as a move of
 *    Pico's own cursor (src/native/focus.ts); confirm as the focused
 *    control's press;
 *  - turns Pico's requests into Korri's (`SurfaceRequest`), and answers the
 *    two that were Pico's host's own: a QR code is drawn here, and a wait is
 *    kept by Korri's clock (see `time` below);
 *  - keeps the catalog window on what Pico shows: Find's query, section and
 *    order go to Korri as `MoveCatalog`, the window follows the cursor with
 *    `Around`, and the game under the cursor is reported with `FocusGame`.
 *
 * Pure: no timers, no Date.now, no randomness, no globals but `surface`.
 */
import type {
  CatalogQuery,
  KorriModel,
  SurfaceMessage,
  SurfaceRequest,
} from "../model"
import { picoModelFrom, type SurfaceModel } from "../model"
import { picoFontFrom, PICO_FONT_KEY } from "../pico-font-preference"
import { PICO_FONTS, PICO_DEFAULT_FONT } from "../pico-fonts"
import { PICO_ALL_SECTIONS } from "../pico-library-view"
import { picoLaunchViewFromModel } from "../pico-launch-view"
import { picoOverlayViewFrom } from "../pico-overlay-view"
import { picoDisplayPlan } from "../pico-display-plan"
import type { PicoMessage } from "../state/messages"
import { initialNavigation, topLayer } from "../state/navigation"
import { initialOnboarding } from "../state/onboarding"
import { updateOnboarding } from "../state/onboarding-update"
import { initialOverlay, updateOverlay } from "../state/overlay"
import type { PicoRequest } from "../state/requests"
import { attractShowing, inviteNotices, launchOpen, runnerOpen, shownScreen } from "../state/shown"
import { update as picoUpdate } from "../state/update"
import { chosenIndex, cursorGame, detailScreen, findQuery, findScreen, homeScreen, sameQuery } from "./catalog"
import { type Direction, moveFocus, settleFocus } from "./focus"
import { cartView } from "./kit"
import { launchScreen, settingsScreen } from "./panel"
import { qrPath } from "./qr"
import { overlayScreen, runnerScreen } from "./sheet"
import { setupScreen } from "./setup"
import { storeScreen } from "./store"
import type { Act, Ctx, Dispatch, Plan, Screen, State } from "./types"

/** How many games a window holds, and how near its edge the cursor may come
 * before the window moves (contract 3.4). */
export const WINDOW = 50
const EDGE = 8

/**
 * Pico's waits, kept without a timer. The contract gives a surface no clock
 * of its own; Korri's `clockLabel` changes once a minute. Attract starts on
 * the second change with no press (60 to 120 s; Pico's own wait was 45 s).
 */
const ATTRACT_AFTER_CLOCK_CHANGES = 2

export function planFor(korri: KorriModel): Plan {
  const displays = korri.displays ?? []
  const main = displays.find(display => display.main) ?? displays[0]
  const width = main?.width ?? 640
  const height = main?.height ?? 480
  const p = Math.max(2, Math.floor(Math.min(width, height) / 360))
  return { width: Math.floor(width / p), height: Math.floor(height / p), p }
}

export function init(korri: KorriModel, preferences: Readonly<Record<string, string>>): State {
  return {
    nav: initialNavigation(),
    setup: initialOnboarding(),
    overlay: initialOverlay,
    font: picoFontFrom(preferences[PICO_FONT_KEY]),
    focus: undefined,
    screen: "",
    overlayFocus: undefined,
    reported: null,
    asked: undefined,
    sections: sectionsOf(korri, []),
    clock: korri.clockLabel,
    quiet: 0,
    typing: undefined,
    noticeRound: 0,
  }
}

interface Step {
  readonly state: State
  readonly requests: SurfaceRequest[]
}

// ---- Requests ----

/** Pico's request as Korri's. Pico's own two (a QR code, a wait) answer here. */
function korriRequest(request: PicoRequest, s: State): SurfaceRequest | undefined {
  switch (request._tag) {
    case "LaunchGame":
      return { _tag: "LaunchGame", gameId: request.gameId, ...(request.locationId === undefined ? {} : { launchLocationId: request.locationId }) }
    case "DismissProblem":
      return { _tag: "Dismiss" }
    case "ChooseFont":
    case "RememberFont":
      return { _tag: "RememberPreference", key: PICO_FONT_KEY, value: request.font }
    case "RenderQr":
    case "Wait":
      return undefined
    default:
      void s
      return request
  }
}

/** Apply one program message, collecting Korri's requests and answering Pico's own. */
function deliver(step: Step, dispatch: Dispatch, korri: KorriModel): Step {
  const model = picoModelFrom(korri)
  let { state } = step
  const requests = [...step.requests]
  const answer = (picoRequests: readonly PicoRequest[], to: "nav" | "setup" | "overlay"): void => {
    for (const request of picoRequests) {
      if (request._tag === "ChooseFont") state = { ...state, font: request.font }
      if (request._tag === "RenderQr") {
        // Drawn by main.slint from the module grid; the program only needs
        // to know the code exists, so it hears back at once.
        const reply = { _tag: "RenderedQr" as const, text: request.text, dataUrl: qrPath(request.text) === undefined ? "" : "drawn" }
        const next = deliver({ state, requests: [] }, to === "setup" ? { to: "setup", message: reply } : { to: "nav", message: reply }, korri)
        state = next.state
        requests.push(...next.requests)
        continue
      }
      if (request._tag === "Wait") {
        // A notice's 8 s is drawn by main.slint and ended by the next press;
        // a range's commit wait ends at once (see `deliverOverlay`).
        if (to === "overlay") {
          const next = deliver({ state, requests: [] }, { to: "overlay", message: { _tag: "Waited", token: request.token } }, korri)
          state = next.state
          requests.push(...next.requests)
        }
        continue
      }
      const translated = korriRequest(request, state)
      if (translated !== undefined) requests.push(translated)
    }
  }
  switch (dispatch.to) {
    case "nav": {
      const result = picoUpdate(state.nav, dispatch.message, model)
      state = { ...state, nav: result.model }
      answer(result.requests, "nav")
      break
    }
    case "setup": {
      const result = updateOnboarding(state.setup, dispatch.message, model)
      state = { ...state, setup: result.model }
      answer(result.requests, "setup")
      break
    }
    case "overlay": {
      if (korri.presentation.kind !== "gameplay-overlay") break
      const view = picoOverlayViewFrom(korri.presentation, korri.status)
      const result = updateOverlay(state.overlay, dispatch.message, view)
      state = { ...state, overlay: result.model }
      answer(result.requests, "overlay")
      break
    }
    case "local":
      state = dispatch.change(state)
      break
    case "focus":
      state = { ...state, focus: dispatch.id }
      break
  }
  return { state, requests }
}

const run = (step: Step, act: Act | undefined, korri: KorriModel): Step =>
  (act ?? []).reduce((acc, dispatch) => deliver(acc, dispatch, korri), step)

// ---- Screens ----

/** What the catalog presentation shows now, as a view and focus targets. */
export function catalogScreen(s: State, korri: KorriModel): Screen {
  const model = picoModelFrom(korri)
  const ctx: Ctx = { s, korri, model, plan: planFor(korri) }
  if (model.onboarding !== undefined) return onPage(setupScreen(ctx), "setup")
  const runner = model.runnerChoice
  if (runner !== undefined && runner._tag !== "Closed") return runnerScreen(runner, s.focus)
  const launch = picoLaunchViewFromModel(model)
  if (launch !== undefined && s.nav.launch !== undefined) return onPage(launchScreen(ctx, launch, s.nav.launch), "launch")
  const shown = shownScreen(s.nav, model)
  switch (shown._tag) {
    case "Home":
      return homeScreen(ctx, shown)
    case "Find":
      return findScreen(ctx, shown)
    case "Detail":
      return detailScreen(ctx, shown)
    case "Settings":
      return onPage(settingsScreen(ctx, shown.settings), "settings")
    case "Store":
      return onPage(storeScreen(ctx, shown.place, shown.store), "store")
  }
}

/**
 * The page a screen belongs to, and what that page shows of it: the panel,
 * a question (a modal, or a list to choose from), the editor, the identity
 * dialog, the store, or setup's step.
 */
function onPage(screen: Screen, page: "setup" | "launch" | "settings" | "store"): Screen {
  const view = screen.view
  const raw = view["page"]
  const card = view["card"] as { readonly shown?: boolean } | undefined
  const mode = raw === "editor" ? "editor"
    : raw === "identity" ? "identity"
    : card?.shown === true ? (screen.key.includes(":pick:") ? "choice" : "modal")
    : page === "store" ? "store"
    : page === "setup" ? "setup"
    : "panel"
  return { ...screen, view: { ...view, page, mode } }
}

function overlayView(s: State, korri: KorriModel): Screen | undefined {
  if (korri.presentation.kind !== "gameplay-overlay") return undefined
  return overlayScreen(picoOverlayViewFrom(korri.presentation, korri.status), s.overlay, s.overlayFocus)
}

/** Keep the cursor on a target of the screen shown, starting where the
 * screen says when the screen changed or its target went. */
function settle(step: Step, korri: KorriModel): Step {
  let { state } = step
  const requests = [...step.requests]
  const shown = overlayView(state, korri)
  if (shown !== undefined) {
    const focus = settleFocus(shown.targets, state.overlayFocus, shown.fallback)
    return { state: focus === state.overlayFocus ? state : { ...state, overlayFocus: focus }, requests }
  }
  for (let round = 0; round < 3; round += 1) {
    const screen = catalogScreen(state, korri)
    const changed = screen.key !== state.screen
    const home = state.nav.home
    // Home asked for the cursor back where it was: on MENU, or on the cart.
    const returning = home.focusOnReturn !== "None" && topLayer(state.nav) === "Home" && screen.key.startsWith("home:")
    const focus = changed || returning
      ? settleFocus(screen.targets, screen.fallback, undefined)
      : settleFocus(screen.targets, state.focus, screen.fallback)
    const moved = focus !== state.focus
    state = { ...state, screen: screen.key, focus }
    // Focus that lands by itself still announces itself, as a DOM focus did:
    // the A hint names MENU's line, the shelf chooses the cart.
    const enter = moved ? screen.targets.find(target => target.id === focus)?.enter : undefined
    if (enter !== undefined && enter.length > 0) {
      const next = run({ state, requests }, enter, korri)
      state = next.state
      requests.splice(0, requests.length, ...next.requests)
    }
    if (returning) {
      const next = deliver({ state, requests }, { to: "nav", message: { _tag: "ReturnedFocus" } }, korri)
      state = next.state
      requests.splice(0, requests.length, ...next.requests)
      continue
    }
    break
  }
  return { state, requests }
}

// ---- The catalog window ----

/** Sections seen in Korri's unfiltered windows, kept in order. */
function sectionsOf(korri: KorriModel, seen: readonly string[]): readonly string[] {
  const catalog = korri.catalog
  if (catalog._tag !== "Ready" || catalog.query.section != null || catalog.query.text !== "") return seen
  const next = [...seen]
  for (const game of catalog.window) if (game.section !== undefined && !next.includes(game.section)) next.push(game.section)
  return next.length === seen.length ? seen : next
}

const DEFAULT_QUERY: CatalogQuery = { text: "", order: "korri" }

/** The query Pico's screen needs answered, and the game to keep in the window. */
function wanted(s: State, model: SurfaceModel): { readonly query: CatalogQuery; readonly anchor: string | undefined } {
  const query = s.nav.find ? findQuery(s.nav.search) : DEFAULT_QUERY
  const cursor = cursorGame(s.focus)
  const anchor = s.nav.detail?.gameId ?? cursor ?? (s.nav.find ? undefined : s.nav.home.selectedGameId)
  void model
  return { query, anchor }
}

/** Ask Korri for the window Pico needs, and say which game has the cursor. */
function sync(step: Step, korri: KorriModel): Step {
  let { state } = step
  const requests = [...step.requests]
  const model = picoModelFrom(korri)
  if (korri.presentation.kind === "catalog" && model.onboarding === undefined) {
    const { query, anchor } = wanted(state, model)
    const catalog = korri.catalog
    if (catalog._tag === "Ready") {
      const window = catalog.window
      const at = anchor === undefined ? -1 : window.findIndex(game => game.id === anchor)
      const sameAsked = sameQuery(catalog.query, query)
      let move: SurfaceRequest | undefined
      if (!sameAsked) {
        move = { _tag: "MoveCatalog", query, from: anchor === undefined || state.nav.find ? { _tag: "Start" } : { _tag: "Around", gameId: anchor }, count: WINDOW }
      } else if (anchor !== undefined && at === -1 && cursorGame(state.focus) === undefined) {
        // A game's own screen, or the shelf's kept cart, outside the window.
        move = { _tag: "MoveCatalog", query, from: { _tag: "Around", gameId: anchor }, count: WINDOW }
      } else if (at !== -1 && ((catalog.after !== undefined && at >= window.length - EDGE) || (catalog.before !== undefined && at < EDGE))) {
        move = { _tag: "MoveCatalog", query, from: { _tag: "Around", gameId: anchor ?? "" }, count: WINDOW }
      }
      if (move !== undefined) {
        const key = JSON.stringify([move, catalog.firstIndex, catalog.total])
        if (key !== state.asked) {
          requests.push(move)
          state = { ...state, asked: key }
        }
      }
    } else if (catalog._tag !== "Loading" && !sameQuery(DEFAULT_QUERY, query)) {
      // An empty answer to Find's query is Korri's Empty: ask once more only when the query changes.
      const key = JSON.stringify(query)
      if (key !== state.asked) {
        requests.push({ _tag: "MoveCatalog", query, from: { _tag: "Start" }, count: WINDOW })
        state = { ...state, asked: key }
      }
    }
    // FocusGame: the game the cursor is on, or the game whose screen is open.
    const reported = state.nav.detail?.gameId ?? cursorGame(state.focus) ?? null
    if (reported !== state.reported) {
      requests.push(reported === null ? { _tag: "FocusGame" } : { _tag: "FocusGame", gameId: reported })
      state = { ...state, reported }
    }
  }
  return { state, requests }
}

// ---- update ----

function pressed(step: Step, button: string, korri: KorriModel): Step {
  const s = step.state
  if (korri.presentation.kind === "gameplay-overlay") {
    const screen = overlayView(s, korri)
    if (screen === undefined) return step
    switch (button) {
      case "back":
        return run(step, [{ to: "overlay", message: { _tag: "PressedBack" } }], korri)
      case "menu":
        return run(step, [{ to: "overlay", message: { _tag: "PressedMenu" } }], korri)
      case "system":
        return run(step, [{ to: "overlay", message: { _tag: "PressedSystem" } }], korri)
      case "options":
        return step
      case "confirm":
        return run(step, screen.targets.find(target => target.id === s.overlayFocus)?.act, korri)
      default: {
        const direction = button as Direction
        const current = screen.targets.find(target => target.id === s.overlayFocus)
        if ((direction === "left" || direction === "right") && current?.horizontal !== undefined) {
          return run(step, direction === "left" ? current.horizontal.left : current.horizontal.right, korri)
        }
        const next = moveFocus(screen.targets, s.overlayFocus, direction)
        if (next === undefined || next === s.overlayFocus) return step
        const leaving: Act = current?.horizontal === undefined || korri.presentation.kind !== "gameplay-overlay" ? [] : leftRange(current.id, korri)
        return run({ ...step, state: { ...s, overlayFocus: next } }, leaving, korri)
      }
    }
  }

  const model = picoModelFrom(korri)
  if (model.onboarding !== undefined) {
    if (button === "back") {
      if (s.typing !== undefined) return { ...step, state: { ...s, typing: undefined } }
      return run(step, [{ to: "setup", message: { _tag: "PressedBack" } }], korri)
    }
    if (button === "options" || button === "menu" || button === "system") return step
  } else {
    // The keyboard over a form field closes first.
    if (button === "back" && s.typing !== undefined) return { ...step, state: { ...s, typing: undefined } }
    const host: Record<string, PicoMessage> = {
      back: { _tag: "PressedBack" },
      options: { _tag: "PressedOptions" },
      menu: { _tag: "PressedMenu" },
      system: { _tag: "PressedSystem" },
    }
    const message = host[button]
    if (message !== undefined) return run(step, [{ to: "nav", message }], korri)
  }

  const screen = catalogScreen(s, korri)
  const waking = model.onboarding === undefined && attractShowing(s.nav, model)
  if (button === "confirm") {
    // A press while attract shows only wakes the screen.
    if (waking) return run(step, [{ to: "nav", message: { _tag: "PressedScreen" } }], korri)
    const target = screen.targets.find(candidate => candidate.id === s.focus)
    const pressedScreen: Act = model.onboarding === undefined ? [{ to: "nav", message: { _tag: "PressedScreen" } }] : []
    return run(step, [...pressedScreen, ...(target?.act ?? [])], korri)
  }
  const direction = button as Direction
  const current = screen.targets.find(target => target.id === s.focus)
  const horizontal = (direction === "left" || direction === "right") ? current?.horizontal ?? screen.horizontal : undefined
  if (horizontal !== undefined) return run(step, direction === "left" ? horizontal.left : horizontal.right, korri)
  const next = moveFocus(screen.targets, s.focus, direction)
  // A d-pad press is activity, and wakes attract; the move itself stands.
  const moved: Act = model.onboarding === undefined ? [{ to: "nav", message: { _tag: "MovedFocus" } }] : []
  if (next === undefined || next === s.focus) return run(step, moved, korri)
  const arrived = screen.targets.find(target => target.id === next)
  return run({ ...step, state: { ...s, focus: next } }, [...moved, ...(arrived?.enter ?? [])], korri)
}

function leftRange(id: string, korri: KorriModel): Act {
  if (korri.presentation.kind !== "gameplay-overlay") return []
  const view = picoOverlayViewFrom(korri.presentation, korri.status)
  const control = [...view.controls, ...view.groups.flatMap(group => group.controls)].find(candidate => `control:${candidate.id}` === id)
  return control === undefined ? [] : [{ to: "overlay", message: { _tag: "LeftRange", control } }]
}

/** Korri's clock moved. A screen left alone long enough starts attract. */
function time(step: Step, korri: KorriModel): Step {
  const s = step.state
  if (korri.clockLabel === s.clock) return step
  const quiet = s.quiet + 1
  let next: Step = { ...step, state: { ...s, clock: korri.clockLabel, quiet } }
  if (quiet >= ATTRACT_AFTER_CLOCK_CHANGES && s.nav.idle._tag === "Awake") {
    next = run(next, [{ to: "nav", message: { _tag: "IdleElapsed" } }], korri)
  }
  return next
}

export function update(state: State, message: SurfaceMessage, korri: KorriModel): { state: State; requests: SurfaceRequest[] } {
  let step: Step = { state, requests: [] }
  const model = picoModelFrom(korri)
  switch (message._tag) {
    case "KorriPublished":
      step = { ...step, state: { ...step.state, sections: sectionsOf(korri, step.state.sections) } }
      step = run(step, [{ to: "nav", message: { _tag: "KorriPublished" } }], korri)
      if (model.onboarding !== undefined) step = run(step, [{ to: "setup", message: { _tag: "KorriPublished" } }], korri)
      if (korri.presentation.kind === "gameplay-overlay") step = run(step, [{ to: "overlay", message: { _tag: "KorriPublished" } }], korri)
      step = time(step, korri)
      break
    case "PreferenceStored":
      break
    case "Pressed":
      step = pressed({ ...step, state: { ...step.state, quiet: 0 } }, message.button, korri)
      break
  }
  // A new notice restarts main.slint's 8 s (PICO_INVITE_NOTICE_MS).
  const before = new Set(state.nav.invites.showing.map(notice => notice.inviteId))
  if (step.state.nav.invites.showing.some(notice => !before.has(notice.inviteId))) {
    step = { ...step, state: { ...step.state, noticeRound: step.state.noticeRound + 1 } }
  }
  step = settle(step, korri)
  step = sync(step, korri)
  return { state: step.state, requests: step.requests }
}

// ---- view ----

const fontFor = (id: State["font"]) => PICO_FONTS.find(font => font.id === id) ?? PICO_FONTS.find(font => font.id === PICO_DEFAULT_FONT)

export function view(s: State, korri: KorriModel) {
  const model = picoModelFrom(korri)
  const screen = catalogScreen(s, korri)
  const font = fontFor(s.font)
  const notices = model.onboarding === undefined ? inviteNotices(s.nav, model) : []
  const shown = shownScreen(s.nav, model)
  const attract = model.onboarding === undefined && attractShowing(s.nav, model) && shown._tag === "Home" && shown.view._tag === "Shelf"
  const displays = korri.displays ?? []
  const plan = splitPlan(picoDisplayPlan(displays))
  return {
    fontFamily: font?.family ?? "Pico Tiny5",
    fontRows: font?.rows ?? 7,
    layer: model.onboarding === undefined ? "catalog" : "setup",
    covered: runnerOpen(model) || launchOpen(model),
    ...screen.view,
    notices: notices.map(notice => ({ playing: `${notice.personName} is playing`, game: notice.gameTitle, where: `on ${notice.deviceLabel}` })),
    noticeRound: s.noticeRound,
    attract: {
      shown: attract,
      carts: attract && shown._tag === "Home" && shown.view._tag === "Shelf"
        ? shown.view.games.slice(0, 12).map(game => cartView(game))
        : [],
      chosen: shown._tag === "Home" && shown.view._tag === "Shelf" ? chosenIndex(shown.view.games, s.nav.home.selectedGameId) : 0,
    },
    split: plan ?? { on: false, showX: 0, showY: 0, showW: 0, showH: 0, chooseX: 0, chooseY: 0, chooseW: 0, chooseH: 0 },
  }
}

/** Where Pico's display plan (src/pico-display-plan.ts) puts the two screens, for main.slint. */
function splitPlan(plan: ReturnType<typeof picoDisplayPlan>) {
  if (plan === undefined) return undefined
  const { show, choose } = plan
  return { on: true, showX: show.x, showY: show.y, showW: show.width, showH: show.height, chooseX: choose.x, chooseY: choose.y, chooseW: choose.width, chooseH: choose.height }
}

export function overlay(s: State, korri: KorriModel) {
  const shown = overlayView(s, korri)
  const font = fontFor(s.font)
  return {
    fontFamily: font?.family ?? "Pico Tiny5",
    fontRows: font?.rows ?? 7,
    ...(shown?.view ?? {}),
  }
}

export { PICO_ALL_SECTIONS }
