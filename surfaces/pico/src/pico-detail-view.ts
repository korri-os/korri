import type { SurfaceGame } from "@contracts/surface/korri-surface"
import type { PicoShelfLocation } from "./pico-shelf-game"

/**
 * A game as its own screen presents it.
 *
 * Wider than `PicoShelfGame` — this is the one place that shows how much a game
 * has been played — but still narrower than the treaty, so a component here
 * cannot reach for a field the design never accounted for. Every value is a
 * string Korri's facts were turned into exactly once, so no component formats.
 */
export interface PicoDetailView {
  readonly id: string
  readonly title: string
  readonly subtitle?: string
  readonly artUrl?: string
  readonly wideArtUrl?: string
  /** "CONTINUE" when Korri says the game resumes; "PLAY" otherwise. */
  readonly primaryLabel: "CONTINUE" | "PLAY"
  /**
   * Facts about play, in display order, each as a figure and its caption. Empty
   * when Korri has never seen the game played, and the screen says so instead.
   */
  readonly stats: readonly { readonly figure: string; readonly caption: string }[]
  readonly locations?: readonly PicoShelfLocation[]
}

export function picoDetailViewFromGame(game: SurfaceGame): PicoDetailView {
  return {
    id: game.id,
    title: game.title,
    ...(game.subtitle === undefined ? {} : { subtitle: game.subtitle }),
    ...(game.coverArtUrl === undefined ? {} : { artUrl: game.coverArtUrl }),
    ...(game.wideArtUrl === undefined ? {} : { wideArtUrl: game.wideArtUrl }),
    primaryLabel: game.resumable === true ? "CONTINUE" : "PLAY",
    stats: statsFor(game),
    ...(game.launchLocations === undefined || game.launchLocations.length === 0
      ? {}
      : { locations: game.launchLocations }),
  }
}

/**
 * Only the facts Korri stated. A `playCount` of 0 is a fact and is shown; an
 * absent one is not and is not. `lastPlayedAt` is deliberately not rendered:
 * turning it into "2 days ago" needs a clock, and the treaty gives the surface
 * a preformatted `clockLabel` precisely so it never reads one itself.
 */
function statsFor(game: SurfaceGame): PicoDetailView["stats"] {
  const stats: { figure: string; caption: string }[] = []
  if (game.playCount !== undefined) {
    stats.push({
      figure: String(game.playCount),
      caption: game.playCount === 1 ? "PLAY" : "PLAYS",
    })
  }
  if (game.totalPlaytimeSeconds !== undefined) {
    stats.push({ figure: playtimeLabel(game.totalPlaytimeSeconds), caption: "PLAYED" })
  }
  return stats
}

/** Whole hours and minutes, in the terse uppercase Pico speaks everywhere. */
export function playtimeLabel(seconds: number): string {
  const minutes = Math.floor(seconds / 60)
  const hours = Math.floor(minutes / 60)
  const rest = minutes % 60
  if (hours === 0) return `${rest}M`
  return rest === 0 ? `${hours}H` : `${hours}H ${rest}M`
}
