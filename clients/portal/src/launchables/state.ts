import type {
  ActiveSession,
  CatalogSnapshotOutcome,
  Game,
  LocalGamesListOutcome,
  SessionPrepareOutcome,
  SessionPrepared,
  SessionStatusOutcome,
  SessionStopOutcome,
} from "@contracts/generated/korrid"
import { foldGameCopies, type PortalGameCopy, type PortalLocalGame } from "./fold-games"

/**
 * Launchables screen state. korrid outcomes are converted into this ADT at
 * the seam; components never inspect an RPC payload directly.
 *
 * Playable entries come from korrid's catalog and local games on this device.
 */
export type PortalEntry =
  | {
      readonly kind: "now-playing"
      readonly session: ActiveSession
      /** Source evidence for this launch only, never a stale launchable entry. */
      readonly localCatalogGame?: Game
    }
  | {
      readonly kind: "local-game"
      readonly game: PortalLocalGame
      readonly alternatives?: readonly PortalGameCopy[]
    }
  | {
      readonly kind: "game"
      readonly game: Game
      readonly alternatives?: readonly PortalGameCopy[]
    }

/**
 * Linux status omits host (lib.rs::host_session_status_outcome), while its
 * catalog uses config.label. Require source.isLocal and the game id; an
 * explicit session host must still match that copy, never a same-id peer.
 */
function localCatalogGameForSession(
  session: ActiveSession,
  entries: readonly PortalEntry[],
): Maybe<Game> {
  const matches = (game: Game) =>
    game.source.isLocal &&
    game.id === session.gameId &&
    (session.host === undefined || game.host === session.host)
  for (const entry of entries) {
    if (entry.kind === "game" && matches(entry.game)) {
      return { _tag: "Some", value: entry.game }
    }
    if (entry.kind === "game" || entry.kind === "local-game") {
      for (const copy of entry.alternatives ?? []) {
        if (copy.kind === "remote" && matches(copy.game)) {
          return { _tag: "Some", value: copy.game }
        }
      }
    }
    if (
      entry.kind === "now-playing" &&
      entry.session.launchId === session.launchId &&
      entry.localCatalogGame !== undefined &&
      matches(entry.localCatalogGame)
    ) return { _tag: "Some", value: entry.localCatalogGame }
  }
  return { _tag: "None" }
}

export const isLocalCatalogSession = (
  session: ActiveSession,
  entries: readonly PortalEntry[],
): boolean => localCatalogGameForSession(session, entries)._tag === "Some"

const sessionEntry = (
  session: ActiveSession,
  entries: readonly PortalEntry[],
  previousEntries: readonly PortalEntry[],
): PortalEntry => {
  // Old catalog facts establish locality only for the exact observed launch.
  const previous = previousEntries.some(entry =>
    entry.kind === "now-playing" && entry.session.launchId === session.launchId,
  ) && !isLocalCatalogSession(session, entries)
    ? localCatalogGameForSession(session, previousEntries)
    : { _tag: "None" as const }
  return {
    kind: "now-playing",
    session,
    ...(previous._tag === "Some" ? { localCatalogGame: previous.value } : {}),
  }
}

/**
 * The Linux executor reports idle as Err(SessionCompleted/NoActiveSession).
 * This is session truth, unlike an observation failure. Share that distinction
 * across refresh, background polling, and exact-stop reconciliation.
 */
export const isAuthoritativeSessionStatus = (status: SessionStatusOutcome): boolean =>
  status._tag === "Ok" ||
  status.payload.code === "SessionCompleted" ||
  status.payload.code === "NoActiveSession"

/** The exact game an in-flight start belongs to, so a later failure keeps it. */
export interface LaunchSubject {
  readonly id: string
  readonly title: string
}

/**
 * A notice always states what it is about. Attributing a launch failure to
 * whatever the surface happens to be showing would name the wrong game.
 */
export interface LaunchNotice {
  readonly message: string
  /** Absent only for notices that belong to no single game. */
  readonly subject?: LaunchSubject
}

interface LaunchablesContent {
  readonly entries: readonly PortalEntry[]
  readonly notice: LaunchNotice | null
}

type ReadyState = { readonly _tag: "Ready" } & LaunchablesContent
type PreparingState = {
  readonly _tag: "Preparing"
  readonly title: string
  readonly subject?: LaunchSubject
} & LaunchablesContent
type LaunchingState = {
  readonly _tag: "Launching"
  readonly title: string
  readonly subject?: LaunchSubject
} & LaunchablesContent
type StoppingState = {
  readonly _tag: "Stopping"
  readonly launchId: string
} & LaunchablesContent

export type LaunchablesState =
  | { readonly _tag: "Loading" }
  | ReadyState
  | PreparingState
  | LaunchingState
  | StoppingState

/** Minimal local Maybe until Effect's Option arrives with the RPC slice. */
export type Maybe<A> =
  | { readonly _tag: "Some"; readonly value: A }
  | { readonly _tag: "None" }

interface StreamTarget {
  readonly hostUuid: string
  readonly appId: number
}

/**
 * Return to browsing while keeping the failed start's own identity, so the
 * surface can state which game could not start.
 */
const readyFrom = (
  state: PreparingState | LaunchingState | StoppingState,
  message: string,
): ReadyState => ({
  _tag: "Ready",
  entries: state.entries,
  notice: {
    message,
    ...("subject" in state && state.subject ? { subject: state.subject } : {}),
  },
})

export const entryKey = (entry: PortalEntry): string => {
  switch (entry.kind) {
    case "now-playing":
      return `now-playing:${entry.session.launchId}`
    case "local-game":
      return `local-game:${entry.game.id}`
    case "game":
      return entry.game.host === undefined
        ? `game:${entry.game.id}`
        : `game:${entry.game.host}:${entry.game.id}`
  }
}

export const entryLabel = (entry: PortalEntry): string =>
  entry.kind === "now-playing"
    ? (entry.session.title ?? entry.session.gameId ?? "Current session")
    : entry.game.title

export const LaunchablesState = {
  loading: (): LaunchablesState => ({ _tag: "Loading" }),

  /**
   * Fold Korri-owned game sources into one state. Sunshine discovery remains
   * available to launch routing, but its app catalog and query failures do not
   * become home-screen content.
   */
  fromSources: (
    korrid: CatalogSnapshotOutcome,
    session?: SessionStatusOutcome,
    localGames?: LocalGamesListOutcome,
    previousEntries: readonly PortalEntry[] = [],
  ): LaunchablesState => {
    const entries: PortalEntry[] = []
    const failures: string[] = []

    const localCatalog = localGames?._tag === "Ok" ? localGames.payload.games : []
    const remoteCatalog = korrid._tag === "Ok" ? korrid.payload.games : []
    for (const folded of foldGameCopies(localCatalog, remoteCatalog)) {
      const alternatives =
        folded.alternatives.length === 0
          ? {}
          : { alternatives: folded.alternatives }
      if (folded.primary.kind === "local") {
        entries.push({
          kind: "local-game",
          game: folded.primary.game,
          ...alternatives,
        })
      } else {
        entries.push({
          kind: "game",
          game: folded.primary.game,
          ...alternatives,
        })
      }
    }

    // Source evidence can outlive a catalog read, but never restores its games.
    if (session?._tag === "Ok" && session.payload.active != null) {
      entries.splice(0, 0,
        sessionEntry(session.payload.active, entries, previousEntries))
    }

    if (localGames?._tag === "Ok") {
      for (const failure of localGames.payload.failures ?? []) {
        failures.push(`local games: ${failure.code}`)
      }
    } else if (
      localGames?._tag === "Err" &&
      localGames.payload.code !== "OperationUnsupported"
    ) {
      // An absent inventory capability is not a failed read. The catalog
      // remains authoritative; no local inventory count is invented.
      failures.push(`local games: ${localGames.payload.code}`)
    }

    if (korrid._tag === "Ok") {
      for (const failure of korrid.payload.failures ?? []) {
        failures.push(`${failure.host}: ${failure.code}`)
      }
    } else {
      failures.push(`games: ${korrid.payload.code}`)
    }

    return {
      _tag: "Ready",
      entries,
      notice:
        failures.length > 0 ? { message: failures.join(" · ") } : null,
    }
  },

  /** Replace the notice on a Ready state, leaving its entries alone. */
  withNotice: (state: ReadyState, message: string): ReadyState => ({
    ...state,
    notice: { message },
  }),

  /** Confirm on a game: enter an input-locked case until activity swap. */
  beginPreparing: (
    state: LaunchablesState,
    title: string,
    subject?: LaunchSubject,
  ): LaunchablesState =>
    state._tag === "Ready"
      ? {
          ...state,
          _tag: "Preparing",
          title,
          ...(subject ? { subject } : {}),
          notice: null,
        }
      : state,

  /** Lock all direct local/stream/resume starts before their Promise runs. */
  beginLaunching: (
    state: LaunchablesState,
    title: string,
    subject?: LaunchSubject,
  ): LaunchablesState =>
    state._tag === "Ready"
      ? {
          ...state,
          _tag: "Launching",
          title,
          ...(subject ? { subject } : {}),
          notice: null,
        }
      : state,

  /** Resume locks the list as Launching, so a refusal folds from that case. */
  withResumeOutcome: (
    state: LaunchablesState,
    outcome: { readonly _tag: "Ok" } | { readonly _tag: "Err"; readonly payload: { readonly code: string; readonly message: string } },
  ): LaunchablesState => {
    if (state._tag !== "Launching") return state
    return outcome._tag === "Ok"
      ? state
      : readyFrom(state, `${outcome.payload.code}: ${outcome.payload.message}`)
  },

  withPrepareOutcome: (
    state: LaunchablesState,
    outcome: SessionPrepareOutcome,
  ): LaunchablesState => {
    if (state._tag !== "Preparing") return state
    if (outcome._tag === "Ok") return { ...state, notice: null }
    return readyFrom(
      state,
      `${outcome.payload.code}: ${outcome.payload.message}`,
    )
  },

  /** Prepare proves an owned launch exists; no native activity swap follows on Linux. */
  withLocalCatalogPrepareOutcome: (
    state: LaunchablesState,
    outcome: SessionPrepareOutcome,
    game: Game,
  ): LaunchablesState => {
    if (state._tag !== "Preparing") return state
    if (outcome._tag === "Err") {
      return readyFrom(state, `${outcome.payload.code}: ${outcome.payload.message}`)
    }
    return LaunchablesState.withLocalCatalogAcknowledgement(
      { _tag: "Ready", entries: state.entries, notice: null }, outcome.payload, game,
    )
  },

  /** An ACK owns its source evidence even if the next catalog read loses it. */
  withLocalCatalogAcknowledgement: (
    state: LaunchablesState,
    session: SessionPrepared,
    game: Game,
  ): LaunchablesState => {
    if (state._tag === "Loading") return state
    return {
      ...state,
      entries: [
        {
          kind: "now-playing",
          session: {
            launchId: session.launchId,
            gameId: session.gameId,
            title: game.title,
            ...(game.host === undefined ? {} : { host: game.host }),
          },
          localCatalogGame: game,
        },
        ...state.entries.filter(entry => entry.kind !== "now-playing"),
      ],
    }
  },

  /** Only authoritative observations replace the last known session. */
  withSessionStatus: (
    state: LaunchablesState,
    status: SessionStatusOutcome,
  ): LaunchablesState => {
    if (state._tag !== "Ready" || !isAuthoritativeSessionStatus(status)) return state
    const active = status._tag === "Ok" && status.payload.active != null ? status.payload.active : undefined
    const previous = state.entries.find(entry => entry.kind === "now-playing")?.session
    if (active === undefined && previous === undefined) return state
    if (
      active !== undefined &&
      previous !== undefined &&
      active.launchId === previous.launchId &&
      active.host === previous.host &&
      active.gameId === previous.gameId &&
      active.title === previous.title &&
      active.phase === previous.phase
    ) {
      return state
    }
    const entries: PortalEntry[] = state.entries.filter(
      entry => entry.kind !== "now-playing",
    )
    if (active !== undefined) {
      entries.unshift(sessionEntry(active, entries, state.entries))
    }
    return { ...state, entries }
  },

  /**
   * Lock input before the asynchronous stop request leaves the portal. The
   * target session is named by the caller rather than inferred from a cursor:
   * which session a surface means is the surface's business, not this ADT's.
   */
  beginStopping: (
    state: LaunchablesState,
    target: PortalEntry,
  ): LaunchablesState => {
    if (state._tag !== "Ready" || target.kind !== "now-playing") return state
    return {
      ...state,
      _tag: "Stopping",
      launchId: target.session.launchId,
      notice: null,
    }
  },

  /** A successful stop request is not the same as an ended session. */
  withStopOutcome: (
    state: LaunchablesState,
    outcome: SessionStopOutcome,
  ): LaunchablesState => {
    if (state._tag !== "Stopping") return state
    return outcome._tag === "Ok"
      ? state
      : readyFrom(
          state,
          `${outcome.payload.code}: ${outcome.payload.message}`,
        )
  },

  /** Fold a stop poll; completion, idle, or a different launch ends this stop. */
  withStatusAfterStop: (
    state: LaunchablesState,
    status: SessionStatusOutcome,
  ): LaunchablesState => {
    if (state._tag !== "Stopping") return state
    if (status._tag === "Err" && !isAuthoritativeSessionStatus(status)) {
      return readyFrom(
        state,
        `${status.payload.code}: ${status.payload.message}`,
      )
    }
    if (
      status._tag === "Ok" &&
      status.payload.active != null &&
      status.payload.active.launchId === state.launchId
    ) {
      return state
    }
    return {
      _tag: "Ready",
      notice: null,
      entries: state.entries.filter(entry => entry.kind !== "now-playing"),
    }
  },

  stopTimedOut: (state: LaunchablesState): LaunchablesState =>
    state._tag === "Stopping"
      ? readyFrom(state, "StopPending: session is still stopping")
      : state,
}
