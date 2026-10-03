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
  SurfaceStatus,
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

/**
 * The rental sticker on the back of a box: how often Korri saw it played.
 * Absent when Korri stated no play count, so the box never claims a figure
 * it was not given. `lastPlayedAt` gets no words: the aisle already says how
 * recent a tape is.
 */
export function rentalSticker(playCount: number | undefined): string | undefined {
  if (playCount === undefined) return undefined
  return playCount === 1 ? "RENTED ONCE" : `RENTED ${playCount} TIMES`
}

/** Only what the store draws, so no other fact can move a shelf. */
function storeGameFrom(game: SurfaceGame): StoreGame {
  const sticker = rentalSticker(game.playCount)
  return {
    id: game.id,
    title: game.title,
    ...(game.subtitle === undefined ? {} : { subtitle: game.subtitle }),
    ...(game.coverArtUrl === undefined
      ? {}
      : { coverArtUrl: game.coverArtUrl }),
    ...(sticker === undefined ? {} : { sticker }),
  }
}

/** Where a tape is shelved: one of the store's places (map.ts). */
export type BoxbusterAisle = keyof StoreShelving

type Placed = Readonly<Record<BoxbusterAisle, readonly SurfaceGame[]>>

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
function placedFrom(games: readonly SurfaceGame[], now: number): Placed {
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

  const toGames = (entries: readonly Indexed[]) =>
    entries.map(entry => entry.game)
  return {
    returns: toGames(returns),
    newReleases: toGames(recent),
    staffPicks: toGames(earlier),
    classics: toGames(never),
  }
}

/** The order a walk in from the door meets them. */
const AISLES: readonly BoxbusterAisle[] = [
  "returns",
  "newReleases",
  "staffPicks",
  "classics",
]

function shelvingFrom(
  games: readonly SurfaceGame[],
  now: number,
): StoreShelving {
  const placed = placedFrom(games, now)
  return {
    returns: placed.returns.map(storeGameFrom),
    newReleases: placed.newReleases.map(storeGameFrom),
    staffPicks: placed.staffPicks.map(storeGameFrom),
    classics: placed.classics.map(storeGameFrom),
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

// ── what a tape says, and what the TV says ────────────────────────────────────
// Read apart from the store, because these are facts the store must not
// rebuild for: launch locations, play counts, and where a launch stands. The
// tapes keep the store's placement order, so both agree on every tape.

/** Somewhere a tape can be played. The id goes back to Korri unchanged. */
export interface TapeLocation {
  readonly id: string
  readonly label: string
}

export type TapeLaunch =
  | { readonly _tag: "Here" }
  /** Korri offers a real choice; the viewing room must ask, never pick one. */
  | { readonly _tag: "Choose"; readonly locations: readonly TapeLocation[] }

/** What the visit needs to know about a tape: its name (for the controls'
 * accessible labels), where it is shelved, and where it can be played. What
 * you read about a tape is printed on the box itself (StoreGame). */
export interface TapeFacts {
  readonly id: string
  readonly title: string
  readonly aisle: BoxbusterAisle
  readonly launch: TapeLaunch
}

function tapeFactsFromGame(
  game: SurfaceGame,
  aisle: BoxbusterAisle,
): TapeFacts {
  const locations = game.launchLocations ?? []
  return {
    id: game.id,
    title: game.title,
    aisle,
    launch:
      locations.length === 0
        ? { _tag: "Here" }
        : {
            _tag: "Choose",
            locations: locations.map(({ id, label }) => ({ id, label })),
          },
  }
}

/** Every tape, in store order. Empty unless the catalog is ready. */
export function tapeFactsFrom(
  model: SurfaceModel,
  now: number,
): readonly TapeFacts[] {
  if (model.catalog._tag !== "Ready") return []
  const placed = placedFrom(model.catalog.games, now)
  return AISLES.flatMap(aisle =>
    placed[aisle].map(game => tapeFactsFromGame(game, aisle)),
  )
}

/** Where a launch stands, in Korri's own words. */
export type TvStatus =
  | { readonly _tag: "Idle" }
  | {
      readonly _tag: "Working"
      readonly kicker: string
      readonly detail?: string
      readonly tapeId?: string
    }
  | { readonly _tag: "Playing"; readonly kicker: string; readonly tapeId?: string }
  | {
      readonly _tag: "Problem"
      readonly kicker: string
      readonly reason: string
      readonly canRetry: boolean
      /** The game the problem belongs to, when Korri named one. */
      readonly title?: string
    }

export function tvStatusFrom(model: SurfaceModel): TvStatus {
  const status: SurfaceStatus = model.status
  switch (status._tag) {
    case "Browsing":
      return { _tag: "Idle" }
    case "Busy":
      return {
        _tag: "Working",
        kicker: status.kicker,
        ...(status.detail === undefined ? {} : { detail: status.detail }),
        ...(status.gameId === undefined ? {} : { tapeId: status.gameId }),
      }
    case "Running":
      return {
        _tag: "Playing",
        kicker: status.kicker,
        ...(status.gameId === undefined ? {} : { tapeId: status.gameId }),
      }
    case "Problem":
      return {
        _tag: "Problem",
        kicker: status.kicker,
        reason: status.reason,
        canRetry: status.canRetry,
        ...(status.gameTitle === undefined ? {} : { title: status.gameTitle }),
      }
  }
}
