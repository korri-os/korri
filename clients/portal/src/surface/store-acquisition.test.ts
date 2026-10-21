import { expect, test } from "bun:test"
import type { Game } from "@contracts/generated/korrid"
import type { SurfaceAction, SurfaceStore, SurfaceStoreGet } from "@contracts/surface/korri-surface"
import {
  createInMemoryStoreAcquisition,
  type InMemoryClaim,
  type InMemoryStoreConfig,
} from "../korrid/store-acquisition-client"
import { ACQUISITION_CODE } from "../korrid/acquisition-codes"
import {
  CANCELLED_NOTE,
  createStoreAcquisition,
  NO_SYSTEM_REASON,
  NOT_AVAILABLE_KICKER,
  QUEUED_DETAIL,
  STOPPED_KICKER,
  STOPPED_REASON,
  storeEntryId,
} from "./store-acquisition"

const BBS = "@korri:pico8"
const art = (system: string, file?: { name: string; sizeBytes?: number }) =>
  ({ kind: "content" as const, format: { id: "rom" }, system, ...(file === undefined ? {} : { file }) })
const claim = (id: string, extra: Partial<InMemoryClaim> = {}): InMemoryClaim => ({
  providerId: BBS, id, title: id.toUpperCase(), url: `https://www.lexaloffle.com/bbs/?tid=${id}`, platform: "PICO-8",
  artifact: art("pico8", { name: `${id}.p8.png`, sizeBytes: 32_000 }), ...extra,
})
const base = (claims: InMemoryClaim[], extra: Partial<InMemoryStoreConfig> = {}): InMemoryStoreConfig => ({
  providers: [{ id: BBS, title: "PICO-8 BBS", pluginId: BBS }],
  claims: { [BBS]: claims },
  bytesPerStep: 16_000,
  ...extra,
})

const flush = () => new Promise(resolve => setTimeout(resolve, 0))

/** The portal's lookup, over a list of library games. */
const ownedIn = (library: readonly Game[]) => (providerId: string, claimId: string) =>
  library.find(game => game.identity?.kind === "provider" &&
    game.identity.value.provider === providerId && game.identity.value.ref === claimId)?.id

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
  // The in-memory korrid writes each imported game into this catalog, with
  // the provider link as its identity, as korrid does (ADR 0014).
  const library: Game[] = []
  korrid.bindCatalog(game => { library.push(game) })
  const store = createStoreAcquisition(client, {
    pollIntervalMs: 2,
    onImported: id => {
      imported.push(id)
      store.libraryChanged()
    },
    ownedGame: ownedIn(library),
  })
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
    store.search(BBS, query)
    await until("results", () => now().search._tag === "Ready")
    store.openEntry(storeEntryId({ providerId: BBS, id }))
    await until("details", () => now().details._tag === "Ready")
  }
  return { korrid, store, calls, imported, seen, now, get, until, press, open, library }
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
  expect(store.getSnapshot().providers).toEqual({ _tag: "Ready", providers: [{ id: BBS, title: "PICO-8 BBS" }] })
})

test("a search shows results, an empty answer, and only for a listed provider", async () => {
  const h = harness(base([claim("celeste"), claim("dusk")]))
  h.store.openStore()
  await h.until("providers", () => h.now().providers._tag === "Ready")
  h.store.search("@korri:nobody", "cel")
  await flush()
  expect(h.calls.some(call => call.startsWith("search"))).toBe(false)
  h.store.search(BBS, "cel")
  expect(h.now().search).toEqual({ _tag: "Searching", providerId: BBS, query: "cel" })
  await h.until("results", () => h.now().search._tag === "Ready")
  expect(h.now().search).toEqual({
    _tag: "Ready", providerId: BBS, query: "cel",
    entries: [{ id: storeEntryId({ providerId: BBS, id: "celeste" }), title: "CELESTE", system: "PICO-8" }],
  })
  h.store.search(BBS, "zelda")
  await h.until("empty results", () => h.now().search._tag === "Ready" && (h.now().search as { query: string }).query === "zelda")
  expect(h.now().search).toMatchObject({ _tag: "Ready", entries: [] })
})

test("a provider that does not answer offers Retry, and Retry asks again", async () => {
  const h = harness(base([claim("celeste")], {
    failingSearches: { [BBS]: "The BBS did not answer. Check the connection and try again." },
    searchFailuresBeforeRecovery: 1,
  }))
  h.store.openStore()
  await h.until("providers", () => h.now().providers._tag === "Ready")
  h.store.search(BBS, "")
  await h.until("problem", () => h.now().search._tag === "Problem")
  const search = h.now().search
  if (search._tag !== "Problem") throw new Error("expected a problem")
  expect(search.message).toBe("The BBS did not answer. Check the connection and try again.")
  h.store.act(search.actions[0]!.id)
  await h.until("results", () => h.now().search._tag === "Ready")
  expect(h.calls.filter(call => call.startsWith("search"))).toHaveLength(2)
})

test("an older search answer never replaces a newer one", async () => {
  let release: () => void = () => {}
  const held = new Promise<void>(resolve => { release = resolve })
  const korrid = createInMemoryStoreAcquisition(base([claim("celeste"), claim("dusk")]))
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
  store.search(BBS, "slow")
  store.search(BBS, "dusk")
  await flush()
  await flush()
  release()
  await flush()
  await flush()
  expect(store.getSnapshot().search).toMatchObject({ _tag: "Ready", query: "dusk" })
})

test("details offer Get; a claim with no system cannot be got, and asks korrid nothing", async () => {
  const h = harness(base([
    claim("celeste", { description: "Slide the tiles." }),
    claim("nosys", { artifact: undefined, platform: undefined }),
  ]))
  await h.open("celeste")
  const details = h.now().details
  expect(details).toMatchObject({ _tag: "Ready", description: "Slide the tiles.", get: { _tag: "Offer" } })
  expect(h.get()).toMatchObject({ actions: [{ label: "Get", enabled: true }] })

  h.store.openEntry(storeEntryId({ providerId: BBS, id: "nosys" }))
  await h.until("nosys", () => h.now().details._tag === "Ready" && h.get()._tag === "Unavailable")
  expect(h.get()).toEqual({ _tag: "Unavailable", reason: NO_SYSTEM_REASON })

  expect(h.calls.some(call => call.startsWith("acquire"))).toBe(false)
})

test("a provider that will not give the file: Not available, with Retry", async () => {
  const h = harness(base([claim("gone", { unavailable: "The provider could not give this file. It may be back later." })]))
  await h.open("gone")
  h.press("Get")
  h.press("Download")
  await h.until("not available", () => h.get()._tag === "Problem")
  expect(h.get()).toMatchObject({
    _tag: "Problem", kicker: NOT_AVAILABLE_KICKER, reason: "The provider could not give this file. It may be back later.",
    actions: [{ label: "Retry" }],
  })
  expect(h.korrid.downloadedBytes()).toBe(0)
})

test("a system this device cannot play is refused before download", async () => {
  const h = harness(base([claim("chip", { platform: "CHIP-8", artifact: art("chip8") })], { unknownSystems: ["chip8"] }))
  await h.open("chip")
  h.press("Get")
  h.press("Download")
  await h.until("refused", () => h.get()._tag === "Unavailable")
  expect(h.get()).toEqual({ _tag: "Unavailable", reason: "This game is for a system this device cannot play, so it can't be downloaded here." })
})

test("a page instead of the file (DownloadNotFinal) is not available here", async () => {
  const korrid = createInMemoryStoreAcquisition(base([claim("itch")]))
  const store = createStoreAcquisition({
    ...korrid,
    acquisitionProviders: () => korrid.acquisitionProviders(),
    acquisitionSearch: request => korrid.acquisitionSearch(request),
    acquisitionDetails: request => korrid.acquisitionDetails(request),
    acquisitionAcquire: async request => ({
      _tag: "Ok",
      payload: { jobId: "j", providerId: request.providerId, id: request.id, state: {
        _tag: "Failed", reason: { code: ACQUISITION_CODE.notFinal, message: "This provider hands out this game only on its website." },
      } },
    }),
  }, { pollIntervalMs: 2 })
  store.openStore(); await flush()
  store.search(BBS, ""); await flush(); await flush()
  store.openEntry(storeEntryId({ providerId: BBS, id: "itch" })); await flush(); await flush()
  const act = (label: string) => {
    const details = store.getSnapshot().details
    if (details._tag !== "Ready" || !("actions" in details.get)) throw new Error("no actions")
    store.act(details.get.actions.find(action => action.label === label)!.id)
  }
  act("Get"); act("Download"); await flush(); await flush()
  expect(store.getSnapshot().details).toMatchObject({ get: { _tag: "Unavailable", reason: "This provider hands out this game only on its website." } })
})

test("details that fail to load offer Retry", async () => {
  const korrid = createInMemoryStoreAcquisition(base([claim("celeste")]))
  let fail = true
  const store = createStoreAcquisition({
    ...korrid,
    acquisitionProviders: () => korrid.acquisitionProviders(),
    acquisitionSearch: request => korrid.acquisitionSearch(request),
    acquisitionDetails: async request => fail
      ? { _tag: "Err", payload: { code: "ProviderUnavailable", message: "The BBS did not answer." } }
      : korrid.acquisitionDetails(request),
  })
  store.openStore()
  await flush()
  store.search(BBS, "")
  await flush()
  await flush()
  store.openEntry(storeEntryId({ providerId: BBS, id: "celeste" }))
  expect(store.getSnapshot().details).toMatchObject({ _tag: "Loading", entry: { title: "CELESTE" } })
  await flush()
  const details = store.getSnapshot().details
  expect(details).toMatchObject({ _tag: "Problem", message: "The BBS did not answer." })
  if (details._tag !== "Problem") return
  fail = false
  store.act(details.actions[0]!.id)
  await flush()
  await flush()
  expect(store.getSnapshot().details).toMatchObject({ _tag: "Ready", get: { _tag: "Offer" } })
})

test("Get asks first with the size; Cancel there asks nothing of korrid", async () => {
  const h = harness(base([claim("celeste"), claim("unsized", { artifact: art("pico8") })]))
  await h.open("celeste")
  h.press("Get")
  expect(h.get()).toMatchObject({ _tag: "Confirm", kicker: "Download to this device", detail: "CELESTE, 32 KB" })
  expect((h.get() as { readonly actions: readonly SurfaceAction[] }).actions.map(action => action.label)).toEqual(["Download", "Cancel"])
  h.press("Cancel")
  expect(h.get()).toMatchObject({ _tag: "Offer" })
  expect(h.calls.some(call => call.startsWith("acquire"))).toBe(false)

  h.store.openEntry(storeEntryId({ providerId: BBS, id: "unsized" }))
  await h.until("unsized", () => h.now().details._tag === "Ready" && (h.now().details as { entry: { title: string } }).entry.title === "UNSIZED")
  h.press("Get")
  expect(h.get()).toMatchObject({ _tag: "Confirm", detail: "UNSIZED, size unknown" })
})

test("Download runs with real progress and Cancel, places without Cancel, and lands in the library", async () => {
  const h = harness(base([claim("celeste")]))
  await h.open("celeste")
  h.press("Get")
  h.press("Download")
  expect(h.get()).toMatchObject({ _tag: "Working", kicker: "Starting download…", actions: [{ label: "Cancel" }] })
  await h.until("downloading", () => h.get()._tag === "Working" && "progress" in h.get())
  expect(h.get()).toMatchObject({
    _tag: "Working", kicker: "Downloading CELESTE", progress: { done: 0, total: 32_000 }, actions: [{ label: "Cancel" }],
  })
  await h.until("half", () => (h.get() as { progress?: { done: number } }).progress?.done === 16_000)
  expect(h.get()).toMatchObject({ detail: "16 KB of 32 KB · 16 KB/s · 0:01 left" })
  await h.until("placing", () => h.get()._tag === "Working" && (h.get() as { kicker: string }).kicker === "Adding to your library…")
  expect(h.get()).toEqual({ _tag: "Working", kicker: "Adding to your library…", actions: [] })
  await h.until("imported", () => h.get()._tag === "InLibrary")
  expect(h.get()).toEqual({ _tag: "InLibrary", detail: "In your library", gameId: "store-korripico8-celeste" })
  expect(h.imported).toEqual(["store-korripico8-celeste"])
  expect(h.korrid.downloadedBytes()).toBe(32_000)
  // Every progress step the surface saw grew.
  const done = h.seen.flatMap(seen => seen.details._tag === "Ready" && seen.details.get._tag === "Working" && seen.details.get.progress?.total !== undefined
    ? [seen.details.get.progress.done] : [])
  expect(done).toEqual([...done].sort((a, b) => a - b))
})

test("already in the library: the entry offers Play at once, in a new portal too", async () => {
  const h = harness(base([claim("celeste")]))
  await h.open("celeste")
  h.press("Get")
  h.press("Download")
  await h.until("imported", () => h.get()._tag === "InLibrary")
  const bytes = h.korrid.downloadedBytes()
  // Reopening the entry in this portal: Play at once.
  h.store.openEntry(storeEntryId({ providerId: BBS, id: "celeste" }))
  await h.until("reopened", () => h.now().details._tag === "Ready")
  expect(h.get()).toEqual({ _tag: "InLibrary", detail: "In your library", gameId: "store-korripico8-celeste" })

  // A portal that reloaded knows no job; the catalog's provider identity is
  // enough: Play, with no Get, no Confirm and no acquire.
  const calls: string[] = []
  const again = createStoreAcquisition({
    ...h.korrid,
    acquisitionProviders: () => h.korrid.acquisitionProviders(),
    acquisitionSearch: request => h.korrid.acquisitionSearch(request),
    acquisitionDetails: request => h.korrid.acquisitionDetails(request),
    acquisitionAcquire: request => { calls.push("acquire"); return h.korrid.acquisitionAcquire(request) },
  }, { pollIntervalMs: 2, ownedGame: ownedIn(h.library) })
  again.openStore(); await flush()
  again.search(BBS, ""); await flush(); await flush()
  again.openEntry(storeEntryId({ providerId: BBS, id: "celeste" })); await flush(); await flush()
  expect(again.getSnapshot().details).toMatchObject({ get: { _tag: "InLibrary", gameId: "store-korripico8-celeste" } })
  expect(calls).toEqual([])
  expect(h.korrid.downloadedBytes()).toBe(bytes)
})
test("Cancel during the download stops it, and Get is offered again", async () => {
  const h = harness(base([claim("celeste", { artifact: art("pico8", { name: "celeste.p8.png", sizeBytes: 160_000 }) })]))
  await h.open("celeste")
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
  const h = harness(base([claim("celeste")], { acquireGate: () => held }))
  await h.open("celeste")
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
  const h = harness(base([claim("celeste", { artifact: art("pico8", { name: "celeste.p8.png", sizeBytes: 160_000 }) })]))
  await h.open("celeste")
  h.press("Get")
  h.press("Download")
  await h.until("downloading", () => ((h.get() as { progress?: { done: number } }).progress?.done ?? 0) >= 32_000)
  h.korrid.restart()
  await h.until("stopped", () => h.get()._tag === "Problem")
  expect(h.get()).toEqual({
    _tag: "Problem", kicker: STOPPED_KICKER, reason: STOPPED_REASON,
    actions: [expect.objectContaining({ label: "Retry" })],
  })
  const before = h.calls.length
  h.press("Retry")
  await h.until("restarted", () => (h.get() as { progress?: { done: number } }).progress !== undefined)
  expect((h.get() as { progress: { done: number } }).progress.done).toBeLessThanOrEqual(16_000)
  // korrid lost the claim with the restart: the store read it again, then asked again.
  expect(h.calls.slice(before, before + 3).map(call => call.split(" ")[0])).toEqual(["acquire", "details", "acquire"])
})

test("after a korrid restart an owned cart is still Play; a removed one offers Get again", async () => {
  const h = harness(base([claim("celeste")]))
  await h.open("celeste")
  h.press("Get")
  h.press("Download")
  await h.until("imported", () => h.get()._tag === "InLibrary")
  // korrid restarts: jobs and held claims are gone, the cart, its game and
  // its provider link stay. The open entry is still Play, with no acquire.
  const before = h.calls.length
  h.korrid.restart()
  h.store.libraryChanged()
  expect(h.get()).toEqual({ _tag: "InLibrary", detail: "In your library", gameId: "store-korripico8-celeste" })
  expect(h.calls.length).toBe(before)
  // Remove download (ADR 0013, ADR 0014 Q4): the game leaves the local
  // catalog, so the entry offers Get again (Q6).
  h.library.splice(0)
  h.store.libraryChanged()
  expect(h.get()).toMatchObject({ _tag: "Offer", actions: [{ label: "Get" }] })
})
test("a second Get waits in korrid's queue with Cancel, and moves when the first ends", async () => {
  const big = art("pico8", { name: "celeste.p8.png", sizeBytes: 64_000 })
  const h = harness(base([claim("celeste", { artifact: big }), claim("dusk"), claim("tide")]))
  await h.open("celeste")
  h.press("Get")
  h.press("Download")
  await h.until("downloading", () => (h.get() as { progress?: unknown }).progress !== undefined)
  // Second entry: korrid has one download moving, so this one waits.
  h.store.openEntry(storeEntryId({ providerId: BBS, id: "dusk" }))
  await h.until("dusk", () => h.now().details._tag === "Ready" && (h.now().details as { entry: { title: string } }).entry.title === "DUSK")
  h.press("Get")
  h.press("Download")
  await h.until("queued", () => h.get()._tag === "Working" && (h.get() as { kicker: string }).kicker === "Waiting to download")
  expect(h.get()).toMatchObject({ _tag: "Working", kicker: "Waiting to download", detail: QUEUED_DETAIL, actions: [{ label: "Cancel" }] })
  expect("progress" in h.get()).toBe(false)
  // A third waits too, and Cancel works while it waits.
  h.store.openEntry(storeEntryId({ providerId: BBS, id: "tide" }))
  await h.until("tide", () => h.now().details._tag === "Ready" && (h.now().details as { entry: { title: string } }).entry.title === "TIDE")
  h.press("Get")
  h.press("Download")
  await h.until("tide queued", () => (h.get() as { kicker?: string }).kicker === "Waiting to download")
  h.press("Cancel")
  await h.until("tide cancelled", () => h.get()._tag === "Offer")
  expect(h.get()).toMatchObject({ note: CANCELLED_NOTE })
  // The first finishes; the second then moves on its own.
  h.store.openEntry(storeEntryId({ providerId: BBS, id: "dusk" }))
  await h.until("dusk moves", () => h.now().details._tag === "Ready" && h.get()._tag === "InLibrary")
})

test("korrid not answering for a moment does not end the watch", async () => {
  const korrid = createInMemoryStoreAcquisition(base([claim("celeste")]))
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
  store.search(BBS, "")
  await flush(); await flush()
  store.openEntry(storeEntryId({ providerId: BBS, id: "celeste" }))
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
  const h = harness(base([claim("celeste")]))
  await h.open("celeste")
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
  const h = harness(base([claim("celeste", { artifact: art("pico8", { name: "celeste.p8.png", sizeBytes: 160_000 }) }), claim("dusk")]))
  await h.open("celeste")
  h.press("Get")
  h.press("Download")
  await h.until("downloading", () => (h.get() as { progress?: unknown }).progress !== undefined)
  h.store.openEntry(storeEntryId({ providerId: BBS, id: "dusk" }))
  await h.until("dusk", () => h.now().details._tag === "Ready" && (h.now().details as { entry: { title: string } }).entry.title === "DUSK")
  expect(h.get()._tag).toBe("Offer")
  const before = h.korrid.downloadedBytes()
  await new Promise(resolve => setTimeout(resolve, 15))
  expect(h.korrid.downloadedBytes()).toBeGreaterThan(before)
  h.store.openEntry(storeEntryId({ providerId: BBS, id: "celeste" }))
  await h.until("back", () => h.now().details._tag === "Ready" && (h.now().details as { entry: { title: string } }).entry.title === "CELESTE")
  expect(["Working", "InLibrary"]).toContain(h.get()._tag)
})

test("the codes the store tells apart are build-3's (SCRIPTING.md)", () => {
  expect(ACQUISITION_CODE).toEqual({
    unknownJob: "AcquisitionUnknownJob",
    unknownClaim: "AcquisitionUnknownClaim",
    systemUnknown: "AcquisitionSystemUnknown",
    notFinal: "DownloadNotFinal",
    resolutionFailed: "DownloadResolutionFailed",
  })
})
