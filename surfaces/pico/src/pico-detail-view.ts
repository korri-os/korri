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
  /** "CONTINUE" when Korri says the game resumes; "PLAY" otherwise. */
  readonly primaryLabel: "CONTINUE" | "PLAY"
  /**
   * Facts about play, in display order, each as a figure and its caption. Empty
   * when Korri has never seen the game played, and the screen says so instead.
   */
  readonly stats: readonly { readonly figure: string; readonly caption: string }[]
  readonly locations?: readonly PicoShelfLocation[]
  /** "1.5 GB DOWNLOAD" when playing first fetches something; see picoDownloadLabel. */
  readonly download?: string
}

export function picoDetailViewFromGame(game: SurfaceGame): PicoDetailView {
  const download = picoDownloadLabel(game)
  return {
    id: game.id,
    title: game.title,
    ...(game.subtitle === undefined ? {} : { subtitle: game.subtitle }),
    ...(game.coverArtUrl === undefined ? {} : { artUrl: game.coverArtUrl }),
    primaryLabel: game.resumable === true ? "CONTINUE" : "PLAY",
    stats: picoStatsFor(game),
    ...(download === undefined ? {} : { download }),
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
/** What Korri said about how much a game has been played, as display chips.
 * Empty when Korri said nothing — the surface states no figure it was not given. */
export function picoStatsFor(
  game: Pick<SurfaceGame, "playCount" | "totalPlaytimeSeconds">,
): PicoDetailView["stats"] {
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

/**
 * What a game still has to fetch before it plays, as one chip: "1.5 GB
 * DOWNLOAD", or "DOWNLOAD NEEDED" when Korri knows that but not how much.
 * Absent when Korri said nothing, which is every game that plays at once.
 */
export function picoDownloadLabel(game: Pick<SurfaceGame, "download">): string | undefined {
  if (game.download === undefined) return undefined
  return game.download.bytes === undefined ? "DOWNLOAD NEEDED" : `${picoBytesLabel(game.download.bytes)} DOWNLOAD`
}

/** "1.5 GB", "612 MB", "48 KB": decimal units, one decimal below ten. */
export function picoBytesLabel(bytes: number): string {
  const scale = (value: number, unit: string) => {
    const text = value < 10 ? value.toFixed(1) : String(Math.round(value))
    return `${text.endsWith(".0") ? text.slice(0, -2) : text} ${unit}`
  }
  if (bytes >= 1e9) return scale(bytes / 1e9, "GB")
  if (bytes >= 1e6) return scale(bytes / 1e6, "MB")
  if (bytes >= 1e3) return scale(bytes / 1e3, "KB")
  return `${Math.round(bytes)} B`
}

/** Whole hours and minutes, in the terse uppercase Pico speaks everywhere. */
export function playtimeLabel(seconds: number): string {
  const minutes = Math.floor(seconds / 60)
  const hours = Math.floor(minutes / 60)
  const rest = minutes % 60
  if (hours === 0) return `${rest}M`
  return rest === 0 ? `${hours}H` : `${hours}H ${rest}M`
}
