/**
 * The launch title card and the results tally, as pure values.
 *
 *     launch   the title card over GAMES. It opens when PLAY or CONTINUE is
 *              chosen (`zoneLaunchOpen`) and holds Korri's facts as they
 *              stood then: the title, the system, and the act, which is
 *              the session about to start (playCount + 1; 1 when Korri has
 *              recorded none, since the treaty says the count is absent
 *              together with lastPlayedAt). `seen` turns true once Korri's
 *              status names the game (Busy, Confirm or Running); after
 *              that the card leaves as soon as the status names it no more.
 *     session  the counts Korri published for a game when it was first
 *              seen Running, kept until the next Browsing.
 *     tally    the results of that session, when and only when the same
 *              game's playCount rose by exactly 1 between the Running and
 *              the next Browsing. TIME is the rise in totalPlaytimeSeconds
 *              (absent when it cannot be known or is 0), PLAYS the new
 *              count, TOTAL the new total. `counted` is false while the
 *              count-up runs: the first input skips it, the next closes.
 *
 * Both leave by playing their way out (`leaving`), and the runtime removes
 * them when that motion ends.
 *
 * Plain functions of plain values; navigation.ts calls them on every
 * message, with Korri's model beside it. Nothing here reads a clock: the
 * count-up, the 8 s close and the two ways out are the runtime's timers,
 * which report `TallyCounted`, `TallyElapsed`, `TallyLeft` and
 * `TitleCardLeft`.
 */
import type { SurfaceGame, SurfaceModel, SurfaceStatus } from "@contracts/surface/korri-surface"
import { catalogGames } from "./games"

export interface ZoneLaunch {
  readonly gameId: string
  readonly title: string
  readonly system?: string
  /** The session about to start: playCount + 1. */
  readonly act: number
  /** Korri's status has named this launch at least once. */
  readonly seen: boolean
  /** Korri has let go of the game: the card plays its way out (M3) and goes. */
  readonly leaving: boolean
}

export interface ZoneSession {
  readonly gameId: string
  /** playCount when the game was first seen Running; absent is none recorded. */
  readonly playCount?: number
  readonly seconds?: number
}

export interface ZoneTally {
  readonly gameId: string
  readonly title: string
  /** This session's length in seconds; absent when it cannot be known or is 0. */
  readonly seconds?: number
  /** The new count. */
  readonly plays: number
  /** The new total in seconds, when Korri publishes one. */
  readonly total?: number
  /** False while the count-up runs. */
  readonly counted: boolean
  /** Closed: it plays its way out (M5) and goes; it holds no press any more. */
  readonly leaving: boolean
}

/** The game a launch names, as the catalog publishes it now. */
const gameOf = (korri: SurfaceModel, gameId: string): SurfaceGame | undefined =>
  catalogGames(korri.catalog).find(game => game.id === gameId)

/** The act a launch opens: the next session, 1 when none is recorded. */
export const zoneActOf = (game: { readonly playCount?: number }): number => (game.playCount ?? 0) + 1

/** The title card for a launch of `gameId`, or nothing when the catalog has no such game. */
export function zoneLaunchOpen(korri: SurfaceModel, gameId: string): ZoneLaunch | undefined {
  const game = gameOf(korri, gameId)
  if (game === undefined || game.title === "") return undefined
  return {
    gameId,
    title: game.title,
    ...(game.subtitle === undefined ? {} : { system: game.subtitle }),
    act: zoneActOf(game),
    seen: false,
    leaving: false,
  }
}

/**
 * Whether Korri's status names the launch: Busy, Confirm or Running for
 * this game, or with no game named at all (the work of the launch that has
 * just been asked for).
 */
const names = (status: SurfaceStatus, gameId: string): boolean => {
  switch (status._tag) {
    case "Busy":
    case "Confirm":
    case "Running":
      return status.gameId === undefined || status.gameId === gameId
    case "ChooseSave":
      return status.gameId === gameId
    case "Browsing":
    case "Problem":
      return false
  }
}

/**
 * The title card against Korri's status. Before Korri has answered it
 * waits (a clock tick is Browsing too), and a Problem sends it out; once
 * Korri has named the launch, the card leaves as soon as the status names
 * it no more. Leaving is final: the runtime's timer removes it.
 */
export function settleLaunch(launch: ZoneLaunch | undefined, status: SurfaceStatus): ZoneLaunch | undefined {
  if (launch === undefined || launch.leaving) return launch
  if (names(status, launch.gameId)) return launch.seen ? launch : { ...launch, seen: true }
  if (launch.seen || status._tag === "Problem") return { ...launch, leaving: true }
  return launch
}

/** A press before Korri has answered: the card was only Zone's announcement, and goes. */
export const dropLaunch = (launch: ZoneLaunch): ZoneLaunch => (launch.leaving ? launch : { ...launch, leaving: true })

/**
 * The session against Korri's status: a Running game is noted the first
 * time it is seen with its counts as they stood; another game Running
 * replaces it; Browsing ends it (and `settleTally` reads it first).
 */
export function settleSession(session: ZoneSession | undefined, korri: SurfaceModel): ZoneSession | undefined {
  const status = korri.status
  if (status._tag === "Browsing") return undefined
  if (status._tag !== "Running" || status.gameId === undefined) return session
  if (session?.gameId === status.gameId) return session
  const game = gameOf(korri, status.gameId)
  if (game === undefined) return undefined
  return {
    gameId: game.id,
    ...(game.playCount === undefined ? {} : { playCount: game.playCount }),
    ...(game.totalPlaytimeSeconds === undefined ? {} : { seconds: game.totalPlaytimeSeconds }),
  }
}

/**
 * The tally a session leaves when Korri goes back to Browsing: only when
 * the game's playCount rose by exactly one. Anything else (no count yet,
 * no rise, a rise of two, the game gone) is ambiguous, and Zone shows
 * nothing rather than invent a result.
 */
export function zoneTallyOf(session: ZoneSession | undefined, korri: SurfaceModel): ZoneTally | undefined {
  if (session === undefined || korri.status._tag !== "Browsing") return undefined
  const game = gameOf(korri, session.gameId)
  if (game === undefined || game.playCount === undefined) return undefined
  if (game.playCount !== (session.playCount ?? 0) + 1) return undefined
  // Before the first recorded session there is no total: it starts at 0.
  const before = session.seconds ?? (session.playCount === undefined ? 0 : undefined)
  const now = game.totalPlaytimeSeconds
  const rise = now === undefined || before === undefined ? undefined : now - before
  return {
    gameId: game.id,
    title: game.title,
    ...(rise === undefined || rise <= 0 ? {} : { seconds: rise }),
    plays: game.playCount,
    ...(now === undefined ? {} : { total: now }),
    counted: false,
    leaving: false,
  }
}

/** An input while the tally shows: the first skips the count-up, the next closes it. */
export const pressTally = (tally: ZoneTally): ZoneTally =>
  tally.counted ? { ...tally, leaving: true } : { ...tally, counted: true }
