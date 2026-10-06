/**
 * Korri's launchables state, expressed in the surface treaty.
 *
 * This is the whole translation layer: everything a surface is allowed to know
 * is decided here, in one pure function, so no surface ever sees a bridge
 * result, a korrid outcome, or an error code. Anything Korri cannot honestly
 * report — cover art, playtime, genres — is simply absent rather than filled
 * in with a placeholder.
 */
import type {
  SurfaceAction,
  SurfaceCatalog,
  SurfaceGame,
  SurfaceIdentityManagement,
  SurfaceLaunchLocation,
  SurfaceModel,
  SurfaceSettingGroup,
  SurfaceSettingsStatus,
  SurfaceStatus,
} from "@contracts/surface/korri-surface"
import { mergePlayStats, type PortalGameCopy } from "../launchables/fold-games"
import type { PlayStats } from "@contracts/generated/korrid"
import {
  entryKey,
  isLocalCatalogSession,
  type LaunchablesState,
  type LaunchNotice,
  type LaunchSubject,
  type PortalEntry,
} from "../launchables/state"

/** Section captions. Grouping is Korri's call; the surface only renders it. */
const SECTION_CONTINUE = "Continue"
const SECTION_THIS_DEVICE = "This device"

/* Every entry korrid publishes today is playable. The rail actions that used
 * this seam were shell permission prompts, which no device asks for now. */
export function isActionEntry(_entry: PortalEntry): boolean {
  return false
}

function actionFromEntry(_entry: PortalEntry): SurfaceAction | null {
  return null
}

function orderedCopiesForEntry(entry: PortalEntry): readonly PortalGameCopy[] {
  if (entry.kind !== "local-game" && entry.kind !== "game") return []
  const copies: PortalGameCopy[] = [
    entry.kind === "local-game"
      ? { kind: "local", game: entry.game }
      : { kind: "remote", game: entry.game },
    ...(entry.alternatives ?? []),
  ]
  return copies.sort((left, right) => {
    if (copyIsLocal(left) !== copyIsLocal(right)) {
      return copyIsLocal(left) ? -1 : 1
    }
    if (left.kind !== right.kind) return left.kind === "local" ? -1 : 1
    if (left.kind === "local" && right.kind === "local") {
      return left.game.id.localeCompare(right.game.id)
    }
    if (left.kind === "remote" && right.kind === "remote") {
      return (
        (left.game.host ?? "").localeCompare(right.game.host ?? "") ||
        left.game.id.localeCompare(right.game.id)
      )
    }
    return 0
  })
}

function copyIsLocal(copy: PortalGameCopy): boolean {
  return copy.kind === "local" || copy.game.source.isLocal
}

function copyHostKey(copy: PortalGameCopy): string {
  if (copy.kind === "local" || copy.game.source.isLocal) return "local"
  return `remote:${copy.game.host ?? ""}`
}

function copyLocationLabel(copy: PortalGameCopy): string {
  if (copy.kind === "local" || copy.game.source.isLocal) return "This device"
  return copy.game.host ?? "Other device"
}

function copyLocationId(copy: PortalGameCopy): string {
  return JSON.stringify([
    copy.kind,
    copy.kind === "remote" ? (copy.game.host ?? null) : null,
    copy.game.id,
  ])
}

function distinctHostCopies(entry: PortalEntry): readonly PortalGameCopy[] {
  const seen = new Set<string>()
  return orderedCopiesForEntry(entry).filter(copy => {
    const key = copyHostKey(copy)
    if (seen.has(key)) return false
    seen.add(key)
    return true
  })
}

/** Host choices for a folded game. Local is always first when it exists. */
export function launchLocationsForEntry(
  entry: PortalEntry,
): readonly SurfaceLaunchLocation[] {
  const copies = distinctHostCopies(entry)
  if (copies.length < 2) return []
  return copies.map(copy => ({
    id: copyLocationId(copy),
    label: copyLocationLabel(copy),
  }))
}

/** Resolve an opaque surface choice to one exact copy, with no fallback. */
export function entryForLaunchLocation(
  entry: PortalEntry,
  launchLocationId: string,
): PortalEntry | undefined {
  const copy = distinctHostCopies(entry).find(
    candidate => copyLocationId(candidate) === launchLocationId,
  )
  if (copy === undefined) return undefined
  return copy.kind === "local"
    ? { kind: "local-game", game: copy.game }
    : { kind: "game", game: copy.game }
}

/**
 * Play facts for a game entry, in surface terms. Merges every folded copy so
 * a game played on Zao and on this tablet reads as one history. Parses the
 * UTC ISO string once here; surfaces never see the string.
 */
function playFactsForEntry(
  entry: PortalEntry,
): Pick<SurfaceGame, "lastPlayedAt" | "playCount" | "totalPlaytimeSeconds"> {
  if (entry.kind !== "local-game" && entry.kind !== "game") return {}
  const stats: PlayStats | undefined = mergePlayStats(orderedCopiesForEntry(entry))
  if (stats === undefined || stats.lastPlayed === undefined) return {}
  const lastPlayedAt = Date.parse(stats.lastPlayed)
  if (Number.isNaN(lastPlayedAt)) return {}
  return {
    lastPlayedAt,
    playCount: stats.playCount,
    totalPlaytimeSeconds: stats.totalPlaytimeSeconds,
  }
}

function alternativeLocation(entry: PortalEntry): string | undefined {
  if (entry.kind !== "local-game" && entry.kind !== "game") return undefined
  const primaryKey = copyHostKey(
    entry.kind === "local-game"
      ? { kind: "local", game: entry.game }
      : { kind: "remote", game: entry.game },
  )
  const visible = distinctHostCopies(entry)
    .filter(copy => copyHostKey(copy) !== primaryKey)
    .map(copy =>
      copyIsLocal(copy) ? "this device" : copyLocationLabel(copy),
    )
  return visible.length === 0 ? undefined : `Also on ${visible.join(", ")}`
}

function gameFromEntry(
  entry: PortalEntry,
  entries: readonly PortalEntry[],
): SurfaceGame | null {
  switch (entry.kind) {
    case "now-playing":
      return {
        id: entryKey(entry),
        title:
          entry.session.title ?? entry.session.gameId ?? "Current session",
        section: SECTION_CONTINUE,
        subtitle: isLocalCatalogSession(entry.session, entries)
          ? SECTION_THIS_DEVICE
          : (entry.session.host ?? "Running now"),
        resumable: true,
      }
    case "local-game": {
      const alternative = alternativeLocation(entry)
      const launchLocations = launchLocationsForEntry(entry)
      return {
        id: entryKey(entry),
        title: entry.game.title,
        section: SECTION_THIS_DEVICE,
        subtitle:
          alternative === undefined
            ? entry.game.system
            : `${entry.game.system} · ${alternative}`,
        ...(entry.game.coverArtUrl === undefined
          ? {}
          : { coverArtUrl: entry.game.coverArtUrl }),
        ...(launchLocations.length < 2 ? {} : { launchLocations }),
        ...playFactsForEntry(entry),
      }
    }
    case "game": {
      const alternative = alternativeLocation(entry)
      const launchLocations = launchLocationsForEntry(entry)
      const location = entry.game.source.isLocal
        ? SECTION_THIS_DEVICE
        : entry.game.host
      const subtitle = [location, alternative].filter(Boolean).join(" · ")
      return {
        id: entryKey(entry),
        title: entry.game.title,
        section: location ?? "Other devices",
        ...(subtitle.length === 0 ? {} : { subtitle }),
        ...(launchLocations.length < 2 ? {} : { launchLocations }),
        ...playFactsForEntry(entry),
      }
    }
    default:
      return null
  }
}

/**
 * The running session is the one game Korri can currently act on beyond
 * launching, so it is the only entry that carries a command sheet today.
 */
export function gameActionsForEntry(
  entry: PortalEntry | undefined,
): readonly SurfaceAction[] {
  if (entry?.kind !== "now-playing") return []
  return [
    { id: "resume", label: "Continue playing", enabled: true },
    { id: "stop", label: "Stop", enabled: true, destructive: true },
  ]
}

/** Surface status names an actual presented entry, never a raw Rust game id. */
function statusGameId(state: LaunchablesState, subject: LaunchSubject | undefined): string | undefined {
  if (state._tag === "Loading" || subject === undefined) return undefined
  if (subject.entryId !== undefined) {
    return state.entries.some(entry => entryKey(entry) === subject.entryId) ? subject.entryId : undefined
  }
  if (subject.id === undefined) return undefined
  const local = state._tag === "Starting" || (state._tag === "Launching" && state.returnLaunchId !== undefined)
  const matches = (copy: PortalGameCopy) => copy.game.id === subject.id && (!local || (copy.kind === "remote" && copy.game.source.isLocal))
  const games = state.entries.filter(entry => (entry.kind === "game" || entry.kind === "local-game") &&
    orderedCopiesForEntry(entry).some(matches))
  if (games.length === 1) return entryKey(games[0]!)
  if (games.length > 1) return undefined
  const launchId = state._tag === "Starting" ? state.launchId : state._tag === "Launching" ? state.returnLaunchId : undefined
  const live = state.entries.find(entry => entry.kind === "now-playing" && entry.session.launchId === launchId && entry.session.gameId === subject.id)
  return live ? entryKey(live) : undefined
}

function statusFrom(state: LaunchablesState): SurfaceStatus {
  const subject = "subject" in state ? state.subject
    : state._tag === "Ready" && state.notice?._tag === "Launch" ? state.notice.subject : undefined
  const gameId = statusGameId(state, subject)
  switch (state._tag) {
    case "Loading":
      return { _tag: "Browsing" }
    case "Preparing":
      return {
        _tag: "Busy",
        kicker: `Preparing ${state.title}…`,
        detail: "Opening your session",
        ...(gameId === undefined ? {} : { gameId }),
      }
    case "Launching":
      return {
        _tag: "Busy",
        kicker: state.returnLaunchId === undefined ? `Starting ${state.title}…` : `Returning to ${state.title}…`,
        detail: "Opening your session",
        ...(gameId === undefined ? {} : { gameId }),
      }
    case "Choosing":
      return {
        _tag: "Busy", kicker: `${state.choices.length} launches are starting`,
        detail: state.notice?.message ?? "Cancel each launch you do not want.",
        actions: state.choices.map((choice, index) => {
          const sameTitle = state.choices.filter(candidate => candidate.title === choice.title)
          const number = state.choices.slice(0, index + 1).filter(candidate => candidate.title === choice.title).length
          return {
            id: `cancel-pending:${choice.launchId}`,
            label: `Cancel ${choice.title}${sameTitle.length > 1 ? ` (${number})` : ""}`,
            description: choice.cancel === "retry" ? "Cancel failed. Try again." : {
              reserved: "Waiting to start", preparing: "Preparing", committing: "Starting",
              waiting: "Waiting for its window", cancelling: "Cancelling",
            }[choice.phase],
            enabled: choice.cancel === "retry" || (choice.cancel === "idle" && choice.phase !== "cancelling"),
          }
        }),
      }
    case "Starting":
      return {
        _tag: "Busy",
        kicker: state.cancelling ? "Cancelling launch…" : `Starting ${state.title}…`,
        detail: state.notice?.message ?? (state.cancelling ? "Waiting for the exact launch to end" : "Opening your session"),
        ...(gameId === undefined ? {} : { gameId }),
        actions: [{ id: "cancel-launch", label: state.cancelling && state.cancelRetryAvailable ? "Retry Cancel" : "Cancel",
          enabled: !state.cancelling || state.cancelRetryAvailable === true }],
      }
    case "Stopping":
      return {
        _tag: "Busy",
        kicker: "Stopping session…",
        detail: "Waiting for the host to finish",
      }
    case "Recovery":
      return { _tag: "Problem", kicker: "Launch recovery needed", reason: state.notice.message, canRetry: true }
    case "Ready": {
      const observation = state.unavailableSessionStatus
      const failure = observation?._tag === "Err" ? observation.payload : observation?.payload.observationFailure
      if (failure) return {
        _tag: "Problem", kicker: "Live session recovery needed",
        reason: [state.notice?.message, `${failure.code}: ${failure.message}`].filter(Boolean).join(" · "), canRetry: true,
      }
      return state.notice === null ? { _tag: "Browsing" } : problemFromNotice(state.notice, gameId)
    }
  }
}

/** `gameId` is the presented entry for the notice's subject, never a raw Rust id. */
function problemFromNotice(notice: LaunchNotice, gameId: string | undefined): SurfaceStatus {
  // Nothing about the failure changed, so an immediate second attempt would
  // fail identically; the user acknowledges instead.
  const problem = { _tag: "Problem", reason: notice.message, canRetry: false } as const
  switch (notice._tag) {
    case "Catalog":
      return { ...problem, kicker: "Catalog problem" }
    case "Stop":
      return { ...problem, kicker: "Couldn't end the session" }
    case "Action":
      return { ...problem, kicker: "Operation failed" }
    case "Launch":
      // A failure that knows its game names that game, so the surface can
      // never attribute it to whatever is currently in view.
      return {
        ...problem,
        kicker: notice.subject ? `Couldn't start ${notice.subject.title}` : "Couldn't start",
        ...(notice.subject
          ? { ...(gameId === undefined ? {} : { gameId }), gameTitle: notice.subject.title }
          : {}),
      }
  }
}

function catalogFrom(state: LaunchablesState): SurfaceCatalog {
  if (state._tag === "Loading") return { _tag: "Loading" }
  const games = state.entries
    .map(entry => gameFromEntry(entry, state.entries))
    .filter((game): game is SurfaceGame => game !== null)
  return games.length === 0
    ? { _tag: "Empty" }
    : { _tag: "Ready", games }
}

export function surfaceModelFrom(
  state: LaunchablesState,
  options: {
    readonly clockLabel?: string
    readonly buildLabel?: string
    readonly settings?: readonly SurfaceSettingGroup[]
    readonly settingsStatus?: SurfaceSettingsStatus
    readonly identityManagement?: SurfaceIdentityManagement
  } = {},
): SurfaceModel {
  const actions =
    state._tag === "Loading"
      ? []
      : state.entries
          .map(actionFromEntry)
          .filter((action): action is SurfaceAction => action !== null)

  return {
    presentation: { kind: "catalog" },
    catalog: catalogFrom(state),
    status: statusFrom(state),
    actions,
    settings: options.settings ?? [],
    settingsStatus: options.settingsStatus ?? { _tag: "Idle" },
    ...(options.identityManagement === undefined
      ? {}
      : { identityManagement: options.identityManagement }),
    ...(options.clockLabel === undefined
      ? {}
      : { clockLabel: options.clockLabel }),
    ...(options.buildLabel === undefined
      ? {}
      : { buildLabel: options.buildLabel }),
  }
}

/** Find the entry a surface id refers to. Surface ids are entry keys. */
export function entryForId(
  state: LaunchablesState,
  id: string,
): PortalEntry | undefined {
  if (state._tag === "Loading") return undefined
  return state.entries.find(entry => entryKey(entry) === id)
}
