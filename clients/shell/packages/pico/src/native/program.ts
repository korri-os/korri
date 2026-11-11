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
 *    two that were Pico's host's own: a QR code is drawn here, and the idle
 *    wait is kept by the shell's `Tick` (contract A1, 7.1);
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
import { PICO_ATTRACT_AFTER_MS } from "../pico-attract"
import { picoLaunchViewFromModel } from "../pico-launch-view"
import { picoOverlayViewFrom } from "../pico-overlay-view"
import { PICO_WINDOW_BEFORE_RESIZED, picoLayoutPlan, type PicoDisplayPlan } from "../pico-display-plan"
import type { PicoShelfGame } from "../pico-shelf-game"
import type { PicoMessage } from "../state/messages"
import { initialNavigation, topLayer } from "../state/navigation"
import { initialOnboarding } from "../state/onboarding"
import { updateOnboarding } from "../state/onboarding-update"
import { initialOverlay, updateOverlay } from "../state/overlay"
import type { PicoRequest } from "../state/requests"
import { attractShowing, canAttract, inviteNotices, launchOpen, runnerOpen, shownScreen } from "../state/shown"
import { update as picoUpdate } from "../state/update"
import { chosenIndex, cursorGame, detailScreen, findQuery, findScreen, homeScreen, sameQuery } from "./catalog"
import { interned } from "./intern"
import { memo, share } from "./memo"
import { type Direction, type FocusPlan, moveFocus, resolveExits, settleFocus, type Target } from "./focus"
import { cartView, type CartView, flatEntry, HINT_A, HINT_B, HINT_LEFT, HINT_RIGHT, type HintView } from "./kit"
import { launchScreen, settingsScreen } from "./panel"
import { qrPath } from "./qr"
import { overlayScreen, runnerScreen } from "./sheet"
import { setupScreen } from "./setup"
import { storeScreen } from "./store"
import type { Act, Ctx, Dispatch, Held, Plan, Screen, State } from "./types"
import { picoShelfGame } from "../pico-home-view"

/** How many games a window holds, and how near its edge the cursor may come
 * before the window moves (contract 3.4). */
export const WINDOW = 50
const EDGE = 8

/** The layout plan the focus graph and the view share (contract A2, 8.2). */
export function planFor(s: State, korri: KorriModel): Plan {
  return planOf(s.window, korri.displays)
}

const planOf = memo(4, (window: State["window"], displays: KorriModel["displays"]) => [window, displays], picoLayoutPlan)

export function init(raw: KorriModel, preferences: Readonly<Record<string, string>>): State {
  const korri = interned(raw)
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
    typing: undefined,
    noticeRound: 0,
    window: PICO_WINDOW_BEFORE_RESIZED,
    remembered: {},
    touched: false,
    anchors: NO_ANCHORS,
    held: undefined,
  }
}

/** No row scrolled by a finger. */
const NO_ANCHORS: State["anchors"] = {}

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
      const view = picoOverlayViewFrom(korri.presentation, korri.status, korri.clockLabel)
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
  return catalogScreenOf(s, korri)
}

/**
 * The parts of Korri's model a screen reads, by identity (the model is
 * interned, so an unchanged part is the same object). The focused game's
 * actions matter only to a game's own screen: the publish that answers
 * FocusGame on the shelf rebuilds nothing.
 */
function korriParts(s: State, korri: KorriModel): readonly unknown[] {
  const parts = PARTS.get(korri) ?? partsOf(korri)
  return s.nav.detail === undefined ? parts : [...parts, korri.focusedGame]
}

const PARTS = new WeakMap<KorriModel, readonly unknown[]>()

function partsOf(korri: KorriModel): readonly unknown[] {
  const record = korri as unknown as Record<string, unknown>
  const parts = Object.keys(record).filter(key => key !== "focusedGame").sort().flatMap(key => [key, record[key]])
  PARTS.set(korri, parts)
  return parts
}

/** A screen reads only these parts of the state (src/native/*.ts). */
const catalogScreenOf = memo(
  6,
  (s: State, korri: KorriModel) => [s.nav, s.setup, s.focus, s.sections, s.typing, s.font, s.window, s.anchors, s.held, ...korriParts(s, korri)],
  (s: State, korri: KorriModel) => screenOf(s, korri),
)

function screenOf(s: State, korri: KorriModel): Screen {
  const model = picoModelFrom(korri)
  const ctx: Ctx = { s, korri, model, plan: planFor(s, korri) }
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
  return overlayScreenOf(s.overlay, s.overlayFocus, korri)
}

const overlayScreenOf = memo(
  4,
  (overlay: State["overlay"], focus: State["overlayFocus"], korri: KorriModel) => [overlay, focus, korri],
  (overlay: State["overlay"], focus: State["overlayFocus"], korri: KorriModel) =>
    korri.presentation.kind !== "gameplay-overlay"
      ? undefined as never
      : overlayScreen(picoOverlayViewFrom(korri.presentation, korri.status, korri.clockLabel), overlay, focus),
)

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
    // A row a finger scrolled (ADR 0055) may draw, or page the window, away
    // from the focused game: focus stays on it out of sight (`held`).
    const kept = !changed && !returning && state.held !== undefined && state.held.id === state.focus
    const focus = kept ? state.focus
      : changed || returning
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
/** The hero is the game played last in the whole library: the first of a
 * window in Korri's `recent` order (contract 3.4), whose resumable games are
 * the RESUME list under it. */
export const HERO_QUERY: CatalogQuery = { text: "", order: "recent" }

/** The query Pico's screen needs answered, and the game to keep in the window. */
function wanted(s: State, model: SurfaceModel): { readonly query: CatalogQuery; readonly anchor: string | undefined } {
  const hero = !s.nav.find && s.nav.home.mode === "hero"
  const query = s.nav.find ? findQuery(s.nav.search) : hero ? HERO_QUERY : DEFAULT_QUERY
  const cursor = cursorGame(s.focus)
  // A row a finger scrolled pages the window from where it was left (ADR 0055).
  const scrolled = s.nav.find ? cursorGame(s.anchors["results"]) : s.nav.home.mode === "shelf" ? cursorGame(s.anchors["shelf"]) : undefined
  const anchor = s.nav.detail?.gameId ?? scrolled ?? (hero ? undefined : cursor) ?? (s.nav.find || hero ? undefined : s.nav.home.selectedGameId)
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
        move = { _tag: "MoveCatalog", query, from: anchor === undefined || state.nav.find || query.order === "recent" ? { _tag: "Start" } : { _tag: "Around", gameId: anchor }, count: WINDOW }
      } else if (anchor !== undefined && at === -1 && (cursorGame(state.focus) === undefined || state.held?.id === state.focus)) {
        // A game's own screen, the shelf's kept cart, or the focused game a
        // scrolled row paged away from, outside the window.
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
        const next = moveFocus(screen.targets, s.overlayFocus, direction, focusPlanOf(s, korri, screen), screen.active, s.remembered)
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
  const next = moveFocus(screen.targets, s.focus, direction, focusPlanOf(s, korri, screen), screen.active, s.remembered)
  // A d-pad press is activity, and wakes attract; the move itself stands.
  const moved: Act = model.onboarding === undefined ? [{ to: "nav", message: { _tag: "MovedFocus" } }] : []
  if (next === undefined || next === s.focus) return run(step, moved, korri)
  const arrived = screen.targets.find(target => target.id === next)
  return run({ ...step, state: { ...s, focus: next } }, [...moved, ...(arrived?.enter ?? [])], korri)
}

function leftRange(id: string, korri: KorriModel): Act {
  if (korri.presentation.kind !== "gameplay-overlay") return []
  const view = picoOverlayViewFrom(korri.presentation, korri.status, korri.clockLabel)
  const control = [...view.controls, ...view.groups.flatMap(group => group.controls)].find(candidate => `control:${candidate.id}` === id)
  return control === undefined ? [] : [{ to: "overlay", message: { _tag: "LeftRange", control } }]
}

/**
 * A tap on a focus target (ADR 0055): the cursor moves there, as a d-pad
 * move would, then it does what Confirm does there. An id that names no
 * target on the screen shown (stale, or a disabled control) does nothing.
 * While attract shows, the tap only wakes the screen. A hint drawn now runs
 * its button's own handler and leaves the cursor where it is; one not drawn
 * is stale. A range takes the
 * finger's place along its track (`at`) through the same hold and wait a
 * step uses.
 */
function activated(step: Step, message: Extract<SurfaceMessage, { _tag: "Activated" }>, korri: KorriModel): Step | undefined {
  const id = message.target
  const s = step.state
  const button = HINT_BUTTONS[id]
  if (button !== undefined) {
    const model = picoModelFrom(korri)
    const overlaid = korri.presentation.kind === "gameplay-overlay"
    if (!overlaid && model.onboarding === undefined && attractShowing(s.nav, model)) return run(step, [{ to: "nav", message: { _tag: "PressedScreen" } }], korri)
    const shown = overlaid ? overlayView(s, korri) : catalogScreen(s, korri)
    return shown !== undefined && hintsDrawn(shown.view).has(id) ? pressed(step, button, korri) : undefined
  }
  if (korri.presentation.kind === "gameplay-overlay") {
    const screen = overlayView(s, korri)
    const target = screen?.targets.find(candidate => candidate.id === id)
    if (screen === undefined || target === undefined) return undefined
    let next = step
    if (id !== s.overlayFocus) {
      const current = screen.targets.find(candidate => candidate.id === s.overlayFocus)
      const leaving: Act = current?.horizontal === undefined ? [] : leftRange(current.id, korri)
      next = run({ ...step, state: { ...s, overlayFocus: id } }, leaving, korri)
    }
    const control = target.horizontal === undefined || message.at === undefined ? undefined : rangeControl(id, korri)
    if (control !== undefined) {
      return run(next, [{ to: "overlay", message: { _tag: "SetRange", control, at: message.at!, released: message.released === true } }], korri)
    }
    return pressed(next, "confirm", korri)
  }
  const model = picoModelFrom(korri)
  // The first tap on attract only wakes the screen, as a press does.
  if (model.onboarding === undefined && attractShowing(s.nav, model)) return run(step, [{ to: "nav", message: { _tag: "PressedScreen" } }], korri)
  const screen = catalogScreen(s, korri)
  const target = screen.targets.find(candidate => candidate.id === id)
  if (target === undefined) return undefined
  let next = step
  if (id !== s.focus) {
    // The move a d-pad press makes: activity, the cursor, what arriving does;
    // then the screen settles as it does between two presses.
    const moved: Act = model.onboarding === undefined ? [{ to: "nav", message: { _tag: "MovedFocus" } }] : []
    next = settle(run({ ...step, state: { ...s, focus: id } }, [...moved, ...(target.enter ?? [])], korri), korri)
    // A game in a list opens on the second tap (ADR 0055): the first only
    // chooses it, as a d-pad move does.
    if (GAME_REGIONS.has(target.region)) return next
  }
  return pressed(next, "confirm", korri)
}

/**
 * The regions whose targets are games in a list (ADR 0055): a tap on one
 * that does not have focus only moves focus there; a tap on the focused one
 * opens it. Every other target acts on one tap. The one place this is decided.
 */
export const GAME_REGIONS: ReadonlySet<string> = new Set(["shelf", "grid", "resume", "results"])

/** The racks that move by focus, which a finger scrolls item by item (ADR 0055). */
export const DRAG_ROWS: ReadonlySet<string> = new Set(["shelf", "resume", "results"])

/**
 * A finger scrolled a row `by` whole items (ADR 0055): the row's scroll
 * anchor moves that far along it, held at its ends; focus does not move. The
 * row draws the items round the anchor, and the catalog pages from it
 * (`wanted`). It starts from focus when focus is in the row, else from what
 * the row keeps in sight. A region that is no drag row, or not on the screen
 * shown, does nothing (undefined). While attract shows, the drag only wakes.
 */
function dragged(step: Step, message: Extract<SurfaceMessage, { _tag: "Dragged" }>, korri: KorriModel): Step | undefined {
  const { region, by } = message
  if (!DRAG_ROWS.has(region) || !Number.isInteger(by) || by === 0 || korri.presentation.kind === "gameplay-overlay") return undefined
  const model = picoModelFrom(korri)
  const s = step.state
  if (model.onboarding === undefined && attractShowing(s.nav, model)) return run(step, [{ to: "nav", message: { _tag: "PressedScreen" } }], korri)
  const screen = catalogScreen(s, korri)
  const row = screen.rows?.[region]
  if (row === undefined || row.length === 0) return undefined
  const from = s.anchors[region] ?? (s.focus !== undefined && row.includes(s.focus) ? s.focus : screen.active?.[region] ?? s.remembered[region])
  const at = from === undefined ? -1 : row.indexOf(from)
  const to = row[Math.max(0, Math.min(row.length - 1, (at === -1 ? 0 : at) + by))]
  if (to === undefined || to === s.anchors[region]) return step
  // The focused game is held before the row draws away from it.
  return { ...step, state: hold({ ...s, anchors: { ...s.anchors, [region]: to } }, korri) }
}

/** The button each hint runs (ADR 0055). */
const HINT_BUTTONS: Readonly<Record<string, string>> = { [HINT_A]: "confirm", [HINT_B]: "back", [HINT_LEFT]: "left", [HINT_RIGHT]: "right" }

/** The hint targets a view draws: its frame's hints, or its sheet's. */
function hintsDrawn(shown: Screen["view"]): Set<string> {
  const ids = new Set<string>()
  for (const part of [shown["frame"], shown["sheet"]] as ({ readonly hints?: readonly HintView[] } | undefined)[]) {
    for (const hint of part?.hints ?? []) {
      ids.add(hint.focusId)
      if (hint.right !== undefined) ids.add(hint.right.focusId)
    }
  }
  return ids
}

/** The pause menu's range a target stands for. */
function rangeControl(id: string, korri: KorriModel) {
  if (korri.presentation.kind !== "gameplay-overlay") return undefined
  const view = picoOverlayViewFrom(korri.presentation, korri.status)
  return [...view.controls, ...view.groups.flatMap(group => group.controls)].find(candidate => `control:${candidate.id}` === id && candidate.range !== undefined)
}

export function update(state: State, message: SurfaceMessage, raw: KorriModel): { state: State; requests: SurfaceRequest[] } {
  const korri = interned(raw)
  const result = updated(state, message, korri)
  // A message that changed nothing returns the same state, so the shell does
  // not draw again (a publish with the same window, a press with nowhere to go).
  return { state: share(state, result.state) === state ? state : result.state, requests: result.requests }
}

function updated(state: State, message: SurfaceMessage, korri: KorriModel): { state: State; requests: SurfaceRequest[] } {
  // A Tick that crosses no threshold, and a Resized to the same size, change
  // nothing: the same state object tells the shell not to draw again.
  if (message._tag === "Tick") {
    if (message.idleMs < PICO_ATTRACT_AFTER_MS || state.nav.idle._tag !== "Awake" || !canAttract(state.nav, picoModelFrom(korri))) {
      return { state, requests: [] }
    }
  }
  if (message._tag === "Resized" && message.width === state.window.width && message.height === state.window.height) {
    return { state, requests: [] }
  }
  let step: Step = { state, requests: [] }
  const model = picoModelFrom(korri)
  switch (message._tag) {
    case "KorriPublished":
      step = { ...step, state: { ...step.state, sections: sectionsOf(korri, step.state.sections) } }
      step = run(step, [{ to: "nav", message: { _tag: "KorriPublished" } }], korri)
      if (model.onboarding !== undefined) step = run(step, [{ to: "setup", message: { _tag: "KorriPublished" } }], korri)
      if (korri.presentation.kind === "gameplay-overlay") step = run(step, [{ to: "overlay", message: { _tag: "KorriPublished" } }], korri)
      break
    case "PreferenceStored":
      break
    case "Resized":
      // The plan follows the window; the cursor keeps its target by id.
      step = { ...step, state: { ...step.state, window: { width: message.width, height: message.height } } }
      break
    case "Tick":
      // The shell's idle clock (contract A1, 7.1): attract after
      // PICO_ATTRACT_AFTER_MS with no press. `update` starts it only over a
      // screen that allows it.
      if (message.idleMs >= PICO_ATTRACT_AFTER_MS && step.state.nav.idle._tag === "Awake") {
        step = run(step, [{ to: "nav", message: { _tag: "IdleElapsed" } }], korri)
      }
      break
    case "Pressed": {
      // A button brings the focus highlight back, and every scrolled row
      // back to focus (ADR 0055).
      const away = step.state.held !== undefined && step.state.held.id === step.state.focus && !inWindow(korri, cursorGame(step.state.focus))
      if (step.state.touched || step.state.anchors !== NO_ANCHORS) step = { ...step, state: { ...step.state, touched: false, anchors: NO_ANCHORS } }
      // The focused game was paged out of the window: this press only brings
      // the window back round it (`sync`).
      if (!away) step = pressed(step, message.button, korri)
      break
    }
    case "Touched":
      // A finger went down (ADR 0055): touch is in use, so the highlight
      // hides until the next button. Nothing else: it wakes nothing.
      if (!step.state.touched) step = { ...step, state: { ...step.state, touched: true } }
      break
    case "Dragged":
      // A drag hides the focus highlight until the next button, as a tap does.
      {
        const slid = dragged(step, message, korri)
        if (slid === undefined) return { state, requests: [] }
        step = slid.state.touched ? slid : { ...slid, state: { ...slid.state, touched: true } }
      }
      break
    case "Activated":
      // A tap hides the focus highlight until the next button.
      {
        const tapped = activated(step, message, korri)
        if (tapped === undefined) return { state, requests: [] }
        step = tapped.state.touched ? tapped : { ...tapped, state: { ...tapped.state, touched: true } }
      }
      break
  }
  // A new notice restarts main.slint's 8 s (PICO_INVITE_NOTICE_MS).
  const before = new Set(state.nav.invites.showing.map(notice => notice.inviteId))
  if (step.state.nav.invites.showing.some(notice => !before.has(notice.inviteId))) {
    step = { ...step, state: { ...step.state, noticeRound: step.state.noticeRound + 1 } }
  }
  step = settle(step, korri)
  step = sync(step, korri)
  step = { ...step, state: hold(remember(step.state, korri), korri) }
  return { state: step.state, requests: step.requests }
}

/** Whether the game `id` is in the catalog window now. */
function inWindow(korri: KorriModel, id: string | undefined): boolean {
  return id !== undefined && korri.catalog._tag === "Ready" && korri.catalog.window.some(game => game.id === id)
}

/**
 * Keep a copy of the focused game while a row is scrolled (ADR 0055), so
 * focus and its stage stay on it if the window pages away; let it go once no
 * row is scrolled and the window holds the game again.
 */
function hold(s: State, korri: KorriModel): State {
  const anchored = s.anchors !== NO_ANCHORS && Object.keys(s.anchors).length > 0
  if (!anchored && s.held === undefined) return s
  const gameId = cursorGame(s.focus)
  const catalog = korri.catalog
  const at = gameId === undefined || catalog._tag !== "Ready" ? -1 : catalog.window.findIndex(game => game.id === gameId)
  if (at === -1) return s.held === undefined || s.held.id === s.focus ? s : { ...s, held: undefined }
  if (!anchored || catalog._tag !== "Ready") return { ...s, held: undefined }
  const game = catalog.window[at]!
  const place = catalog.firstIndex + at
  if (s.held !== undefined && s.held.id === s.focus && heldSource.get(s.held) === game && s.held.place === place) return s
  const held: Held = { id: s.focus!, game: picoShelfGame(game), place }
  heldSource.set(held, game)
  return { ...s, held }
}

const heldSource = new WeakMap<Held, object>()

// ---- view ----

const fontFor = (id: State["font"]) => PICO_FONTS.find(font => font.id === id) ?? PICO_FONTS.find(font => font.id === PICO_DEFAULT_FONT)

/** The plan's exits for the regions on this screen: the view's `focusPlan`. */
function focusPlanOf(s: State, korri: KorriModel, screen: Screen): FocusPlan {
  return exitsOf(planFor(s, korri).exits, regionsOf(screen.targets))
}

/** The regions a screen's targets are in, as one string: a press makes a new
 * screen, mostly over the same regions, whose exits are then the same plan. */
const regionsOf = memo(4, (targets: Screen["targets"]) => [targets], (targets: Screen["targets"]) => {
  const regions: string[] = []
  for (const target of targets) if (!regions.includes(target.region)) regions.push(target.region)
  return regions.join(" ")
})

const exitsOf = memo(8, (exits: Plan["exits"], regions: string) => [exits, regions], (exits: Plan["exits"], regions: string) =>
  resolveExits(exits, regions === "" ? [] : regions.split(" ").map(region => ({ region }))))

/** Each region's last focused target, kept for `remembered` exits. */
function remember(s: State, korri: KorriModel): State {
  const overlaid = overlayView(s, korri)
  const screen = overlaid ?? catalogScreen(s, korri)
  const focus = overlaid === undefined ? s.focus : s.overlayFocus
  const region = focus === undefined ? undefined : targetsOf(screen).get(focus)?.region
  if (region === undefined || focus === undefined || s.remembered[region] === focus) return s
  return { ...s, remembered: { ...s.remembered, [region]: focus } }
}

/**
 * The focus convention (contract 8.4): every entry that stands for a target
 * carries its region, and a region's active target says so; an entry that
 * is no target (a disabled button, a cart drawn for show) drops `focused`.
 * The shell strips these keys before Slint sees the view.
 */
function marked(value: unknown, screen: Screen): unknown {
  return markIn(value, targetsOf(screen), screen.active ?? {})
}

/** A screen's targets by id, built once per screen, and the hints: a finger
 * presses them, the cursor never walks onto them (ADR 0055). */
const targetsOf = memo(8, (screen: Screen) => [screen.targets], (screen: Screen) => new Map([...screen.targets, ...HINT_TARGETS].map(target => [target.id, target] as const)))

const HINT_TARGETS: readonly Target<Act>[] = [HINT_A, HINT_B, HINT_LEFT, HINT_RIGHT].map((id, i) => ({ id, region: "hints", box: { x: i * 10, y: 2000, w: 8, h: 8 } }))

/**
 * What marking found in a part of a view, by the part: nothing to mark
 * (`plain`: the part is its own marked form); a marked form that is the same
 * on every screen, as the part holds only entries that are no target
 * (`fixed`); or an entry whose children hold no entry, marked by its own
 * target alone (`id`), as it was for the target's region and whether it was
 * active. One lookup per part.
 */
type Mark =
  | { readonly kind: "plain" }
  | { readonly kind: "fixed"; readonly out: unknown }
  | { readonly kind: "entry"; readonly id: string; readonly region: string; readonly active: boolean; readonly out: unknown }

const MARKS = new WeakMap<object, Mark>()
const PLAIN: Mark = { kind: "plain" }

/** Whether the part `markIn` last returned is marked the same on every screen. */
let stable = true

/** `value` marked. A part returned as it is was plain: it holds no entry. */
function markIn(value: unknown, targets: ReadonlyMap<string, Target<Act>>, active: Readonly<Record<string, string>>): unknown {
  stable = true
  if (value === null || typeof value !== "object") return value
  const known = MARKS.get(value)
  if (known !== undefined) {
    if (known.kind === "plain") return value
    if (known.kind === "fixed") return known.out
    const target = targets.get(known.id)
    if ((target === undefined ? "" : target.region) === known.region && (target !== undefined && active[target.region] === target.id) === known.active) {
      stable = false
      return known.out
    }
  }
  if (Array.isArray(value)) {
    let plain = true
    let fixed = true
    const out: unknown[] = []
    for (let i = 0; i < value.length; i += 1) {
      const item: unknown = value[i]
      // A call costs more than the test in QuickJS: values are kept here.
      if (item === null || typeof item !== "object") {
        out.push(item)
        continue
      }
      const kept = markIn(item, targets, active)
      if (!stable) fixed = false
      if (kept !== item) plain = false
      out.push(kept)
    }
    stable = fixed
    if (plain) {
      MARKS.set(value, PLAIN)
      return value
    }
    if (fixed) MARKS.set(value, { kind: "fixed", out })
    return out
  }
  const entry = value as Record<string, unknown>
  if ("$image" in entry) {
    MARKS.set(entry, PLAIN)
    return entry
  }
  const focusEntry = "focused" in entry
  let plain = !focusEntry
  let childrenPlain = true
  let fixed = true
  let out: Record<string, unknown>
  if (focusEntry && flatEntry(entry)) {
    // Copied whole, in C: QuickJS walks an object's fields far slower. Its
    // `focusId` stays: it is what a tap on it reports (ADR 0055).
    out = { ...entry }
  } else {
    out = {}
    for (const key in entry) {
      const item = entry[key]
      if (item === null || typeof item !== "object") {
        out[key] = item
        continue
      }
      const kept = markIn(item, targets, active)
      if (!stable) fixed = false
      if (kept !== item) {
        plain = false
        childrenPlain = false
      }
      out[key] = kept
    }
  }
  if (plain) {
    stable = true
    MARKS.set(entry, PLAIN)
    return entry
  }
  const id = entry["focusId"]
  if (focusEntry) {
    const target = typeof id === "string" ? targets.get(id) : undefined
    const isActive = target !== undefined && active[target.region] === target.id
    // `target` tells main.slint whether `focused` is current: an entry that
    // is no target sends no `focused`, and the shell keeps a field it is not
    // sent, so its old value must not show.
    out["target"] = target !== undefined
    if (target === undefined) {
      delete out["focused"]
    } else {
      out["focusRegion"] = target.region
      if (isActive) out["focusActive"] = true
    }
    if (typeof id === "string") {
      // A target on another screen may be none here: marked per screen.
      fixed = false
      if (childrenPlain) MARKS.set(entry, { kind: "entry", id, region: target === undefined ? "" : target.region, active: isActive, out })
    }
  }
  if (fixed) MARKS.set(entry, { kind: "fixed", out })
  stable = fixed
  return out
}

/**
 * The targets a screen lists that its view does not draw (for tests): a
 * target with no entry in the view is no place for the cursor, and no region
 * for an exit.
 */
export function undrawnTargets(screen: Screen): string[] {
  const drawn = new Set<string>()
  const collect = (value: unknown): void => {
    if (Array.isArray(value)) value.forEach(collect)
    else if (value !== null && typeof value === "object") {
      const entry = value as Record<string, unknown>
      if (typeof entry["focusId"] === "string") drawn.add(entry["focusId"])
      for (const item of Object.values(entry)) collect(item)
    }
  }
  collect(screen.view)
  return screen.targets.filter(target => !drawn.has(target.id)).map(target => target.id)
}

export function view(s: State, raw: KorriModel) {
  return viewOf(s, interned(raw))
}


/** The view reads the screen's inputs, the notices' round and the font. */
const viewOf = memo(
  2,
  (s: State, korri: KorriModel) => [s.nav, s.setup, s.focus, s.sections, s.typing, s.font, s.window, s.noticeRound, s.touched, s.anchors, s.held, ...korriParts(s, korri)],
  (s: State, korri: KorriModel) => buildView(s, korri),
)

function buildView(s: State, korri: KorriModel) {
  const model = picoModelFrom(korri)
  const screen = catalogScreen(s, korri)
  const font = fontFor(s.font)
  const notices = model.onboarding === undefined ? inviteNotices(s.nav, model) : []
  const shown = shownScreen(s.nav, model)
  const attract = model.onboarding === undefined && attractShowing(s.nav, model) && shown._tag === "Home" && shown.view._tag === "Shelf"
  const plan = splitPlan(planFor(s, korri).split)
  // On one display the attract covers the whole window: the page under it
  // is not sent (the shell keeps what it was last sent), and nothing on it
  // takes the cursor.
  const covering = attract && plan === undefined
  const drawn = {
    fontFamily: font?.family ?? "Pico Tiny5",
    fontRows: font?.rows ?? 7,
    layer: model.onboarding === undefined ? "catalog" : "setup",
    covered: runnerOpen(model) || launchOpen(model),
    ...(covering ? { page: screen.view["page"] } : screen.view),
    notices: notices.map(notice => ({ playing: `${notice.personName} is playing`, game: notice.gameTitle, where: `on ${notice.deviceLabel}` })),
    noticeRound: s.noticeRound,
    attract: {
      shown: attract,
      carts: attract && shown._tag === "Home" && shown.view._tag === "Shelf" ? attractCarts(shown.view.games) : NO_CARTS,
      chosen: shown._tag === "Home" && shown.view._tag === "Shelf" ? chosenIndex(shown.view.games, s.nav.home.selectedGameId) : 0,
    },
    split: plan ?? NO_SPLIT,
    touching: s.touched,
  }
  return { ...(marked(drawn, screen) as typeof drawn), focusPlan: covering ? {} : focusPlanOf(s, korri, screen) }
}

const NO_CARTS: readonly CartView[] = []

/** The attract's parade: the window's first twelve games, once per window. */
const attractCarts = memo(2, (games: readonly PicoShelfGame[]) => [games], (games: readonly PicoShelfGame[]) => games.slice(0, 12).map(game => cartView(game)))

/** One screen: main.slint reads the rectangles only on a split, so they are not sent. */
const NO_SPLIT = { on: false }

/** Where Pico's display plan (src/pico-display-plan.ts) puts the two screens, for main.slint. */
function splitPlan(plan: PicoDisplayPlan | undefined) {
  if (plan === undefined) return undefined
  const { show, choose } = plan
  return { on: true, showX: show.x, showY: show.y, showW: show.width, showH: show.height, chooseX: choose.x, chooseY: choose.y, chooseW: choose.width, chooseH: choose.height }
}

export function overlay(s: State, raw: KorriModel) {
  return overlayOf(s, interned(raw))
}

const overlayOf = memo(
  2,
  (s: State, korri: KorriModel) => [s.overlay, s.overlayFocus, s.font, s.window, s.touched, korri],
  (s: State, korri: KorriModel) => buildOverlay(s, korri),
)

function buildOverlay(s: State, korri: KorriModel) {
  const shown = overlayView(s, korri)
  const font = fontFor(s.font)
  const drawn = {
    fontFamily: font?.family ?? "Pico Tiny5",
    fontRows: font?.rows ?? 7,
    touching: s.touched,
    ...(shown?.view ?? {}),
  }
  return shown === undefined ? drawn : { ...(marked(drawn, shown) as typeof drawn), focusPlan: focusPlanOf(s, korri, shown) }
}

export { PICO_ALL_SECTIONS }
