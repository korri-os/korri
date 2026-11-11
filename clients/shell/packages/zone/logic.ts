/**
 * Zone's logic for the native shell (docs/briefs/2026-10-31-native-surfaces-contract.md,
 * section 3.1). Bundled to logic.js:
 *
 *     bun build logic.ts --outfile logic.js --format iife --target browser
 *
 * Zone's own program (src/state/surface.ts, with every model it composes)
 * runs unchanged against the generated treaty types. This file is the thin
 * adapter around it:
 *
 *   SurfaceMessage → Zone's messages
 *     Pressed      src/press.ts turns a press into the fact the page would
 *                  have reported (focus lives in the model: the surface moves
 *                  it, where the portal's spatial focus did before)
 *     KorriPublished  Zone's KorriPublished, then the band keeps its game
 *     PreferenceStored  nothing: the look is already Zone's state
 *     Tick         the shell's idle clock (contract 7, A1): Zone's IdleElapsed
 *                  at ZONE_IDLE_AFTER_MS, then its IdleTicked every
 *                  ZONE_IDLE_CYCLE_MS, so the attract is Zone's own layer and
 *                  the first press only wakes it; and the results tally's
 *                  clock (`tallyClock`): TallyCounted when its count-up has
 *                  run, TallyElapsed at ZONE_TALLY_CLOSE_MS, as the portal's
 *                  timers did
 *     Resized      the window's size (contract 8, A2): Zone's plan for it
 *                  (src/plan/screen.ts), which the focus graph and view()
 *                  both read
 *     Pressed back by backRoute (contract 7.2): Korri's declineActionId as
 *                  RunAction; an unmarked Busy swallows it; else Zone's Back
 *     Activated    a tap on a drawn target (ADR 0055): the fact its arrival
 *                  would report (src/press.ts focusOn), then Confirm there; an
 *                  id no target drawn now carries does nothing; a hint runs its
 *                  button; on the attract or the tally, only the one press
 *     Dragged      a finger dragged the band or the save band `by` cards: that
 *                  many d-pad presses along it (src/press.ts dragOn)
 *     Touched      a finger went down: touch is in use, nothing else
 *     Touched, Activated and Dragged hide the focus highlight until the next Pressed
 *
 *   Zone's requests → SurfaceRequests
 *     RememberLook   RememberPreference { key: "zone.look" }
 *     DismissProblem Dismiss
 *     LaunchGame     LaunchGame { launchLocationId }
 *     the rest       unchanged (one variant per host method, same arguments)
 *
 *   and three of the surface's own:
 *     FocusGame      whenever the band's focused game changes (GAME OPTS.
 *                    reads Korri's actions for it from model.focusedGame)
 *     MoveCatalog    the FIND draft as the query's text, and a window around
 *                    the focused game near either end of the window
 *
 * The moments' ways out have no message: a moment that starts to leave is
 * let go at once. The tally's count-up is drawn by ZoneTally on the view's
 * clock; its end and the close come from Ticks.
 *
 * Pure: no timers, no Date, no randomness.
 */
import type {
  CatalogQuery,
  SurfaceButton,
  SurfaceGame,
  SurfaceStatus,
  SurfaceMessage as Message,
  SurfaceModel as Model,
  SurfaceRequest as Request,
} from "../../../../contracts/generated/surface"
import { isZoneLook, ZONE_DEFAULT_LOOK } from "./src/plan/look"
import { zoneScreen, ZONE_DEFAULT_SIZE, type ZoneScreen } from "./src/plan/screen"
import { focusOn, press, scrollOn, zoneGameTarget, type ZoneScroll } from "./src/press"
import { bandGames, cardCount, topLayer, ZONE_IDLE_AFTER_MS, ZONE_IDLE_CYCLE_MS, ZONE_TALLY_CLOSE_MS } from "./src/state/navigation"
import { zoneTallyCountMs } from "./src/plan/motion"
import { zoneTallyRows } from "./src/plan/tally"
import type { ZoneTally } from "./src/state/tally"
import type { ZoneRequest } from "./src/state/requests"
import { update as zoneUpdate, zoneShown, zoneSurfaceStart, type ZoneSurfaceMessage, type ZoneSurfaceState } from "./src/state/surface"
import { attractStep, overlayView, view as zoneView, ZONE_HINT_BACK, ZONE_HINT_CONFIRM, type ZoneTouchView } from "./src/view"

export interface State {
  readonly zone: ZoneSurfaceState
  /** The focused game Korri last heard of (FocusGame). */
  readonly reported: string | null
  /** A MoveCatalog is out; no second one until Korri publishes. */
  readonly moving: boolean
  /** The idle turns already sent: 0 awake, 1 after IdleElapsed, then one more per IdleTicked. */
  readonly idleTurns: number
  /** The window, from the shell's Resized (logical px). */
  readonly size: { readonly width: number; readonly height: number }
  /** The results tally's clock while it shows (`tallyClock`). */
  readonly tally?: TallyClock
  /** A tap or a drag came last, not a button (ADR 0055): the focus highlight
   * hides until the next Pressed. The cursor stays where it is. */
  readonly touched?: boolean
  /** Where a finger scrolled the band and the save band (ADR 0055); focus
   * does not move with it. The next button press clears it. */
  readonly scroll?: ZoneScroll
  /** The band's focused game, kept while the band is scrolled, so focus stays
   * on it even when the window pages away from it. */
  readonly held?: SurfaceGame
}

/** How long the tally has shown, from the shell's Ticks: `idle` is the last Tick's idleMs. */
export interface TallyClock {
  readonly key: string
  readonly elapsed: number
  readonly idle: number
}

/**
 * The tally's clock after a Tick. Its first Tick starts it; each later Tick
 * adds the idle time since the one before, or all of it when a press reset
 * the idle time in between, so the clock runs on across presses.
 */
export function tallyClock(tally: ZoneTally, previous: TallyClock | undefined, idleMs: number): TallyClock {
  const key = `${tally.gameId}:${tally.plays}`
  if (previous?.key !== key) return { key, elapsed: 0, idle: idleMs }
  const step = idleMs >= previous.idle ? idleMs - previous.idle : idleMs
  return { key, elapsed: previous.elapsed + step, idle: idleMs }
}

/** Zone's plan for the window and the look it is drawn in. */
let lastScreen: ZoneScreen | undefined
export function screenOf(state: State): ZoneScreen {
  const { width, height } = state.size
  const look = state.zone.look
  if (lastScreen === undefined || lastScreen.width !== width || lastScreen.height !== height || lastScreen.look !== look) {
    lastScreen = zoneScreen(width, height, look)
  }
  return lastScreen
}

/**
 * The action Korri marks as Back's on its status (contract 7, A1): every
 * Confirm, and Busy, ChooseSave and Paused. Read by its field, never inferred
 * from an action's id or label.
 */
export function declineOf(status: SurfaceStatus): string | undefined {
  return "declineActionId" in status && status.declineActionId !== undefined && status.declineActionId !== "" ? status.declineActionId : undefined
}

/**
 * Where Back goes (contract 7.2, A1), in this order:
 *   1. the status carries declineActionId: run it;
 *   2. Busy with no declineActionId (a korrid write under way, such as saves'
 *      Picking): swallow it, not even Zone's own navigation;
 *   3. anything else, including a Confirm with no declineActionId (owner
 *      binding's single-action Confirms): Zone's own Back, as under the portal.
 */
export type BackRoute = { readonly _tag: "Decline"; readonly actionId: string } | { readonly _tag: "Swallow" } | { readonly _tag: "Zone" }

export function backRoute(status: SurfaceStatus): BackRoute {
  const decline = declineOf(status)
  if (decline !== undefined) return { _tag: "Decline", actionId: decline }
  if (status._tag === "Busy") return { _tag: "Swallow" }
  return { _tag: "Zone" }
}

const LOOK_KEY = "zone.look"
/** Cards from a window edge at which Zone asks for a window around the focus. */
const EDGE = 8
const WINDOW = 50

/** Zone's request in the treaty's words. */
function treaty(request: ZoneRequest): Request {
  switch (request._tag) {
    case "LaunchGame":
      return request.locationId === undefined
        ? { _tag: "LaunchGame", gameId: request.gameId }
        : { _tag: "LaunchGame", gameId: request.gameId, launchLocationId: request.locationId }
    case "DismissProblem":
      return { _tag: "Dismiss" }
    case "RememberLook":
      return { _tag: "RememberPreference", key: LOOK_KEY, value: request.look }
    default:
      return request
  }
}

/** One step of Zone's own program; its requests are added to `out`. */
function run(state: ZoneSurfaceState, message: ZoneSurfaceMessage, model: Model, out: Request[]): ZoneSurfaceState {
  const step = zoneUpdate(state, message, model)
  for (const request of step.requests) out.push(treaty(request))
  return step.model
}

/** The moments' ways out have no timer here: a card or a tally that starts to leave is gone. */
function settle(state: ZoneSurfaceState, model: Model, out: Request[]): ZoneSurfaceState {
  let next = state
  if (next.nav.launch?.leaving === true) next = run(next, { _tag: "Games", message: { _tag: "TitleCardLeft" } }, model, out)
  if (next.nav.tally?.leaving === true) next = run(next, { _tag: "Games", message: { _tag: "TallyLeft" } }, model, out)
  return next
}

const focusedId = (state: ZoneSurfaceState, model: Model): string | null =>
  bandGames(state.nav, model)[state.nav.card]?.id ?? null

const query = (text: string, model: Model): CatalogQuery => {
  const asked = model.catalog._tag === "Ready" ? model.catalog.query : undefined
  return { text, ...(asked?.section === undefined ? {} : { section: asked.section }), order: asked?.order ?? "korri" }
}

/** Zone's start in the stored look (Blend when nothing valid is stored). */
export function init(model: Model, preferences: Readonly<Record<string, string>>): State {
  const stored = preferences[LOOK_KEY]
  const zone = zoneSurfaceStart(model, isZoneLook(stored) ? stored : ZONE_DEFAULT_LOOK)
  return { zone, reported: null, moving: false, idleTurns: 0, size: ZONE_DEFAULT_SIZE }
}

export function update(state: State, message: Message, model: Model): { state: State; requests: Request[] } {
  const out: Request[] = []
  let zone = state.zone
  let moving = state.moving
  let idleTurns = state.idleTurns
  let size = state.size
  let tally = state.tally
  let touched = state.touched === true
  let scroll = state.scroll
  let held = state.held
  switch (message._tag) {
    case "Resized":
      size = { width: Math.max(0, message.width), height: Math.max(0, message.height) }
      break
    case "Tick": {
      // The results tally holds the window: its count-up ends, then it closes
      // by itself at ZONE_TALLY_CLOSE_MS; the attract waits until it has gone.
      const shown = zone.nav.tally
      if (shown !== undefined && !shown.leaving) {
        tally = tallyClock(shown, tally, message.idleMs)
        if (!shown.counted && tally.elapsed >= zoneTallyCountMs(zoneTallyRows(shown).length)) {
          zone = run(zone, { _tag: "Games", message: { _tag: "TallyCounted" } }, model, out)
        }
        if (tally.elapsed >= ZONE_TALLY_CLOSE_MS) zone = run(zone, { _tag: "Games", message: { _tag: "TallyElapsed" } }, model, out)
        break
      }
      // Zone's idle: the attract opens after ZONE_IDLE_AFTER_MS without a
      // press and moves to the next game every ZONE_IDLE_CYCLE_MS.
      const due = message.idleMs < ZONE_IDLE_AFTER_MS ? 0 : 1 + Math.floor((message.idleMs - ZONE_IDLE_AFTER_MS) / ZONE_IDLE_CYCLE_MS)
      // Most Ticks change nothing: the same state back, so the view is not rebuilt.
      if (due === idleTurns || (due > 0 && due < idleTurns)) return { state, requests: [] }
      if (due === 0) idleTurns = 0
      if (idleTurns === 0 && due > 0) {
        zone = run(zone, { _tag: "Games", message: { _tag: "IdleElapsed" } }, model, out)
        // The attract shows: a scrolled band comes back to its focus.
        scroll = undefined
        idleTurns = 1
      }
      // The attract turns through the cards in a circle: a long wait costs at most one lap.
      const lap = Math.max(1, cardCount(model))
      let turns = due - idleTurns
      if (turns > lap) turns = turns % lap
      for (let turn = 0; turn < turns; turn++) zone = run(zone, { _tag: "Games", message: { _tag: "IdleTicked" } }, model, out)
      idleTurns = Math.max(idleTurns, due)
      break
    }
    case "KorriPublished": {
      moving = false
      zone = run(zone, { _tag: "KorriPublished" }, model, out)
      // A new window: keep the cursor on the game it was on.
      if (state.reported !== null && model.catalog._tag === "Ready") {
        const index = bandGames(zone.nav, model).findIndex(game => game.id === state.reported)
        // Off the band (a scrolled band paged the window), focus stays where it is.
        if (index !== -1 && index !== zone.nav.card) {
          zone = zone.nav.zone === "cards" || state.scroll === undefined
            ? run(zone, { _tag: "Games", message: { _tag: "FocusedCard", index } }, model, out)
            : { ...zone, nav: { ...zone.nav, card: index } }
        }
      }
      break
    }
    case "PreferenceStored":
      break
    case "Pressed": {
      idleTurns = 0
      touched = false
      // A button brings every scrolled rack back to focus (ADR 0055). When
      // the window paged away from the focused game, this press only brings
      // it back round it (the paging below).
      const away = held !== undefined && heldAway(zone, model, held)
      scroll = undefined
      if (!away) zone = pressed(zone, message.button, model, screenOf(state), out)
      break
    }
    case "Activated": {
      const id = message.target
      const screen = screenOf(state)
      const overlaid = model.presentation.kind === "gameplay-overlay"
      if (!overlaid && (holdsEveryPress(zone, model) || topLayer(zone.nav) === "idle")) {
        // The attract, or the results tally: the tap is one press, nothing more.
        zone = pressed(zone, "confirm", model, screen, out)
      } else {
        // Only a target drawn now (a hint too): a stale id, or one from a layer that went, does nothing.
        const drawn = overlaid ? overlay(state, model) : view(state, model)
        if (!drawnTargets(drawn).has(id)) return { state, requests: [] }
        if (id === ZONE_HINT_BACK || id === ZONE_HINT_CONFIRM) {
          zone = pressed(zone, id === ZONE_HINT_BACK ? "back" : "confirm", model, screen, out)
          idleTurns = 0
          touched = true
          break
        }
        const range = overlaid && message.at !== undefined && id.startsWith("control:") ? id.slice("control:".length) : undefined
        // A game in a list opens on the second tap: the first only chooses it (ADR 0055).
        const choosing = !overlaid && zoneGameTarget(id) && focusedTarget(drawn) !== id
        focusOn(zone, id, model, screen, inner => {
          zone = run(zone, inner, model, out)
        })
        if (choosing) {
          // A game the band holds again, chosen by a tap: no longer held.
          if (id.startsWith("card:")) held = scroll?.band === undefined ? undefined : bandGames(zone.nav, model)[zone.nav.card]
        } else {
          zone = range === undefined
            ? pressed(zone, "confirm", model, screen, out)
            : run(zone, { _tag: "Paused", message: { _tag: "SetRange", controlId: range, at: message.at! } }, model, out)
        }
      }
      idleTurns = 0
      touched = true
      break
    }
    case "Touched":
      // A finger went down (ADR 0055): touch is in use, so the highlight
      // hides until the next button. It wakes nothing by itself.
      if (touched) return { state, requests: [] }
      touched = true
      break
    case "Dragged": {
      const screen = screenOf(state)
      if (!Number.isInteger(message.by) || message.by === 0) return { state, requests: [] }
      if (topLayer(zone.nav) === "idle" && model.presentation.kind === "catalog") {
        // The first drag on the attract only wakes it.
        zone = pressed(zone, "confirm", model, screen, out)
      } else {
        // The rack scrolls; focus stays (ADR 0055).
        const next = scrollOn(zone, scroll ?? {}, message.region, message.by, model)
        if (next === undefined) return { state, requests: [] }
        if (message.region === "band" && held === undefined) held = bandGames(zone.nav, model)[zone.nav.card]
        scroll = next
      }
      idleTurns = 0
      touched = true
      break
    }
  }
  zone = settle(zone, model, out)

  // The held game: kept while the band is scrolled or the window is away from it.
  if (held !== undefined && scroll?.band === undefined && !heldAway(zone, model, held)) held = undefined
  // The focused game, for GAME OPTS.' actions: the held one while the window is away from it.
  const away = held !== undefined && heldAway(zone, model, held)
  const focused = away ? held!.id : focusedId(zone, model)
  if (focused !== state.reported) out.push(focused === null ? { _tag: "FocusGame" } : { _tag: "FocusGame", gameId: focused })

  // The catalog: Korri filters and pages it.
  const catalog = model.catalog
  const scrolledAt = scroll?.band === undefined ? -1 : bandGames(zone.nav, model).findIndex(game => game.id === scroll!.band)
  if (!moving && catalog._tag === "Ready" && zoneShown(zone, model) === "games") {
    const draft = zone.nav.find.draft.trim()
    const from = focused === null ? { _tag: "Start" as const } : { _tag: "Around" as const, gameId: focused }
    if (draft.toLocaleLowerCase() !== catalog.query.text.trim().toLocaleLowerCase()) {
      out.push({ _tag: "MoveCatalog", query: query(draft, model), from, count: WINDOW })
      moving = true
    } else if (scrolledAt !== -1) {
      // A scrolled band pages from where the finger left it (ADR 0055), so a long fling keeps loading.
      if ((catalog.after !== undefined && scrolledAt >= catalog.window.length - EDGE) || (catalog.before !== undefined && scrolledAt < EDGE)) {
        out.push({ _tag: "MoveCatalog", query: catalog.query, from: { _tag: "Around", gameId: scroll!.band! }, count: WINDOW })
        moving = true
      }
    } else if (away) {
      // The band is back to its focus, which the window paged away from: bring it back round it.
      out.push({ _tag: "MoveCatalog", query: catalog.query, from, count: WINDOW })
      moving = true
    } else if (focused !== null && zone.nav.zone === "cards") {
      const card = zone.nav.card
      if ((catalog.after !== undefined && card >= catalog.window.length - EDGE) || (catalog.before !== undefined && card < EDGE)) {
        out.push({ _tag: "MoveCatalog", query: catalog.query, from, count: WINDOW })
        moving = true
      }
    }
  }
  // The clock lives while the tally does.
  if (zone.nav.tally === undefined || zone.nav.tally.leaving) tally = undefined
  return {
    state: {
      zone, reported: focused, moving, idleTurns, size,
      ...(tally === undefined ? {} : { tally }),
      ...(touched ? { touched } : {}),
      ...(scroll === undefined ? {} : { scroll }),
      ...(held === undefined ? {} : { held }),
    },
    requests: out,
  }
}

/** Whether the band's held game is out of the band now: the window paged away from it. */
function heldAway(zone: ZoneSurfaceState, model: Model, held: SurfaceGame): boolean {
  return !bandGames(zone.nav, model).some(game => game.id === held.id)
}

/** The target the view has focused, if any: what a second tap on a game opens. */
function focusedTarget(drawn: unknown): string | undefined {
  let found: string | undefined
  const walk = (value: unknown): void => {
    if (found !== undefined) return
    if (Array.isArray(value)) {
      for (const item of value) walk(item)
      return
    }
    if (value === null || typeof value !== "object") return
    const entry = value as Record<string, unknown>
    if ("$image" in entry) return
    if (entry.focused === true && typeof entry.focusId === "string" && entry.focusId !== "") {
      found = entry.focusId
      return
    }
    for (const key in entry) if (key !== "focusPlan") walk(entry[key])
  }
  walk(drawn)
  return found
}

/** A button press, as Pressed delivers it: Back by backRoute (contract 7.2), the rest by src/press.ts. */
function pressed(zone: ZoneSurfaceState, button: SurfaceButton, model: Model, screen: ZoneScreen, out: Request[]): ZoneSurfaceState {
  if (button === "back") {
    const route = backRoute(model.status)
    if (route._tag === "Decline") {
      out.push({ _tag: "RunAction", actionId: route.actionId })
      return zone
    }
    if (route._tag === "Swallow") return zone
  }
  let next = zone
  press(next, button, model, screen, inner => {
    next = run(next, inner, model, out)
  })
  return next
}

/** The results tally holds every press while it shows (navigation's rule). */
function holdsEveryPress(zone: ZoneSurfaceState, model: Model): boolean {
  return zone.nav.tally !== undefined && !zone.nav.tally.leaving && zoneShown(zone, model) === "games"
}

/** The focus targets a view draws: every entry with a focus region and a focusId. */
function drawnTargets(drawn: unknown): Set<string> {
  const ids = new Set<string>()
  const walk = (value: unknown): void => {
    if (Array.isArray(value)) {
      for (const item of value) walk(item)
      return
    }
    if (value === null || typeof value !== "object") return
    const entry = value as Record<string, unknown>
    if ("$image" in entry) return
    if (typeof entry.focusId === "string" && entry.focusId !== "" && typeof entry.focusRegion === "string" && "focused" in entry) ids.add(entry.focusId)
    for (const key in entry) if (key !== "focusPlan") walk(entry[key])
  }
  walk(drawn)
  return ids
}

// The view is a pure function of (state, model): the same two give the last view back.
let lastView: { state: State; model: Model; view: ReturnType<typeof zoneView> } | undefined
let lastOverlay: { state: State; model: Model; view: ReturnType<typeof overlayView> } | undefined

export function view(state: State, model: Model) {
  if (lastView?.state === state && lastView.model === model) return lastView.view
  // The attract's next game changes the attract's game and nothing else: the
  // last view with that game (and its card) put in, not a whole new view.
  const last = lastView
  if (last !== undefined && last.model === model && last.state.size === state.size && last.state.scroll === state.scroll && last.state.held === state.held && onlyAttractMoved(last.state.zone, state.zone)) {
    lastView = { state, model, view: touching(attractStep(last.view, state.zone, model, screenOf(state)), state) }
    return lastView.view
  }
  lastView = { state, model, view: touching(zoneView(state.zone, model, screenOf(state), touchView(state, model)), state) }
  return lastView.view
}

/** What touch changes in the view (ADR 0055): where a finger scrolled each rack, and the held game while the window is away from it. */
function touchView(state: State, model: Model): ZoneTouchView {
  if (state.scroll === undefined && state.held === undefined) return NO_TOUCH
  const at = state.scroll?.band === undefined ? -1 : bandGames(state.zone.nav, model).findIndex(game => game.id === state.scroll!.band)
  return {
    ...(at === -1 ? {} : { bandAt: at }),
    ...(state.held !== undefined && heldAway(state.zone, model, state.held) ? { held: state.held } : {}),
    ...(state.scroll?.saves === undefined ? {} : { saveAt: state.scroll.saves }),
  }
}

const NO_TOUCH: ZoneTouchView = {}

/** The view with `touching`: a tap or a drag came last, so the focus highlight hides (ADR 0055). */
function touching<V extends object>(drawn: V, state: State): V & { touching: boolean } {
  const out = drawn as V & { touching: boolean }
  out.touching = state.touched === true
  return out
}

/** Two states that differ only in the attract's game (navigation's idleCard). */
function onlyAttractMoved(before: ZoneSurfaceState, after: ZoneSurfaceState): boolean {
  if (before === after || before.nav === after.nav) return false
  for (const key in after) {
    if (key !== "nav" && (after as unknown as Record<string, unknown>)[key] !== (before as unknown as Record<string, unknown>)[key]) return false
  }
  for (const key in after.nav) {
    if (key !== "idleCard" && (after.nav as unknown as Record<string, unknown>)[key] !== (before.nav as unknown as Record<string, unknown>)[key]) return false
  }
  return true
}

export function overlay(state: State, model: Model) {
  if (lastOverlay?.state === state && lastOverlay.model === model) return lastOverlay.view
  lastOverlay = { state, model, view: touching(overlayView(state.zone, model, screenOf(state)), state) }
  return lastOverlay.view
}

// The look follows the person (ADR 0018's rule by kind): `shared`.
const preferences = { [LOOK_KEY]: { mark: "shared", initial: ZONE_DEFAULT_LOOK } } as const

// As the portal registered Zone: the main display only.
const displays = "main"

globalThis.surface = { displays, preferences, init, update, view, overlay }

declare global {
  // The one global the contract allows.
  var surface: {
    displays: typeof displays
    preferences: typeof preferences
    init: typeof init
    update: typeof update
    view: typeof view
    overlay: typeof overlay
  }
}
