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
  type LaunchablesState,
  type PortalEntry,
} from "../launchables/state"

/** Section captions. Grouping is Korri's call; the surface only renders it. */
const SECTION_CONTINUE = "Continue"
const SECTION_THIS_DEVICE = "This device"

/** Non-game entries become rail actions rather than games. */
export function isActionEntry(entry: PortalEntry): boolean {
  return entry.kind === "storage-access" || entry.kind === "background-notice"
}

function actionFromEntry(entry: PortalEntry): SurfaceAction | null {
  switch (entry.kind) {
    case "storage-access":
      return {
        id: entryKey(entry),
        label: "Allow file access",
        description:
          "Korri keeps your settings and games in a folder you can open.",
        enabled: true,
      }
    case "background-notice":
      return {
        id: entryKey(entry),
        label: entry.visible ? "Hide background notice" : "Show Korri running",
        description: entry.visible
          ? "Stop showing the notice that Korri is running."
          : "Show a notice while Korri keeps running in the background.",
        enabled: true,
      }
    default:
      return null
  }
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

function copyHostKey(copy: PortalGameCopy): string {
  return copy.kind === "local" ? "local" : `remote:${copy.game.host ?? ""}`
}

function copyLocationLabel(copy: PortalGameCopy): string {
  return copy.kind === "local"
    ? "This device"
    : (copy.game.host ?? "Other device")
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
  const primaryKey =
    entry.kind === "local-game"
      ? "local"
      : `remote:${entry.game.host ?? ""}`
  const visible = distinctHostCopies(entry)
    .filter(copy => copyHostKey(copy) !== primaryKey)
    .map(copy =>
      copy.kind === "local" ? "this device" : copyLocationLabel(copy),
    )
  return visible.length === 0 ? undefined : `Also on ${visible.join(", ")}`
}

function gameFromEntry(entry: PortalEntry): SurfaceGame | null {
  switch (entry.kind) {
    case "now-playing":
      return {
        id: entryKey(entry),
        title:
          entry.session.title ?? entry.session.gameId ?? "Current session",
        section: SECTION_CONTINUE,
        subtitle: entry.session.host ?? "Running now",
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
      const subtitle = [entry.game.host, alternative].filter(Boolean).join(" · ")
      return {
        id: entryKey(entry),
        title: entry.game.title,
        section: entry.game.host ?? "Other devices",
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

function statusFrom(state: LaunchablesState): SurfaceStatus {
  switch (state._tag) {
    case "Loading":
      return { _tag: "Browsing" }
    case "Preparing":
      return {
        _tag: "Busy",
        kicker: `Preparing ${state.title}…`,
        detail: "Your stream will start in a moment",
        ...(state.subject ? { gameId: state.subject.id } : {}),
      }
    case "Launching":
      return {
        _tag: "Busy",
        kicker: `Starting ${state.title}…`,
        detail: "Opening your session",
        ...(state.subject ? { gameId: state.subject.id } : {}),
      }
    case "Stopping":
      return {
        _tag: "Busy",
        kicker: "Stopping session…",
        detail: "Waiting for the host to finish",
      }
    case "Ready":
      return state.notice === null
        ? { _tag: "Browsing" }
        : {
            _tag: "Problem",
            // A failure that knows its game names that game, so the surface
            // can never attribute it to whatever is currently in view.
            kicker: state.notice.subject
              ? `Couldn't start ${state.notice.subject.title}`
              : "Couldn't start",
            reason: state.notice.message,
            // Nothing about the failure changed, so an immediate second
            // attempt would fail identically; the user acknowledges instead.
            canRetry: false,
            ...(state.notice.subject
              ? {
                  gameId: state.notice.subject.id,
                  gameTitle: state.notice.subject.title,
                }
              : {}),
          }
  }
}

function catalogFrom(state: LaunchablesState): SurfaceCatalog {
  if (state._tag === "Loading") return { _tag: "Loading" }
  const games = state.entries
    .map(gameFromEntry)
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
