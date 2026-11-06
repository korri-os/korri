import type { SurfaceCatalog, SurfaceGame } from "./model"
import type { PicoShelfGame } from "./pico-shelf-game"

/**
 * What the home screen is currently showing.
 *
 * One closed set of cases, so the page renders a state rather than testing a
 * pile of booleans and nullable arrays. Every case the treaty can produce is
 * named here; a new catalog state becomes a compile error at the conversion
 * below rather than a blank screen at runtime.
 */
export type PicoHomeView =
  | { readonly _tag: "Loading" }
  | { readonly _tag: "Empty" }
  | { readonly _tag: "Failed"; readonly message: string }
  | { readonly _tag: "Shelf"; readonly games: readonly PicoShelfGame[] }

/**
 * Convert Korri's catalog into the home's own state, once, at the seam.
 *
 * A `Ready` catalog with no games is `Empty`: the treaty allows both spellings
 * of "there is nothing to play", and letting them diverge would give the shelf
 * an empty strip to render and the user a screen that says nothing at all.
 */
export function picoHomeViewFromCatalog(catalog: SurfaceCatalog): PicoHomeView {
  // Korri replaces the catalog whole when it changes, and a library can hold
  // thousands of games. One view per catalog object keeps every game's view
  // the same object between renders, so a cart that did not change is not
  // drawn again (PicoCart is memoized).
  const cached = homeViews.get(catalog)
  if (cached !== undefined) return cached
  const view = homeViewFrom(catalog)
  homeViews.set(catalog, view)
  return view
}

const homeViews = new WeakMap<SurfaceCatalog, PicoHomeView>()

function homeViewFrom(catalog: SurfaceCatalog): PicoHomeView {
  switch (catalog._tag) {
    case "Loading":
      return { _tag: "Loading" }
    case "Error":
      return { _tag: "Failed", message: catalog.message }
    case "Empty":
      return { _tag: "Empty" }
    case "Ready":
      return catalog.games.length === 0
        ? { _tag: "Empty" }
        : {
            _tag: "Shelf",
            games: catalog.games.map(picoShelfGame),
          }
  }
}

/** One game as the home holds it, once per game object: Korri's model keeps
 * an unchanged game's object across windows, so the carts built from it are
 * kept too. Fields are set one by one, in the treaty's order of interest:
 * QuickJS spreads an object several times slower. */
export function picoShelfGame(game: SurfaceGame): PicoShelfGame {
  const known = shelfGames.get(game)
  if (known !== undefined) return known
  const shelf: { -readonly [K in keyof PicoShelfGame]: PicoShelfGame[K] } = { id: game.id, title: game.title }
  if (game.subtitle !== undefined) shelf.subtitle = game.subtitle
  if (game.coverArtUrl !== undefined) shelf.artUrl = game.coverArtUrl
  if (game.resumable !== undefined) shelf.resumable = game.resumable
  if (game.section !== undefined) shelf.section = game.section
  if (game.lastPlayedAt !== undefined) shelf.lastPlayedAt = game.lastPlayedAt
  if (game.playCount !== undefined) shelf.playCount = game.playCount
  if (game.totalPlaytimeSeconds !== undefined) shelf.totalPlaytimeSeconds = game.totalPlaytimeSeconds
  if (game.download !== undefined) {
    const download: { bytes?: number; queued?: true; progress?: { readonly done: number; readonly total?: number } } = {}
    if (game.download.bytes !== undefined) download.bytes = game.download.bytes
    if (game.download.queued === true) download.queued = true
    if (game.download.progress !== undefined) {
      download.progress = game.download.progress.total === undefined
        ? { done: game.download.progress.done }
        : { done: game.download.progress.done, total: game.download.progress.total }
    }
    shelf.download = download
  }
  if (game.launchLocations !== undefined) shelf.locations = game.launchLocations.map(location => ({ id: location.id, label: location.label }))
  shelfGames.set(game, shelf)
  return shelf
}

const shelfGames = new WeakMap<SurfaceGame, PicoShelfGame>()
