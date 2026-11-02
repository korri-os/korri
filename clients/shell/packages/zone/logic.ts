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
 *                  the first press only wakes it
 *     Resized      the window's size (contract 8, A2): Zone's plan for it
 *                  (src/plan/screen.ts), which the focus graph and view()
 *                  both read
 *     Pressed back by backRoute (contract 7.2): Korri's declineActionId as
 *                  RunAction; an unmarked Busy swallows it; else Zone's Back
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
 * Zone's other timers (the tally's count and close, the moments' ways out)
 * have no message: the count is drawn by main.slint on its own clock, and a
 * moment that starts to leave is let go at once.
 *
 * Pure: no timers, no Date, no randomness.
 */
import type {
  CatalogQuery,
  SurfaceStatus,
  SurfaceMessage as Message,
  SurfaceModel as Model,
  SurfaceRequest as Request,
} from "../../../../contracts/generated/surface"
import { isZoneLook, ZONE_DEFAULT_LOOK } from "./src/plan/look"
import { zoneScreen, ZONE_DEFAULT_SIZE, type ZoneScreen } from "./src/plan/screen"
import { press } from "./src/press"
import { bandGames, ZONE_IDLE_AFTER_MS, ZONE_IDLE_CYCLE_MS } from "./src/state/navigation"
import type { ZoneRequest } from "./src/state/requests"
import { update as zoneUpdate, zoneShown, zoneSurfaceStart, type ZoneSurfaceMessage, type ZoneSurfaceState } from "./src/state/surface"
import { overlayView, view as zoneView } from "./src/view"

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
}

/** Zone's plan for the window and the look it is drawn in. */
export const screenOf = (state: State): ZoneScreen => zoneScreen(state.size.width, state.size.height, state.zone.look)

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
  switch (message._tag) {
    case "Resized":
      size = { width: Math.max(0, message.width), height: Math.max(0, message.height) }
      break
    case "Tick": {
      // Zone's idle: the attract opens after ZONE_IDLE_AFTER_MS without a
      // press and moves to the next game every ZONE_IDLE_CYCLE_MS.
      const due = message.idleMs < ZONE_IDLE_AFTER_MS ? 0 : 1 + Math.floor((message.idleMs - ZONE_IDLE_AFTER_MS) / ZONE_IDLE_CYCLE_MS)
      if (due === 0) idleTurns = 0
      while (idleTurns < due) {
        zone = run(zone, { _tag: "Games", message: { _tag: idleTurns === 0 ? "IdleElapsed" : "IdleTicked" } }, model, out)
        idleTurns += 1
      }
      break
    }
    case "KorriPublished": {
      moving = false
      zone = run(zone, { _tag: "KorriPublished" }, model, out)
      // A new window: keep the cursor on the game it was on.
      if (state.reported !== null && model.catalog._tag === "Ready") {
        const index = bandGames(zone.nav, model).findIndex(game => game.id === state.reported)
        if (index !== -1 && index !== zone.nav.card) zone = run(zone, { _tag: "Games", message: { _tag: "FocusedCard", index } }, model, out)
      }
      break
    }
    case "PreferenceStored":
      break
    case "Pressed": {
      idleTurns = 0
      if (message.button === "back") {
        const route = backRoute(model.status)
        if (route._tag === "Decline") {
          out.push({ _tag: "RunAction", actionId: route.actionId })
          break
        }
        if (route._tag === "Swallow") break
      }
      const before = zone
      press(before, message.button, model, screenOf(state), inner => {
        zone = run(zone, inner, model, out)
      })
      break
    }
  }
  zone = settle(zone, model, out)

  // The focused game, for GAME OPTS.' actions.
  const focused = focusedId(zone, model)
  if (focused !== state.reported) out.push(focused === null ? { _tag: "FocusGame" } : { _tag: "FocusGame", gameId: focused })

  // The catalog: Korri filters and pages it.
  const catalog = model.catalog
  if (!moving && catalog._tag === "Ready" && zoneShown(zone, model) === "games") {
    const draft = zone.nav.find.draft.trim()
    const from = focused === null ? { _tag: "Start" as const } : { _tag: "Around" as const, gameId: focused }
    if (draft.toLocaleLowerCase() !== catalog.query.text.trim().toLocaleLowerCase()) {
      out.push({ _tag: "MoveCatalog", query: query(draft, model), from, count: WINDOW })
      moving = true
    } else if (focused !== null && zone.nav.zone === "cards") {
      const card = zone.nav.card
      if ((catalog.after !== undefined && card >= catalog.window.length - EDGE) || (catalog.before !== undefined && card < EDGE)) {
        out.push({ _tag: "MoveCatalog", query: catalog.query, from, count: WINDOW })
        moving = true
      }
    }
  }
  return { state: { zone, reported: focused, moving, idleTurns, size }, requests: out }
}

export function view(state: State, model: Model) {
  return zoneView(state.zone, model, screenOf(state))
}

export function overlay(state: State, model: Model) {
  return overlayView(state.zone, model, screenOf(state))
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
