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
 * Zone's timers (idle, the tally's count and close, the moments' ways out)
 * have no message in the contract: the attract and the count are drawn by
 * main.slint on its own clock, and a moment that starts to leave is let go
 * at once.
 *
 * Pure: no timers, no Date, no randomness.
 */
import type {
  CatalogQuery,
  SurfaceMessage as Message,
  SurfaceModel as Model,
  SurfaceRequest as Request,
} from "../../../../contracts/generated/surface"
import { isZoneLook, ZONE_DEFAULT_LOOK } from "./src/plan/look"
import { press } from "./src/press"
import { bandGames } from "./src/state/navigation"
import type { ZoneRequest } from "./src/state/requests"
import { update as zoneUpdate, zoneShown, zoneSurfaceStart, type ZoneSurfaceMessage, type ZoneSurfaceState } from "./src/state/surface"
import { overlayView, view as zoneView } from "./src/view"

export interface State {
  readonly zone: ZoneSurfaceState
  /** The focused game Korri last heard of (FocusGame). */
  readonly reported: string | null
  /** A MoveCatalog is out; no second one until Korri publishes. */
  readonly moving: boolean
  /** Presses so far: main.slint restarts its idle clock when this changes. */
  readonly presses: number
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
  return { zone, reported: null, moving: false, presses: 0 }
}

export function update(state: State, message: Message, model: Model): { state: State; requests: Request[] } {
  const out: Request[] = []
  let zone = state.zone
  let moving = state.moving
  let presses = state.presses
  switch (message._tag) {
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
      presses += 1
      const before = zone
      press(before, message.button, model, inner => {
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
  return { state: { zone, reported: focused, moving, presses }, requests: out }
}

export function view(state: State, model: Model) {
  return { ...zoneView(state.zone, model), presses: state.presses }
}

export function overlay(state: State, model: Model) {
  return overlayView(state.zone, model)
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
