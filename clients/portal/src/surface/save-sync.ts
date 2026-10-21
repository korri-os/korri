/**
 * Save sync on the host side (ADR 0014): each game's save mark, the save
 * history screen, the split choice, and the check before a launch.
 *
 * Host-owned, like the acquisition gate: no DOM, hardware or surface
 * dependency. korrid owns every rule (what a split is, what a snapshot holds,
 * what may launch). This store owns the questions, the waiting and the words.
 * Device keys become names here, from the peer list and from this device's
 * own key, which a local catalog game's source states.
 *
 * Before a launch the store asks korrid to check with the owner's other
 * devices, for at most 3 seconds, with a Skip that ends the wait in the
 * portal. A split asks for a choice; an in-use device, or korrid's release or
 * runner warning, asks Launch anyway or Cancel. When no device answered, the
 * game shows Not checked. korrid still refuses a launch over a split, so a
 * skipped or failed check cannot lose a save silently.
 */
import {
  SaveMark,
  type RpcFailure,
  type SaveCheck,
  type SaveHistory,
  type SaveHistoryEntry,
  type SaveInUseWarning,
} from "@contracts/generated/korrid"
import type {
  SurfaceAction,
  SurfaceGame,
  SurfaceSaveHistory,
  SurfaceSaveHistoryEntry,
  SurfaceStatus,
} from "@contracts/surface/korri-surface"
import type { KorridClient } from "../korrid/client"

/** ADR 0014: the pre-launch wait for other devices is at most 3 seconds. */
export const SAVE_CHECK_LIMIT_MS = 3000
/** Marks change when sessions end and when devices meet; re-read this often. */
export const SAVE_STATUS_POLL_MS = 15_000

/** Game action that opens a game's save history. */
export const SAVE_HISTORY_ACTION_ID = "save-history"
/** The Split mark's action: open the split choice without launching. */
export const SAVE_CHOOSE_ACTION_ID = "save-choose"

type SaveCheckWire = SaveCheck

/** The game a save question is about: korrid's id, the surface's id, its title. */
export interface SaveGameRef {
  readonly id: string
  readonly surfaceId: string
  readonly title: string
  readonly runnerId?: string
}

interface Flow {
  readonly game: SaveGameRef
  /** Absent for a choice opened from the split review: nothing to launch. */
  readonly proceed?: () => void
  readonly abandon: () => void
}

type Phase =
  | { readonly _tag: "Idle" }
  | { readonly _tag: "Checking"; readonly flow: Flow }
  | { readonly _tag: "Reading"; readonly flow: Flow; readonly check?: SaveCheckWire; readonly offline: boolean }
  | {
      readonly _tag: "Choosing"
      readonly flow: Flow
      readonly sides: readonly SaveHistoryEntry[]
      readonly check?: SaveCheckWire
      readonly offline: boolean
    }
  | { readonly _tag: "Picking"; readonly flow: Flow; readonly check?: SaveCheckWire }
  | { readonly _tag: "Confirming"; readonly flow: Flow; readonly check: SaveCheckWire }
  | {
      readonly _tag: "Failed"
      readonly flow: Flow
      readonly kicker: string
      readonly reason: string
      /** Absent when trying again cannot help (korrid refused the start). */
      readonly retry?: () => void
    }

type HistoryPhase =
  | { readonly _tag: "Closed" }
  | { readonly _tag: "Loading"; readonly game: SaveGameRef }
  | { readonly _tag: "Error"; readonly game: SaveGameRef; readonly message: string }
  | { readonly _tag: "Ready"; readonly game: SaveGameRef; readonly history: SaveHistory }

export interface SaveMarkFact {
  readonly mark: SaveMark
  readonly heads: readonly string[]
}

export interface SaveSync {
  /** korrid game id -> its save mark, for every game korrid stated one. */
  getMarks(): ReadonlyMap<string, SaveMarkFact>
  /** The status to show in place of the launch status; undefined while idle. */
  getStatus(): SurfaceStatus | undefined
  /** The open save history, in the treaty's shape; undefined while closed. */
  getHistory(): SurfaceSaveHistory | undefined
  /** korrid's id of the game the open question is about. */
  getGameId(): string | undefined
  /** korrid's own status read stated a synced mark for this game, so it
   * keeps a save history for it. A check this device made does not count. */
  hasHistory(gameId: string): boolean
  subscribe(listener: () => void): () => void
  /** Re-read every mark and the peer names. */
  refresh(): Promise<void>
  /** Check, ask, then `proceed` into the launch; `abandon` when it does not. */
  launch(game: SaveGameRef, proceed: () => void, abandon?: () => void): void
  /** The split choice for one game, from the review: no launch follows. */
  choose(game: SaveGameRef): void
  /** korrid refused a launch over a split (SavesSplit): ask, then `proceed`. */
  resolveSplit(game: SaveGameRef, proceed: () => void, abandon?: () => void): void
  /** korrid refused a start for a reason the person must fix first
   * (DeviceNeedsOwner): say so in plain words; Dismiss or Back closes. */
  refuseStart(game: SaveGameRef, failure: RpcFailure, abandon?: () => void): void
  openHistory(game: SaveGameRef): void
  /** A published `saves:` action id. True when consumed. */
  act(actionId: string): boolean
  /** The host's Back while a question is open. True when consumed. */
  back(): boolean
  retry(): boolean
  dismiss(): boolean
  dispose(): void
}

export interface SaveSyncOptions {
  readonly checkLimitMs?: number
  readonly pollIntervalMs?: number
  /** Device keys that are this device (a local catalog game's source key). */
  readonly localDeviceKeys?: () => ReadonlySet<string>
  /** Marks changed after a choice or a restore. */
  readonly onChanged?: () => void
}

/** The treaty's save mark for one game, with the host's choose action on a split. */
export function surfaceSavesFrom(fact: SaveMarkFact | undefined): SurfaceGame["saves"] {
  if (fact === undefined) return undefined
  if (fact.mark === SaveMark.Split) {
    return { _tag: "Split", choose: { id: SAVE_CHOOSE_ACTION_ID, label: "Choose a save", enabled: true } }
  }
  return { _tag: fact.mark }
}

/** History is meaningful only where Korri keeps snapshots. */
export function offersSaveHistory(fact: SaveMarkFact | undefined): boolean {
  return fact !== undefined && fact.mark !== SaveMark.NotSynced && fact.mark !== SaveMark.SyncedBySteam
}

/** Plain words for a save failure. No code reaches the person. */
export function saveFailureReason(failure: RpcFailure): string {
  switch (failure.code) {
    case "SavesSplit":
      return "This game's saves are split between two devices. Pick one save first."
    case "SaveContentUnavailable":
      return "This save's files are not on this device. Try again when the device that made it is in reach."
    case "SavePayloadUnavailable":
      return "That older save's files are not on any device in reach. Try again when the device that made it is on."
    case "ActiveSessionConflict":
      return "A game is running. Stop it, then try again."
    case "DeviceNeedsOwner":
      return "This device needs an owner before it can start a game."
    case "SavesUnavailable":
      return "Korri could not read or update the saves on this device."
    case "BrainUnreachable":
      return "Korri did not answer. Try again in a moment."
    case "PermissionDenied":
      return "This portal cannot change saves."
    case "GameNotFound":
      return "Korri has no saves for this game."
    default:
      return "Korri could not finish that. Try again in a moment."
  }
}

/** "4:02 PM" today, else "Oct 12, 4:02 PM". */
export function formatSince(since: number, now: Date = new Date()): string {
  const date = new Date(since)
  const sameDay = date.toDateString() === now.toDateString()
  return sameDay
    ? date.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })
    : date.toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" })
}

/**
 * Newest first by ancestry: a snapshot always comes before the snapshots it
 * started from. Among snapshots ancestry does not order (the two sides of a
 * split), the later end time comes first; that is display order only.
 */
export function newestFirst(records: readonly SaveHistoryEntry[]): readonly SaveHistoryEntry[] {
  const byId = new Map(records.map(entry => [entry.snapshotId, entry]))
  const children = new Map<string, number>()
  for (const entry of records) {
    for (const parent of entry.record.parents) {
      if (byId.has(parent)) children.set(parent, (children.get(parent) ?? 0) + 1)
    }
  }
  const time = (entry: SaveHistoryEntry) => {
    const parsed = Date.parse(entry.record.endedAt)
    return Number.isFinite(parsed) ? parsed : 0
  }
  const ready = records.filter(entry => (children.get(entry.snapshotId) ?? 0) === 0)
  const out: SaveHistoryEntry[] = []
  const seen = new Set<string>()
  while (ready.length > 0) {
    ready.sort((a, b) => time(b) - time(a))
    const next = ready.shift()!
    if (seen.has(next.snapshotId)) continue
    seen.add(next.snapshotId)
    out.push(next)
    for (const parent of next.record.parents) {
      const left = (children.get(parent) ?? 0) - 1
      children.set(parent, left)
      const entry = byId.get(parent)
      if (left === 0 && entry !== undefined) ready.push(entry)
    }
  }
  // A cycle would be korrid's defect; never drop a snapshot because of it.
  for (const entry of records) if (!seen.has(entry.snapshotId)) out.push(entry)
  return out
}

export function createSaveSync(
  korrid: Pick<KorridClient, "savesStatus" | "savesHistory" | "savesChoose" | "savesRestore" | "savesCheck" | "peerList">,
  options: SaveSyncOptions = {},
): SaveSync {
  const checkLimitMs = options.checkLimitMs ?? SAVE_CHECK_LIMIT_MS
  const pollIntervalMs = options.pollIntervalMs ?? SAVE_STATUS_POLL_MS
  const localKeys = options.localDeviceKeys ?? (() => new Set<string>())
  let phase: Phase = { _tag: "Idle" }
  let historyPhase: HistoryPhase = { _tag: "Closed" }
  let status: SurfaceStatus | undefined
  let historyView: SurfaceSaveHistory | undefined
  let marks: ReadonlyMap<string, SaveMarkFact> = new Map()
  /** What the latest check found, until the next check of that game. */
  /** The mark this device's last check found, and the heads korrid stated
   * when it ran. New heads mean new snapshots: the check no longer speaks. */
  const checked = new Map<string, { readonly mark: SaveMark; readonly heads: readonly string[] }>()
  let peers: ReadonlyMap<string, string> = new Map()
  let generation = 0
  let historyGeneration = 0
  let disposed = false
  let timer: ReturnType<typeof setTimeout> | undefined
  const listeners = new Set<() => void>()
  const notify = () => {
    for (const listener of listeners) listener()
  }

  const deviceName = (key: string) =>
    localKeys().has(key) ? "This device" : peers.get(key) ?? "Another device"
  const action = (id: string, label: string, extra: Partial<SurfaceAction> = {}): SurfaceAction => ({
    id: `saves:${generation}:${id}`,
    label,
    enabled: true,
    ...extra,
  })
  const historyAction = (id: string, label: string, extra: Partial<SurfaceAction> = {}): SurfaceAction => ({
    id: `saves:h${historyGeneration}:${id}`,
    label,
    enabled: true,
    ...extra,
  })

  const confirmCopy = (check: SaveCheckWire): { kicker: string; detail?: string } => {
    const [first, ...more] = check.inUse
    const warnings = [...check.warnings]
    if (first !== undefined) {
      const others = more.map(use => `Also in use on ${deviceName(use.device)} since ${formatSince(use.since)}.`)
      return {
        kicker: `In use on ${deviceName(first.device)} since ${formatSince(first.since)}`,
        detail: ["If you play here too, Korri asks which save to keep afterwards.", ...others, ...warnings].join(" "),
      }
    }
    return { kicker: "Saves may not load", detail: warnings.join(" ") }
  }

  const statusFor = (next: Phase): SurfaceStatus | undefined => {
    const gameId = next._tag === "Idle" ? "" : next.flow.game.surfaceId
    switch (next._tag) {
      case "Idle":
        return undefined
      case "Checking":
        return { _tag: "Busy", kicker: "Checking other devices…", gameId, actions: [action("skip", "Skip")] }
      case "Reading":
        return { _tag: "Busy", kicker: "Reading both saves…", gameId }
      case "Picking":
        return { _tag: "Busy", kicker: "Continuing from that save…", gameId }
      case "Choosing":
        return {
          _tag: "ChooseSave",
          gameId,
          kicker: "Saves split between two devices",
          ...(next.offline
            ? { detail: "No other device answered. Your pick syncs to them when they are next in reach." }
            : {}),
          // Exactly one action besides the sides, so Back always has a meaning.
          actions: [action("not-now", "Not now")],
          sides: next.sides.map(entry => ({
            actionId: action(`pick:${entry.snapshotId}`, "").id,
            device: deviceName(entry.record.device),
            endedAt: Date.parse(entry.record.endedAt),
            durationSeconds: entry.record.durationSeconds,
          })),
        }
      case "Confirming": {
        const copy = confirmCopy(next.check)
        return {
          _tag: "Confirm",
          kicker: copy.kicker,
          ...(copy.detail === undefined || copy.detail === "" ? {} : { detail: copy.detail }),
          actions: [action("launch-anyway", "Launch anyway"), action("cancel", "Cancel")],
          gameId,
        }
      }
      case "Failed":
        return {
          _tag: "Problem",
          kicker: next.kicker,
          reason: next.reason,
          canRetry: next.retry !== undefined,
          gameId,
          gameTitle: next.flow.game.title,
        }
    }
  }

  const historyFor = (next: HistoryPhase): SurfaceSaveHistory | undefined => {
    if (next._tag === "Closed") return undefined
    // Exactly one host action, so Back always closes.
    const actions = [historyAction("close", "Close")]
    const gameId = next.game.surfaceId
    switch (next._tag) {
      case "Loading":
        return { _tag: "Loading", gameId, actions }
      case "Error":
        return { _tag: "Error", gameId, message: next.message, actions }
      case "Ready":
        return {
          _tag: "Ready",
          gameId,
          actions,
          entries: newestFirst(next.history.records).map((entry): SurfaceSaveHistoryEntry => {
            const current = entry.snapshotId === next.history.current
            return {
              snapshotId: entry.snapshotId,
              device: deviceName(entry.record.device),
              endedAt: Date.parse(entry.record.endedAt),
              durationSeconds: entry.record.durationSeconds,
              interrupted: entry.record.interrupted,
              current,
              ...(current ? {} : {
                restore: historyAction(`restore:${entry.snapshotId}`, "Restore", {
                  // A restore replaces what the game plays from, so every
                  // surface asks first. The replaced save stays in history.
                  destructive: true,
                  description: "Makes a new save from this one. The save you have now stays in the history.",
                }),
              }),
            }
          }),
        }
    }
  }

  const publish = (next: Phase) => {
    if (disposed) return
    phase = next
    status = statusFor(next)
    notify()
  }
  const publishHistory = (next: HistoryPhase) => {
    if (disposed) return
    historyPhase = next
    historyView = historyFor(next)
    notify()
  }
  /** Re-render names after the peer list moved, without changing a phase. */
  const rename = () => {
    status = statusFor(phase)
    historyView = historyFor(historyPhase)
  }

  const setMarks = (next: ReadonlyMap<string, SaveMarkFact>) => {
    marks = next
    notify()
  }
  const sameHeads = (a: readonly string[], b: readonly string[]) =>
    a.length === b.length && [...a].sort().every((head, index) => head === [...b].sort()[index])
  /**
   * korrid's marks, with what this device's last check found laid over them.
   * A Split korrid states always wins, and a check made against other heads
   * is dropped: korrid has seen snapshots the check did not.
   */
  const merged = (base: ReadonlyMap<string, SaveMarkFact>) => {
    const out = new Map(base)
    for (const [gameId, check] of [...checked]) {
      const fact = out.get(gameId)
      if (fact !== undefined && (fact.mark === SaveMark.Split || !sameHeads(fact.heads, check.heads))) {
        checked.delete(gameId)
        continue
      }
      out.set(gameId, { mark: check.mark, heads: fact?.heads ?? check.heads })
    }
    return out
  }
  let base: ReadonlyMap<string, SaveMarkFact> = new Map()

  const refresh = async () => {
    const [statusResult, peerResult] = await Promise.all([korrid.savesStatus(), korrid.peerList()])
    if (disposed) return
    if (peerResult._tag === "Ok") {
      peers = new Map(peerResult.payload.peers.map(peer => [peer.devicePublicKey, peer.label]))
      rename()
    }
    // A failed read states nothing: the games show no mark rather than a guess.
    base = statusResult._tag === "Ok"
      ? new Map(statusResult.payload.games.map(game => [game.gameId, { mark: game.status, heads: game.heads }]))
      : new Map()
    setMarks(merged(base))
  }
  const schedule = () => {
    if (disposed || pollIntervalMs <= 0) return
    timer = setTimeout(() => {
      void refresh().finally(schedule)
    }, pollIntervalMs)
  }
  const changed = () => {
    void refresh()
    options.onChanged?.()
  }

  const current = (operation: number) => !disposed && operation === generation
  const close = (flow: Flow) => {
    generation++
    publish({ _tag: "Idle" })
    flow.abandon()
  }
  const finish = (flow: Flow) => {
    generation++
    publish({ _tag: "Idle" })
    flow.proceed?.()
  }
  const noteChecked = (gameId: string, mark: SaveMark) => {
    checked.set(gameId, { mark, heads: base.get(gameId)?.heads ?? [] })
    setMarks(merged(base))
  }

  /** After the split (or none): ask about in-use and warnings, then go. */
  const afterSplit = (flow: Flow, check: SaveCheckWire | undefined) => {
    if (flow.proceed !== undefined && check !== undefined &&
      (check.inUse.length > 0 || check.warnings.length > 0)) {
      generation++
      publish({ _tag: "Confirming", flow, check })
      return
    }
    if (flow.proceed === undefined) {
      close(flow)
      return
    }
    finish(flow)
  }

  /** Read the history, find both sides by the split's heads, and ask. */
  const readSides = async (flow: Flow, check: SaveCheckWire | undefined, offline: boolean) => {
    const operation = ++generation
    publish({ _tag: "Reading", flow, ...(check === undefined ? {} : { check }), offline })
    const result = await korrid.savesHistory(flow.game.id)
    if (!current(operation)) return
    if (result._tag === "Err") {
      failed(flow, "Couldn't read the saves", saveFailureReason(result.payload), () => void readSides(flow, check, offline))
      return
    }
    const heads = marks.get(flow.game.id)?.heads ?? []
    const records = newestFirst(result.payload.records)
    const sides = heads.length > 1
      ? records.filter(entry => heads.includes(entry.snapshotId))
      : []
    if (sides.length < 2) {
      // korrid said split but the sides are not in its history: say so, and
      // let korrid's own launch check decide.
      failed(flow, "Couldn't read both saves", "Korri found a split but not both of its saves. Try again.",
        () => { void refresh().then(() => readSides(flow, check, offline)) })
      return
    }
    generation++
    publish({ _tag: "Choosing", flow, sides, ...(check === undefined ? {} : { check }), offline })
  }

  const failed = (flow: Flow, kicker: string, reason: string, retry?: () => void) => {
    generation++
    publish({ _tag: "Failed", flow, kicker, reason, ...(retry === undefined ? {} : { retry }) })
  }

  const check = async (flow: Flow) => {
    const operation = ++generation
    publish({ _tag: "Checking", flow })
    let timeout: ReturnType<typeof setTimeout> | undefined
    const limit = new Promise<"timeout">(resolve => {
      timeout = setTimeout(() => resolve("timeout"), checkLimitMs)
    })
    const answer = await Promise.race([
      korrid.savesCheck(flow.game.id, flow.game.runnerId, checkLimitMs + 2000),
      limit,
    ])
    clearTimeout(timeout)
    if (!current(operation) || phase._tag !== "Checking") return
    if (answer === "timeout" || answer._tag === "Err") {
      unanswered(flow)
      return
    }
    const result = answer.payload as SaveCheckWire
    noteChecked(flow.game.id, result.status)
    if (result.status === SaveMark.Split) {
      await refresh()
      if (!current(operation)) return
      void readSides(flow, result, false)
      return
    }
    afterSplit(flow, result)
  }

  /** No device answered, the check failed, or the person skipped it. */
  const unanswered = (flow: Flow) => {
    noteChecked(flow.game.id, SaveMark.NotChecked)
    // A split korrid already knows still needs a choice: korrid refuses the
    // launch otherwise. The pick syncs when the devices next meet.
    if (marks.get(flow.game.id)?.mark === SaveMark.Split) {
      void readSides(flow, undefined, true)
      return
    }
    finish(flow)
  }

  const pick = async (owner: Extract<Phase, { _tag: "Choosing" }>, snapshotId: string) => {
    const operation = ++generation
    publish({ _tag: "Picking", flow: owner.flow, ...(owner.check === undefined ? {} : { check: owner.check }) })
    const result = await korrid.savesChoose(owner.flow.game.id, snapshotId)
    if (!current(operation)) return
    if (result._tag === "Err") {
      failed(owner.flow, "Couldn't use that save", saveFailureReason(result.payload), () => void pick(owner, snapshotId))
      return
    }
    checked.delete(owner.flow.game.id)
    changed()
    afterSplit(owner.flow, owner.check)
  }

  const loadHistory = async (game: SaveGameRef) => {
    const operation = ++historyGeneration
    publishHistory({ _tag: "Loading", game })
    const result = await korrid.savesHistory(game.id)
    if (disposed || operation !== historyGeneration) return
    publishHistory(result._tag === "Ok"
      ? { _tag: "Ready", game, history: result.payload }
      : { _tag: "Error", game, message: saveFailureReason(result.payload) })
  }

  const restore = async (game: SaveGameRef, snapshotId: string) => {
    const operation = ++historyGeneration
    publishHistory({ _tag: "Loading", game })
    const result = await korrid.savesRestore(game.id, snapshotId)
    if (disposed || operation !== historyGeneration) return
    if (result._tag === "Err") {
      publishHistory({ _tag: "Error", game, message: `Couldn't restore that save. ${saveFailureReason(result.payload)}` })
      return
    }
    publishHistory({ _tag: "Ready", game, history: result.payload })
    changed()
  }

  const published = (id: string): boolean => {
    const candidates: SurfaceAction[] = []
    if (status?._tag === "Busy" || status?._tag === "Confirm") candidates.push(...(status.actions ?? []))
    if (status?._tag === "ChooseSave") {
      candidates.push(...status.actions)
      for (const side of status.sides) candidates.push({ id: side.actionId, label: "", enabled: true })
    }
    if (historyView !== undefined) {
      candidates.push(...historyView.actions)
      if (historyView._tag === "Ready") {
        for (const entry of historyView.entries) if (entry.restore) candidates.push(entry.restore)
      }
    }
    return candidates.some(candidate => candidate.id === id && candidate.enabled)
  }

  schedule()

  return {
    getMarks: () => marks,
    getStatus: () => status,
    getHistory: () => historyView,
    getGameId: () => phase._tag === "Idle" ? undefined : phase.flow.game.id,
    hasHistory: gameId => offersSaveHistory(base.get(gameId)),
    subscribe: listener => {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    refresh,
    launch: (game, proceed, abandon = () => {}) => {
      if (disposed || phase._tag !== "Idle") return
      void check({ game, proceed, abandon })
    },
    choose: game => {
      if (disposed || phase._tag !== "Idle") return
      void readSides({ game, abandon: () => {} }, undefined, false)
    },
    resolveSplit: (game, proceed, abandon = () => {}) => {
      if (disposed || phase._tag !== "Idle") return
      const flow = { game, proceed, abandon }
      // Re-read the heads korrid just refused on, then ask for the choice.
      const operation = ++generation
      publish({ _tag: "Reading", flow, offline: false })
      void refresh().then(() => {
        if (current(operation)) void readSides(flow, undefined, false)
      })
    },
    refuseStart: (game, failure, abandon = () => {}) => {
      if (disposed || phase._tag !== "Idle") return
      failed({ game, abandon }, `Couldn't start ${game.title}`, saveFailureReason(failure))
    },
    openHistory: game => {
      if (disposed) return
      void loadHistory(game)
    },
    act: actionId => {
      if (!actionId.startsWith("saves:")) return false
      // Only what is published now, enabled, is authority.
      if (!published(actionId)) return true
      const rest = actionId.slice(actionId.indexOf(":", "saves:".length) + 1)
      if (actionId.startsWith("saves:h")) {
        if (historyPhase._tag === "Closed") return true
        if (rest === "close") {
          historyGeneration++
          publishHistory({ _tag: "Closed" })
        } else if (rest.startsWith("restore:") && historyPhase._tag === "Ready") {
          void restore(historyPhase.game, rest.slice("restore:".length))
        }
        return true
      }
      switch (phase._tag) {
        case "Checking":
          // Skip ends the wait here; korrid still enforces the split.
          if (rest === "skip") {
            generation++
            unanswered(phase.flow)
          }
          break
        case "Choosing":
          if (rest === "not-now") close(phase.flow)
          else if (rest.startsWith("pick:")) void pick(phase, rest.slice("pick:".length))
          break
        case "Confirming":
          if (rest === "launch-anyway") finish(phase.flow)
          else if (rest === "cancel") close(phase.flow)
          break
        default:
          break
      }
      return true
    },
    back: () => {
      if (phase._tag === "Idle") return false
      if (phase._tag !== "Picking") close(phase.flow)
      return true
    },
    retry: () => {
      if (phase._tag !== "Failed") return false
      phase.retry?.()
      return true
    },
    dismiss: () => {
      if (phase._tag !== "Failed") return false
      close(phase.flow)
      return true
    },
    dispose: () => {
      disposed = true
      generation++
      historyGeneration++
      if (timer !== undefined) clearTimeout(timer)
      listeners.clear()
    },
  }
}
