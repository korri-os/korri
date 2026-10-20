/**
 * Zone's program: every model composed (src/state/surface.ts) and run
 * against Korri's model with one useZoneProgram, its subscriptions, and the
 * GAMES page's props read from it.
 *
 * - The host's four buttons, registered once per host.
 * - Korri's model: every model Korri publishes is a message, so the layers
 *   Korri owns are settled against it in every model.
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
import { catalogGames, launchChoices, playtime, sectionStarts, ZONE_FIND_MAX } from "./state/games"
import {
  bandGames,
  topLayer,
  ZONE_IDLE_CYCLE_MS,
  ZONE_TALLY_CLOSE_MS,
  type ZoneNavigation,
  type ZoneNavMessage,
} from "./state/navigation"
import { zoneActOf } from "./state/tally"
import { ZONE_TALLY_OUT_MS, ZONE_TITLE_OUT_MS, zoneTallyCountMs } from "./plan/motion"
import { deviceNameSetting } from "./state/onboarding"
import type { ZoneRequest } from "./state/requests"
import {
  update,
  zoneSurfaceStart,
  type ZoneSurfaceMessage,
  type ZoneSurfaceState,
} from "./state/surface"
import type { ZoneBandCard } from "./ui/molecules/ZoneCardBand"
import type { ZonePlace } from "./ui/molecules/ZoneCard"
import { performZoneRequest, readZoneGameActions, subscribeZoneHostButtons } from "./zone-host"
import { useZoneIdle } from "./use-zone-idle"
import { useZonePlan } from "./use-zone-plan"
import type { ZonePlan } from "./plan/zone-select"
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
    // A game with only wide art wears it, letterboxed in the cover well like any art.
    art: game.coverArtUrl ?? game.wideArtUrl,
    ...(game.download === undefined ? {} : { download: game.download }),
    section: sections[index],
  }))
}

/**
 * The device's name for the foot: the value of Korri's device-name setting,
 * found as first boot finds it (by its id, `device-name`), editable or not.
 * Absent, the tag is empty.
 */
export const zoneDeviceName = (korri: SurfaceModel): string | undefined => deviceNameSetting(korri)?.value

/** The GAMES page's props from the navigation and Korri's model. */
export function zoneGamesProps(
  nav: ZoneNavigation,
  korri: SurfaceModel,
  host: SurfaceHost,
  dispatch: (message: ZoneNavMessage) => void,
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
      download: game.download,
    },
    launches: game === undefined ? [] : launchChoices(game),
    focusedAction: nav.action,
    find: { ...nav.find, max: ZONE_FIND_MAX },
    finding: top === "keyboard",
    opts: top === "gameopts" && game !== undefined ? {
      gameTitle: game.title,
      actions,
      row: nav.opts.row,
      asking: asking === undefined ? undefined : actions.find(action => action.id === asking.actionId),
      answer: nav.opts.answer,
    } : undefined,
    idle: idleGame === undefined ? undefined : {
      key: `${nav.idleCard}:${idleGame.id}`,
      title: idleGame.title,
      system: idleGame.subtitle,
      act: zoneActOf(idleGame),
      art: idleGame.wideArtUrl ?? idleGame.coverArtUrl,
      plays: idleGame.playCount,
    },
    launchHeld: nav.launch !== undefined && nav.launch.seen && !nav.launch.leaving,
    tally: nav.tally,
    clock: korri.clockLabel,
    device: zoneDeviceName(korri),
    connection: korri.network?.connection,
    build: korri.buildLabel,
    onEvent: dispatch,
  }
}

export interface ZoneSurfaceProgram {
  readonly state: ZoneSurfaceState
  readonly dispatch: (message: ZoneSurfaceMessage) => void
  /** Navigation's own messages: the GAMES page, the tabs and idle report in them. */
  readonly dispatchNav: (message: ZoneNavMessage) => void
  readonly layout: ZonePlan
}

/**
 * The program behind ZoneSurface. `opening` replays messages over the start
 * without performing their requests, so a Caliper part can open mid-way.
 */
export function useZoneSurface(
  korri: SurfaceModel,
  host: SurfaceHost,
  root: RefObject<HTMLElement | null>,
  opening: readonly ZoneSurfaceMessage[] = [],
): ZoneSurfaceProgram {
  const perform = useCallback((request: ZoneRequest) => performZoneRequest(host, request), [host])
  const { model: state, dispatch } = useZoneProgram<ZoneSurfaceState, ZoneSurfaceMessage, SurfaceModel>(
    () => opening.reduce((current, message) => update(current, message, korri).model, zoneSurfaceStart(korri)),
    update,
    korri,
    perform,
  )
  const dispatchNav = useCallback((message: ZoneNavMessage) => dispatch({ _tag: "Games", message }), [dispatch])

  useEffect(() => subscribeZoneHostButtons(host, dispatch), [host, dispatch])
  useEffect(() => {
    dispatch({ _tag: "KorriPublished" })
  }, [korri, dispatch])
  useZoneIdle(root, state.nav, dispatchNav)

  const idle = topLayer(state.nav) === "idle"
  useEffect(() => {
    if (!idle) return
    const cycle = setInterval(() => dispatchNav({ _tag: "IdleTicked" }), ZONE_IDLE_CYCLE_MS)
    return () => clearInterval(cycle)
  }, [idle, dispatchNav])

  // The results tally: its count-up ends (unless a press skipped it), and
  // it closes by itself ZONE_TALLY_CLOSE_MS after it showed.
  const tally = state.nav.tally
  const tallyKey = tally === undefined ? undefined : `${tally.gameId}:${tally.plays}`
  const counting = tally !== undefined && !tally.counted
  useEffect(() => {
    if (tallyKey === undefined) return
    const close = setTimeout(() => dispatchNav({ _tag: "TallyElapsed" }), ZONE_TALLY_CLOSE_MS)
    return () => clearTimeout(close)
  }, [tallyKey, dispatchNav])
  const rows = tally === undefined ? 0 : (tally.seconds === undefined ? 0 : 1) + 1 + (tally.total === undefined ? 0 : 1)
  useEffect(() => {
    if (!counting) return
    const counted = setTimeout(() => dispatchNav({ _tag: "TallyCounted" }), zoneTallyCountMs(rows))
    return () => clearTimeout(counted)
  }, [counting, rows, dispatchNav])

  // The two ways out: once the motion has run, the moment goes.
  const launchLeaving = state.nav.launch?.leaving === true
  useEffect(() => {
    if (!launchLeaving) return
    const gone = setTimeout(() => dispatchNav({ _tag: "TitleCardLeft" }), ZONE_TITLE_OUT_MS)
    return () => clearTimeout(gone)
  }, [launchLeaving, dispatchNav])
  const tallyLeaving = tally?.leaving === true
  useEffect(() => {
    if (!tallyLeaving) return
    const gone = setTimeout(() => dispatchNav({ _tag: "TallyLeft" }), ZONE_TALLY_OUT_MS)
    return () => clearTimeout(gone)
  }, [tallyLeaving, dispatchNav])

  const layout = useZonePlan(root)
  return { state, dispatch, dispatchNav, layout }
}

/** The GAMES page's whole props, the plan's answers included. */
export function zoneGamesPage(
  program: ZoneSurfaceProgram,
  korri: SurfaceModel,
  host: SurfaceHost,
): ZoneGamesProps {
  const { state, dispatchNav, layout } = program
  return {
    ...zoneGamesProps(state.nav, korri, host, dispatchNav),
    strips: layout.strips,
    band: layout.band,
    factsAt: layout.facts,
    keysAt: layout.keys,
  }
}

export interface ZoneGamesProgram {
  readonly nav: ZoneNavigation
  readonly dispatch: (message: ZoneNavMessage) => void
  readonly page: ZoneGamesProps
}

/**
 * The GAMES page alone, for its Caliper part: the same program, opened by
 * navigation messages, with only the page's props handed out.
 */
export function useZoneGames(
  korri: SurfaceModel,
  host: SurfaceHost,
  root: RefObject<HTMLElement | null>,
  opening: readonly ZoneNavMessage[] = [],
): ZoneGamesProgram {
  const program = useZoneSurface(korri, host, root, opening.map(message => ({ _tag: "Games", message }) as const))
  return { nav: program.state.nav, dispatch: program.dispatchNav, page: zoneGamesPage(program, korri, host) }
}
