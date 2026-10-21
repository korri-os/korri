/**
 * The in-memory brain's save sync (ADR 0014), for browser dev and tests.
 *
 * Shapes are korrid's generated wire types and nothing else. Behaviour follows
 * the approved rules closely enough to drive every portal state: a split is
 * more than one head; choosing makes a new snapshot that names every head as
 * a parent; restoring makes a new snapshot on the current one. Snapshot ids
 * here are counters, not content hashes: nothing in the portal reads them.
 *
 * With no config every method answers SavesUnavailable, exactly as korrid
 * does until its save lifecycle lands, so that state is the default.
 */
import {
  SaveMark,
  type RpcFailure,
  type SaveCheck,
  type SaveCheckOutcome,
  type SaveHistory,
  type SaveHistoryEntry,
  type SaveHistoryOutcome,
  type SaveInUseWarning,
  type SaveStatusOutcome,
} from "@contracts/generated/korrid"

export interface InMemorySaveGame {
  /** Every snapshot this device knows, in any order. */
  readonly records: readonly SaveHistoryEntry[]
  /** Latest snapshot ids; two or more is a split. */
  readonly heads: readonly string[]
  /** The snapshot this device continues from. */
  readonly current?: string
  /** A mark other than the one the heads imply (NotSynced, SyncedBySteam,
   * NotChecked). */
  readonly mark?: SaveMark
  /** What app.saves.check reports besides the mark. */
  readonly inUse?: readonly SaveInUseWarning[]
  /** Daemon-written mismatch copy (SaveCheck.warnings). */
  readonly warnings?: readonly string[]
  /** How long the check waits on the other devices. Past the portal's 3 s
   * limit this is "no device answered". */
  readonly checkDelayMs?: number
  /** The check's own mark, when it differs (NotChecked: nobody answered). */
  readonly checkMark?: SaveMark
}

export interface InMemorySavesConfig {
  readonly games?: Readonly<Record<string, InMemorySaveGame>>
  /** Device key the snapshots made here carry. */
  readonly localDevice?: string
  /** Fail these methods with this failure instead of answering. */
  readonly failures?: Partial<Record<"status" | "history" | "choose" | "restore" | "check", RpcFailure>>
}

const unavailable = (): { _tag: "Err"; payload: RpcFailure } => ({
  _tag: "Err",
  payload: { code: "SavesUnavailable", message: "save session integration is unavailable" },
})

interface GameState {
  records: SaveHistoryEntry[]
  heads: string[]
  current?: string
  mark?: SaveMark
  readonly seed: InMemorySaveGame
}

export function createInMemorySaves(config: InMemorySavesConfig | undefined) {
  const games = new Map<string, GameState>()
  for (const [gameId, seed] of Object.entries(config?.games ?? {})) {
    games.set(gameId, {
      records: structuredClone([...seed.records]),
      heads: [...seed.heads],
      ...(seed.current === undefined ? {} : { current: seed.current }),
      ...(seed.mark === undefined ? {} : { mark: seed.mark }),
      seed,
    })
  }
  let sequence = 0
  const markOf = (state: GameState): SaveMark =>
    state.heads.length > 1 ? SaveMark.Split : state.mark ?? SaveMark.Synced
  const history = (state: GameState): SaveHistory => ({
    records: structuredClone(state.records),
    ...(state.current === undefined ? {} : { current: state.current }),
  })
  const failure = (method: keyof NonNullable<InMemorySavesConfig["failures"]>) => {
    const failed = config?.failures?.[method]
    return failed === undefined ? undefined : { _tag: "Err" as const, payload: structuredClone(failed) }
  }
  const missing = (): { _tag: "Err"; payload: RpcFailure } => ({
    _tag: "Err",
    payload: { code: "GameNotFound", message: "Korri has no save history for this game." },
  })
  /** A new snapshot with `from`'s content, started from `parents`. */
  const snapshot = (state: GameState, from: SaveHistoryEntry, parents: readonly string[]) => {
    sequence += 1
    const snapshotId = `sha256:${String(sequence).padStart(64, "0")}`
    state.records.push({
      snapshotId,
      record: {
        ...structuredClone(from.record),
        parents: [...parents],
        device: config?.localDevice ?? from.record.device,
        endedAt: new Date().toISOString(),
        interrupted: false,
      },
    })
    state.heads = [snapshotId]
    state.current = snapshotId
    if (state.mark === SaveMark.NotChecked) delete state.mark
    return snapshotId
  }

  return {
    async savesStatus(): Promise<SaveStatusOutcome> {
      if (config === undefined) return unavailable()
      const failed = failure("status")
      if (failed) return failed
      return {
        _tag: "Ok",
        payload: {
          games: [...games].map(([gameId, state]) => ({ gameId, status: markOf(state), heads: [...state.heads] })),
        },
      }
    },
    async savesHistory(gameId: string): Promise<SaveHistoryOutcome> {
      if (config === undefined) return unavailable()
      const failed = failure("history")
      if (failed) return failed
      const state = games.get(gameId)
      return state === undefined ? missing() : { _tag: "Ok", payload: history(state) }
    },
    async savesChoose(gameId: string, snapshotId: string): Promise<SaveHistoryOutcome> {
      if (config === undefined) return unavailable()
      const failed = failure("choose")
      if (failed) return failed
      const state = games.get(gameId)
      if (state === undefined) return missing()
      const picked = state.records.find(entry => entry.snapshotId === snapshotId)
      if (picked === undefined || !state.heads.includes(snapshotId)) {
        return { _tag: "Err", payload: { code: "SnapshotNotHead", message: "That save is not one side of the split." } }
      }
      snapshot(state, picked, state.heads)
      return { _tag: "Ok", payload: history(state) }
    },
    async savesRestore(gameId: string, snapshotId: string): Promise<SaveHistoryOutcome> {
      if (config === undefined) return unavailable()
      const failed = failure("restore")
      if (failed) return failed
      const state = games.get(gameId)
      if (state === undefined) return missing()
      const picked = state.records.find(entry => entry.snapshotId === snapshotId)
      if (picked === undefined) return { _tag: "Err", payload: { code: "SnapshotUnknown", message: "Korri does not have that save." } }
      snapshot(state, picked, state.current === undefined ? state.heads : [state.current])
      return { _tag: "Ok", payload: history(state) }
    },
    async savesCheck(gameId: string, _runnerId?: string): Promise<SaveCheckOutcome> {
      if (config === undefined) return unavailable()
      const failed = failure("check")
      if (failed) return failed
      const state = games.get(gameId)
      if (state === undefined) return missing()
      const delay = state.seed.checkDelayMs ?? 0
      if (delay > 0) await new Promise(resolve => setTimeout(resolve, delay))
      // korrid words any release or runner mismatch itself, as `warnings`.
      const check: SaveCheck = {
        warnings: [...(state.seed.warnings ?? [])],
        status: state.heads.length > 1 ? SaveMark.Split : state.seed.checkMark ?? markOf(state),
        inUse: structuredClone([...(state.seed.inUse ?? [])]),
      }
      return { _tag: "Ok", payload: check }
    },
  }
}
