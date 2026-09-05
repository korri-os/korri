import type { SurfaceCatalog } from "@contracts/surface/korri-surface"
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
            games: catalog.games.map((game) => ({
              id: game.id,
              title: game.title,
              ...(game.subtitle === undefined ? {} : { subtitle: game.subtitle }),
              ...(game.coverArtUrl === undefined
                ? {}
                : { artUrl: game.coverArtUrl }),
              ...(game.wideArtUrl === undefined
                ? {}
                : { wideArtUrl: game.wideArtUrl }),
              ...(game.resumable === undefined
                ? {}
                : { resumable: game.resumable }),
              ...(game.section === undefined ? {} : { section: game.section }),
              ...(game.launchLocations === undefined
                ? {}
                : {
                    locations: game.launchLocations.map((location) => ({
                      id: location.id,
                      label: location.label,
                    })),
                  }),
            })),
          }
  }
}
