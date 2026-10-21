/**
 * The store as Korri can publish it, for previews and tests: the three
 * providers in scope, a search with real results, and one entry in each
 * state getting it can reach. Fixture data only; no request is made.
 */
import type {
  SurfaceStore,
  SurfaceStoreDetails,
  SurfaceStoreEntry,
  SurfaceStoreGet,
} from "@contracts/surface/korri-surface"

export const STORE_PROVIDERS = [
  { id: "@korri:pico8", title: "PICO-8 BBS" },
  { id: "@korri:homebrewhub", title: "Homebrew Hub" },
  { id: "@korri:retrobrews", title: "RetroBrews" },
] as const

export const STORE_ENTRIES: readonly SurfaceStoreEntry[] = [
  { id: "e:2048", title: "2048", system: "Game Boy" },
  { id: "e:celeste", title: "Celeste Classic 2", system: "PICO-8" },
  // Stress: the longest title shape the BBS allows in practice, with an unbroken word.
  { id: "e:long", title: "The Legend of the Supercalifragilisticexpialidocious Lighthouse Keeper", system: "Game Boy Advance" },
  { id: "e:nosys", title: "untitled work in progress" },
]

const action = (id: string, label: string, enabled = true) => ({ id, label, enabled })

export const STORE_GETS = {
  offer: { _tag: "Offer", actions: [action("store:1:get", "Get")] },
  cancelled: { _tag: "Offer", note: "Download cancelled. Nothing was kept.", actions: [action("store:2:get", "Get")] },
  unavailable: { _tag: "Unavailable", reason: "No folder on this device holds Game Boy games. Add one in Settings, then get this game again." },
  confirm: { _tag: "Confirm", kicker: "Download to this device", detail: "2048, size unknown", actions: [action("store:3:download", "Download"), action("store:3:cancel", "Cancel")] },
  folders: {
    _tag: "Confirm", kicker: "Choose a folder",
    detail: "More than one folder on this device holds Game Boy games. Choose where this one goes.",
    actions: [action("store:4:source.0", "Game Boy"), action("store:4:source.1", "SD card · Game Boy"), action("store:4:cancel", "Cancel")],
  },
  downloading: {
    _tag: "Working", kicker: "Downloading 2048", detail: "16 KB of 32.8 KB · 16 KB/s · 0:01 left",
    progress: { done: 16_000, total: 32_768 }, actions: [action("store:5:cancel", "Cancel")],
  },
  unsized: {
    _tag: "Working", kicker: "Downloading Ambushed", detail: "128 KB so far · 64 KB/s",
    progress: { done: 128_000 }, actions: [action("store:6:cancel", "Cancel")],
  },
  placing: { _tag: "Working", kicker: "Adding to your library…", progress: { done: 0 }, actions: [] },
  inLibrary: { _tag: "InLibrary", detail: "In your library", gameId: "local-game:wl4" },
  staged: { _tag: "Staged", message: "Downloaded to this device, but your library did not recognise it as a game." },
  stopped: {
    _tag: "Problem", kicker: "Download stopped",
    reason: "Korri restarted before the download finished. Nothing was kept, and your library is unchanged.",
    actions: [action("store:7:retry", "Retry")],
  },
} as const satisfies Record<string, SurfaceStoreGet>

export const storeDetails = (get: SurfaceStoreGet, entry: SurfaceStoreEntry = STORE_ENTRIES[0]!): SurfaceStoreDetails => ({
  _tag: "Ready",
  entry,
  description: "Slide the tiles and join equal numbers to reach 2048.",
  get,
})

export const storeFixture = (overrides: Partial<SurfaceStore> = {}): SurfaceStore => ({
  providers: { _tag: "Ready", providers: STORE_PROVIDERS },
  search: { _tag: "Ready", providerId: "@korri:homebrewhub", query: "", entries: STORE_ENTRIES },
  details: storeDetails(STORE_GETS.offer),
  ...overrides,
})
