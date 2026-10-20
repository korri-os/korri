/**
 * The six game-acquisition RPC methods, over HTTP and in memory.
 *
 * Kept apart from client.ts because their wire types are still the local
 * mirror in store-acquisition-types.ts (a GAP to delete at integration): the
 * HTTP half posts the mirror's tagged request through the same `/rpc` call
 * every other method uses. The in-memory half is the browser preview's and
 * the tests' korrid: providers, claims, folders and a job that moves one step
 * per status read, so a polled observation is what moves it.
 */
import type { Game, RpcFailure, RpcRequest } from "@contracts/generated/korrid"
import {
  ACQUISITION_CODE,
  type AcquireArtifactRequest,
  type AcquireJobOutcome,
  type AcquireJobState,
  type AcquireJobStatus,
  type AcquisitionProvider,
  type DetailsOutcome,
  type DetailsRequest,
  type PluginListOutcome,
  type ProviderClaim,
  type SearchOutcome,
  type SearchRequest,
  type StoreAcquisitionRpcRequest,
} from "./store-acquisition-types"

export interface StoreAcquisitionMethods {
  /** Providers every enabled provider plugin declares. */
  acquisitionProviders(): Promise<PluginListOutcome>
  /** Search exactly one provider. */
  acquisitionSearch(request: SearchRequest): Promise<SearchOutcome>
  acquisitionDetails(request: DetailsRequest): Promise<DetailsOutcome>
  /** Start, or return the live or imported job for the same claim. */
  acquisitionAcquire(request: AcquireArtifactRequest): Promise<AcquireJobOutcome>
  acquisitionAcquireStatus(jobId: string): Promise<AcquireJobOutcome>
  acquisitionAcquireCancel(jobId: string): Promise<AcquireJobOutcome>
}

type Call = (request: RpcRequest) => Promise<{ readonly outcome: unknown }>
type Failed = (error: unknown) => { readonly _tag: "Err"; readonly payload: RpcFailure }

/** The HTTP half. `call` is client.ts's `/rpc` post; `failed` its error map. */
export function httpStoreAcquisition(call: Call, failed: Failed): StoreAcquisitionMethods {
  const send = async <Outcome>(request: StoreAcquisitionRpcRequest): Promise<Outcome> => {
    try {
      // GAP: the mirror's requests are not in the generated RpcRequest union yet.
      const response = await call(request as unknown as RpcRequest)
      return response.outcome as Outcome
    } catch (error) {
      return failed(error) as Outcome
    }
  }
  return {
    acquisitionProviders: () => send({ _tag: "app.acquisition.providers", payload: {} }),
    acquisitionSearch: payload => send({ _tag: "app.acquisition.search", payload: { query: payload.query, providerIds: [...payload.providerIds] } }),
    acquisitionDetails: payload => send({ _tag: "app.acquisition.details", payload }),
    acquisitionAcquire: payload => send({ _tag: "app.acquisition.acquire", payload }),
    acquisitionAcquireStatus: jobId => send({ _tag: "app.acquisition.acquire-status", payload: { jobId } }),
    acquisitionAcquireCancel: jobId => send({ _tag: "app.acquisition.acquire-cancel", payload: { jobId } }),
  }
}

/** One claim the in-memory providers offer, and how its download goes. */
export interface InMemoryClaim extends Omit<ProviderClaim, "_tag"> {
  readonly description?: string
  /** Fail once, with this plain copy, when the download reaches `atFraction`
   * (default half). The next acquire completes. */
  readonly failure?: { readonly message: string; readonly atFraction?: number }
  /** The library does not recognise the file: the job ends Staged. */
  readonly unrecognised?: boolean
  /** The provider will not hand out the file (FailedDownload): the job fails
   * DownloadResolutionFailed with this copy. */
  readonly unavailable?: string
  /** The size the server states once the download starts (Content-Length),
   * when the provider's claim stated none. */
  readonly contentLength?: number
  /** The bytes are already in the folder: acquire answers Imported at once. */
  readonly inLibrary?: boolean
}

export interface InMemoryStoreConfig {
  readonly providers?: readonly AcquisitionProvider[]
  /** Keyed by provider id. */
  readonly claims?: Readonly<Record<string, readonly InMemoryClaim[]>>
  /** Folders on this device, keyed by system id. Absent: none. */
  readonly sources?: Readonly<Record<string, readonly { readonly id: string; readonly label: string }[]>>
  /** Systems the registry does not know: their claims cannot be acquired. */
  readonly unknownSystems?: readonly string[]
  /** Provider ids whose searches fail, with korrid's message. */
  readonly failingSearches?: Readonly<Record<string, string>>
  /** Failing searches recover after this many tries. Default: never. */
  readonly searchFailuresBeforeRecovery?: number
  /** Default 64 KB a step. */
  readonly bytesPerStep?: number
  /** Status reads a job spends Resolving, then Placing. Default 1 each. */
  readonly resolveSteps?: number
  readonly placeSteps?: number
  /** acquire answers after this resolves, so a test can press Cancel first. */
  readonly acquireGate?: () => Promise<void>
}

export interface InMemoryStoreAcquisition extends StoreAcquisitionMethods {
  /** korrid restarted: every job is gone, and no partial file remains. */
  restart(): void
  /** Bytes korrid fetched in total, across every job: proves no second download. */
  readonly downloadedBytes: () => number
  /** Where an imported game goes: the in-memory client's catalog. */
  bindCatalog(addGame: (game: Game) => void): void
}

/**
 * The browser preview's korrid: the three providers in scope (workflow
 * contract section 1) with the entries the providers worker found. Curated
 * entries state no size, so Confirm says the size is unknown and the size
 * arrives with the download (`contentLength`). No art: the portal never loads
 * a provider's image. Folders: one each for Game Boy, NES and PICO-8, two for
 * GBA (a choice). Ambushed fails once partway, so Retry shows.
 */
export const SAMPLE_STORE: InMemoryStoreConfig = {
  providers: [
    { id: "@korri:pico8", title: "PICO-8 BBS", pluginId: "@korri:pico8" },
    { id: "@korri:homebrewhub", title: "Homebrew Hub", pluginId: "@korri:homebrewhub" },
    { id: "@korri:retrobrews", title: "RetroBrews", pluginId: "@korri:retrobrews" },
  ],
  claims: {
    "@korri:homebrewhub": [
      {
        providerId: "@korri:homebrewhub", id: "2048gb", title: "2048", url: "https://hh3.gbdev.io/game/2048gb",
        platform: "Game Boy", description: "Slide the tiles and join equal numbers to reach 2048.",
        artifact: { kind: "content", format: { id: "gb-rom" }, system: "gb", file: { name: "2048.gb" } },
        contentLength: 32_768,
      },
    ],
    "@korri:retrobrews": [
      {
        providerId: "@korri:retrobrews", id: "anguna", title: "Anguna", url: "https://retrobrews.github.io/gba-games/anguna",
        platform: "Game Boy Advance", description: "A top-down action adventure through a dungeon, made for the GBA.",
        artifact: { kind: "content", format: { id: "gba-rom" }, system: "gba", file: { name: "anguna.gba" } },
        contentLength: 1_700_000,
      },
      {
        providerId: "@korri:retrobrews", id: "ambushed", title: "Ambushed", url: "https://retrobrews.github.io/nes-games/ambushed",
        platform: "NES", artifact: { kind: "content", format: { id: "nes-rom" }, system: "nes", file: { name: "ambushed.nes" } },
        failure: { message: "The download did not finish. Try again." },
      },
    ],
    "@korri:pico8": [
      {
        providerId: "@korri:pico8", id: "celeste_classic_2-5", title: "Celeste Classic 2", url: "https://www.lexaloffle.com/bbs/?tid=41282",
        platform: "PICO-8", description: "Madeline climbs again, with a grappling hook this time.",
        artifact: { kind: "content", format: { id: "pico8-cart" }, system: "pico8", file: { name: "celeste_classic_2-5.p8.png" } },
        contentLength: 15_872,
      },
      {
        providerId: "@korri:pico8", id: "untitled-wip", title: "untitled work in progress", url: "https://www.lexaloffle.com/bbs/?tid=1",
        description: "A cart its author has not finished.",
      },
    ],
  },
  sources: {
    gb: [{ id: "games-gb", label: "Game Boy" }],
    nes: [{ id: "games-nes", label: "NES" }],
    gba: [
      { id: "games-gba", label: "Game Boy Advance" },
      { id: "sd-gba", label: "SD card · GBA" },
    ],
    pico8: [{ id: "games-pico8", label: "PICO-8" }],
  },
  failingSearches: { "@korri:pico8": "The provider did not answer. Check the connection and try again." },
  searchFailuresBeforeRecovery: 1,
}

interface InMemoryJob {
  status: AcquireJobStatus
  readonly claim: InMemoryClaim
  readonly sourceId: string
  partial: number
  resolveLeft: number
  placeLeft: number
}

/** What Downloading reports as `bytesTotal`: the server's length, else the claim's. */
const totalOf = (claim: InMemoryClaim): number | undefined => claim.contentLength ?? claim.artifact?.file?.sizeBytes

const failure = (code: string, message: string): { _tag: "Err"; payload: RpcFailure } =>
  ({ _tag: "Err", payload: { code, message } })

export function createInMemoryStoreAcquisition(
  config: InMemoryStoreConfig = SAMPLE_STORE,
): InMemoryStoreAcquisition {
  let addGame: (game: Game) => void = () => {}
  const providers = config.providers ?? []
  const claims = config.claims ?? {}
  const sources = config.sources ?? {}
  const bytesPerStep = config.bytesPerStep ?? 64_000
  let jobs = new Map<string, InMemoryJob>()
  /** legacy activeByClaim: one job per claim. */
  let byClaim = new Map<string, string>()
  const searchFailures = new Map<string, number>()
  const imported = new Set<string>()
  /** Claims whose `failure` already happened once. */
  const failedOnce = new Set<string>()
  let jobSeq = 0
  let downloaded = 0
  const claimOf = (providerId: string, id: string) => claims[providerId]?.find(claim => claim.id === id)
  const snapshot = (job: InMemoryJob): AcquireJobOutcome => ({ _tag: "Ok", payload: structuredClone(job.status) })
  const set = (job: InMemoryJob, state: AcquireJobState) => {
    job.status = { ...job.status, state }
  }
  const finish = (job: InMemoryJob) => {
    const claim = job.claim
    if (claim.unrecognised) {
      set(job, { _tag: "Staged", message: "" })
      return
    }
    const gameId = `store-${claim.providerId.replace(/[^a-z0-9]/g, "")}-${claim.id}`
    if (!imported.has(gameId)) {
      imported.add(gameId)
      addGame({ id: gameId, title: claim.title, supportsRunnerSelection: false, source: { label: "browser", isLocal: true } })
    }
    set(job, { _tag: "Imported", gameId })
  }
  const step = (job: InMemoryJob) => {
    const state = job.status.state
    if (state._tag === "Resolving") {
      if (job.resolveLeft > 0) job.resolveLeft -= 1
      else {
        const total = totalOf(job.claim)
        set(job, { _tag: "Downloading", bytesDone: 0, ...(total === undefined ? {} : { bytesTotal: total }) })
      }
    } else if (state._tag === "Downloading") {
      const total = totalOf(job.claim)
      // A size korrid does not know still ends: the server closes at 4 steps.
      const end = total ?? bytesPerStep * 4
      const done = Math.min(end, job.partial + bytesPerStep)
      downloaded += done - job.partial
      job.partial = done
      const fault = job.claim.failure
      const key = JSON.stringify([job.claim.providerId, job.claim.id])
      if (fault !== undefined && !failedOnce.has(key) && done / end >= (fault.atFraction ?? 0.5)) {
        failedOnce.add(key)
        set(job, { _tag: "Failed", reason: { code: "DownloadFailed", message: fault.message } })
      } else if (done >= end) {
        job.placeLeft = config.placeSteps ?? 1
        set(job, { _tag: "Placing" })
      } else {
        set(job, { _tag: "Downloading", bytesDone: done, ...(total === undefined ? {} : { bytesTotal: total }), bytesPerSecond: bytesPerStep })
      }
    } else if (state._tag === "Placing") {
      if (job.placeLeft > 0) job.placeLeft -= 1
      else finish(job)
    }
  }
  return {
    async acquisitionProviders() {
      return { _tag: "Ok", payload: { providers: structuredClone([...providers]) } }
    },
    async acquisitionSearch({ query, providerIds }) {
      if (providerIds.length !== 1) return failure("AcquisitionCaller", "Search one provider at a time.")
      const providerId = providerIds[0] ?? ""
      if (!providers.some(provider => provider.id === providerId)) return failure("AcquisitionUnknownProvider", "That provider is not on this device.")
      const fails = config.failingSearches?.[providerId]
      if (fails !== undefined) {
        const tries = (searchFailures.get(providerId) ?? 0) + 1
        searchFailures.set(providerId, tries)
        const recoverAfter = config.searchFailuresBeforeRecovery
        if (recoverAfter === undefined || tries <= recoverAfter) return failure("ProviderUnreachable", fails)
      }
      const needle = query.trim().toLowerCase()
      const found = (claims[providerId] ?? []).filter(claim => needle === "" || claim.title.toLowerCase().includes(needle))
      return {
        _tag: "Ok",
        payload: {
          claims: found.map(({ description: _description, failure: _failure, unrecognised: _u, unavailable: _a, inLibrary: _l, ...claim }) =>
            ({ _tag: "ProviderClaim" as const, ...structuredClone(claim) })),
        },
      }
    },
    async acquisitionDetails({ providerId, id }) {
      const claim = claimOf(providerId, id)
      if (claim === undefined) return failure("AcquisitionUnknownClaim", "Search again: Korri no longer holds this game.")
      const { providerId: owner, id: claimId, title, url, description, artifact } = claim
      return {
        _tag: "Ok",
        payload: structuredClone({
          _tag: "ProviderClaimDetails" as const, providerId: owner, id: claimId, title, url,
          ...(description === undefined ? {} : { description }),
          ...(artifact === undefined ? {} : { artifact }),
        }),
      }
    },
    async acquisitionAcquire({ providerId, id, sourceId }) {
      await config.acquireGate?.()
      const claim = claimOf(providerId, id)
      if (claim === undefined) return failure("AcquisitionUnknownClaim", "Search again: Korri no longer holds this game.")
      const key = JSON.stringify([providerId, id])
      const existing = byClaim.get(key)
      const running = existing === undefined ? undefined : jobs.get(existing)
      // A repeated acquire for a claim whose newest job is live or Imported
      // returns that job; after Failed, Cancelled or Staged it is a new try.
      const tag = running?.status.state._tag
      if (running !== undefined && (tag === "Resolving" || tag === "Downloading" || tag === "Placing" || tag === "Imported")) {
        return snapshot(running)
      }
      const system = claim.artifact?.system
      if (system === undefined || config.unknownSystems?.includes(system) === true) {
        return failure(ACQUISITION_CODE.systemUnknown, "This game is for a system this device cannot play, so it can't be downloaded here.")
      }
      const folders = sources[system] ?? []
      if (folders.length === 0) {
        return failure(ACQUISITION_CODE.noSource, `No folder on this device holds ${claim.platform ?? system} games. Add one in Settings, then get this game again.`)
      }
      if (sourceId !== undefined && !folders.some(folder => folder.id === sourceId)) {
        return failure("AcquisitionCaller", "That folder cannot take this game.")
      }
      const jobId = `job-${++jobSeq}`
      const job: InMemoryJob = {
        claim,
        sourceId: sourceId ?? folders[0]?.id ?? "",
        partial: 0,
        resolveLeft: config.resolveSteps ?? 1,
        placeLeft: config.placeSteps ?? 1,
        status: {
          jobId, providerId, id, system,
          ...(claim.artifact?.file?.name === undefined ? {} : { fileName: claim.artifact.file.name }),
          state: { _tag: "Resolving" },
        },
      }
      jobs.set(jobId, job)
      byClaim.set(key, jobId)
      // Several folders and none chosen: a terminal job that downloads nothing.
      if (folders.length > 1 && sourceId === undefined) {
        set(job, { _tag: "ChooseSource", sources: structuredClone([...folders]) })
        return snapshot(job)
      }
      // The provider will not hand out the file.
      if (claim.unavailable !== undefined) {
        set(job, { _tag: "Failed", reason: { code: ACQUISITION_CODE.resolutionFailed, message: claim.unavailable } })
        return snapshot(job)
      }
      // The same bytes already sit in the folder: korrid reuses them.
      if (claim.inLibrary) finish(job)
      return snapshot(job)
    },
    async acquisitionAcquireStatus(jobId) {
      const job = jobs.get(jobId)
      if (job === undefined) return failure(ACQUISITION_CODE.unknownJob, "Korri has no such download.")
      step(job)
      return snapshot(job)
    },
    async acquisitionAcquireCancel(jobId) {
      const job = jobs.get(jobId)
      if (job === undefined) return failure(ACQUISITION_CODE.unknownJob, "Korri has no such download.")
      const tag = job.status.state._tag
      if (tag === "Resolving" || tag === "Downloading") set(job, { _tag: "Cancelled" })
      return snapshot(job)
    },
    restart() {
      jobs = new Map()
      byClaim = new Map()
    },
    downloadedBytes: () => downloaded,
    bindCatalog(next) {
      addGame = next
    },
  }
}
