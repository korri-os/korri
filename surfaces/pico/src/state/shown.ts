/**
 * What the catalog screen shows, from navigation and Korri's model.
 *
 * Korri's status outranks every screen Pico owns: while a game starts, runs or
 * fails, that is the truth about this device. A game's own screen is drawn
 * only while its game is on the shelf, so a launch from it takes the screen
 * the way a launch from the shelf does.
 */
import type { SurfaceGame, SurfaceModel } from "@contracts/surface/korri-surface"
import { type PicoScreenView, picoScreenViewFromModel } from "../pico-screen-view"
import type { Detail, Home, PicoNavigation, Search, Settings } from "./navigation"

export type PicoShown =
  | { readonly _tag: "Settings"; readonly settings: Settings }
  | { readonly _tag: "Find"; readonly search: Search }
  | { readonly _tag: "Detail"; readonly detail: Detail; readonly game: SurfaceGame }
  /** Home also draws Korri's loading, Busy, Running and Problem screens. */
  | { readonly _tag: "Home"; readonly home: Home; readonly view: PicoScreenView }

export function shownScreen(nav: PicoNavigation, korri: SurfaceModel): PicoShown {
  const view = picoScreenViewFromModel(korri)
  const browsing = korri.status._tag === "Browsing"
  const game = nav.detail !== undefined && view._tag === "Shelf" && korri.catalog._tag === "Ready"
    ? korri.catalog.games.find(candidate => candidate.id === nav.detail?.gameId)
    : undefined

  if (nav.settings !== undefined && browsing) return { _tag: "Settings", settings: nav.settings }
  if (nav.find && browsing && game === undefined) return { _tag: "Find", search: nav.search }
  if (nav.detail !== undefined && game !== undefined) return { _tag: "Detail", detail: nav.detail, game }
  return { _tag: "Home", home: nav.home, view }
}

export const runnerOpen = (korri: SurfaceModel): boolean =>
  korri.runnerChoice !== undefined && korri.runnerChoice._tag !== "Closed"

/**
 * Attract may show only over a shelf that is sitting there: never over a
 * running game, a launch, a failure, a library Korri is still reading, or any
 * screen or question the player opened. Those are all things the player is
 * looking at, and decoration must not hide them.
 */
export function canAttract(nav: PicoNavigation, korri: SurfaceModel): boolean {
  const shown = shownScreen(nav, korri)
  return !runnerOpen(korri)
    && shown._tag === "Home"
    && shown.view._tag === "Shelf"
    && nav.home.menu === "Closed"
    && nav.detail === undefined
}

/** Whether attract is on screen now. A press while it is wakes it and does nothing else. */
export const attractShowing = (nav: PicoNavigation, korri: SurfaceModel): boolean =>
  nav.idle._tag === "Attracting" && canAttract(nav, korri)

/** The identity action whose dialog is open. It belongs to Settings. */
export const identityAction = (nav: PicoNavigation): string | null =>
  nav.settings?.question._tag === "Identity" ? nav.settings.question.actionId : null
