/**
 * Getting a game from a provider: the providers, one search, one open entry,
 * and every download a person started (workflow contract sections 2 and 5).
 *
 * Host-owned, like the acquisition gate: no DOM, hardware or surface
 * dependency. It makes every RPC call, keeps each claim's job id, watches the
 * job at the host's 1 Hz cadence, and publishes one `SurfaceStore`. The
 * surface asks for providers, a search and an entry, and runs the actions
 * this publishes; nothing else crosses.
 *
 * korrid owns the job, the download and its staging file, and keeps jobs in
 * memory only. This module owns the question before a download, the
 * observation, and the words. A download keeps going while the person looks
 * at something else; it shows again on its entry.
 *
 * Every action id carries the version of what it answers, so a press that
 * lands after the state moved on finds nothing published and sends nothing.
 */
import type {
  SurfaceAction,
  SurfaceStore,
  SurfaceStoreDetails,
  SurfaceStoreEntry,
  SurfaceStoreGet,
  SurfaceStoreProviders,
  SurfaceStoreSearch,
} from "@contracts/surface/korri-surface"
import type { KorridClient } from "../korrid/client"
import {
  ACQUIRE_FAILURE,
  type AcquireJobState,
  type AcquireJobStatus,
  type AcquireSourceChoice,
  type AcquisitionProvider,
  type ProviderClaim,
  type ProviderClaimDetails,
} from "../korrid/store-acquisition-types"
import { ACQUISITION_POLL_INTERVAL_MS, formatBytes, formatEta } from "./acquisition-gate"

/** Unreachable reads tolerated in a row while watching a job: korrid
 * restarting answers nothing for a while, then NotFound. */
export const LOST_AFTER_POLLS = 30

export type StoreAcquisitionClient = Pick<
  KorridClient,
  | "acquisitionProviders"
  | "acquisitionSearch"
  | "acquisitionDetails"
  | "acquisitionAcquire"
  | "acquisitionAcquireStatus"
  | "acquisitionAcquireCancel"
>

export interface StoreAcquisitionOptions {
  readonly pollIntervalMs?: number
  /** A job reached Imported: the catalog should be read again. */
  readonly onImported?: (gameId: string) => void
}

export interface StoreAcquisition {
  /** `InLibrary.gameId` is korrid's game id here; the root maps it to the
   * surface's id once the catalog holds it. */
  getSnapshot(): SurfaceStore
  subscribe(listener: () => void): () => void
  openStore(): void
  search(providerId: string, query: string): void
  openEntry(entryId: string): void
  /** A `store:` action id. True when this module owns the id, whether or not
   * it is still published. */
  act(actionId: string): boolean
  dispose(): void
}

/** Where one claim's download stands, as the host keeps it. */
type Job =
  | { readonly _tag: "Confirming" }
  | { readonly _tag: "ChoosingSource"; readonly sources: readonly AcquireSourceChoice[] }
  /** acquire sent, not answered. A Cancel waits for the answer. */
  | { readonly _tag: "Starting"; readonly cancelRequested: boolean }
  | { readonly _tag: "Active"; readonly jobId: string; readonly state: AcquireJobState; readonly cancelling: boolean }
  | { readonly _tag: "Imported"; readonly gameId: string }
  | { readonly _tag: "Staged"; readonly message: string }
  | {
      readonly _tag: "Failed"
      readonly kicker: string
      readonly reason: string
      readonly retry: boolean
      readonly sourceId?: string
    }
  | { readonly _tag: "Unavailable"; readonly reason: string }
  | { readonly _tag: "Cancelled" }

interface JobSlot {
  readonly claim: ProviderClaim
  readonly job: Job
  /** Bumped on every change; action ids carry it. */
  readonly version: number
}

type Providers =
  | { readonly _tag: "Loading" }
  | { readonly _tag: "Problem"; readonly message: string; readonly version: number }
  | { readonly _tag: "Ready"; readonly providers: readonly AcquisitionProvider[] }

type Search =
  | { readonly _tag: "Idle" }
  | { readonly _tag: "Searching"; readonly providerId: string; readonly query: string }
  | {
      readonly _tag: "Problem"
      readonly providerId: string
      readonly query: string
      readonly message: string
      readonly version: number
    }
  | { readonly _tag: "Ready"; readonly providerId: string; readonly query: string; readonly claims: readonly ProviderClaim[] }

type Details =
  | { readonly _tag: "Closed" }
  | { readonly _tag: "Loading"; readonly claim: ProviderClaim }
  | { readonly _tag: "Problem"; readonly claim: ProviderClaim; readonly message: string; readonly version: number }
  | { readonly _tag: "Ready"; readonly details: ProviderClaimDetails }

/** The opaque id a surface hands back for one claim. */
export const storeEntryId = (claim: Pick<ProviderClaim, "providerId" | "id">): string =>
  JSON.stringify([claim.providerId, claim.id])

/** Legacy Shift's ungetable guard, in its own words. */
export const NO_SYSTEM_REASON =
  "This source doesn't say which console this game is for, so it can't be downloaded yet."
export const STOPPED_KICKER = "Download stopped"
export const STOPPED_REASON =
  "Korri restarted before the download finished. Nothing was kept, and your library is unchanged."
export const CANCELLED_NOTE = "Download cancelled. Nothing was kept."
export const STAGED_MESSAGE =
  "Downloaded to this device, but your library did not recognise it as a game."

const UNAVAILABLE_CODES: ReadonlySet<string> = new Set([
  ACQUIRE_FAILURE.noSource,
  ACQUIRE_FAILURE.unknownSystem,
  ACQUIRE_FAILURE.unavailable,
])

function entryFrom(claim: ProviderClaim): SurfaceStoreEntry {
  const system = claim.platform ?? claim.artifact?.system
  return {
    id: storeEntryId(claim),
    title: claim.title,
    ...(system === undefined ? {} : { system }),
    ...(claim.thumbnailUrl === undefined ? {} : { thumbnailUrl: claim.thumbnailUrl }),
  }
}

/** "612 KB of 1.7 MB · 48 KB/s · 0:23 left", with unknown parts left out. */
export function storeDownloadDetail(state: Extract<AcquireJobState, { _tag: "Downloading" }>): string {
  const parts = [
    state.bytesTotal === undefined
      ? `${formatBytes(state.bytesDone)} so far`
      : `${formatBytes(state.bytesDone)} of ${formatBytes(state.bytesTotal)}`,
  ]
  if (state.bytesPerSecond !== undefined && state.bytesPerSecond > 0) {
    parts.push(`${formatBytes(state.bytesPerSecond)}/s`)
    if (state.bytesTotal !== undefined) {
      parts.push(`${formatEta((state.bytesTotal - state.bytesDone) / state.bytesPerSecond)} left`)
    }
  }
  return parts.join(" · ")
}

/** A job snapshot as the host keeps it. */
function jobFrom(status: AcquireJobStatus, cancelling = false): Job {
  const state = status.state
  switch (state._tag) {
    case "Imported":
      return { _tag: "Imported", gameId: state.gameId }
    case "Staged":
      return { _tag: "Staged", message: state.message.trim() === "" ? STAGED_MESSAGE : state.message }
    case "Cancelled":
      return { _tag: "Cancelled" }
    case "Failed":
      return UNAVAILABLE_CODES.has(state.reason.code)
        ? { _tag: "Unavailable", reason: state.reason.message }
        : { _tag: "Failed", kicker: "Download failed", reason: state.reason.message, retry: true }
    case "Resolving":
    case "Downloading":
    case "Placing":
      return { _tag: "Active", jobId: status.jobId, state, cancelling }
  }
}

export function createStoreAcquisition(
  korrid: StoreAcquisitionClient,
  options: StoreAcquisitionOptions = {},
): StoreAcquisition {
  const pollIntervalMs = options.pollIntervalMs ?? ACQUISITION_POLL_INTERVAL_MS
  let disposed = false
  let version = 0
  const next = () => ++version
  let providers: Providers = { _tag: "Loading" }
  let providersSeq = 0
  let search: Search = { _tag: "Idle" }
  let searchSeq = 0
  let details: Details = { _tag: "Closed" }
  let detailsSeq = 0
  const jobs = new Map<string, JobSlot>()
  /** Which watch loop owns a claim's job; a newer one makes an older stale. */
  const watchers = new Map<string, number>()
  const listeners = new Set<() => void>()

  const action = (id: string, label: string): SurfaceAction => ({ id: `store:${id}`, label, enabled: true })
  const jobAction = (key: string, slot: JobSlot | undefined, kind: string, label: string) =>
    action(`${slot?.version ?? 0}:${kind}:${key}`, label)

  const providersView = (): SurfaceStoreProviders => {
    switch (providers._tag) {
      case "Loading":
        return { _tag: "Loading" }
      case "Problem":
        return { _tag: "Problem", message: providers.message, actions: [action(`${providers.version}:providers-retry`, "Retry")] }
      case "Ready":
        return { _tag: "Ready", providers: providers.providers.map(({ id, title }) => ({ id, title })) }
    }
  }

  const searchView = (): SurfaceStoreSearch => {
    switch (search._tag) {
      case "Idle":
        return { _tag: "Idle" }
      case "Searching":
        return { _tag: "Searching", providerId: search.providerId, query: search.query }
      case "Problem":
        return {
          _tag: "Problem", providerId: search.providerId, query: search.query, message: search.message,
          actions: [action(`${search.version}:search-retry`, "Retry")],
        }
      case "Ready":
        return { _tag: "Ready", providerId: search.providerId, query: search.query, entries: search.claims.map(entryFrom) }
    }
  }

  const getView = (claim: ProviderClaimDetails): SurfaceStoreGet => {
    const key = storeEntryId(claim)
    const slot = jobs.get(key)
    const cancel = () => jobAction(key, slot, "cancel", "Cancel")
    const job = slot?.job
    if (job === undefined) {
      if (claim.artifact?.system === undefined) return { _tag: "Unavailable", reason: NO_SYSTEM_REASON }
      return { _tag: "Offer", actions: [jobAction(key, slot, "get", "Get")] }
    }
    switch (job._tag) {
      case "Cancelled":
        return { _tag: "Offer", note: CANCELLED_NOTE, actions: [jobAction(key, slot, "get", "Get")] }
      case "Confirming": {
        const bytes = claim.artifact?.file?.sizeBytes
        return {
          _tag: "Confirm",
          kicker: "Download to this device",
          detail: `${claim.title}, ${bytes === undefined ? "size unknown" : formatBytes(bytes)}`,
          actions: [jobAction(key, slot, "download", "Download"), cancel()],
        }
      }
      case "ChoosingSource":
        return {
          _tag: "Confirm",
          kicker: "Choose a folder",
          detail: `More than one folder on this device holds ${claim.platform ?? claim.artifact?.system ?? "these"} games. Choose where this one goes.`,
          actions: [
            ...job.sources.map((source, index) => jobAction(key, slot, `source.${index}`, source.label)),
            cancel(),
          ],
        }
      case "Starting":
        return job.cancelRequested
          ? { _tag: "Working", kicker: "Cancelling download…", detail: "Nothing will be kept", actions: [] }
          : { _tag: "Working", kicker: "Starting download…", detail: "Asking the provider for the file", actions: [cancel()] }
      case "Active": {
        if (job.cancelling) return { _tag: "Working", kicker: "Cancelling download…", detail: "Nothing will be kept", actions: [] }
        const state = job.state
        if (state._tag === "Downloading") {
          return {
            _tag: "Working",
            kicker: `Downloading ${claim.title}`,
            detail: storeDownloadDetail(state),
            progress: { done: state.bytesDone, ...(state.bytesTotal === undefined ? {} : { total: state.bytesTotal }) },
            actions: [cancel()],
          }
        }
        // Placing has no position and no Cancel: the file is being moved in.
        if (state._tag === "Placing") return { _tag: "Working", kicker: "Adding to your library…", progress: { done: 0 }, actions: [] }
        return { _tag: "Working", kicker: "Starting download…", detail: "Asking the provider for the file", actions: [cancel()] }
      }
      case "Imported":
        return { _tag: "InLibrary", detail: "In your library", gameId: job.gameId }
      case "Staged":
        return { _tag: "Staged", message: job.message }
      case "Failed":
        return {
          _tag: "Problem", kicker: job.kicker, reason: job.reason,
          actions: job.retry ? [jobAction(key, slot, "retry", "Retry")] : [],
        }
      case "Unavailable":
        return { _tag: "Unavailable", reason: job.reason }
    }
  }

  const detailsView = (): SurfaceStoreDetails => {
    switch (details._tag) {
      case "Closed":
        return { _tag: "Closed" }
      case "Loading":
        return { _tag: "Loading", entry: entryFrom(details.claim) }
      case "Problem":
        return {
          _tag: "Problem", entry: entryFrom(details.claim), message: details.message,
          actions: [action(`${details.version}:details-retry`, "Retry")],
        }
      case "Ready":
        return {
          _tag: "Ready",
          entry: entryFrom(details.details),
          ...(details.details.description === undefined ? {} : { description: details.details.description }),
          get: getView(details.details),
        }
    }
  }

  const view = (): SurfaceStore => ({ providers: providersView(), search: searchView(), details: detailsView() })
  let snapshot = view()
  const publish = () => {
    if (disposed) return
    snapshot = view()
    for (const listener of listeners) listener()
  }

  const setJob = (key: string, claim: ProviderClaim, job: Job) => {
    jobs.set(key, { claim, job, version: next() })
    publish()
  }
  const settle = (key: string, claim: ProviderClaim, job: Job) => {
    watchers.delete(key)
    setJob(key, claim, job)
    if (job._tag === "Imported") options.onImported?.(job.gameId)
  }

  const watch = async (key: string, claim: ProviderClaim, jobId: string) => {
    const token = next()
    watchers.set(key, token)
    const mine = () => !disposed && watchers.get(key) === token
    let unreachable = 0
    while (mine()) {
      await new Promise(resolve => setTimeout(resolve, pollIntervalMs))
      if (!mine() || jobs.get(key)?.job._tag !== "Active") return
      const result = await korrid.acquisitionAcquireStatus(jobId)
      const now = jobs.get(key)?.job
      if (!mine() || now?._tag !== "Active") return
      if (result._tag === "Err") {
        // korrid restarted: the job is gone, and with it any partial file.
        if (result.payload.code === ACQUIRE_FAILURE.jobNotFound) {
          settle(key, claim, { _tag: "Failed", kicker: STOPPED_KICKER, reason: STOPPED_REASON, retry: true })
          return
        }
        if (result.payload.code === "BrainUnreachable" && ++unreachable < LOST_AFTER_POLLS) continue
        settle(key, claim, { _tag: "Failed", kicker: "Lost sight of the download", reason: result.payload.message, retry: true })
        return
      }
      unreachable = 0
      const job = jobFrom(result.payload, now.cancelling)
      if (job._tag !== "Active") {
        settle(key, claim, job)
        return
      }
      // Only a change is worth a new model.
      if (JSON.stringify(job.state) !== JSON.stringify(now.state)) setJob(key, claim, job)
    }
  }

  const cancelJob = async (key: string, claim: ProviderClaim, active: Extract<Job, { _tag: "Active" }>) => {
    watchers.delete(key)
    setJob(key, claim, { ...active, cancelling: true })
    const result = await korrid.acquisitionAcquireCancel(active.jobId)
    if (disposed || jobs.get(key)?.job._tag !== "Active") return
    if (result._tag === "Err") {
      if (result.payload.code === ACQUIRE_FAILURE.jobNotFound) {
        settle(key, claim, { _tag: "Cancelled" })
        return
      }
      settle(key, claim, { _tag: "Failed", kicker: "Couldn't cancel the download", reason: result.payload.message, retry: false })
      return
    }
    // korrid answers with the job as it stands. One that finished first keeps
    // its outcome; one still moving is cancelled from here on.
    const job = jobFrom(result.payload)
    settle(key, claim, job._tag === "Active" ? { _tag: "Cancelled" } : job)
  }

  const start = async (key: string, claim: ProviderClaim, sourceId?: string) => {
    setJob(key, claim, { _tag: "Starting", cancelRequested: false })
    const result = await korrid.acquisitionAcquire({
      providerId: claim.providerId,
      id: claim.id,
      ...(sourceId === undefined ? {} : { sourceId }),
    })
    const now = jobs.get(key)?.job
    if (disposed || now?._tag !== "Starting") return
    if (result._tag === "Err") {
      const failure = result.payload
      if (now.cancelRequested) setJob(key, claim, { _tag: "Cancelled" })
      else if (failure.code === ACQUIRE_FAILURE.severalSources && (failure.sources?.length ?? 0) > 1) {
        setJob(key, claim, { _tag: "ChoosingSource", sources: failure.sources ?? [] })
      } else if (UNAVAILABLE_CODES.has(failure.code)) setJob(key, claim, { _tag: "Unavailable", reason: failure.message })
      else {
        setJob(key, claim, {
          _tag: "Failed", kicker: "Couldn't start the download", reason: failure.message, retry: true,
          ...(sourceId === undefined ? {} : { sourceId }),
        })
      }
      return
    }
    const job = jobFrom(result.payload)
    if (job._tag !== "Active") {
      // Already in the library, or finished: korrid returned the claim's job.
      settle(key, claim, job)
      return
    }
    // A Cancel pressed while the acquire was unanswered: what it started is
    // what there is to stop.
    if (now.cancelRequested) {
      void cancelJob(key, claim, job)
      return
    }
    setJob(key, claim, job)
    void watch(key, claim, job.jobId)
  }

  const loadProviders = async () => {
    const seq = ++providersSeq
    providers = { _tag: "Loading" }
    publish()
    const result = await korrid.acquisitionProviders()
    if (disposed || seq !== providersSeq) return
    providers = result._tag === "Ok"
      ? { _tag: "Ready", providers: result.payload.providers }
      : { _tag: "Problem", message: result.payload.message, version: next() }
    publish()
  }

  const runSearch = async (providerId: string, query: string) => {
    const seq = ++searchSeq
    search = { _tag: "Searching", providerId, query }
    publish()
    const result = await korrid.acquisitionSearch({ query, providerIds: [providerId] })
    if (disposed || seq !== searchSeq) return
    search = result._tag === "Ok"
      ? { _tag: "Ready", providerId, query, claims: result.payload.claims }
      : { _tag: "Problem", providerId, query, message: result.payload.message, version: next() }
    publish()
  }

  const loadDetails = async (claim: ProviderClaim) => {
    const seq = ++detailsSeq
    details = { _tag: "Loading", claim }
    publish()
    const result = await korrid.acquisitionDetails({ providerId: claim.providerId, id: claim.id })
    if (disposed || seq !== detailsSeq) return
    if (result._tag === "Ok") {
      details = { _tag: "Ready", details: result.payload }
    } else if (UNAVAILABLE_CODES.has(result.payload.code)) {
      // The provider no longer offers it: say so where Get would be.
      details = { _tag: "Ready", details: claim }
      const key = storeEntryId(claim)
      if (jobs.get(key)?.job._tag !== "Active") {
        jobs.set(key, { claim, job: { _tag: "Unavailable", reason: result.payload.message }, version: next() })
      }
    } else {
      details = { _tag: "Problem", claim, message: result.payload.message, version: next() }
    }
    publish()
  }

  /** Every action id published now, enabled: the only authority a press has. */
  const published = (): ReadonlySet<string> => {
    const ids = new Set<string>()
    const add = (actions: readonly SurfaceAction[]) => {
      for (const candidate of actions) if (candidate.enabled) ids.add(candidate.id)
    }
    if (snapshot.providers._tag === "Problem") add(snapshot.providers.actions)
    if (snapshot.search._tag === "Problem") add(snapshot.search.actions)
    if (snapshot.details._tag === "Problem") add(snapshot.details.actions)
    if (snapshot.details._tag === "Ready") {
      const get = snapshot.details.get
      if (get._tag === "Offer" || get._tag === "Confirm" || get._tag === "Working" || get._tag === "Problem") add(get.actions)
    }
    return ids
  }

  const actOnJob = (kind: string, key: string) => {
    const open = details._tag === "Ready" ? details.details : undefined
    if (open === undefined || storeEntryId(open) !== key) return
    const job = jobs.get(key)?.job
    if (kind === "get") {
      if (job === undefined || job._tag === "Cancelled") setJob(key, open, { _tag: "Confirming" })
    } else if (kind === "download" && job?._tag === "Confirming") {
      void start(key, open)
    } else if (kind.startsWith("source.") && job?._tag === "ChoosingSource") {
      const source = job.sources[Number(kind.slice("source.".length))]
      if (source !== undefined) void start(key, open, source.id)
    } else if (kind === "retry" && job?._tag === "Failed" && job.retry) {
      void start(key, open, job.sourceId)
    } else if (kind === "cancel") {
      if (job?._tag === "Confirming" || job?._tag === "ChoosingSource") {
        jobs.delete(key)
        publish()
      } else if (job?._tag === "Starting" && !job.cancelRequested) {
        setJob(key, open, { ...job, cancelRequested: true })
      } else if (job?._tag === "Active" && !job.cancelling) {
        void cancelJob(key, open, job)
      }
    }
  }

  return {
    getSnapshot: () => snapshot,
    subscribe: listener => {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    openStore: () => {
      if (!disposed) void loadProviders()
    },
    search: (providerId, query) => {
      if (disposed) return
      // Only a provider Korri listed: an id from nowhere sends nothing.
      if (providers._tag !== "Ready" || !providers.providers.some(provider => provider.id === providerId)) return
      void runSearch(providerId, query)
    },
    openEntry: entryId => {
      if (disposed) return
      const claims = search._tag === "Ready" ? search.claims : []
      const claim = claims.find(candidate => storeEntryId(candidate) === entryId)
      if (claim !== undefined) void loadDetails(claim)
    },
    act: actionId => {
      if (!actionId.startsWith("store:")) return false
      if (disposed || !published().has(actionId)) return true
      const [, , kind = "", ...rest] = actionId.split(":")
      const key = rest.join(":")
      if (kind === "providers-retry") void loadProviders()
      else if (kind === "search-retry" && search._tag === "Problem") void runSearch(search.providerId, search.query)
      else if (kind === "details-retry" && details._tag === "Problem") void loadDetails(details.claim)
      else actOnJob(kind, key)
      return true
    },
    dispose: () => {
      disposed = true
      watchers.clear()
      listeners.clear()
    },
  }
}
