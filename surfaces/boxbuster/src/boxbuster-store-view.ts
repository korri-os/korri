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

// ── the counter ────────────────────────────────────────────────────────────────
// The counter is where every decision happens: legible type, no dither, built
// from the same placement as the store, so the index and the place agree. It
// is converted apart from the store because it reads facts the store must not
// rebuild for (launch locations, play counts, status).

/** Somewhere a tape can be played. The id goes back to Korri unchanged. */
export interface CounterLocation {
  readonly id: string
  readonly label: string
}

export type CounterLaunch =
  | { readonly _tag: "Here" }
  /** Korri offers a real choice; the counter must ask, never pick one. */
  | { readonly _tag: "Choose"; readonly locations: readonly CounterLocation[] }

/** A tape as the counter presents it: the back of the box. */
export interface CounterTape {
  readonly id: string
  readonly title: string
  readonly subtitle?: string
  readonly coverArtUrl?: string
  readonly aisle: BoxbusterAisle
  readonly verb: "Resume" | "Play"
  /** Play facts Korri stated, as sentences. Empty when it stated none. */
  readonly facts: readonly string[]
  readonly launch: CounterLaunch
}

function playtimeLabel(seconds: number): string {
  const minutes = Math.floor(seconds / 60)
  const hours = Math.floor(minutes / 60)
  const rest = minutes % 60
  if (hours === 0) return `${rest} min`
  return rest === 0 ? `${hours} h` : `${hours} h ${rest} min`
}

/**
 * Only what Korri stated. `lastPlayedAt` is not turned into words: the aisle
 * already says how recent a tape is, and "2 days ago" would need a clock the
 * treaty deliberately keeps from the surface.
 */
function factsFor(game: SurfaceGame): string[] {
  const facts: string[] = []
  if (game.playCount !== undefined) {
    facts.push(
      game.playCount === 1 ? "Played once" : `Played ${game.playCount} times`,
    )
  }
  if (game.totalPlaytimeSeconds !== undefined) {
    facts.push(`${playtimeLabel(game.totalPlaytimeSeconds)} in all`)
  }
  return facts
}

function counterTapeFrom(
  game: SurfaceGame,
  aisle: BoxbusterAisle,
): CounterTape {
  const locations = game.launchLocations ?? []
  return {
    id: game.id,
    title: game.title,
    ...(game.subtitle === undefined ? {} : { subtitle: game.subtitle }),
    ...(game.coverArtUrl === undefined
      ? {}
      : { coverArtUrl: game.coverArtUrl }),
    aisle,
    verb: game.resumable === true ? "Resume" : "Play",
    facts: factsFor(game),
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
export function counterTapesFrom(
  model: SurfaceModel,
  now: number,
): readonly CounterTape[] {
  if (model.catalog._tag !== "Ready") return []
  const placed = placedFrom(model.catalog.games, now)
  return AISLES.flatMap(aisle =>
    placed[aisle].map(game => counterTapeFrom(game, aisle)),
  )
}

/** Where a launch stands, in Korri's own words. */
export type CounterStatus =
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

export function counterStatusFrom(model: SurfaceModel): CounterStatus {
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
