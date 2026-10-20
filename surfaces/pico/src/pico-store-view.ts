/**
 * The store's entries as Pico's own pieces draw them: a result is a cart on
 * the results shelf, and an entry under way is a cart in the launch slot.
 * The provider's words, nothing added. No provider art reaches Pico (it is
 * on the provider's site), so each cart wears its initials sticker.
 */
import type { SurfaceStore, SurfaceStoreEntry } from "@contracts/surface/korri-surface"
import type { PicoLaunchCart } from "./pico-screen-view"
import type { PicoShelfGame } from "./pico-shelf-game"

export const picoStoreResult = (entry: SurfaceStoreEntry): PicoShelfGame => ({
  id: entry.id,
  title: entry.title,
  ...(entry.system === undefined ? {} : { subtitle: entry.system }),
})

export const picoStoreCart = (entry: SurfaceStoreEntry): PicoLaunchCart => ({ id: entry.id, title: entry.title })

/** Long enough for any title; the store's keys stop there. */
export const PICO_STORE_QUERY_MAX = 64

/** The provider's name, when Korri lists it. */
export function picoStoreProviderTitle(store: SurfaceStore, providerId: string | undefined): string | undefined {
  if (providerId === undefined || store.providers._tag !== "Ready") return undefined
  return store.providers.providers.find(provider => provider.id === providerId)?.title
}

/** The search Korri holds for this provider, or none. */
export function picoStoreSearchFor(store: SurfaceStore, providerId: string | undefined): SurfaceStore["search"] {
  const search = store.search
  return search._tag !== "Idle" && search.providerId === providerId ? search : { _tag: "Idle" }
}
