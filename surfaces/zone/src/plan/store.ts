/**
 * GET GAMES' pure rules, read by its state model and by its page alike, so
 * the two cannot disagree about which rows exist: the row ids, the search
 * that belongs to the provider on screen, and whether Korri publishes an
 * action now.
 */
import type { SurfaceAction, SurfaceStore, SurfaceStoreGet, SurfaceStoreSearch } from "@contracts/surface/korri-surface"

export type ZoneStorePage = "providers" | "results" | "entry"

/** The SEARCH row at the head of a provider's results. */
export const ZONE_STORE_SEARCH_ROW = "search"
export const zoneStoreProviderRow = (providerId: string): string => `provider:${providerId}`
export const zoneStoreEntryRow = (entryId: string): string => `entry:${entryId}`
/** A button carrying one of Korri's actions, wherever it stands. */
export const zoneStoreActionRow = (actionId: string): string => `action:${actionId}`
/** PLAY on an entry that is in the library. */
export const zoneStorePlayRow = (gameId: string): string => `play:${gameId}`

/** The most characters a search holds, as FIND's draft. */
export const ZONE_STORE_QUERY_MAX = 40

/** Korri's search when it belongs to this provider; Idle otherwise. */
export function zoneStoreSearch(store: SurfaceStore, provider: string | undefined): SurfaceStoreSearch {
  const search = store.search
  return search._tag !== "Idle" && search.providerId === provider ? search : { _tag: "Idle" }
}

/** The provider's title, as Korri lists it. */
export function zoneStoreProviderTitle(store: SurfaceStore, provider: string | undefined): string | undefined {
  return store.providers._tag === "Ready" ? store.providers.providers.find(item => item.id === provider)?.title : undefined
}

/** Korri's actions on the getting of the open entry; none in Unavailable or InLibrary. */
export function zoneStoreGetActions(get: SurfaceStoreGet): readonly SurfaceAction[] {
  switch (get._tag) {
    case "Offer":
    case "Confirm":
    case "Working":
    case "Problem":
      return get.actions
    case "Unavailable":
    case "InLibrary":
      return []
  }
}

/** Every action Korri publishes in the store now. */
export function zoneStoreActions(store: SurfaceStore): readonly SurfaceAction[] {
  return [
    ...(store.providers._tag === "Problem" ? store.providers.actions : []),
    ...(store.search._tag === "Problem" ? store.search.actions : []),
    ...(store.details._tag === "Problem" ? store.details.actions : []),
    ...(store.details._tag === "Ready" ? zoneStoreGetActions(store.details.get) : []),
  ]
}

/** Whether Korri publishes this action now, enabled: the only authority a press has. */
export const zoneStoreActionLive = (store: SurfaceStore, actionId: string): boolean =>
  zoneStoreActions(store).some(action => action.id === actionId && action.enabled)
