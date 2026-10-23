/**
 * What the catalog screen shows, from navigation and Korri's model.
 *
 * Korri's status outranks every screen Pico owns: while a game starts, runs or
 * fails, that is the truth about this device. A game's own screen is drawn
 * only while its game is on the shelf, so a launch from it takes the screen
 * the way a launch from the shelf does.
 */
import type { SurfaceGame, SurfaceModel, SurfaceStore } from "@contracts/surface/korri-surface"
import type { PicoInviteNoticeView } from "../pico-invite-view"
import { picoLaunchOpen } from "../pico-launch-view"
import { type PicoScreenView, picoScreenViewFromModel } from "../pico-screen-view"
import type { Detail, Home, PicoNavigation, Search, Settings, StorePlace } from "./navigation"

export type PicoShown =
  | { readonly _tag: "Settings"; readonly settings: Settings }
  | { readonly _tag: "Find"; readonly search: Search }
  | { readonly _tag: "Store"; readonly place: StorePlace; readonly store: SurfaceStore }
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
  if (nav.store && browsing && game === undefined && korri.store !== undefined) {
    return { _tag: "Store", place: nav.storePlace, store: korri.store }
  }
  if (nav.find && browsing && game === undefined) return { _tag: "Find", search: nav.search }
  if (nav.detail !== undefined && game !== undefined) return { _tag: "Detail", detail: nav.detail, game }
  return { _tag: "Home", home: nav.home, view }
}

export const runnerOpen = (korri: SurfaceModel): boolean =>
  korri.runnerChoice !== undefined && korri.runnerChoice._tag !== "Closed"

/** Korri shows a game's launch settings. Like the runner picker, it covers
 * everything else Pico draws. */
export const launchOpen = picoLaunchOpen

/**
 * Attract may show only over a shelf that is sitting there: never over a
 * running game, a launch, a failure, a library Korri is still reading, or any
 * screen or question the player opened. Those are all things the player is
 * looking at, and decoration must not hide them.
 */
export function canAttract(nav: PicoNavigation, korri: SurfaceModel): boolean {
  const shown = shownScreen(nav, korri)
  return !runnerOpen(korri)
    && !launchOpen(korri)
    && shown._tag === "Home"
    && shown.view._tag === "Shelf"
    && nav.home.menu === "Closed"
    && nav.detail === undefined
}

/** Home is showing the library: the shelf, or why there is none. Not a
 * launch, a running game, a failure, or anything else Korri's status says. */
export const picoLibraryShown = (view: PicoScreenView): boolean =>
  view._tag === "Shelf" || view._tag === "Loading" || view._tag === "Empty" || view._tag === "Failed"

/**
 * Invite notices show over the idle home and Find, and nowhere else: not
 * over a launch, a question, Settings or a game's own screen.
 */
export function invitesNoticeable(nav: PicoNavigation, korri: SurfaceModel): boolean {
  if (runnerOpen(korri) || launchOpen(korri)) return false
  const shown = shownScreen(nav, korri)
  return shown._tag === "Find" || (shown._tag === "Home" && picoLibraryShown(shown.view))
}

/** The notices on screen now, newest first, in Korri's words. */
export function inviteNotices(nav: PicoNavigation, korri: SurfaceModel): readonly PicoInviteNoticeView[] {
  if (!invitesNoticeable(nav, korri)) return []
  const invites = korri.invites ?? []
  return nav.invites.showing.flatMap(({ inviteId }) => {
    const invite = invites.find(candidate => candidate.id === inviteId)
    return invite === undefined ? [] : [{
      id: invite.id,
      personName: invite.personName,
      gameTitle: invite.gameTitle,
      deviceLabel: invite.deviceLabel,
    }]
  })
}

/** Whether attract is on screen now. A press while it is wakes it and does nothing else. */
export const attractShowing = (nav: PicoNavigation, korri: SurfaceModel): boolean =>
  nav.idle._tag === "Attracting" && canAttract(nav, korri)

/** The identity action whose dialog is open. It belongs to Settings. */
export const identityAction = (nav: PicoNavigation): string | null =>
  nav.settings?.question._tag === "Identity" ? nav.settings.question.actionId : null
