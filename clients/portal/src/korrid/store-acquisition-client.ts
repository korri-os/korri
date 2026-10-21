/**
 * The six game-acquisition RPC methods, over HTTP and in memory.
 *
 * The HTTP half posts the generated tagged requests through the same `/rpc`
 * call every other method uses. The in-memory half is the browser preview's and
 * the tests' korrid: providers, claims and a job that moves one step
 * per status read, so a polled observation is what moves it.
 */
import type {
  AcquireArtifactRequest,
  AcquireJobOutcome,
  AcquireJobState,
  AcquireJobStatus,
  AcquisitionProvider,
  DetailsOutcome,
  DetailsRequest,
  Game,
  PluginListOutcome,
  ProviderClaim,
  RpcFailure,
  RpcRequest,
  RpcResponse,
  SearchOutcome,
  SearchRequest,
} from "@contracts/generated/korrid"
import { ACQUISITION_CODE } from "./acquisition-codes"

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

/** client.ts's typed `/rpc` post: the response carries the request's tag. */
type Call = <Request extends RpcRequest>(
  request: Request,
) => Promise<Extract<RpcResponse, { readonly _tag: Request["_tag"] }>>
type Failed = (error: unknown) => { readonly _tag: "Err"; readonly payload: RpcFailure }

/** The HTTP half. `call` is client.ts's `/rpc` post; `failed` its error map. */
export function httpStoreAcquisition(call: Call, failed: Failed): StoreAcquisitionMethods {
  return {
    async acquisitionProviders() {
      try { return (await call({ _tag: "app.acquisition.providers", payload: {} })).outcome } catch (error) { return failed(error) }
    },
    async acquisitionSearch({ query, providerIds }) {
      try {
        return (await call({ _tag: "app.acquisition.search", payload: { query, providerIds: [...providerIds] } })).outcome
      } catch (error) { return failed(error) }
    },
    async acquisitionDetails(payload) {
      try { return (await call({ _tag: "app.acquisition.details", payload })).outcome } catch (error) { return failed(error) }
    },
    async acquisitionAcquire(payload) {
      try { return (await call({ _tag: "app.acquisition.acquire", payload })).outcome } catch (error) { return failed(error) }
    },
    async acquisitionAcquireStatus(jobId) {
      try { return (await call({ _tag: "app.acquisition.acquire-status", payload: { jobId } })).outcome } catch (error) { return failed(error) }
    },
    async acquisitionAcquireCancel(jobId) {
      try { return (await call({ _tag: "app.acquisition.acquire-cancel", payload: { jobId } })).outcome } catch (error) { return failed(error) }
    },
  }
}

/** One claim the in-memory providers offer, and how its download goes. */
export interface InMemoryClaim extends Omit<ProviderClaim, "_tag"> {
  readonly description?: string
  /** Fail once, with this plain copy, when the download reaches `atFraction`
   * (default half). The next acquire completes. */
  readonly failure?: { readonly message: string; readonly atFraction?: number }
  /** The provider will not hand out the file (FailedDownload): the job fails
   * DownloadResolutionFailed with this copy. */
  readonly unavailable?: string
  /** The size the server states once the download starts (Content-Length),
   * when the provider's claim stated none. */
  readonly contentLength?: number
}

export interface InMemoryStoreConfig {
  readonly providers?: readonly AcquisitionProvider[]
  /** Keyed by provider id. */
  readonly claims?: Readonly<Record<string, readonly InMemoryClaim[]>>
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
  /** korrid restarted: every job and every held claim is gone, and no
   * partial file remains. A cart that finished stays in the library and the
   * cache, so acquiring it again answers Imported at once (section 11). */
  restart(): void
  /** Bytes korrid fetched in total, across every job: proves no second download. */
  readonly downloadedBytes: () => number
  /** Where an imported game goes: the in-memory client's catalog. */
  bindCatalog(addGame: (game: Game) => void): void
}

/**
 * The browser preview's korrid: the one provider in scope that searches
 * (workflow contract section 10), the PICO-8 BBS. Homebrew Hub and RetroBrews
 * are declared games under ADR 0013, not providers. The BBS states no size,
 * so Confirm says the size is unknown and the size arrives with the download
 * (`contentLength`). No art: the portal never loads a provider's image. The
 * first search does not answer, so Retry shows; one cart names no system.
 */
export const SAMPLE_STORE: InMemoryStoreConfig = {
  providers: [{ id: "@korri:pico8", title: "PICO-8 BBS", pluginId: "@korri:pico8" }],
  claims: {
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
  failingSearches: { "@korri:pico8": "The provider did not answer. Check the connection and try again." },
  searchFailuresBeforeRecovery: 1,
}

interface InMemoryJob {
  status: AcquireJobStatus
  readonly claim: InMemoryClaim
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
  const bytesPerStep = config.bytesPerStep ?? 64_000
  let jobs = new Map<string, InMemoryJob>()
  /** legacy activeByClaim: one job per claim. */
  let byClaim = new Map<string, string>()
  const searchFailures = new Map<string, number>()
  const imported = new Set<string>()
  /** Claims search or details returned since the last start: acquire needs one. */
  let held = new Set<string>()
  /** claim key -> game id, for carts in the library and the cache. Survives a restart. */
  const inLibrary = new Map<string, string>()
  const claimKey = (providerId: string, id: string) => JSON.stringify([providerId, id])
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
    const gameId = `store-${claim.providerId.replace(/[^a-z0-9]/g, "")}-${claim.id}`
    inLibrary.set(claimKey(claim.providerId, claim.id), gameId)
    if (!imported.has(gameId)) {
      imported.add(gameId)
      addGame({ id: gameId, title: claim.title, supportsRunnerSelection: false, source: { label: "browser", isLocal: true } })
    }
    set(job, { _tag: "Imported", gameId })
  }
  /** ADR 0013's queue: one job moves at a time; the rest wait, in order. */
  const moving = (job: InMemoryJob) => {
    const tag = job.status.state._tag
    return tag === "Resolving" || tag === "Downloading" || tag === "Placing"
  }
  const queueFree = (except: InMemoryJob) => ![...jobs.values()].some(other => other !== except && moving(other))
  const step = (job: InMemoryJob) => {
    const state = job.status.state
    if (state._tag === "Queued") {
      const first = [...jobs.values()].find(other => other.status.state._tag === "Queued")
      if (first === job && queueFree(job)) set(job, { _tag: "Resolving" })
    } else if (state._tag === "Resolving") {
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
      for (const claim of found) held.add(claimKey(claim.providerId, claim.id))
      return {
        _tag: "Ok",
        payload: {
          claims: found.map(({ description: _description, failure: _failure, unavailable: _a, contentLength: _c, ...claim }) =>
            ({ _tag: "ProviderClaim" as const, ...structuredClone(claim) })),
        },
      }
    },
    async acquisitionDetails({ providerId, id }) {
      const claim = claimOf(providerId, id)
      if (claim === undefined) return failure(ACQUISITION_CODE.unknownClaim, "The provider no longer lists this game.")
      held.add(claimKey(providerId, id))
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
    async acquisitionAcquire({ providerId, id }) {
      await config.acquireGate?.()
      const claim = claimOf(providerId, id)
      const key = claimKey(providerId, id)
      // An owned cart answers Imported before korrid needs the claim in memory.
      const owned = inLibrary.get(key)
      if (owned !== undefined) {
        const jobId = `job-${++jobSeq}`
        const status: AcquireJobStatus = { jobId, providerId, id, state: { _tag: "Imported", gameId: owned } }
        return { _tag: "Ok", payload: status }
      }
      if (claim === undefined || !held.has(key)) return failure(ACQUISITION_CODE.unknownClaim, "Korri no longer holds this game. Search again.")
      const existing = byClaim.get(key)
      const running = existing === undefined ? undefined : jobs.get(existing)
      // A repeated acquire for a claim whose newest job is live or Imported
      // returns that job; after Failed or Cancelled it is a new try.
      const tag = running?.status.state._tag
      if (running !== undefined && (tag === "Queued" || tag === "Resolving" || tag === "Downloading" || tag === "Placing" || tag === "Imported")) {
        return snapshot(running)
      }
      const system = claim.artifact?.system
      if (system === undefined || config.unknownSystems?.includes(system) === true) {
        return failure(ACQUISITION_CODE.systemUnknown, "This game is for a system this device cannot play, so it can't be downloaded here.")
      }
      const jobId = `job-${++jobSeq}`
      const job: InMemoryJob = {
        claim,
        partial: 0,
        resolveLeft: config.resolveSteps ?? 1,
        placeLeft: config.placeSteps ?? 1,
        status: {
          jobId, providerId, id, system,
          ...(claim.artifact?.file?.name === undefined ? {} : { fileName: claim.artifact.file.name }),
          state: { _tag: "Resolving" },
        },
      }
      // Another download is moving: this one waits its turn.
      if (!queueFree(job)) set(job, { _tag: "Queued" })
      jobs.set(jobId, job)
      byClaim.set(key, jobId)
      // The provider will not hand out the file.
      if (claim.unavailable !== undefined) {
        set(job, { _tag: "Failed", reason: { code: ACQUISITION_CODE.resolutionFailed, message: claim.unavailable } })
        return snapshot(job)
      }
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
      if (tag === "Queued" || tag === "Resolving" || tag === "Downloading") set(job, { _tag: "Cancelled" })
      return snapshot(job)
    },
    restart() {
      jobs = new Map()
      byClaim = new Map()
      held = new Set()
    },
    downloadedBytes: () => downloaded,
    bindCatalog(next) {
      addGame = next
    },
  }
}
