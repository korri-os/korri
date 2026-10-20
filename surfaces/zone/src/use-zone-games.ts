/**
 * The home program: Zone's navigation run against Korri's model, with its
 * subscriptions, and the GAMES page's props read from it.
 *
 * - The host's four buttons, registered once per host.
 * - Korri's model: every model Korri publishes is a message, so the layers
 *   Korri owns are settled against it.
 * - The idle timer, and while idle shows, the title card's four-second cycle.
 *
 * The props are plain values: the band's cards with their sections, the
 * focused game's facts and launch choices, GAME OPTS. with Korri's actions
 * read through the host door, and the idle card. The page never sees the
 * navigation model or the host.
 *
 * `opening` replays messages over the start, so a Caliper part can open on
 * GAME OPTS. or the keyboard and still answer input as the device does.
 */
import type { SurfaceGame, SurfaceHost, SurfaceModel } from "@contracts/surface/korri-surface"
import { useCallback, useEffect, type RefObject } from "react"
import type { ZoneGamesProps } from "./pages/ZoneGames"
import type { ZoneMessage } from "./state/messages"
import { catalogGames, launchChoices, playtime, sectionStarts } from "./state/games"
import {
  bandGames,
  topLayer,
  update,
  ZONE_IDLE_CYCLE_MS,
  ZONE_START,
  type ZoneNavigation,
} from "./state/navigation"
import type { ZoneRequest } from "./state/requests"
import type { ZoneBandCard } from "./ui/molecules/ZoneCardBand"
import type { ZonePlace } from "./ui/molecules/ZoneCard"
import { performZoneRequest, readZoneGameActions, subscribeZoneHostButtons } from "./zone-host"
import { useZoneIdle } from "./use-zone-idle"
import { useZonePlan } from "./use-zone-plan"
import { useZoneProgram } from "./use-zone-program"

/**
 * Where a game plays, as its tags say it. No published place: here. One:
 * that place by its name. A choice: the first is here (Korri orders the
 * local device first), the rest by name.
 */
function placesOf(game: SurfaceGame): readonly ZonePlace[] {
  const locations = game.launchLocations ?? []
  if (locations.length === 0) return [{ label: "Here", here: true }]
  if (locations.length === 1) return [{ label: locations[0]!.label, here: false }]
  return locations.map((location, index) => ({ label: location.label, here: index === 0 }))
}

export function zoneBandCards(games: readonly SurfaceGame[]): readonly ZoneBandCard[] {
  const sections = sectionStarts(games)
  return games.map((game, index) => ({
    id: game.id,
    title: game.title,
    system: game.subtitle,
    places: placesOf(game),
    resumable: game.resumable === true,
    art: game.coverArtUrl,
    section: sections[index],
  }))
}

/** The GAMES page's props from the navigation and Korri's model. */
export function zoneGamesProps(
  nav: ZoneNavigation,
  korri: SurfaceModel,
  host: SurfaceHost,
  dispatch: (message: ZoneMessage) => void,
): Omit<ZoneGamesProps, "strips" | "band" | "factsAt" | "keysAt"> {
  const games = bandGames(nav, korri)
  const game = games[nav.card]
  const top = topLayer(nav)
  const actions = top === "gameopts" && game !== undefined ? readZoneGameActions(host, game.id) : []
  const asking = nav.opts.asking
  const idleGame = top === "idle" ? catalogGames(korri.catalog)[nav.idleCard] : undefined
  const catalog = korri.catalog
  return {
    tab: nav.tab,
    tabCursor: nav.tabCursor,
    zone: nav.zone,
    catalog: catalog._tag === "Ready" && catalog.games.length === 0 ? { _tag: "Empty" }
      : catalog._tag === "Ready" ? { _tag: "Ready" } : catalog,
    cards: zoneBandCards(games),
    focusedCard: nav.card,
    facts: game === undefined ? undefined : {
      lastPlayedAt: game.lastPlayedAt,
      playCount: game.playCount,
      playtime: game.totalPlaytimeSeconds === undefined ? undefined : playtime(game.totalPlaytimeSeconds),
    },
    launches: game === undefined ? [] : launchChoices(game),
    focusedAction: nav.action,
    find: nav.find,
    finding: top === "keyboard",
    opts: top === "gameopts" && game !== undefined ? {
      gameTitle: game.title,
      actions,
      row: nav.opts.row,
      asking: asking === undefined ? undefined : actions.find(action => action.id === asking.actionId),
      answer: nav.opts.answer,
    } : undefined,
    idle: idleGame === undefined ? undefined : { title: idleGame.title, act: idleGame.subtitle },
    clock: korri.clockLabel,
    connection: korri.network?.connection,
    build: korri.buildLabel,
    onEvent: dispatch,
  }
}

export interface ZoneGamesProgram {
  readonly nav: ZoneNavigation
  readonly dispatch: (message: ZoneMessage) => void
  readonly page: ZoneGamesProps
}

export function useZoneGames(
  korri: SurfaceModel,
  host: SurfaceHost,
  root: RefObject<HTMLElement | null>,
  opening: readonly ZoneMessage[] = [],
): ZoneGamesProgram {
  const perform = useCallback((request: ZoneRequest) => performZoneRequest(host, request), [host])
  const { model: nav, dispatch } = useZoneProgram<ZoneNavigation, ZoneMessage, SurfaceModel>(
    () => opening.reduce((current, message) => update(current, message, korri).model, update(ZONE_START, { _tag: "KorriPublished" }, korri).model),
    update,
    korri,
    perform,
  )

  useEffect(() => subscribeZoneHostButtons(host, dispatch), [host, dispatch])
  useEffect(() => {
    dispatch({ _tag: "KorriPublished" })
  }, [korri, dispatch])
  useZoneIdle(root, nav, dispatch)

  const idle = topLayer(nav) === "idle"
  useEffect(() => {
    if (!idle) return
    const cycle = setInterval(() => dispatch({ _tag: "IdleTicked" }), ZONE_IDLE_CYCLE_MS)
    return () => clearInterval(cycle)
  }, [idle, dispatch])

  const layout = useZonePlan(root)
  return {
    nav,
    dispatch,
    page: { ...zoneGamesProps(nav, korri, host, dispatch), strips: layout.strips, band: layout.band, factsAt: layout.facts, keysAt: layout.keys },
  }
}
