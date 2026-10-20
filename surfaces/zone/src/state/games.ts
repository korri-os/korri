/**
 * The GAMES page's pure rules: which games the band shows, where each
 * section starts, what the action row offers for a game, and how a key on
 * the find keyboard changes the draft.
 *
 * Plain functions of plain values. The navigation update (navigation.ts)
 * calls them to keep its indices honest; the binding (use-zone-games.ts)
 * calls them to hand the page its props. Nothing here formats a date: that
 * needs a clock, and the page owns words.
 */
import type { SurfaceCatalog, SurfaceGame } from "@contracts/surface/korri-surface"

/** The keyboard's four layers, as ZoneKeyboard names them. */
export type ZoneFindLayer = "letters" | "shift" | "symbols" | "uri"

/** What a key does when A lands on it, as ZoneKeyboard reports it. */
export type ZoneFindKey =
  | { readonly kind: "char"; readonly char: string }
  | { readonly kind: "space" }
  | { readonly kind: "delete" }
  | { readonly kind: "done" }
  | { readonly kind: "layer"; readonly layer: ZoneFindLayer }

/**
 * Find by text. `draft` is the filter: the band shows the titles that
 * contain it, live, while the keyboard is up and after DONE. Empty means
 * no filter. `key` is the focused key's id; `caret` the insertion point.
 */
export interface ZoneFind {
  readonly draft: string
  readonly caret: number
  readonly layer: ZoneFindLayer
  readonly key: string
}

export const ZONE_FIND_START: ZoneFind = { draft: "", caret: 0, layer: "letters", key: "q" }

/** The most characters a find draft holds: a title fragment, not a sentence. */
export const ZONE_FIND_MAX = 40

/** The catalog's games, all of them; none while it is not Ready. */
export function catalogGames(catalog: SurfaceCatalog): readonly SurfaceGame[] {
  return catalog._tag === "Ready" ? catalog.games : []
}

/** The games the band shows: titles containing the draft, case-insensitive. */
export function visibleGames(catalog: SurfaceCatalog, draft: string): readonly SurfaceGame[] {
  const games = catalogGames(catalog)
  const needle = draft.trim().toLocaleLowerCase()
  return needle === "" ? games : games.filter(game => game.title.toLocaleLowerCase().includes(needle))
}

/**
 * The section caption that opens before each game, or undefined. A divider
 * opens when a game's section differs from the game before it in the band;
 * a game with no section never opens one.
 */
export function sectionStarts(games: readonly SurfaceGame[]): readonly (string | undefined)[] {
  return games.map((game, index) =>
    game.section !== undefined && game.section !== games[index - 1]?.section ? game.section : undefined)
}

/** One button in the action row: what it says and where it launches. */
export interface ZoneLaunchChoice {
  /** The button's id in the row. */
  readonly id: string
  readonly verb: "play" | "continue"
  /** Where: absent for a one-place game; `here` for the first of a choice. */
  readonly place?: { readonly here: true } | { readonly here: false; readonly label: string }
  readonly locationId?: string
}

/**
 * What the action row offers for a game.
 *
 *  - No locations: one button, PLAY (CONTINUE when resumable).
 *  - One location: one button too, carrying that location's id, since Korri
 *    requires it whenever it publishes locations.
 *  - Two or more: one button per location, HERE for the first (Korri orders
 *    the local device first), ON <LABEL> for each other.
 */
export function launchChoices(game: SurfaceGame): readonly ZoneLaunchChoice[] {
  const verb = game.resumable === true ? "continue" : "play"
  const locations = game.launchLocations ?? []
  if (locations.length === 0) return [{ id: verb, verb }]
  if (locations.length === 1) return [{ id: `${verb}:${locations[0]!.id}`, verb, locationId: locations[0]!.id }]
  return locations.map((location, index) => ({
    id: `${verb}:${location.id}`,
    verb,
    place: index === 0 ? { here: true } as const : { here: false, label: location.label } as const,
    locationId: location.id,
  }))
}

/** True when the person must pick a place before anything launches. */
export const needsPlace = (game: SurfaceGame): boolean => launchChoices(game).length > 1

/** Whole hours and the minutes left over, from Korri's summed seconds. */
export function playtime(seconds: number): { readonly hours: number; readonly minutes: number } {
  const minutes = Math.floor(Math.max(0, seconds) / 60)
  return { hours: Math.floor(minutes / 60), minutes: minutes % 60 }
}

/** A key on the find keyboard, applied to the draft. DONE is the caller's. */
export function pressFindKey(find: ZoneFind, key: ZoneFindKey): ZoneFind {
  switch (key.kind) {
    case "char":
      return insert(find, key.char)
    case "space":
      return insert(find, " ")
    case "delete":
      return deleteBack(find)
    case "layer":
      return { ...find, layer: key.layer }
    case "done":
      return find
  }
}

/** One character out before the caret; nothing when the caret is at the start. */
export function deleteBack(find: ZoneFind): ZoneFind {
  if (find.caret === 0) return find
  return {
    ...find,
    draft: find.draft.slice(0, find.caret - 1) + find.draft.slice(find.caret),
    caret: find.caret - 1,
  }
}

function insert(find: ZoneFind, text: string): ZoneFind {
  if (find.draft.length + text.length > ZONE_FIND_MAX) return find
  return {
    ...find,
    draft: find.draft.slice(0, find.caret) + text + find.draft.slice(find.caret),
    caret: find.caret + text.length,
  }
}
