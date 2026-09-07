import type {
  BackgroundNoticeResult,
  LaunchLocalResult,
  QueryStreamAppsResult,
  StartStreamResult,
  StorageAccessResult,
  StreamHost,
} from "@contracts/bridge/korri-native-bridge"
import type {
  ActiveSession,
  CatalogSnapshotOutcome,
  Game,
  LocalGameLaunchOutcome,
  LocalGamesListOutcome,
  ResolvedMoonlight,
  SessionPrepareOutcome,
  SessionStatusOutcome,
  SessionStopOutcome,
} from "@contracts/generated/korrid"
import { foldGameCopies, type PortalGameCopy, type PortalLocalGame } from "./fold-games"

/**
 * Launchables screen state. Raw bridge results are converted into this ADT
 * at the seam; components never inspect bridge payloads directly.
 *
 * Playable entries come from korrid's catalog and local games on this device.
 * Sunshine's advertised app list is transport data: it may identify Korri's
 * streaming endpoint, but it is not Korri's game catalog.
 */
export type PortalEntry =
  /**
   * Korri cannot reach its own settings, plugins, or local-game files until
   * the user grants file access. This is an entry rather than a passive
   * banner because the portal is controller-first: a message the user cannot
   * focus and confirm would be unreachable without a touchscreen.
   */
  | { readonly kind: "storage-access" }
  /**
   * Whether the user can see Korri running in the background. A setting
   * rather than a warning: the brain running on is what makes leaving a
   * game safe, so this exists to be seen and switched, not fixed.
   */
  | { readonly kind: "background-notice"; readonly visible: boolean }
  | { readonly kind: "now-playing"; readonly session: ActiveSession }
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

/** One provisioned host's app-query outcome, as gathered by the Root. */
export interface StreamSource {
  readonly host: StreamHost
  readonly apps: QueryStreamAppsResult
}

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
    case "background-notice":
      return "background-notice"
    case "storage-access":
      return "storage-access"
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
  entry.kind === "background-notice"
    ? entry.visible
      ? "Background notice: on — tap to hide it"
      : "Background notice: off — tap to show it"
    :
  entry.kind === "storage-access"
    ? "Korri needs file access — open settings"
    : entry.kind === "now-playing"
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
    _streams: readonly StreamSource[],
    korrid: CatalogSnapshotOutcome,
    _hostsError?: string,
    session?: SessionStatusOutcome,
    localGames?: LocalGamesListOutcome,
    storage?: StorageAccessResult,
    notice?: BackgroundNoticeResult,
  ): LaunchablesState => {
    const entries: PortalEntry[] = []
    const failures: string[] = []

    // Denied file access comes first: without it Korri cannot read its own
    // settings, so it outranks everything else on screen. An inconclusive
    // query is not treated as denial — we do not nag on a failed check.
    if (storage?._tag === "Denied") {
      entries.push({ kind: "storage-access" })
    }

    // An active host session renders first as a now-playing banner. A
    // status failure degrades silently — no banner, no notice — rather
    // than blocking the list. `!= null` also remains compatible with older
    // korrid builds that emitted Option::None as explicit null.
    if (session?._tag === "Ok" && session.payload.active != null) {
      entries.push({ kind: "now-playing", session: session.payload.active })
    }

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

    // Korri keeps its brain running after you leave, and the user is
    // entitled to see that and switch it off. Always present, and last:
    // it is a setting, not something to play.
    entries.push({ kind: "background-notice", visible: notice?._tag === "Visible" })

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

  /** Select the plugin-owned Sunshine app, constrained to a game's origin host. */
  korriStreamTarget: (
    moonlight: ResolvedMoonlight,
    streams: readonly StreamSource[],
    hostName?: string,
  ): Maybe<StreamTarget> => {
    const candidates =
      hostName === undefined
        ? streams
        : streams.filter(source => source.host.name === hostName)
    for (const source of candidates) {
      if (source.apps._tag !== "StreamApps") continue
      const app = source.apps.items.find(
        app => app.name === moonlight.sunshineApp,
      )
      if (app !== undefined) {
        return {
          _tag: "Some",
          value: { hostUuid: source.host.uuid, appId: app.id },
        }
      }
    }
    return { _tag: "None" }
  },

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

  withLocalLaunchOutcome: (
    state: LaunchablesState,
    outcome: LocalGameLaunchOutcome,
  ): LaunchablesState => {
    if (state._tag !== "Launching") return state
    return outcome._tag === "Ok"
      ? state
      : readyFrom(
          state,
          `${outcome.payload.code}: ${outcome.payload.message}`,
        )
  },

  withLocalLaunchResult: (
    state: LaunchablesState,
    result: LaunchLocalResult,
  ): LaunchablesState => {
    if (state._tag !== "Launching") return state
    return result._tag === "Launched"
      ? state
      : readyFrom(state, `${result.reason}: ${result.message}`)
  },

  withStartStreamResult: (
    state: LaunchablesState,
    result: StartStreamResult,
  ): LaunchablesState => {
    if (state._tag !== "Launching" && state._tag !== "Preparing") return state
    if (result._tag === "StreamStarted") return { ...state, notice: null }
    return readyFrom(state, `${result.reason}: ${result.message}`)
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

  /** Fold a status poll while stopping; only idle removes the banner. */
  withStatusAfterStop: (
    state: LaunchablesState,
    status: SessionStatusOutcome,
  ): LaunchablesState => {
    if (state._tag !== "Stopping") return state
    if (status._tag === "Err") {
      return readyFrom(
        state,
        `${status.payload.code}: ${status.payload.message}`,
      )
    }
    if (
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
