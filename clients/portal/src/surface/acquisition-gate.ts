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
 *
 * A download the person asked for from the game (ADR 0013) may already be
 * queued or running when the gate opens. The gate moves it to the front,
 * because a launch waits for it, but never cancels it: leaving the gate stops
 * only what the gate itself started. Which games still need what, outside a
 * launch, is game-downloads.ts.
 */
import type { RpcFailure } from "@contracts/generated/korrid"
import type { SurfaceAction, SurfaceStatus } from "@contracts/surface/korri-surface"
import type { ArtifactStatus } from "../korrid/acquisition-wire"
import type { KorridClient } from "../korrid/client"

/** The cadence the host already observes app.session.status at: 1 Hz. */
export const ACQUISITION_POLL_INTERVAL_MS = 1000

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
  /** Artifacts already queued or under way when the gate first saw them,
   * and not started by it: someone else's to stop, never the gate's. */
  readonly held: Set<string>
  /** Artifacts this gate asked korrid to start. Only these does it cancel. */
  readonly owned: Set<string>
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
  dispose(): void
}

export interface AcquisitionGateOptions {
  readonly pollIntervalMs?: number
  /** The gate closed after it changed what is on disk or under way: the
   * host re-reads what each game still needs. */
  readonly onSettled?: () => void
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

/** Queued, downloading or unpacking: korrid has accepted the work. */
export const isActive = (artifact: ArtifactStatus) =>
  artifact.state._tag === "Queued" || artifact.state._tag === "Downloading" || artifact.state._tag === "Unpacking"

/** What Cancel can stop: a download under way, or one waiting its turn. */
const isStoppable = (artifact: ArtifactStatus) =>
  artifact.state._tag === "Downloading" || artifact.state._tag === "Queued"

/** What this gate started and Cancel can still stop. Never what it found under way. */
const stoppable = (launch: GatedLaunch, artifacts: readonly ArtifactStatus[]) =>
  artifacts.filter(artifact => isStoppable(artifact) && launch.owned.has(artifact.id) && !launch.held.has(artifact.id))

/** korrid deletes a cancelled download's partial file (ADR 0013). */
const CANCELLING_DETAIL = "Deleting the partial download"

export function createAcquisitionGate(
  korrid: Pick<KorridClient, "acquisitionStatus" | "acquisitionStart" | "acquisitionCancel">,
  options: AcquisitionGateOptions = {},
): AcquisitionGate {
  const pollIntervalMs = options.pollIntervalMs ?? ACQUISITION_POLL_INTERVAL_MS
  const settled = () => {
    if (!disposed) options.onSettled?.()
  }
  let phase: Phase = { _tag: "Idle" }
  let status: SurfaceStatus | undefined
  let generation = 0
  let disposed = false
  const listeners = new Set<() => void>()
  const notify = () => {
    for (const listener of listeners) listener()
  }
  const current = (operation: number) => !disposed && operation === generation
  const currentPhase = (): Phase => phase
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
        // Cancel stops what this gate started. Work it only joined goes on
        // without the launch, so leaving says that instead.
        return next.cancelRequested
          ? { _tag: "Busy", kicker: "Cancelling download…", detail: CANCELLING_DETAIL, gameId: next.launch.game.surfaceId }
          : acquiringStatus(next.artifacts, next.launch.game.surfaceId,
            action("cancel", next.starting || stoppable(next.launch, next.artifacts).length > 0 ? "Cancel" : "Play later"))
      case "Cancelling":
        return { _tag: "Busy", kicker: "Cancelling download…", detail: CANCELLING_DETAIL, gameId: next.launch.game.surfaceId }
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
    settled()
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
    for (const id of ids) if (!owner.launch.held.has(id)) owner.launch.owned.add(id)
    publish({ _tag: "Acquiring", launch: owner.launch, artifacts: owner.artifacts, starting: true, cancelRequested: false })
    const result = await korrid.acquisitionStart(ids)
    if (!current(operation) || phase._tag !== "Acquiring") return
    // A Cancel pressed meanwhile waited for this answer: what korrid now
    // reports as under way is what there is to stop.
    const acquiring = phase
    if (result._tag === "Err") {
      if (acquiring.cancelRequested) {
        abandon(acquiring.launch)
        settled()
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
    // What korrid already has under way (a download the person asked for, or
    // a restart mid-way) is someone else's to stop: remember it, so leaving
    // never cancels it.
    for (const artifact of artifacts) {
      if (isActive(artifact) && !launch.owned.has(artifact.id)) launch.held.add(artifact.id)
    }
    // Observe it rather than ask again. What waits in the queue goes to the
    // front, because this launch waits for it; korrid's start is idempotent
    // and pauses the background download. What is left Missing is asked for
    // once that work is done.
    if (artifacts.some(isActive)) {
      const queued = artifacts.filter(artifact => artifact.state._tag === "Queued").map(artifact => artifact.id)
      publish({ _tag: "Acquiring", launch, artifacts, starting: false, cancelRequested: false })
      if (queued.length > 0) {
        const moved = await korrid.acquisitionStart(queued)
        // `phase` moved on while korrid answered; read it again.
        const joined = currentPhase()
        if (!current(operation) || joined._tag !== "Acquiring") return
        // Refused: the queue still gets there, so keep watching.
        if (moved._tag === "Ok") settle(joined, artifacts.map(artifact =>
          moved.payload.artifacts.find(update => update.id === artifact.id) ?? artifact))
        if (!current(operation)) return
      }
      void observe(operation)
      return
    }
    publish({ _tag: "Confirming", launch, artifacts })
  }

  /**
   * Stop what this gate started and korrid still has queued or downloading,
   * then close without launching. korrid deletes the partial file. Unpacking
   * is never cancelled, and neither is anything the gate found under way. A
   * refused cancel of a live download is a problem worth stating; nothing
   * else is.
   */
  const cancelDownloads = async (launch: GatedLaunch, artifacts: readonly ArtifactStatus[]) => {
    const operation = ++generation
    publish({ _tag: "Cancelling", launch })
    for (const artifact of stoppable(launch, artifacts)) {
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
    settled()
  }

  /**
   * Cancel while acquiring. With a start still pending, the intent is kept
   * and honoured when the start answers, so nothing it began runs on
   * unstopped; otherwise what it started is stopped at once. With nothing of
   * its own under way, the gate just closes and the download goes on.
   */
  const requestCancel = (owner: Extract<Phase, { _tag: "Acquiring" }>) => {
    if (owner.cancelRequested) return
    if (owner.starting) {
      publish({ ...owner, cancelRequested: true })
      return
    }
    if (stoppable(owner.launch, owner.artifacts).length === 0) {
      abandon(owner.launch)
      settled()
      return
    }
    void cancelDownloads(owner.launch, owner.artifacts)
  }

  /** Leave a Problem: stop what the gate started and is still under way, then close. */
  const leaveFailed = (owner: Extract<Phase, { _tag: "Failed" }>) => {
    if (stoppable(owner.launch, owner.artifacts).length > 0) {
      void cancelDownloads(owner.launch, owner.artifacts)
      return
    }
    abandon(owner.launch)
    settled()
  }

  const published = (id: string): SurfaceAction | undefined => {
    if (status?._tag !== "Confirm" && status?._tag !== "Busy") return undefined
    return (status.actions ?? []).find(candidate => candidate.id === id && candidate.enabled)
  }

  return {
    getStatus: () => status,
    subscribe: listener => {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    launch: (game, proceed, abandonLaunch = () => {}) => {
      if (disposed || phase._tag !== "Idle") return
      void check({ game, proceed, abandon: abandonLaunch, held: new Set(), owned: new Set() })
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
    dispose: () => {
      disposed = true
      generation++
      listeners.clear()
    },
  }
}
