import { expect, test } from "bun:test"
import type { SurfaceAction, SurfaceStore, SurfaceStoreGet } from "@contracts/surface/korri-surface"
import {
  createInMemoryStoreAcquisition,
  type InMemoryClaim,
  type InMemoryStoreConfig,
} from "../korrid/store-acquisition-client"
import { ACQUIRE_FAILURE } from "../korrid/store-acquisition-types"
import {
  CANCELLED_NOTE,
  createStoreAcquisition,
  NO_SYSTEM_REASON,
  STAGED_MESSAGE,
  STOPPED_KICKER,
  STOPPED_REASON,
  storeEntryId,
} from "./store-acquisition"

const HUB = "@korri:homebrewhub"
const claim = (id: string, extra: Partial<InMemoryClaim> = {}): InMemoryClaim => ({
  providerId: HUB, id, title: id.toUpperCase(), url: `https://hh3.gbdev.io/game/${id}`, platform: "Game Boy",
  artifact: { system: "gb", file: { name: `${id}.gb`, sizeBytes: 32_000 } }, ...extra,
})
const base = (claims: InMemoryClaim[], extra: Partial<InMemoryStoreConfig> = {}): InMemoryStoreConfig => ({
  providers: [{ id: HUB, title: "Homebrew Hub", pluginId: HUB }],
  claims: { [HUB]: claims },
  sources: { gb: [{ id: "games-gb", label: "Game Boy" }] },
  bytesPerStep: 16_000,
  ...extra,
})

const flush = () => new Promise(resolve => setTimeout(resolve, 0))

/** A store over the in-memory korrid, with every RPC recorded. */
function harness(config: InMemoryStoreConfig) {
  const korrid = createInMemoryStoreAcquisition(config)
  const calls: string[] = []
  const imported: string[] = []
  const record = <A extends unknown[], R>(name: string, call: (...args: A) => Promise<R>) =>
    async (...args: A) => {
      calls.push(`${name} ${JSON.stringify(args)}`)
      return call(...args)
    }
  const client = {
    acquisitionProviders: record("providers", () => korrid.acquisitionProviders()),
    acquisitionSearch: record("search", (request: Parameters<typeof korrid.acquisitionSearch>[0]) => korrid.acquisitionSearch(request)),
    acquisitionDetails: record("details", (request: Parameters<typeof korrid.acquisitionDetails>[0]) => korrid.acquisitionDetails(request)),
    acquisitionAcquire: record("acquire", (request: Parameters<typeof korrid.acquisitionAcquire>[0]) => korrid.acquisitionAcquire(request)),
    acquisitionAcquireStatus: record("status", (jobId: string) => korrid.acquisitionAcquireStatus(jobId)),
    acquisitionAcquireCancel: record("cancel", (jobId: string) => korrid.acquisitionAcquireCancel(jobId)),
  }
  const store = createStoreAcquisition(client, { pollIntervalMs: 2, onImported: id => imported.push(id) })
  const seen: SurfaceStore[] = []
  store.subscribe(() => seen.push(store.getSnapshot()))
  const now = () => store.getSnapshot()
  const get = (): SurfaceStoreGet => {
    const details = now().details
    if (details._tag !== "Ready") throw new Error(`details are ${details._tag}`)
    return details.get
  }
  const until = async (what: string, ok: () => boolean) => {
    for (let i = 0; i < 400; i++) {
      if (ok()) return
      await new Promise(resolve => setTimeout(resolve, 2))
    }
    throw new Error(`never saw ${what}; get is ${JSON.stringify(now().details)}`)
  }
  const press = (label: string) => {
    const get = now().details._tag === "Ready" ? (now().details as Extract<SurfaceStore["details"], { _tag: "Ready" }>).get : undefined
    const actions: readonly SurfaceAction[] = get !== undefined && "actions" in get ? get.actions : []
    const found = actions.find(action => action.label === label)
    if (found === undefined) throw new Error(`no ${label} in ${JSON.stringify(get)}`)
    expect(store.act(found.id)).toBe(true)
    return found.id
  }
  /** Providers, a search and one entry, opened the way a surface does. */
  const open = async (id: string, query = "") => {
    store.openStore()
    await until("providers", () => now().providers._tag === "Ready")
    store.search(HUB, query)
    await until("results", () => now().search._tag === "Ready")
    store.openEntry(storeEntryId({ providerId: HUB, id }))
    await until("details", () => now().details._tag === "Ready")
  }
  return { korrid, store, calls, imported, seen, now, get, until, press, open }
}

test("the provider list loads, and a refused list offers Retry", async () => {
  let fail = true
  const h = harness(base([]))
  const original = h.korrid.acquisitionProviders
  h.korrid.acquisitionProviders = async () => {
    if (fail) return { _tag: "Err", payload: { code: "PermissionDenied", message: "This portal cannot list providers." } }
    return original()
  }
  // The harness recorded the original method; route through the patched one.
  const store = createStoreAcquisition({ ...h.korrid, acquisitionProviders: () => h.korrid.acquisitionProviders() }, { pollIntervalMs: 2 })
  expect(store.getSnapshot().providers).toEqual({ _tag: "Loading" })
  store.openStore()
  await flush()
  const problem = store.getSnapshot().providers
  expect(problem._tag).toBe("Problem")
  if (problem._tag !== "Problem") return
  expect(problem.message).toBe("This portal cannot list providers.")
  expect(problem.actions.map(action => action.label)).toEqual(["Retry"])
  fail = false
  store.act(problem.actions[0]!.id)
  await flush()
  await flush()
  expect(store.getSnapshot().providers).toEqual({ _tag: "Ready", providers: [{ id: HUB, title: "Homebrew Hub" }] })
})

test("a search shows results, an empty answer, and only for a listed provider", async () => {
  const h = harness(base([claim("2048gb"), claim("tobu")]))
  h.store.openStore()
  await h.until("providers", () => h.now().providers._tag === "Ready")
  h.store.search("@korri:nobody", "2048")
  await flush()
  expect(h.calls.some(call => call.startsWith("search"))).toBe(false)
  h.store.search(HUB, "2048")
  expect(h.now().search).toEqual({ _tag: "Searching", providerId: HUB, query: "2048" })
  await h.until("results", () => h.now().search._tag === "Ready")
  expect(h.now().search).toEqual({
    _tag: "Ready", providerId: HUB, query: "2048",
    entries: [{ id: storeEntryId({ providerId: HUB, id: "2048gb" }), title: "2048GB", system: "Game Boy" }],
  })
  h.store.search(HUB, "zelda")
  await h.until("empty results", () => h.now().search._tag === "Ready" && (h.now().search as { query: string }).query === "zelda")
  expect(h.now().search).toMatchObject({ _tag: "Ready", entries: [] })
})

test("a provider that does not answer offers Retry, and Retry asks again", async () => {
  const h = harness(base([claim("2048gb")], {
    failingSearches: { [HUB]: "Homebrew Hub did not answer. Check the connection and try again." },
    searchFailuresBeforeRecovery: 1,
  }))
  h.store.openStore()
  await h.until("providers", () => h.now().providers._tag === "Ready")
  h.store.search(HUB, "")
  await h.until("problem", () => h.now().search._tag === "Problem")
  const search = h.now().search
  if (search._tag !== "Problem") throw new Error("expected a problem")
  expect(search.message).toBe("Homebrew Hub did not answer. Check the connection and try again.")
  h.store.act(search.actions[0]!.id)
  await h.until("results", () => h.now().search._tag === "Ready")
  expect(h.calls.filter(call => call.startsWith("search"))).toHaveLength(2)
})

test("an older search answer never replaces a newer one", async () => {
  let release: () => void = () => {}
  const held = new Promise<void>(resolve => { release = resolve })
  const korrid = createInMemoryStoreAcquisition(base([claim("2048gb"), claim("tobu")]))
  const store = createStoreAcquisition({
    ...korrid,
    acquisitionProviders: () => korrid.acquisitionProviders(),
    acquisitionSearch: async request => {
      if (request.query === "slow") await held
      return korrid.acquisitionSearch(request)
    },
  })
  store.openStore()
  await flush()
  store.search(HUB, "slow")
  store.search(HUB, "tobu")
  await flush()
  await flush()
  release()
  await flush()
  await flush()
  expect(store.getSnapshot().search).toMatchObject({ _tag: "Ready", query: "tobu" })
})

test("details offer Get; a claim with no system and one the provider withdrew cannot be got", async () => {
  const h = harness(base([
    claim("2048gb", { description: "Slide the tiles." }),
    claim("nosys", { artifact: undefined, platform: undefined }),
    claim("gone", { unavailable: "Homebrew Hub no longer offers this game." }),
  ]))
  await h.open("2048gb")
  const details = h.now().details
  expect(details).toMatchObject({ _tag: "Ready", description: "Slide the tiles.", get: { _tag: "Offer" } })
  expect(h.get()).toMatchObject({ actions: [{ label: "Get", enabled: true }] })

  h.store.openEntry(storeEntryId({ providerId: HUB, id: "nosys" }))
  await h.until("nosys", () => h.now().details._tag === "Ready" && h.get()._tag === "Unavailable")
  expect(h.get()).toEqual({ _tag: "Unavailable", reason: NO_SYSTEM_REASON })

  h.store.openEntry(storeEntryId({ providerId: HUB, id: "gone" }))
  await h.until("gone", () => h.now().details._tag === "Ready" && (h.now().details as { entry: { title: string } }).entry.title === "GONE")
  expect(h.get()).toEqual({ _tag: "Unavailable", reason: "Homebrew Hub no longer offers this game." })
  expect(h.calls.some(call => call.startsWith("acquire"))).toBe(false)
})

test("details that fail to load offer Retry", async () => {
  const korrid = createInMemoryStoreAcquisition(base([claim("2048gb")]))
  let fail = true
  const store = createStoreAcquisition({
    ...korrid,
    acquisitionProviders: () => korrid.acquisitionProviders(),
    acquisitionSearch: request => korrid.acquisitionSearch(request),
    acquisitionDetails: async request => fail
      ? { _tag: "Err", payload: { code: "ProviderUnavailable", message: "Homebrew Hub did not answer." } }
      : korrid.acquisitionDetails(request),
  })
  store.openStore()
  await flush()
  store.search(HUB, "")
  await flush()
  await flush()
  store.openEntry(storeEntryId({ providerId: HUB, id: "2048gb" }))
  expect(store.getSnapshot().details).toMatchObject({ _tag: "Loading", entry: { title: "2048GB" } })
  await flush()
  const details = store.getSnapshot().details
  expect(details).toMatchObject({ _tag: "Problem", message: "Homebrew Hub did not answer." })
  if (details._tag !== "Problem") return
  fail = false
  store.act(details.actions[0]!.id)
  await flush()
  await flush()
  expect(store.getSnapshot().details).toMatchObject({ _tag: "Ready", get: { _tag: "Offer" } })
})

test("Get asks first with the size; Cancel there asks nothing of korrid", async () => {
  const h = harness(base([claim("2048gb"), claim("unsized", { artifact: { system: "gb" } })]))
  await h.open("2048gb")
  h.press("Get")
  expect(h.get()).toMatchObject({ _tag: "Confirm", kicker: "Download to this device", detail: "2048GB, 32 KB" })
  expect((h.get() as { readonly actions: readonly SurfaceAction[] }).actions.map(action => action.label)).toEqual(["Download", "Cancel"])
  h.press("Cancel")
  expect(h.get()).toMatchObject({ _tag: "Offer" })
  expect(h.calls.some(call => call.startsWith("acquire"))).toBe(false)

  h.store.openEntry(storeEntryId({ providerId: HUB, id: "unsized" }))
  await h.until("unsized", () => h.now().details._tag === "Ready" && (h.now().details as { entry: { title: string } }).entry.title === "UNSIZED")
  h.press("Get")
  expect(h.get()).toMatchObject({ _tag: "Confirm", detail: "UNSIZED, size unknown" })
})

test("Download runs with real progress and Cancel, places without Cancel, and lands in the library", async () => {
  const h = harness(base([claim("2048gb")]))
  await h.open("2048gb")
  h.press("Get")
  h.press("Download")
  expect(h.get()).toMatchObject({ _tag: "Working", kicker: "Starting download…", actions: [{ label: "Cancel" }] })
  await h.until("downloading", () => h.get()._tag === "Working" && "progress" in h.get())
  expect(h.get()).toMatchObject({
    _tag: "Working", kicker: "Downloading 2048GB", progress: { done: 0, total: 32_000 }, actions: [{ label: "Cancel" }],
  })
  await h.until("half", () => (h.get() as { progress?: { done: number } }).progress?.done === 16_000)
  expect(h.get()).toMatchObject({ detail: "16 KB of 32 KB · 16 KB/s · 0:01 left" })
  await h.until("placing", () => h.get()._tag === "Working" && (h.get() as { kicker: string }).kicker === "Adding to your library…")
  expect(h.get()).toEqual({ _tag: "Working", kicker: "Adding to your library…", progress: { done: 0 }, actions: [] })
  await h.until("imported", () => h.get()._tag === "InLibrary")
  expect(h.get()).toEqual({ _tag: "InLibrary", detail: "In your library", gameId: "store-korrihomebrewhub-2048gb" })
  expect(h.imported).toEqual(["store-korrihomebrewhub-2048gb"])
  expect(h.korrid.downloadedBytes()).toBe(32_000)
  // Every progress step the surface saw grew.
  const done = h.seen.flatMap(seen => seen.details._tag === "Ready" && seen.details.get._tag === "Working" && seen.details.get.progress?.total !== undefined
    ? [seen.details.get.progress.done] : [])
  expect(done).toEqual([...done].sort((a, b) => a - b))
})

test("no folder for the system is refused before any download, naming the system", async () => {
  const h = harness(base([claim("2048gb")], { sources: {} }))
  await h.open("2048gb")
  h.press("Get")
  h.press("Download")
  await h.until("refused", () => h.get()._tag === "Unavailable")
  expect(h.get()).toEqual({
    _tag: "Unavailable",
    reason: "No folder on this device holds Game Boy games. Add one in Settings, then get this game again.",
  })
  expect(h.korrid.downloadedBytes()).toBe(0)
  expect(h.calls.some(call => call.startsWith("status"))).toBe(false)
})

test("several folders: the person chooses, and acquire goes again with that source", async () => {
  const h = harness(base([claim("anguna", { platform: "Game Boy Advance", artifact: { system: "gba", file: { name: "anguna.gba", sizeBytes: 16_000 } } })], {
    sources: { gba: [{ id: "games-gba", label: "Games · GBA" }, { id: "sd-gba", label: "SD card · GBA" }] },
  }))
  await h.open("anguna")
  h.press("Get")
  h.press("Download")
  await h.until("choice", () => h.get()._tag === "Confirm" && (h.get() as { kicker: string }).kicker === "Choose a folder")
  expect(h.get()).toMatchObject({
    detail: "More than one folder on this device holds Game Boy Advance games. Choose where this one goes.",
  })
  expect((h.get() as { readonly actions: readonly SurfaceAction[] }).actions.map(action => action.label)).toEqual(["Games · GBA", "SD card · GBA", "Cancel"])
  h.press("SD card · GBA")
  await h.until("imported", () => h.get()._tag === "InLibrary")
  const acquires = h.calls.filter(call => call.startsWith("acquire"))
  expect(acquires).toHaveLength(2)
  expect(acquires[1]).toContain("\"sourceId\":\"sd-gba\"")
})

test("already in the library: Get becomes Play without a second download", async () => {
  const h = harness(base([claim("2048gb")]))
  await h.open("2048gb")
  h.press("Get")
  h.press("Download")
  await h.until("imported", () => h.get()._tag === "InLibrary")
  // Reopening the entry in this portal: Play at once.
  h.store.openEntry(storeEntryId({ providerId: HUB, id: "2048gb" }))
  await h.until("reopened", () => h.now().details._tag === "Ready")
  expect(h.get()._tag).toBe("InLibrary")

  // A portal that reloaded knows nothing; korrid returns the claim's job.
  const again = createStoreAcquisition(h.korrid, { pollIntervalMs: 2 })
  again.openStore()
  await flush()
  again.search(HUB, "")
  await flush()
  await flush()
  again.openEntry(storeEntryId({ providerId: HUB, id: "2048gb" }))
  await flush()
  await flush()
  const offer = again.getSnapshot().details
  if (offer._tag !== "Ready" || offer.get._tag !== "Offer") throw new Error("expected Get")
  again.act(offer.get.actions[0]!.id)
  const confirm = again.getSnapshot().details
  if (confirm._tag !== "Ready" || confirm.get._tag !== "Confirm") throw new Error("expected Confirm")
  again.act(confirm.get.actions[0]!.id)
  await flush()
  await flush()
  expect(again.getSnapshot().details).toMatchObject({ get: { _tag: "InLibrary", gameId: "store-korrihomebrewhub-2048gb" } })
  expect(h.korrid.downloadedBytes()).toBe(32_000)
})

test("the same bytes already in the folder import with nothing downloaded", async () => {
  const h = harness(base([claim("2048gb", { inLibrary: true })]))
  await h.open("2048gb")
  h.press("Get")
  h.press("Download")
  await h.until("imported", () => h.get()._tag === "InLibrary")
  expect(h.korrid.downloadedBytes()).toBe(0)
  expect(h.calls.some(call => call.startsWith("status"))).toBe(false)
})

test("downloaded but not recognised says so plainly", async () => {
  const h = harness(base([claim("odd", { unrecognised: true })]))
  await h.open("odd")
  h.press("Get")
  h.press("Download")
  await h.until("staged", () => h.get()._tag === "Staged")
  expect(h.get()).toEqual({ _tag: "Staged", message: STAGED_MESSAGE })
  expect(h.imported).toEqual([])
})

test("Cancel during the download stops it, and Get is offered again", async () => {
  const h = harness(base([claim("2048gb", { artifact: { system: "gb", file: { name: "2048.gb", sizeBytes: 160_000 } } })]))
  await h.open("2048gb")
  h.press("Get")
  h.press("Download")
  await h.until("downloading", () => (h.get() as { progress?: { done: number } }).progress?.done === 16_000)
  h.press("Cancel")
  expect(h.get()).toEqual({ _tag: "Working", kicker: "Cancelling download…", detail: "Nothing will be kept", actions: [] })
  await h.until("cancelled", () => h.get()._tag === "Offer")
  expect(h.get()).toMatchObject({ _tag: "Offer", note: CANCELLED_NOTE, actions: [{ label: "Get" }] })
  const polls = h.calls.filter(call => call.startsWith("status")).length
  await new Promise(resolve => setTimeout(resolve, 20))
  expect(h.calls.filter(call => call.startsWith("status")).length).toBe(polls)
  expect(h.calls.filter(call => call.startsWith("cancel"))).toHaveLength(1)
})

test("Cancel before acquire answers is honoured against the job it started", async () => {
  let release: () => void = () => {}
  const held = new Promise<void>(resolve => { release = resolve })
  const h = harness(base([claim("2048gb")], { acquireGate: () => held }))
  await h.open("2048gb")
  h.press("Get")
  h.press("Download")
  h.press("Cancel")
  expect(h.get()).toEqual({ _tag: "Working", kicker: "Cancelling download…", detail: "Nothing will be kept", actions: [] })
  release()
  await h.until("cancelled", () => h.get()._tag === "Offer")
  expect(h.get()).toMatchObject({ note: CANCELLED_NOTE })
  expect(h.calls.filter(call => call.startsWith("cancel"))).toHaveLength(1)
  expect(h.calls.some(call => call.startsWith("status"))).toBe(false)
})

test("a failed download offers Retry, and Retry finishes it", async () => {
  const h = harness(base([claim("ambushed", { failure: { message: "The provider stopped answering partway through. Try again." } })]))
  await h.open("ambushed")
  h.press("Get")
  h.press("Download")
  await h.until("failed", () => h.get()._tag === "Problem")
  expect(h.get()).toMatchObject({
    _tag: "Problem", kicker: "Download failed", reason: "The provider stopped answering partway through. Try again.",
    actions: [{ label: "Retry" }],
  })
  h.press("Retry")
  await h.until("imported", () => h.get()._tag === "InLibrary")
})

test("korrid restarting under a download: Download stopped, Retry starts from zero", async () => {
  const h = harness(base([claim("2048gb", { artifact: { system: "gb", file: { name: "2048.gb", sizeBytes: 160_000 } } })]))
  await h.open("2048gb")
  h.press("Get")
  h.press("Download")
  await h.until("downloading", () => ((h.get() as { progress?: { done: number } }).progress?.done ?? 0) >= 32_000)
  h.korrid.restart()
  await h.until("stopped", () => h.get()._tag === "Problem")
  expect(h.get()).toEqual({
    _tag: "Problem", kicker: STOPPED_KICKER, reason: STOPPED_REASON,
    actions: [expect.objectContaining({ label: "Retry" })],
  })
  h.press("Retry")
  await h.until("restarted", () => (h.get() as { progress?: { done: number } }).progress !== undefined)
  expect((h.get() as { progress: { done: number } }).progress.done).toBeLessThanOrEqual(16_000)
})

test("korrid not answering for a moment does not end the watch", async () => {
  const korrid = createInMemoryStoreAcquisition(base([claim("2048gb")]))
  let dark = 0
  const store = createStoreAcquisition({
    ...korrid,
    acquisitionProviders: () => korrid.acquisitionProviders(),
    acquisitionSearch: request => korrid.acquisitionSearch(request),
    acquisitionDetails: request => korrid.acquisitionDetails(request),
    acquisitionAcquire: request => korrid.acquisitionAcquire(request),
    acquisitionAcquireStatus: async jobId => dark-- > 0
      ? { _tag: "Err", payload: { code: "BrainUnreachable", message: "fetch failed" } }
      : korrid.acquisitionAcquireStatus(jobId),
  }, { pollIntervalMs: 2 })
  store.openStore()
  await flush()
  store.search(HUB, "")
  await flush(); await flush()
  store.openEntry(storeEntryId({ providerId: HUB, id: "2048gb" }))
  await flush(); await flush()
  dark = 5
  const press = (label: string) => {
    const details = store.getSnapshot().details
    if (details._tag !== "Ready" || !("actions" in details.get)) throw new Error("no actions")
    store.act(details.get.actions.find(action => action.label === label)!.id)
  }
  press("Get")
  press("Download")
  for (let i = 0; i < 200 && (store.getSnapshot().details as { get: SurfaceStoreGet }).get._tag !== "InLibrary"; i++) {
    await new Promise(resolve => setTimeout(resolve, 2))
  }
  expect((store.getSnapshot().details as { get: SurfaceStoreGet }).get._tag).toBe("InLibrary")
})

test("a stale or repeated press sends nothing", async () => {
  const h = harness(base([claim("2048gb")]))
  await h.open("2048gb")
  const getId = h.press("Get")
  // Get again with the old id: Confirm is unchanged, nothing is asked.
  expect(h.store.act(getId)).toBe(true)
  expect(h.get()._tag).toBe("Confirm")
  const downloadId = h.press("Download")
  expect(h.store.act(downloadId)).toBe(true)
  await h.until("imported", () => h.get()._tag === "InLibrary")
  expect(h.calls.filter(call => call.startsWith("acquire"))).toHaveLength(1)
  // An id this module never published, and one for a closed entry.
  expect(h.store.act("store:999:download:nothing")).toBe(true)
  expect(h.store.act("runner:cancel")).toBe(false)
  expect(h.calls.filter(call => call.startsWith("acquire"))).toHaveLength(1)
})

test("a download keeps going while another entry is open, and shows again on its own", async () => {
  const h = harness(base([claim("2048gb", { artifact: { system: "gb", file: { name: "2048.gb", sizeBytes: 160_000 } } }), claim("tobu")]))
  await h.open("2048gb")
  h.press("Get")
  h.press("Download")
  await h.until("downloading", () => (h.get() as { progress?: unknown }).progress !== undefined)
  h.store.openEntry(storeEntryId({ providerId: HUB, id: "tobu" }))
  await h.until("tobu", () => h.now().details._tag === "Ready" && (h.now().details as { entry: { title: string } }).entry.title === "TOBU")
  expect(h.get()._tag).toBe("Offer")
  const before = h.korrid.downloadedBytes()
  await new Promise(resolve => setTimeout(resolve, 15))
  expect(h.korrid.downloadedBytes()).toBeGreaterThan(before)
  h.store.openEntry(storeEntryId({ providerId: HUB, id: "2048gb" }))
  await h.until("back", () => h.now().details._tag === "Ready" && (h.now().details as { entry: { title: string } }).entry.title === "2048GB")
  expect(["Working", "InLibrary"]).toContain(h.get()._tag)
})

test("the failure codes the store tells apart are the mirror's", () => {
  // A reminder for integration: these names come from the local mirror, a GAP.
  expect(ACQUIRE_FAILURE).toEqual({
    noSource: "AcquireNoSource",
    severalSources: "AcquireSeveralSources",
    unknownSystem: "AcquireUnknownSystem",
    unavailable: "AcquireEntryUnavailable",
    jobNotFound: "NotFound",
  })
})
