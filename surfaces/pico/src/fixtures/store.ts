/**
 * The store as Korri can publish it, for parts and tests: the PICO-8 BBS, the
 * one provider that searches (Homebrew Hub and RetroBrews are declared games),
 * results, and one entry in each state getting it can reach.
 * Fixture data only; Pico asks Korri for none of it.
 */
import type {
  SurfaceStore,
  SurfaceStoreDetails,
  SurfaceStoreEntry,
  SurfaceStoreGet,
} from "@contracts/surface/korri-surface"

export const STORE_PROVIDERS = [{ id: "@korri:pico8", title: "PICO-8 BBS" }] as const

export const STORE_ENTRIES: readonly SurfaceStoreEntry[] = [
  { id: "e:celeste", title: "Celeste Classic 2", system: "PICO-8" },
  // Stress: a long title with an unbroken word.
  { id: "e:long", title: "The Legend of the Supercalifragilisticexpialidocious Lighthouse Keeper", system: "PICO-8" },
  // A cart that names no system: shown, but it cannot be got.
  { id: "e:nosys", title: "untitled work in progress" },
]

const action = (id: string, label: string, enabled = true) => ({ id, label, enabled })

export const STORE_GETS = {
  offer: { _tag: "Offer", actions: [action("store:1:get", "Get")] },
  cancelled: { _tag: "Offer", note: "Download cancelled. Nothing was kept.", actions: [action("store:2:get", "Get")] },
  unavailable: { _tag: "Unavailable", reason: "This game is for a system this device cannot play, so it can't be downloaded here." },
  confirm: { _tag: "Confirm", kicker: "Download to this device", detail: "Celeste Classic 2, size unknown", actions: [action("store:3:download", "Download"), action("store:3:cancel", "Cancel")] },
  downloading: {
    _tag: "Working", kicker: "Downloading Celeste Classic 2", detail: "8 KB of 15.9 KB · 8 KB/s · 0:01 left",
    progress: { done: 8_000, total: 15_872 }, actions: [action("store:5:cancel", "Cancel")],
  },
  unsized: {
    _tag: "Working", kicker: "Downloading Celeste Classic 2", detail: "8 KB so far · 4 KB/s",
    progress: { done: 8_000 }, actions: [action("store:6:cancel", "Cancel")],
  },
  placing: { _tag: "Working", kicker: "Adding to your library…", progress: { done: 0 }, actions: [] },
  inLibrary: { _tag: "InLibrary", detail: "In your library", gameId: "hollow" },
  failed: {
    _tag: "Problem", kicker: "Download failed", reason: "The download did not finish. Try again.",
    actions: [action("store:7:retry", "Retry")],
  },
  stopped: {
    _tag: "Problem", kicker: "Download stopped",
    reason: "Korri restarted before the download finished. Nothing was kept, and your library is unchanged.",
    actions: [action("store:8:retry", "Retry")],
  },
} as const satisfies Record<string, SurfaceStoreGet>

export const storeDetails = (get: SurfaceStoreGet, entry: SurfaceStoreEntry = STORE_ENTRIES[0]!): Extract<SurfaceStoreDetails, { readonly _tag: "Ready" }> => ({
  _tag: "Ready",
  entry,
  description: "Madeline climbs again, with a grappling hook this time.",
  get,
})

export const storeFixture = (overrides: Partial<SurfaceStore> = {}): SurfaceStore => ({
  providers: { _tag: "Ready", providers: STORE_PROVIDERS },
  search: { _tag: "Ready", providerId: "@korri:pico8", query: "", entries: STORE_ENTRIES },
  details: storeDetails(STORE_GETS.offer),
  ...overrides,
})
