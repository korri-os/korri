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
