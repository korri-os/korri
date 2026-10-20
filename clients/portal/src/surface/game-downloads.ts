/**
 * What each local game still needs downloaded, and the downloads a person
 * asks for from the game rather than from a launch (ADR 0013).
 *
 * Host-owned, like the acquisition gate: no DOM, hardware or surface
 * dependency. korrid owns the queue, the bytes and the cache. This store
 * keeps two things it learns from korrid and folds them into one fact per
 * game:
 *
 *   - which artifacts each game needs, and which of those its release names
 *     (`release`), from game-scoped status reads;
 *   - where each artifact stands, from any read.
 *
 * The membership changes only when plugins do, so while anything downloads
 * the 1 Hz poll is one device-wide read, folded through what it already
 * knows. Game-scoped reads happen on a refresh (the set of games changed, a
 * launch's download finished) and only when something on the device is not
 * Ready, as before; a game whose files are all Ready is read once, when a
 * surface first asks what can be done with it.
 */
import type { RpcFailure } from "@contracts/generated/korrid"
import type { SurfaceStatus } from "@contracts/surface/korri-surface"
import type { ArtifactStatus } from "../korrid/acquisition-wire"
import type { KorridClient } from "../korrid/client"
import { ACQUISITION_POLL_INTERVAL_MS, isActive, totalBytes } from "./acquisition-gate"

/** Per-game status reads during a refresh run this many at once. */
const READ_CONCURRENCY = 8

/** One game's download, as korrid's artifacts add up. */
export interface GameDownloadFacts {
  /** Some artifact the game needs is not Ready. */
  readonly needs: boolean
  /** The declared size of everything not Ready; absent when one is unknown. */
  readonly bytes?: number
  /** Something is queued, downloading or unpacking for it. */
  readonly active: boolean
  /** Something of it is queued or downloading: what Cancel can stop. */
  readonly stoppable: boolean
  /** Active, and waiting behind another download. */
  readonly queued: boolean
  /** While it downloads, in the treaty's Busy.progress shape. */
  readonly progress?: { readonly done: number; readonly total?: number }
  /** Some artifact its release names is Ready: there is a download to remove. */
  readonly ownReady: boolean
  /** korrid's words for the last failed try, while nothing is under way. */
  readonly failure?: string
}

/** The game an action is about: korrid's id, the surface's id, and its title. */
export interface DownloadGameRef {
  readonly id: string
  readonly surfaceId: string
  readonly title: string
}

export interface GameDownloads {
  /** korrid game id -> facts, for every game whose artifacts are known. */
  getFacts(): ReadonlyMap<string, GameDownloadFacts>
  /** A Problem from a download action, until dismissed; else undefined. */
  getStatus(): SurfaceStatus | undefined
  subscribe(listener: () => void): () => void
  /** Re-read what the given games (or the last given) need. */
  refresh(gameIds?: readonly string[]): Promise<void>
  /** Read one game's artifacts if they are not known yet. Answers later. */
  learn(gameId: string): void
  /** An action for this game was sent and korrid has not answered. */
  isPending(gameId: string): boolean
  download(game: DownloadGameRef): void
  cancel(game: DownloadGameRef): void
  remove(game: DownloadGameRef): void
  /** The surface's Dismiss. True when a download problem was showing. */
  dismiss(): boolean
  dispose(): void
}

export interface GameDownloadsOptions {
  readonly pollIntervalMs?: number
}

interface Member {
  readonly id: string
  readonly release: boolean
}

const isReady = (artifact: ArtifactStatus) => artifact.state._tag === "Ready"
const isStoppable = (artifact: ArtifactStatus) =>
  artifact.state._tag === "Downloading" || artifact.state._tag === "Queued"

/** korrid's words, except where they are not words a person reads. */
function plainReason(failure: RpcFailure, game: DownloadGameRef): string {
  switch (failure.code) {
    case "ActiveSessionConflict":
      return `${game.title} is running. Stop it first, then remove the download.`
    case "BrainUnreachable":
      return "Korri did not answer. Try again in a moment."
    default:
      return failure.message
  }
}

export function createGameDownloads(
  korrid: Pick<KorridClient, "acquisitionStatus" | "acquisitionCancel" | "acquisitionDownload" | "acquisitionRemove">,
  options: GameDownloadsOptions = {},
): GameDownloads {
  const pollIntervalMs = options.pollIntervalMs ?? ACQUISITION_POLL_INTERVAL_MS
  const membership = new Map<string, readonly Member[]>()
  const states = new Map<string, ArtifactStatus>()
  /** What was not Ready when a game's download began: the bar's whole. */
  const batches = new Map<string, Set<string>>()
  const learning = new Set<string>()
  /** Games korrid would not resolve (no runner chosen): not asked again
   * until the next refresh. */
  const unresolved = new Set<string>()
  const pending = new Set<string>()
  const listeners = new Set<() => void>()
  let facts: ReadonlyMap<string, GameDownloadFacts> = new Map()
  let problem: SurfaceStatus | undefined
  let lastIds: readonly string[] = []
  /** Every game in `lastIds` has known membership. */
  let complete = false
  let refreshSeq = 0
  let refreshing = 0
  let timer: ReturnType<typeof setTimeout> | undefined
  let disposed = false

  const absorb = (artifacts: readonly ArtifactStatus[], gameId?: string) => {
    for (const artifact of artifacts) states.set(artifact.id, artifact)
    if (gameId !== undefined) {
      membership.set(gameId, artifacts.map(artifact => ({ id: artifact.id, release: artifact.release === true })))
    }
  }

  const factsFor = (gameId: string, members: readonly Member[]): GameDownloadFacts => {
    const artifacts = members.flatMap(member => {
      const status = states.get(member.id)
      return status === undefined ? [] : [status]
    })
    const missing = artifacts.filter(artifact => !isReady(artifact))
    const active = artifacts.some(isActive)
    let batch = batches.get(gameId)
    if (!active) {
      batches.delete(gameId)
      batch = undefined
    } else {
      batch ??= new Set()
      for (const artifact of missing) batch.add(artifact.id)
      batches.set(gameId, batch)
    }
    const downloading = artifacts.some(artifact => artifact.state._tag === "Downloading")
    const unpacking = artifacts.some(artifact => artifact.state._tag === "Unpacking")
    let progress: GameDownloadFacts["progress"]
    if (downloading && batch !== undefined) {
      // The whole is what was missing when this download began, so the bar
      // only moves forward as each artifact finishes.
      const whole = [...batch].flatMap(id => {
        const status = states.get(id)
        return status === undefined ? [] : [status]
      })
      const total = totalBytes(whole)
      const done = whole.reduce((sum, artifact) => {
        if (artifact.state._tag === "Downloading") return sum + artifact.state.bytesDone
        if (artifact.state._tag === "Ready" || artifact.state._tag === "Unpacking") return sum + (artifact.bytesTotal ?? 0)
        return sum
      }, 0)
      progress = total === undefined ? { done } : { done, total }
    } else if (unpacking) {
      // Unpacking has no position to show and no known end.
      progress = { done: 0 }
    }
    const failed = artifacts.find(artifact => artifact.state._tag === "Failed")
    const bytes = totalBytes(missing)
    return {
      needs: missing.length > 0,
      ...(bytes === undefined ? {} : { bytes }),
      active,
      stoppable: artifacts.some(isStoppable),
      queued: active && !downloading && !unpacking,
      ...(progress === undefined ? {} : { progress }),
      ownReady: members.some(member => member.release && states.get(member.id)?.state._tag === "Ready"),
      ...(failed?.state._tag === "Failed" && !active ? { failure: failed.state.reason.message } : {}),
    }
  }

  const publish = () => {
    if (disposed) return
    const next = new Map<string, GameDownloadFacts>()
    for (const [gameId, members] of membership) next.set(gameId, factsFor(gameId, members))
    facts = next
    for (const listener of listeners) listener()
  }

  const anyActive = () => [...states.values()].some(isActive)
  const schedulePoll = () => {
    if (disposed || timer !== undefined || !anyActive()) return
    timer = setTimeout(() => {
      timer = undefined
      void tick()
    }, pollIntervalMs)
  }
  /** One device-wide read, folded through the known membership. A game
   * whose membership was skipped while all was Ready is read in full. */
  const tick = async () => {
    if (disposed) return
    if (refreshing > 0) {
      schedulePoll()
      return
    }
    const all = await korrid.acquisitionStatus()
    if (disposed) return
    if (all._tag === "Ok") {
      absorb(all.payload.artifacts)
      if (!complete && !all.payload.artifacts.every(isReady)) {
        await refresh()
        return
      }
    }
    publish()
    schedulePoll()
  }

  async function refresh(gameIds?: readonly string[]): Promise<void> {
    if (disposed) return
    if (gameIds !== undefined) lastIds = gameIds
    const ids = lastIds
    const seq = ++refreshSeq
    unresolved.clear()
    refreshing++
    try {
      const all = await korrid.acquisitionStatus()
      if (disposed || seq !== refreshSeq) return
      if (all._tag !== "Ok") return
      absorb(all.payload.artifacts)
      // Only when something on the device is not Ready can any game need a
      // download, so the common steady state costs one read.
      if (!all.payload.artifacts.every(isReady) && ids.length > 0) {
        for (let at = 0; at < ids.length; at += READ_CONCURRENCY) {
          const slice = ids.slice(at, at + READ_CONCURRENCY)
          const results = await Promise.all(slice.map(id => korrid.acquisitionStatus(id)))
          if (disposed || seq !== refreshSeq) return
          results.forEach((result, index) => {
            if (result._tag === "Ok") absorb(result.payload.artifacts, slice[index]!)
          })
        }
      }
      complete = ids.every(id => membership.has(id))
      publish()
      schedulePoll()
    } finally {
      refreshing--
    }
  }

  const learn = (gameId: string) => {
    if (disposed || membership.has(gameId) || learning.has(gameId) || unresolved.has(gameId)) return
    learning.add(gameId)
    void korrid.acquisitionStatus(gameId).then(result => {
      learning.delete(gameId)
      if (disposed) return
      if (result._tag !== "Ok") {
        unresolved.add(gameId)
        return
      }
      absorb(result.payload.artifacts, gameId)
      publish()
      schedulePoll()
    })
  }

  /** Run one action against korrid for a game, at most one at a time. */
  const act = (game: DownloadGameRef, kicker: string, run: () => Promise<RpcFailure | undefined>) => {
    if (disposed || pending.has(game.id)) return
    pending.add(game.id)
    publish()
    void run().then(failure => {
      pending.delete(game.id)
      if (disposed) return
      if (failure !== undefined) {
        problem = {
          _tag: "Problem", kicker, reason: plainReason(failure, game), canRetry: false,
          gameId: game.surfaceId, gameTitle: game.title,
        }
      }
      publish()
      schedulePoll()
    })
  }

  return {
    getFacts: () => facts,
    getStatus: () => problem,
    subscribe: listener => {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    refresh,
    learn,
    isPending: gameId => pending.has(gameId),
    download: game => act(game, `Couldn't download ${game.title}`, async () => {
      const result = await korrid.acquisitionDownload(game.id)
      if (result._tag === "Err") return result.payload
      absorb(result.payload.artifacts, game.id)
      return undefined
    }),
    // Cancel stops everything of this game that is queued or downloading;
    // korrid deletes the partial files. Unpacking finishes on its own.
    cancel: game => act(game, "Couldn't cancel the download", async () => {
      const members = membership.get(game.id) ?? []
      for (const member of members) {
        const status = states.get(member.id)
        if (status === undefined || !isStoppable(status)) continue
        const result = await korrid.acquisitionCancel(member.id)
        if (result._tag === "Err") return result.payload
        absorb(result.payload.artifacts)
      }
      const after = await korrid.acquisitionStatus(game.id)
      if (after._tag === "Ok") absorb(after.payload.artifacts, game.id)
      return undefined
    }),
    remove: game => act(game, "Couldn't remove the download", async () => {
      const result = await korrid.acquisitionRemove(game.id)
      if (result._tag === "Err") return result.payload
      absorb(result.payload.artifacts, game.id)
      return undefined
    }),
    dismiss: () => {
      if (problem === undefined) return false
      problem = undefined
      publish()
      return true
    },
    dispose: () => {
      disposed = true
      if (timer !== undefined) clearTimeout(timer)
      timer = undefined
      listeners.clear()
    },
  }
}
