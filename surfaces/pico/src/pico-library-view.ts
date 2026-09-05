import type { SurfaceCatalog } from "@contracts/surface/korri-surface"
import type { PicoShelfGame } from "./pico-shelf-game"
import { picoHomeViewFromCatalog } from "./pico-home-view"

/** The collection filter: a section Korri grouped by, or the whole library. */
export const PICO_ALL_SECTIONS = "ALL"

export interface PicoLibraryView {
  /** `ALL` first, then the sections Korri actually used, in catalog order. */
  readonly sections: readonly string[]
  readonly results: readonly PicoShelfGame[]
  readonly query: string
}

/**
 * The library, narrowed by what the user typed and which collection they chose.
 *
 * Both filters are the surface's own: Korri published the catalog once, and
 * asking it again to search would make a screen that works offline depend on a
 * round trip. The match is over title and provenance together, because "switch"
 * is how someone looks for the Switch games and it appears in neither title.
 */
export function picoLibraryViewFrom(
  catalog: SurfaceCatalog,
  query: string,
  section: string,
): PicoLibraryView {
  const home = picoHomeViewFromCatalog(catalog)
  const games = home._tag === "Shelf" ? home.games : []
  const needle = query.trim().toLowerCase()

  return {
    sections: [PICO_ALL_SECTIONS, ...sectionsOf(catalog)],
    query,
    results: games.filter((game) => {
      const inSection =
        section === PICO_ALL_SECTIONS || sectionOf(catalog, game.id) === section
      if (!inSection) return false
      if (needle === "") return true
      const haystack = `${game.title} ${game.subtitle ?? ""}`.toLowerCase()
      return haystack.includes(needle)
    }),
  }
}

/**
 * The sections Korri used, in the order they first appear. Not sorted: the
 * treaty says games sharing a section arrive consecutively, so catalog order is
 * already the order Korri intends them to be read in.
 */
function sectionsOf(catalog: SurfaceCatalog): readonly string[] {
  if (catalog._tag !== "Ready") return []
  const seen: string[] = []
  for (const game of catalog.games) {
    const section = game.section
    if (section !== undefined && !seen.includes(section)) seen.push(section)
  }
  return seen
}

function sectionOf(catalog: SurfaceCatalog, gameId: string): string | undefined {
  if (catalog._tag !== "Ready") return undefined
  return catalog.games.find((game) => game.id === gameId)?.section
}

/** A run of games under one of Korri's section captions. */
export interface PicoCollection {
  readonly title: string
  readonly games: readonly PicoShelfGame[]
}

/**
 * The library grouped the way Korri grouped it.
 *
 * Games Korri left ungrouped collect under "GAMES" rather than "Other" or
 * "Uncategorised": Korri said nothing about them, so neither does Pico, and a
 * heading that editorialises is a fact the surface invented.
 */
export function picoCollectionsFrom(
  games: readonly PicoShelfGame[],
): readonly PicoCollection[] {
  const collections: { title: string; games: PicoShelfGame[] }[] = []
  for (const game of games) {
    const title = (game.section ?? "GAMES").toUpperCase()
    const existing = collections.find((candidate) => candidate.title === title)
    if (existing === undefined) collections.push({ title, games: [game] })
    else existing.games.push(game)
  }
  return collections
}

/**
 * Which game leads the hero, and why.
 *
 * Korri publishes no "featured" flag, and inventing one would be an editorial
 * claim the device has no basis for. The most recently played game is a fact
 * Korri does publish, so that is the rule — and the screen names it, so the
 * user reads "last played" rather than assuming a recommendation.
 */
export function picoHeroPick(
  games: readonly PicoShelfGame[],
): { readonly game: PicoShelfGame; readonly reason?: string } | undefined {
  const timed = games.filter((game) => game.lastPlayedAt !== undefined)
  const latest = timed.reduce<PicoShelfGame | undefined>(
    (best, game) =>
      best === undefined || (game.lastPlayedAt ?? 0) > (best.lastPlayedAt ?? 0)
        ? game
        : best,
    undefined,
  )
  if (latest !== undefined) return { game: latest, reason: "LAST PLAYED" }
  const first = games[0]
  return first === undefined ? undefined : { game: first }
}
