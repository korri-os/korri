/**
 * The one conversion from the treaty to the store.
 *
 * Boxbuster reads Korri's model here and nowhere else below the surface root:
 * everything past this file speaks in tapes, rooms, and shelves. Pure, with
 * the time passed in, so the same library at the same moment always builds the
 * same store — the charter's first test of place.
 */
import type {
  SurfaceGame,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
import {
  computeMap,
  type StoreGame,
  type StoreMap,
  type StoreShelving,
} from "./map"

export type BoxbusterStoreView =
  | { readonly _tag: "Loading" }
  | { readonly _tag: "Error"; readonly message: string }
  | { readonly _tag: "Empty" }
  | { readonly _tag: "Open"; readonly map: StoreMap }

/** How long a game stays on the New Releases wall after its last session. */
export const NEW_RELEASE_WINDOW_MS = 14 * 24 * 60 * 60 * 1000

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

/**
 * The store merchandises itself from how you have treated each game:
 *
 * - one you can resume waits on the return cart by the door;
 * - one played in the last 14 days is on the New Releases wall, newest first;
 * - one played before that is a Staff Pick, most played first;
 * - one never played sits with the Classics, in catalog order.
 *
 * Every tie falls back to catalog order, so placement never depends on sort
 * stability or anything the treaty does not carry.
 */
function shelvingFrom(
  games: readonly SurfaceGame[],
  now: number,
): StoreShelving {
  const indexed = games.map((game, index) => ({ game, index }))
  type Indexed = (typeof indexed)[number]
  const byCatalog = (a: Indexed, b: Indexed) => a.index - b.index
  const lastPlayed = (entry: Indexed) => entry.game.lastPlayedAt ?? 0
  const plays = (entry: Indexed) => entry.game.playCount ?? 0

  const returns: Indexed[] = []
  const recent: Indexed[] = []
  const earlier: Indexed[] = []
  const never: Indexed[] = []
  for (const entry of indexed) {
    const { resumable, lastPlayedAt } = entry.game
    if (resumable === true) returns.push(entry)
    else if (lastPlayedAt === undefined) never.push(entry)
    else if (lastPlayedAt >= now - NEW_RELEASE_WINDOW_MS) recent.push(entry)
    else earlier.push(entry)
  }

  recent.sort((a, b) => lastPlayed(b) - lastPlayed(a) || byCatalog(a, b))
  earlier.sort(
    (a, b) =>
      plays(b) - plays(a) || lastPlayed(b) - lastPlayed(a) || byCatalog(a, b),
  )

  const toStore = (entries: readonly Indexed[]) =>
    entries.map(entry => storeGameFrom(entry.game))
  return {
    returns: toStore(returns),
    newReleases: toStore(recent),
    staffPicks: toStore(earlier),
    classics: toStore(never),
  }
}

type StoreSource =
  | { readonly _tag: "Loading" }
  | { readonly _tag: "Error"; readonly message: string }
  | { readonly _tag: "Empty" }
  | { readonly _tag: "Stocked"; readonly shelving: StoreShelving }

function storeSourceFrom(model: SurfaceModel, now: number): StoreSource {
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
        : { _tag: "Stocked", shelving: shelvingFrom(catalog.games, now) }
  }
}

/**
 * Identifies the store a model builds at a moment. Korri republishes the whole
 * model for any change, a clock tick included, and time passing moves a tape
 * only when its last session leaves the 14-day window. The surface rebuilds
 * the store, its textures, and its cover requests only when this changes.
 */
export function storeSignature(model: SurfaceModel, now: number): string {
  return JSON.stringify(storeSourceFrom(model, now))
}

export function storeViewFrom(
  model: SurfaceModel,
  now: number,
): BoxbusterStoreView {
  const source = storeSourceFrom(model, now)
  return source._tag === "Stocked"
    ? { _tag: "Open", map: computeMap(source.shelving) }
    : source
}
