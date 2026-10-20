/**
 * GET GAMES as Korri can publish it, for parts and tests: the PICO-8 BBS,
 * the one provider that searches, its results, and one entry in each state
 * getting it can reach. The words are the portal's
 * (clients/portal/src/surface/store-acquisition.ts); every value is typed
 * against the treaty, so a part cannot show a field Korri does not have.
 *
 * `createStoreProgram` runs the real `updateStore` from src/state, as the
 * wire stage does, so a part's buttons answer. A part may not import
 * src/state; fixture code may, because the product never runs it. With no
 * Korri behind it the program plays Korri's part in the smallest way: a
 * search answers with the entries whose title holds the words, an opened
 * entry is on offer, and GET, DOWNLOAD and CANCEL move it along.
 */
import type {
  SurfaceAction,
  SurfaceModel,
  SurfaceStore,
  SurfaceStoreDetails,
  SurfaceStoreEntry,
  SurfaceStoreGet,
} from "@contracts/surface/korri-surface"
import type { ZoneRequest } from "../state/requests"
import { updateStore, ZONE_STORE_START, type ZoneStoreMessage, type ZoneStoreModel } from "../state/store"

export const fixtureStoreProviders = [{ id: "@korri:pico8", title: "PICO-8 BBS" }] as const

export const fixtureStoreEntries: readonly SurfaceStoreEntry[] = [
  { id: "celeste_classic_2-5", title: "Celeste Classic 2", system: "PICO-8" },
  // Stress: a long title with an unbroken word.
  { id: "lighthouse-1", title: "The Legend of the Supercalifragilisticexpialidocious Lighthouse Keeper", system: "PICO-8" },
  // A cart that names no system: listed, but it cannot be got.
  { id: "wip-3", title: "untitled work in progress" },
]

/** Enough results that the list scrolls on a handheld. */
export const fixtureStoreManyEntries: readonly SurfaceStoreEntry[] = [
  ...fixtureStoreEntries,
  ...["Poom", "Dank Tomb", "Picoracer-2048", "Porklike", "Just One Boss", "Mot's Grand Prix", "Lazy Devs Shmup"]
    .map((title, index) => ({ id: `cart-${index}`, title, system: "PICO-8" })),
]

const action = (id: string, label: string, enabled = true): SurfaceAction => ({ id, label, enabled })

export const fixtureStoreGets = {
  offer: { _tag: "Offer", actions: [action("store:1:get", "Get")] },
  cancelled: { _tag: "Offer", note: "Download cancelled. Nothing was kept.", actions: [action("store:2:get", "Get")] },
  unavailable: {
    _tag: "Unavailable",
    reason: "The provider doesn't say which system this game is for, so it can't be downloaded here.",
  },
  confirm: {
    _tag: "Confirm",
    kicker: "Download to this device",
    detail: "Celeste Classic 2, 15.9 KB",
    actions: [action("store:3:download", "Download"), action("store:3:cancel", "Cancel")],
  },
  starting: {
    _tag: "Working", kicker: "Starting download…", detail: "Asking the provider for the file",
    actions: [action("store:4:cancel", "Cancel")],
  },
  downloading: {
    _tag: "Working", kicker: "Downloading Celeste Classic 2", detail: "8 KB of 15.9 KB · 8 KB/s · 0:01 left",
    progress: { done: 8_000, total: 15_872 }, actions: [action("store:5:cancel", "Cancel")],
  },
  unsized: {
    _tag: "Working", kicker: "Downloading Celeste Classic 2", detail: "8 KB so far · 4 KB/s",
    progress: { done: 8_000 }, actions: [action("store:6:cancel", "Cancel")],
  },
  queued: {
    _tag: "Working", kicker: "Waiting to download", detail: "Waiting for the download before it", queued: true,
    actions: [action("store:9:cancel", "Cancel")],
  },
  placing: { _tag: "Working", kicker: "Adding to your library…", actions: [] },
  inLibrary: { _tag: "InLibrary", detail: "In your library", gameId: "celeste" },
  failed: {
    _tag: "Problem", kicker: "Couldn't start the download", reason: "The provider did not answer. Try again.",
    actions: [action("store:7:retry", "Retry")],
  },
  stopped: {
    _tag: "Problem", kicker: "Download stopped",
    reason: "Korri restarted before the download finished. Nothing was kept, and your library is unchanged.",
    actions: [action("store:8:retry", "Retry")],
  },
} as const satisfies Record<string, SurfaceStoreGet>

export const fixtureStoreDetails = (
  get: SurfaceStoreGet,
  entry: SurfaceStoreEntry = fixtureStoreEntries[0]!,
): SurfaceStoreDetails => ({
  _tag: "Ready",
  entry,
  description: "Madeline climbs again, with a grappling hook this time.",
  get,
})

/** The store with the provider listed, "cel" searched, and Celeste Classic 2 on offer. */
export const fixtureStore = (overrides: Partial<SurfaceStore> = {}): SurfaceStore => ({
  providers: { _tag: "Ready", providers: fixtureStoreProviders },
  search: { _tag: "Ready", providerId: "@korri:pico8", query: "cel", entries: fixtureStoreEntries },
  details: fixtureStoreDetails(fixtureStoreGets.offer),
  ...overrides,
})

/** Nothing asked yet: the providers alone. */
export const fixtureStoreFresh: SurfaceStore = fixtureStore({ search: { _tag: "Idle" }, details: { _tag: "Closed" } })

/** The messages that open each page, replayed over the start. */
export const fixtureStoreOpenings = {
  providers: [{ _tag: "OpenedStore" }],
  keyboard: [
    { _tag: "OpenedStore" },
    { _tag: "PressedRow", rowId: "provider:@korri:pico8" },
    { _tag: "PressedRow", rowId: "search" },
  ],
  results: [{ _tag: "OpenedStore" }, { _tag: "PressedRow", rowId: "provider:@korri:pico8" }],
  entry: [
    { _tag: "OpenedStore" },
    { _tag: "PressedRow", rowId: "provider:@korri:pico8" },
    { _tag: "PressedRow", rowId: "entry:celeste_classic_2-5" },
  ],
} as const satisfies Record<string, readonly ZoneStoreMessage[]>

export interface StoreProgram {
  readonly subscribe: (listener: () => void) => () => void
  readonly get: () => { readonly store: SurfaceStore; readonly state: ZoneStoreModel }
  readonly dispatch: (message: ZoneStoreMessage) => void
}

/** The store page on the real update. `opening` is replayed over the start. */
export function createStoreProgram(
  store: SurfaceStore,
  opening: readonly ZoneStoreMessage[] = fixtureStoreOpenings.providers,
  perform?: (request: ZoneRequest) => void,
): StoreProgram {
  const listeners = new Set<() => void>()
  let current = {
    store,
    state: opening.reduce<ZoneStoreModel>((state, message) => updateStore(state, message, { store }).model, ZONE_STORE_START),
  }
  return {
    subscribe(listener) {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    get: () => current,
    dispatch(message) {
      const model: Pick<SurfaceModel, "store"> = { store: current.store }
      const step = updateStore(current.state, message, model)
      let next = current.store
      for (const request of step.requests) {
        perform?.(request)
        next = answer(next, request)
      }
      current = { store: next, state: step.model }
      for (const listener of [...listeners]) listener()
    },
  }
}

/** Korri's part, as small as it can be. */
function answer(store: SurfaceStore, request: ZoneRequest): SurfaceStore {
  switch (request._tag) {
    case "SearchStore": {
      const words = request.query.toLocaleLowerCase()
      const entries = fixtureStoreEntries.filter(entry => entry.title.toLocaleLowerCase().includes(words))
      return { ...store, search: { _tag: "Ready", providerId: request.providerId, query: request.query, entries } }
    }
    case "OpenStoreEntry": {
      const entry = fixtureStoreEntries.find(candidate => candidate.id === request.entryId)
      if (entry === undefined) return store
      return { ...store, details: fixtureStoreDetails(entry.system === undefined ? fixtureStoreGets.unavailable : fixtureStoreGets.offer, entry) }
    }
    case "RunAction": {
      if (store.details._tag !== "Ready") return store
      const kind = request.actionId.split(":").at(-1)
      const get = kind === "get" ? fixtureStoreGets.confirm
        : kind === "download" ? fixtureStoreGets.downloading
        : kind === "cancel" ? fixtureStoreGets.cancelled
        : kind === "retry" ? fixtureStoreGets.starting
        : store.details.get
      return { ...store, details: { ...store.details, get } }
    }
    default:
      return store
  }
}

/** The GET GAMES page's callbacks, each dispatching the message it reports. */
export function storeCallbacks(dispatch: (message: ZoneStoreMessage) => void) {
  return {
    onRowFocus: (rowId: string) => dispatch({ _tag: "FocusedRow", rowId }),
    onRowPress: (rowId: string) => dispatch({ _tag: "PressedRow", rowId }),
    onKeyFocus: (id: string) => dispatch({ _tag: "FocusedKey", id }),
    onKey: (key: Extract<ZoneStoreMessage, { _tag: "PressedKey" }>["key"]) => dispatch({ _tag: "PressedKey", key }),
    onDialogFocus: (index: number) => dispatch({ _tag: "FocusedDialogButton", index }),
    onAction: (actionId: string) => dispatch({ _tag: "PressedAction", actionId }),
  }
}
