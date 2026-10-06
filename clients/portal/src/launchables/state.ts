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
import { InitialHandoff, PendingLaunchPhase } from "@contracts/generated/korrid"
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
  (status._tag === "Ok" && status.payload.observationFailure === undefined) ||
  (status._tag === "Err" && (status.payload.code === "SessionCompleted" ||
    status.payload.code === "NoActiveSession"))

/** The exact game an in-flight start belongs to, so a later failure keeps it. */
export interface LaunchSubject {
  /** Raw content identity, absent when the daemon does not know it. */
  readonly id?: string
  /** Existing presented entry key captured from the exact selected copy. */
  readonly entryId?: string
  readonly title: string
}

/**
 * A notice always states what it is about. Attributing a launch failure to
 * whatever the surface happens to be showing would name the wrong game.
 */
export type LaunchNotice =
  | { readonly _tag: "Catalog"; readonly message: string }
  | { readonly _tag: "Launch"; readonly message: string; readonly subject?: LaunchSubject }
  | { readonly _tag: "Stop"; readonly message: string }
  | { readonly _tag: "Action"; readonly message: string }

interface LaunchablesContent {
  readonly entries: readonly PortalEntry[]
  readonly notice: LaunchNotice | null
  /** Actual failed daemon observation; entries retain only last-known identity. */
  readonly unavailableSessionStatus?: SessionStatusOutcome
}

type ReadyState = { readonly _tag: "Ready" } & LaunchablesContent
type PreparingState = {
  readonly _tag: "Preparing"
  readonly title: string
  readonly subject?: LaunchSubject
} & LaunchablesContent
type LaunchingState = {
  readonly _tag: "Launching"
  readonly returnLaunchId?: string
  readonly title: string
  readonly subject?: LaunchSubject
} & LaunchablesContent
type StartingState = {
  readonly _tag: "Starting"
  readonly title: string
  readonly subject: LaunchSubject
  readonly launchId?: string
  readonly cancelling: boolean
  readonly cancelRetryAvailable?: boolean
} & LaunchablesContent

export interface PendingLaunchChoice {
  readonly launchId: string
  readonly gameId?: string
  readonly title: string
  readonly phase: PendingLaunchPhase | "waiting"
  readonly cancel: "idle" | "sent" | "retry"
}

type ChoosingState = {
  readonly _tag: "Choosing"
  readonly choices: readonly PendingLaunchChoice[]
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
  | StartingState
  | ChoosingState
  | ({ readonly _tag: "Recovery"; readonly notice: LaunchNotice } & LaunchablesContent)

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
  state: PreparingState | LaunchingState | StoppingState | StartingState,
  message: string,
): ReadyState => ({
  _tag: "Ready",
  entries: state.entries,
  ...(state.unavailableSessionStatus === undefined ? {} : { unavailableSessionStatus: state.unavailableSessionStatus }),
  notice: state._tag === "Stopping"
    ? { _tag: "Stop", message }
    : { _tag: "Launch", message, ...(state.subject ? { subject: state.subject } : {}) },
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

export function launchSubjectForGame(game: Game, entries: readonly PortalEntry[]): LaunchSubject {
  const matches = (candidate: Game) => candidate.id === game.id && candidate.host === game.host && candidate.source.isLocal === game.source.isLocal
  const entry = entries.find(entry =>
    (entry.kind === "game" && matches(entry.game)) ||
    ((entry.kind === "game" || entry.kind === "local-game") && entry.alternatives?.some(copy => copy.kind === "remote" && matches(copy.game))))
  return { id: game.id, title: game.title, ...(entry === undefined ? {} : { entryId: entryKey(entry) }) }
}

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
    if (session?._tag === "Ok" && isAuthoritativeSessionStatus(session) && session.payload.active != null) {
      entries.splice(0, 0,
        sessionEntry(session.payload.active, entries, previousEntries))
    }

    if (localGames?._tag === "Ok") {
      for (const failure of localGames.payload.failures ?? []) {
        failures.push(`local games: ${failure.code}: ${failure.message}`)
      }
    } else if (
      localGames?._tag === "Err" &&
      localGames.payload.code !== "OperationUnsupported"
    ) {
      // An absent inventory capability is not a failed read. The catalog
      // remains authoritative; no local inventory count is invented.
      failures.push(`local games: ${localGames.payload.code}: ${localGames.payload.message}`)
    }

    if (korrid._tag === "Ok") {
      for (const failure of korrid.payload.failures ?? []) {
        failures.push(`${failure.host}: ${failure.code}: ${failure.message}`)
      }
    } else {
      failures.push(`games: ${korrid.payload.code}: ${korrid.payload.message}`)
    }

    if (session !== undefined && !isAuthoritativeSessionStatus(session)) {
      const known = previousEntries.find(entry => entry.kind === "now-playing" && isLocalCatalogSession(entry.session, previousEntries))
      if (known?.kind === "now-playing") {
        if (!entries.some(entry => entry.kind === "now-playing")) entries.unshift(sessionEntry(known.session, entries, previousEntries))
        return {
          _tag: "Ready", entries, unavailableSessionStatus: session,
          notice: failures.length > 0 ? { _tag: "Catalog", message: failures.join(" · ") } : null,
        }
      }
    }
    return LaunchablesState.recoverStartup({
      _tag: "Ready",
      entries,
      notice:
        failures.length > 0 ? { _tag: "Catalog", message: failures.join(" · ") } : null,
    }, session)
  },

  /** Daemon facts, never current focus or browser history, define fresh recovery. */
  recoverStartup: (state: ReadyState, status?: SessionStatusOutcome, previousChoices: readonly PendingLaunchChoice[] = []): LaunchablesState => {
    if (status === undefined) return state
    const failure = status._tag === "Err" ? status.payload : status.payload.observationFailure
    if (status._tag === "Err") {
      return !isAuthoritativeSessionStatus(status) && state.entries.some(entry =>
        (entry.kind === "game" && entry.game.source.isLocal) ||
        ((entry.kind === "game" || entry.kind === "local-game") && entry.alternatives?.some(copy => copy.kind === "remote" && copy.game.source.isLocal)))
        ? { ...state, _tag: "Recovery", notice: { _tag: "Launch", message: `${status.payload.code}: ${status.payload.message}` } } : state
    }
    const active = isAuthoritativeSessionStatus(status) ? status.payload.active : undefined
    const returnedOrStopping = active?.phase === "frozen" || active?.phase === "focus-failed" || active?.phase === "stopping"
    const candidates = new Map<string, { launchId: string; gameId?: string; phase: PendingLaunchChoice["phase"] }>((status.payload.pendingLaunches ?? [])
      .filter(pending => !(pending.session.launchId === active?.launchId &&
        (returnedOrStopping || active.initialHandoff === InitialHandoff.Observed || active.initialHandoff === InitialHandoff.Recovered)))
      .map(pending => [pending.session.launchId, { ...pending.session, phase: pending.phase }]))
    if (active?.initialHandoff === InitialHandoff.Waiting && !returnedOrStopping && !candidates.has(active.launchId)) {
      candidates.set(active.launchId, {
        launchId: active.launchId,
        ...(active.gameId === undefined ? {} : { gameId: active.gameId }),
        phase: "waiting",
      })
    }
    const localGames = state.entries.flatMap(entry => [
      ...(entry.kind === "game" ? [entry.game] : []),
      ...((entry.kind === "game" || entry.kind === "local-game") ? (entry.alternatives ?? []).flatMap(copy => copy.kind === "remote" ? [copy.game] : []) : []),
    ])
    const choices: PendingLaunchChoice[] = [...candidates.values()].map(candidate => ({
      ...candidate,
      title: localGames.find(game => game.source.isLocal && game.id === candidate.gameId)?.title
        ?? (active?.launchId === candidate.launchId ? active.title : undefined) ?? candidate.gameId ?? "live session",
      cancel: candidate.phase === PendingLaunchPhase.Cancelling ? "sent"
        : previousChoices.find(choice => choice.launchId === candidate.launchId)?.cancel ?? "idle",
    })).sort((left, right) => left.launchId < right.launchId ? -1 : left.launchId > right.launchId ? 1 : 0)
    if (choices.length > 1) return {
      ...state, _tag: "Choosing", choices,
      ...(failure ? { notice: { _tag: "Launch", message: failure.message } } : {}),
    }
    const choice = choices[0]
    if (!choice) return failure ? { ...state, _tag: "Recovery", notice: { _tag: "Launch", message: `${failure.code}: ${failure.message}` } } : state
    const localGame = localGames.find(game => game.source.isLocal && game.id === choice.gameId)
    return {
      ...state, _tag: "Starting", title: choice.title,
      subject: localGame === undefined ? { ...(choice.gameId === undefined ? {} : { id: choice.gameId }), title: choice.title }
        : launchSubjectForGame(localGame, state.entries),
      launchId: choice.launchId,
      cancelling: choice.phase === PendingLaunchPhase.Cancelling || choice.cancel === "sent" || choice.cancel === "retry",
      ...(choice.cancel === "retry" ? { cancelRetryAvailable: true } : {}),
      ...(failure ? { notice: { _tag: "Launch", message: failure.message }, cancelRetryAvailable: choice.phase !== PendingLaunchPhase.Cancelling && choice.cancel !== "sent" } : {}),
    }
  },

  beginPendingCancellation: (state: LaunchablesState, launchId: string): LaunchablesState => {
    if (state._tag !== "Choosing" || !state.choices.some(choice =>
      choice.launchId === launchId && (choice.cancel === "retry" ||
        (choice.cancel === "idle" && choice.phase !== PendingLaunchPhase.Cancelling)))) return state
    return { ...state, choices: state.choices.map(choice => choice.launchId === launchId ? { ...choice, cancel: "sent" } : choice) }
  },

  withPendingCancellationOutcome: (state: LaunchablesState, launchId: string, outcome: SessionStopOutcome): LaunchablesState => {
    const choosing = state._tag === "Choosing" && state.choices.some(choice => choice.launchId === launchId && choice.cancel === "sent")
    const single = state._tag === "Starting" && state.launchId === launchId && state.cancelling
    if (!choosing && !single) return state
    if (outcome._tag === "Ok") return { ...state, notice: null }
    if (["StaleLaunchIdentity", "NoActiveSession", "SessionCompleted"].includes(outcome.payload.code)) return state
    if (state._tag === "Starting") return LaunchablesState.startupProblem(state, outcome.payload.message)
    if (state._tag !== "Choosing") return state
    return { ...state, notice: { _tag: "Launch", message: outcome.payload.message },
      choices: state.choices.map(choice => choice.launchId === launchId ? { ...choice, cancel: "retry" } : choice),
    }
  },

  /** Replace the notice on a Ready state, leaving its entries alone. */
  withNotice: (state: ReadyState, message: string): ReadyState => ({
    ...state,
    notice: { _tag: "Action", message },
  }),

  beginStartup: (state: LaunchablesState, game: Game): LaunchablesState =>
    state._tag === "Ready" ? {
      ...state, _tag: "Starting", title: game.title,
      subject: launchSubjectForGame(game, state.entries), notice: null,
      cancelling: false,
    } : state,

  withStartupReservation: (state: LaunchablesState, session: SessionPrepared, game: Game): LaunchablesState => {
    if (state._tag !== "Starting" || state.subject.id !== session.gameId) return state
    return LaunchablesState.withLocalCatalogAcknowledgement({ ...state, launchId: session.launchId }, session, game)
  },

  cancelStartup: (state: LaunchablesState): LaunchablesState =>
    state._tag === "Starting" ? { ...state, cancelling: true, cancelRetryAvailable: false } : state,

  startupProblem: (state: LaunchablesState, message: string): LaunchablesState =>
    state._tag === "Starting" ? { ...state, notice: { _tag: "Launch", message, subject: state.subject }, cancelRetryAvailable: true } : state,

  failStartup: (state: LaunchablesState, message: string, preserveSession = false): LaunchablesState =>
    state._tag === "Starting" ? readyFrom({
      ...state,
      entries: preserveSession ? state.entries : state.entries.filter(entry =>
        entry.kind !== "now-playing" || entry.session.launchId !== state.launchId),
    }, message) : state,

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

  beginReturn: (state: ReadyState, session: ActiveSession): LaunchablesState => ({
    ...state, _tag: "Launching", returnLaunchId: session.launchId,
    title: session.title ?? session.gameId ?? "Live session",
    subject: {
      ...(session.gameId === undefined ? {} : { id: session.gameId }),
      ...(state.entries.some(entry => entry.kind === "now-playing" && entry.session.launchId === session.launchId)
        ? { entryId: entryKey({ kind: "now-playing", session }) } : {}),
      title: session.title ?? session.gameId ?? "Live session",
    },
  }),

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
    const active = status._tag === "Ok" ? status.payload.active : undefined
    if (state._tag === "Choosing") {
      if (!isAuthoritativeSessionStatus(status)) {
        const failure = status._tag === "Err" ? status.payload : status.payload.observationFailure
        return failure ? { ...state, notice: { _tag: "Launch", message: failure.message }, unavailableSessionStatus: status,
          // Failed cleanup authorizes exact retry, not a fabricated new phase.
          choices: failure.code === "HostRecoveryBlocked" ? state.choices.map(choice =>
            choice.cancel === "sent" ? { ...choice, cancel: "retry" } : choice) : state.choices,
        } : state
      }
      const entries: PortalEntry[] = state.entries.filter(entry => entry.kind !== "now-playing")
      if (active) entries.unshift(sessionEntry(active, entries, state.entries))
      return LaunchablesState.recoverStartup({ _tag: "Ready", entries,
        notice: state.unavailableSessionStatus === undefined ? state.notice : null,
      }, status, state.choices)
    }
    if (state._tag === "Starting") {
      if (state.launchId === undefined) return state
      const pending = status._tag === "Ok" ? status.payload.pendingLaunches?.find(pending => pending.session.launchId === state.launchId) : undefined
      if (!isAuthoritativeSessionStatus(status)) {
        const failure = status._tag === "Ok" ? status.payload.observationFailure : status.payload
        return failure?.code === "HostRecoveryBlocked" ? LaunchablesState.startupProblem(state, failure.message) : state
      }
      if (active?.launchId === state.launchId) {
        const entries: PortalEntry[] = state.entries.filter(entry => entry.kind !== "now-playing")
        entries.unshift(sessionEntry(active, entries, state.entries))
        const returned = active.phase === "frozen" || active.phase === "focus-failed"
        const handedOff = active.initialHandoff === InitialHandoff.Observed || active.initialHandoff === InitialHandoff.Recovered
        return !state.cancelling && (returned || handedOff)
          ? { _tag: "Ready", entries, notice: state.notice } : { ...state, entries }
      }
      // An owned reservation can still commit with no unit and even after a lost ACK.
      if (pending) return state
      // Positive daemon absence of BOTH the exact pending and active identity
      // retires it. No timer, acknowledgement flag or transport guess is involved.
      return LaunchablesState.withSessionStatus(
        { _tag: "Ready", entries: state.entries, notice: state.notice }, status,
      )
    }
    if (!isAuthoritativeSessionStatus(status)) {
      if (state._tag === "Ready") {
        return state.entries.some(entry => entry.kind === "now-playing" && isLocalCatalogSession(entry.session, state.entries))
          ? { ...state, unavailableSessionStatus: status }
          : LaunchablesState.recoverStartup(state, status)
      }
      return state
    }
    if (state._tag === "Ready" && state.unavailableSessionStatus !== undefined) {
      const { unavailableSessionStatus, ...healthy } = state
      return LaunchablesState.withSessionStatus(healthy, status)
    }
    if (state._tag === "Launching" && state.returnLaunchId !== undefined && active?.launchId !== state.returnLaunchId) {
      return LaunchablesState.withSessionStatus({ _tag: "Ready", entries: state.entries, notice: state.notice }, status)
    }
    if (state._tag === "Recovery") {
      const entries: PortalEntry[] = state.entries.filter(entry => entry.kind !== "now-playing")
      if (active) entries.unshift(sessionEntry(active, entries, state.entries))
      return LaunchablesState.recoverStartup({ _tag: "Ready", entries, notice: null }, status)
    }
    if (state._tag !== "Ready") return state
    const previous = state.entries.find(entry => entry.kind === "now-playing")?.session
    if (active === undefined && previous === undefined) return LaunchablesState.recoverStartup(state, status)
    if (
      active !== undefined &&
      previous !== undefined &&
      active.launchId === previous.launchId &&
      active.host === previous.host &&
      active.gameId === previous.gameId &&
      active.title === previous.title &&
      active.phase === previous.phase &&
      active.focusOwnership === previous.focusOwnership &&
      active.initialHandoff === previous.initialHandoff &&
      !(status._tag === "Ok" && status.payload.pendingLaunches?.some(pending => pending.session.launchId !== active.launchId))
    ) {
      return state
    }
    const entries: PortalEntry[] = state.entries.filter(
      entry => entry.kind !== "now-playing",
    )
    if (active !== undefined) {
      entries.unshift(sessionEntry(active, entries, state.entries))
    }
    const ready: ReadyState = { ...state, entries }
    return active?.launchId !== previous?.launchId || (status._tag === "Ok" && status.payload.pendingLaunches?.some(pending => pending.session.launchId !== previous?.launchId))
      ? LaunchablesState.recoverStartup(ready, status) : ready
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
    if (!isAuthoritativeSessionStatus(status)) {
      const failure = status._tag === "Err" ? status.payload : status.payload.observationFailure
      return failure ? readyFrom(
        { ...state, unavailableSessionStatus: status },
        `${failure.code}: ${failure.message}`,
      ) : state
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
