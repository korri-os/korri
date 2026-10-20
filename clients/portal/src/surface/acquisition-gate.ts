/**
 * The gate between "play" and the launch: what this game still needs
 * downloaded, and the download itself (ADR 0012).
 *
 * Host-owned, like the runner chooser: no DOM, hardware or surface
 * dependency. Before the shared reserve/start lifecycle begins, the gate asks
 * korrid which artifacts the game's runner needs. When every one is Ready it
 * steps aside at once; otherwise it publishes the treaty's Confirm, then Busy
 * with real progress while korrid downloads and unpacks, then hands over to
 * the launch it was given. Every status it publishes is a SurfaceStatus the
 * root puts in place of the launch status while the gate is open.
 *
 * korrid owns the download, the retries and the partial file. The gate owns
 * nothing but the question, the observation and the words.
 */
import type { ArtifactStatus, RpcFailure } from "@contracts/generated/korrid"
import type { SurfaceAction, SurfaceStatus } from "@contracts/surface/korri-surface"
import type { KorridClient } from "../korrid/client"

/** The cadence the host already observes app.session.status at: 1 Hz. */
export const ACQUISITION_POLL_INTERVAL_MS = 1000
/** Per-game status reads during a device-wide refresh run this many at once. */
const NEEDS_CONCURRENCY = 8

/** The game a launch is about: korrid's id, the surface's id, and its title. */
export interface AcquisitionGameRef {
  readonly id: string
  readonly surfaceId: string
  readonly title: string
  /** The runner whose artifacts to resolve, when the user chose one. */
  readonly runnerId?: string
}

/** One gated launch: what to do once every artifact is Ready, and what to do
 * if the gate closes without getting there. */
interface GatedLaunch {
  readonly game: AcquisitionGameRef
  readonly proceed: () => void
  readonly abandon: () => void
}

type Phase =
  | { readonly _tag: "Idle" }
  | { readonly _tag: "Checking"; readonly launch: GatedLaunch }
  | {
      readonly _tag: "Confirming"
      readonly launch: GatedLaunch
      readonly artifacts: readonly ArtifactStatus[]
    }
  | {
      readonly _tag: "Acquiring"
      readonly launch: GatedLaunch
      readonly artifacts: readonly ArtifactStatus[]
      /**
       * A start this gate sent has not answered yet. korrid makes a Downloading
       * state the moment it accepts work, so once any observation has come
       * back, a Missing artifact is not queued: it is not started.
       */
      readonly starting: boolean
      /** Cancel pressed while the start was still pending: honoured when it
       * answers, against what it reports as Downloading. */
      readonly cancelRequested: boolean
    }
  | { readonly _tag: "Cancelling"; readonly launch: GatedLaunch }
  | {
      readonly _tag: "Failed"
      readonly launch: GatedLaunch
      readonly artifacts: readonly ArtifactStatus[]
      readonly kicker: string
      readonly reason: string
      /** What Retry does: read the artifacts again, or ask korrid to start again. */
      readonly retry: "check" | "start" | "none"
    }

export interface AcquisitionGate {
  /** The status to show in place of the launch status; undefined while idle. */
  getStatus(): SurfaceStatus | undefined
  /** korrid game id -> bytes still to download (undefined when the size is
   * unknown), for every game whose runner artifacts are not all Ready. */
  getNeeds(): ReadonlyMap<string, number | undefined>
  subscribe(listener: () => void): () => void
  /**
   * Check, ask, download, then `proceed` into the launch. `abandon` runs
   * instead when the gate closes without getting there (Cancel, Dismiss,
   * Back). No-op while open.
   */
  launch(game: AcquisitionGameRef, proceed: () => void, abandon?: () => void): void
  /** A published `acquisition:` action id. True when the gate consumed it. */
  act(actionId: string): boolean
  /** The host's Back while the gate is open. True when the gate consumed it. */
  back(): boolean
  /** The surface's Retry. True when the gate consumed it. */
  retry(): boolean
  /** The surface's Dismiss. True when the gate consumed it. */
  dismiss(): boolean
  /** Re-read what the given games (or the last given) still need. */
  refreshNeeds(gameIds?: readonly string[]): Promise<void>
  dispose(): void
}

export interface AcquisitionGateOptions {
  readonly pollIntervalMs?: number
}

const isReady = (artifact: ArtifactStatus) => artifact.state._tag === "Ready"
const allReady = (artifacts: readonly ArtifactStatus[]) => artifacts.every(isReady)

/**
 * "1.5 GB", "612 MB", "6.4 MB", "48 KB": decimal units, the way ADR 0010
 * measured. One decimal below ten of a unit, where it still says something.
 */
export function formatBytes(bytes: number): string {
  const scale = (value: number, unit: string) => {
    const text = value < 10 ? value.toFixed(1) : String(Math.round(value))
    return `${text.endsWith(".0") ? text.slice(0, -2) : text} ${unit}`
  }
  if (bytes >= 1e9) return scale(bytes / 1e9, "GB")
  if (bytes >= 1e6) return scale(bytes / 1e6, "MB")
  if (bytes >= 1e3) return scale(bytes / 1e3, "KB")
  return `${Math.round(bytes)} B`
}

/** "0:45", "1:30", "1:02:03": what is left, as a clock reads it. */
export function formatEta(seconds: number): string {
  const whole = Math.max(0, Math.round(seconds))
  const hours = Math.floor(whole / 3600)
  const minutes = Math.floor((whole % 3600) / 60)
  const rest = whole % 60
  const two = (n: number) => String(n).padStart(2, "0")
  return hours > 0 ? `${hours}:${two(minutes)}:${two(rest)}` : `${minutes}:${two(rest)}`
}

/** "A", "A and B", "A, B and C". */
function listTitles(titles: readonly string[]): string {
  if (titles.length <= 1) return titles[0] ?? ""
  return `${titles.slice(0, -1).join(", ")} and ${titles[titles.length - 1]}`
}

/** The sum of every artifact's declared size, or undefined when one is unknown. */
export function totalBytes(artifacts: readonly ArtifactStatus[]): number | undefined {
  let sum = 0
  for (const artifact of artifacts) {
    if (artifact.bytesTotal === undefined) return undefined
    sum += artifact.bytesTotal
  }
  return sum
}

/** "FEX rootfs and Proton-CachyOS, 1.5 GB" or ", size unknown". */
export function confirmDetail(missing: readonly ArtifactStatus[]): string {
  const bytes = totalBytes(missing)
  return `${listTitles(missing.map(artifact => artifact.title))}, ${bytes === undefined ? "size unknown" : formatBytes(bytes)}`
}

/** "612 MB of 1.2 GB · 6.4 MB/s · 1:30 left", with unknown parts left out. */
export function downloadDetail(state: Extract<ArtifactStatus["state"], { _tag: "Downloading" }>): string {
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

/**
 * The Busy status for artifacts korrid is working on. Downloading outranks
 * unpacking, because it is the one with a position to show. Unpacking has a
 * position of nothing and no known end, which is what `progress` without a
 * `total` means in the treaty. With neither, korrid has accepted the start
 * and not yet begun.
 */
export function acquiringStatus(
  artifacts: readonly ArtifactStatus[],
  gameId: string,
  cancel: SurfaceAction,
): SurfaceStatus {
  const downloading = artifacts.find(artifact => artifact.state._tag === "Downloading")
  if (downloading && downloading.state._tag === "Downloading") {
    const { state } = downloading
    return {
      _tag: "Busy",
      kicker: `Downloading ${downloading.title}`,
      detail: downloadDetail(state),
      progress: { done: state.bytesDone, ...(state.bytesTotal === undefined ? {} : { total: state.bytesTotal }) },
      actions: [cancel],
      gameId,
    }
  }
  const unpacking = artifacts.find(artifact => artifact.state._tag === "Unpacking")
  if (unpacking) {
    return { _tag: "Busy", kicker: `Unpacking ${unpacking.title}`, progress: { done: 0 }, gameId }
  }
  return { _tag: "Busy", kicker: "Starting download…", detail: "Waiting for the first bytes", actions: [cancel], gameId }
}

const isActive = (artifact: ArtifactStatus) =>
  artifact.state._tag === "Downloading" || artifact.state._tag === "Unpacking"

export function createAcquisitionGate(
  korrid: Pick<KorridClient, "acquisitionStatus" | "acquisitionStart" | "acquisitionCancel">,
  options: AcquisitionGateOptions = {},
): AcquisitionGate {
  const pollIntervalMs = options.pollIntervalMs ?? ACQUISITION_POLL_INTERVAL_MS
  let phase: Phase = { _tag: "Idle" }
  let status: SurfaceStatus | undefined
  let needs: ReadonlyMap<string, number | undefined> = new Map()
  let lastNeedIds: readonly string[] = []
  let generation = 0
  let needsSeq = 0
  let disposed = false
  const listeners = new Set<() => void>()
  const notify = () => {
    for (const listener of listeners) listener()
  }
  const current = (operation: number) => !disposed && operation === generation
  const action = (id: string, label: string): SurfaceAction => ({
    id: `acquisition:${generation}:${id}`,
    label,
    enabled: true,
  })
  const read = (game: AcquisitionGameRef) => korrid.acquisitionStatus(game.id, game.runnerId)

  const statusFor = (next: Phase): SurfaceStatus | undefined => {
    switch (next._tag) {
      case "Idle":
        return undefined
      case "Checking":
        return { _tag: "Busy", kicker: `Starting ${next.launch.game.title}…`, detail: "Checking what it needs", gameId: next.launch.game.surfaceId }
      case "Confirming":
        return {
          _tag: "Confirm",
          kicker: "Needs a download",
          detail: confirmDetail(next.artifacts.filter(artifact => !isReady(artifact))),
          actions: [action("download", "Download"), action("cancel", "Cancel")],
          gameId: next.launch.game.surfaceId,
        }
      case "Acquiring":
        return next.cancelRequested
          ? { _tag: "Busy", kicker: "Cancelling download…", detail: "Keeping what was downloaded so far", gameId: next.launch.game.surfaceId }
          : acquiringStatus(next.artifacts, next.launch.game.surfaceId, action("cancel", "Cancel"))
      case "Cancelling":
        return { _tag: "Busy", kicker: "Cancelling download…", detail: "Keeping what was downloaded so far", gameId: next.launch.game.surfaceId }
      case "Failed":
        return {
          _tag: "Problem",
          kicker: next.kicker,
          reason: next.reason,
          canRetry: next.retry !== "none",
          gameId: next.launch.game.surfaceId,
          gameTitle: next.launch.game.title,
        }
    }
  }

  const publish = (next: Phase) => {
    if (disposed) return
    phase = next
    status = statusFor(next)
    notify()
  }
  /** Close without launching: the caller learns through `abandon`. */
  const abandon = (launch: GatedLaunch) => {
    generation++
    publish({ _tag: "Idle" })
    launch.abandon()
  }
  const failed = (
    base: Extract<Phase, { _tag: "Checking" | "Confirming" | "Acquiring" }>,
    kicker: string,
    failure: RpcFailure,
    retry: Extract<Phase, { _tag: "Failed" }>["retry"],
  ) => {
    generation++
    publish({
      _tag: "Failed",
      launch: base.launch,
      artifacts: base._tag === "Checking" ? [] : base.artifacts,
      kicker,
      reason: failure.message,
      retry,
    })
  }

  const finish = (launch: GatedLaunch) => {
    generation++
    publish({ _tag: "Idle" })
    void refreshNeeds()
    launch.proceed()
  }

  /**
   * Fold an observed artifact list into the acquiring phase. Everything
   * Ready: launch. A failure: say so. Nothing moving but Missing artifacts:
   * whether this gate found them so or korrid lost the work it had accepted
   * (a restart between polls), they still need consent and a size, so ask;
   * nothing restarts on its own. Otherwise keep watching.
   */
  const settle = (owner: Extract<Phase, { _tag: "Acquiring" }>, artifacts: readonly ArtifactStatus[]) => {
    if (allReady(artifacts)) {
      finish(owner.launch)
      return
    }
    const broken = artifacts.find(artifact => artifact.state._tag === "Failed")
    if (broken && broken.state._tag === "Failed") {
      failed({ ...owner, artifacts }, `Couldn't download ${broken.title}`, broken.state.reason, "start")
      return
    }
    if (!artifacts.some(isActive)) {
      generation++
      publish({ _tag: "Confirming", launch: owner.launch, artifacts })
      return
    }
    publish({ ...owner, artifacts, starting: false })
  }

  const observe = async (operation: number) => {
    while (current(operation) && phase._tag === "Acquiring") {
      await new Promise(resolve => setTimeout(resolve, pollIntervalMs))
      if (!current(operation) || phase._tag !== "Acquiring") return
      const owner = phase
      const result = await read(owner.launch.game)
      if (!current(operation) || phase._tag !== "Acquiring") return
      if (result._tag === "Err") {
        failed(owner, "Lost sight of the download", result.payload, "start")
        return
      }
      settle(owner, result.payload.artifacts)
    }
  }

  const start = async (owner: Extract<Phase, { _tag: "Confirming" | "Acquiring" | "Failed" }>) => {
    const operation = ++generation
    const ids = owner.artifacts.filter(artifact => !isReady(artifact)).map(artifact => artifact.id)
    publish({ _tag: "Acquiring", launch: owner.launch, artifacts: owner.artifacts, starting: true, cancelRequested: false })
    const result = await korrid.acquisitionStart(ids)
    if (!current(operation) || phase._tag !== "Acquiring") return
    // A Cancel pressed meanwhile waited for this answer: what korrid now
    // reports as Downloading is what there is to stop.
    const acquiring = phase
    if (result._tag === "Err") {
      if (acquiring.cancelRequested) {
        abandon(acquiring.launch)
        void refreshNeeds()
        return
      }
      failed(acquiring, "Couldn't start the download", result.payload, "start")
      return
    }
    if (acquiring.cancelRequested) {
      void cancelDownloads(acquiring.launch, result.payload.artifacts)
      return
    }
    settle(acquiring, result.payload.artifacts)
    void observe(operation)
  }

  const check = async (launch: GatedLaunch) => {
    const operation = ++generation
    const checking: Extract<Phase, { _tag: "Checking" }> = { _tag: "Checking", launch }
    publish(checking)
    const result = await read(launch.game)
    if (!current(operation) || phase._tag !== "Checking") return
    if (result._tag === "Err") {
      failed(checking, "Couldn't check for downloads", result.payload, "check")
      return
    }
    const artifacts = result.payload.artifacts
    if (allReady(artifacts)) {
      finish(launch)
      return
    }
    // korrid is already working on it (another launch, or a restart mid-way):
    // observe rather than ask again. What is left Missing is asked for once
    // that work is done.
    if (artifacts.some(isActive)) {
      publish({ _tag: "Acquiring", launch, artifacts, starting: false, cancelRequested: false })
      void observe(operation)
      return
    }
    publish({ _tag: "Confirming", launch, artifacts })
  }

  /**
   * Stop every live download korrid keeps the partial file of, then close
   * without launching. Unpacking is never cancelled. A refused cancel of a
   * live download is a problem worth stating; nothing else is.
   */
  const cancelDownloads = async (launch: GatedLaunch, artifacts: readonly ArtifactStatus[]) => {
    const operation = ++generation
    publish({ _tag: "Cancelling", launch })
    for (const artifact of artifacts) {
      if (artifact.state._tag !== "Downloading") continue
      const result = await korrid.acquisitionCancel(artifact.id)
      if (!current(operation)) return
      if (result._tag === "Err") {
        generation++
        publish({
          _tag: "Failed", launch, artifacts,
          kicker: "Couldn't cancel the download", reason: result.payload.message, retry: "none",
        })
        return
      }
    }
    abandon(launch)
    void refreshNeeds()
  }

  /**
   * Cancel while acquiring. With a start still pending, the intent is kept
   * and honoured when the start answers, so nothing it began runs on
   * unstopped; otherwise what is downloading now is stopped at once.
   */
  const requestCancel = (owner: Extract<Phase, { _tag: "Acquiring" }>) => {
    if (owner.cancelRequested) return
    if (owner.starting) {
      publish({ ...owner, cancelRequested: true })
      return
    }
    void cancelDownloads(owner.launch, owner.artifacts)
  }

  /** Leave a Problem: stop what is still downloading, then close. */
  const leaveFailed = (owner: Extract<Phase, { _tag: "Failed" }>) => {
    if (owner.artifacts.some(artifact => artifact.state._tag === "Downloading")) {
      void cancelDownloads(owner.launch, owner.artifacts)
      return
    }
    abandon(owner.launch)
    void refreshNeeds()
  }

  async function refreshNeeds(gameIds?: readonly string[]): Promise<void> {
    if (disposed) return
    if (gameIds !== undefined) lastNeedIds = gameIds
    const ids = lastNeedIds
    const seq = ++needsSeq
    const next = new Map<string, number | undefined>()
    const all = await korrid.acquisitionStatus()
    if (disposed || seq !== needsSeq) return
    // Only when something on the device is not Ready can any game need a
    // download, so the common steady state costs one read.
    if (all._tag === "Ok" && !allReady(all.payload.artifacts) && ids.length > 0) {
      for (let at = 0; at < ids.length; at += NEEDS_CONCURRENCY) {
        const slice = ids.slice(at, at + NEEDS_CONCURRENCY)
        const results = await Promise.all(slice.map(id => korrid.acquisitionStatus(id)))
        if (disposed || seq !== needsSeq) return
        results.forEach((result, index) => {
          if (result._tag !== "Ok") return
          const missing = result.payload.artifacts.filter(artifact => !isReady(artifact))
          if (missing.length > 0) next.set(slice[index]!, totalBytes(missing))
        })
      }
    }
    needs = next
    notify()
  }

  const published = (id: string): SurfaceAction | undefined => {
    if (status?._tag !== "Confirm" && status?._tag !== "Busy") return undefined
    return (status.actions ?? []).find(candidate => candidate.id === id && candidate.enabled)
  }

  return {
    getStatus: () => status,
    getNeeds: () => needs,
    subscribe: listener => {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    launch: (game, proceed, abandonLaunch = () => {}) => {
      if (disposed || phase._tag !== "Idle") return
      void check({ game, proceed, abandon: abandonLaunch })
    },
    act: actionId => {
      if (!actionId.startsWith("acquisition:")) return false
      // Only what is published now, enabled, is authority: a stale press after
      // the work moved on sends nothing.
      if (published(actionId) === undefined) return true
      const kind = actionId.slice(actionId.lastIndexOf(":") + 1)
      if (phase._tag === "Confirming") {
        if (kind === "download") void start(phase)
        else if (kind === "cancel") abandon(phase.launch)
      } else if (phase._tag === "Acquiring" && kind === "cancel") {
        requestCancel(phase)
      }
      return true
    },
    back: () => {
      switch (phase._tag) {
        case "Idle":
          return false
        case "Checking":
        case "Confirming":
          abandon(phase.launch)
          return true
        case "Failed":
          leaveFailed(phase)
          return true
        case "Acquiring": {
          // Back is Cancel only where Cancel is on offer; unpacking has none.
          const cancel = status?._tag === "Busy" ? status.actions?.find(candidate => candidate.enabled) : undefined
          if (cancel) requestCancel(phase)
          return true
        }
        case "Cancelling":
          return true
      }
    },
    retry: () => {
      if (phase._tag !== "Failed") return false
      const owner = phase
      if (owner.retry === "check") void check(owner.launch)
      else if (owner.retry === "start") void start(owner)
      return true
    },
    dismiss: () => {
      if (phase._tag !== "Failed") return false
      leaveFailed(phase)
      return true
    },
    refreshNeeds,
    dispose: () => {
      disposed = true
      generation++
      needsSeq++
      listeners.clear()
    },
  }
}
