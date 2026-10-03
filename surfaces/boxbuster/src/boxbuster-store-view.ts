/**
 * The one conversion from the treaty to the store.
 *
 * Boxbuster reads Korri's model here and nowhere else below the surface root:
 * everything past this file speaks in tapes, rooms, and shelves. Pure, so the
 * same model always builds the same store — the charter's first test of place.
 */
import type {
  SurfaceGame,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
import { computeMap, type StoreGame, type StoreMap } from "./map"

export type BoxbusterStoreView =
  | { readonly _tag: "Loading" }
  | { readonly _tag: "Error"; readonly message: string }
  | { readonly _tag: "Empty" }
  | { readonly _tag: "Open"; readonly map: StoreMap }

/** Only what the store draws, so no other fact can move a shelf. */
function storeGameFrom(game: SurfaceGame): StoreGame {
  return {
    id: game.id,
    title: game.title,
    ...(game.subtitle === undefined ? {} : { subtitle: game.subtitle }),
    ...(game.coverArtUrl === undefined
      ? {}
      : { coverArtUrl: game.coverArtUrl }),
  }
}

type StoreSource =
  | { readonly _tag: "Loading" }
  | { readonly _tag: "Error"; readonly message: string }
  | { readonly _tag: "Empty" }
  | { readonly _tag: "Stocked"; readonly games: readonly StoreGame[] }

function storeSourceFrom(model: SurfaceModel): StoreSource {
  const { catalog } = model
  switch (catalog._tag) {
    case "Loading":
    case "Empty":
      return { _tag: catalog._tag }
    case "Error":
      return { _tag: "Error", message: catalog.message }
    case "Ready":
      return catalog.games.length === 0
        ? { _tag: "Empty" }
        : { _tag: "Stocked", games: catalog.games.map(storeGameFrom) }
  }
}

/**
 * Identifies the store a model builds. Korri republishes the whole model for
 * any change, a clock tick included; the surface rebuilds the store, its
 * textures, and its cover requests only when this changes.
 */
export function storeSignature(model: SurfaceModel): string {
  return JSON.stringify(storeSourceFrom(model))
}

export function storeViewFrom(model: SurfaceModel): BoxbusterStoreView {
  const source = storeSourceFrom(model)
  return source._tag === "Stocked"
    ? { _tag: "Open", map: computeMap(source.games) }
    : source
}
