/**
 * Surface-side presentation of the launch lifecycle for the cinematic home.
 *
 * The host hands the surface a `SurfaceStatus` — already reduced to calm,
 * user-facing copy — and this maps it to the hero's tone plus the legend
 * affordances the scene should show. Shift deliberately cannot see failure
 * codes, exit statuses, or stderr: interpreting failure is the host's job, and
 * a surface that cannot reach that detail cannot leak it.
 */
import type { SurfaceStatus } from "@contracts/surface/korri-surface"

export type LaunchStatusTone =
  | "launching"
  | "preparing"
  | "launched"
  | "cooling"
  | "recovering"
  | "failed"
  | "unavailable"

/**
 * In-progress tones (the launch is still resolving) should show a loading
 * indicator; the terminal tones (launched / failed / unavailable) should not.
 */
export function isLaunchInProgress(tone: LaunchStatusTone): boolean {
  return (
    tone === "launching" ||
    tone === "preparing" ||
    tone === "cooling" ||
    tone === "recovering"
  )
}

export interface LaunchStatusView {
  readonly tone: LaunchStatusTone
  /** Short, calm headline shown in place of the hero kicker. */
  readonly kicker: string
  /** One glanceable reason chip (failure/progress detail only). */
  readonly reason?: string
  /** Whether the scene should offer "A = Retry". */
  readonly canRetry: boolean
  /**
   * The game this status belongs to, when Korri states one. The scene must
   * show the status against this game rather than whatever is focused.
   */
  readonly gameId?: string
}

/**
 * Derive the in-scene launch view from the host's status. Returns null while
 * browsing, so the scene renders its normal hero.
 */
export function launchStatusView(
  status: SurfaceStatus | undefined,
): LaunchStatusView | null {
  switch (status?._tag) {
    case "Busy":
      return {
        tone: "launching",
        kicker: status.kicker,
        ...(status.detail ? { reason: status.detail } : {}),
        canRetry: false,
        ...(status.gameId ? { gameId: status.gameId } : {}),
      }
    // A question before Korri acts reads like work about to start; its
    // answers are Korri's actions on the rail.
    case "Confirm":
      return {
        tone: "launching",
        kicker: status.kicker,
        ...(status.detail ? { reason: status.detail } : {}),
        canRetry: false,
        ...(status.gameId ? { gameId: status.gameId } : {}),
      }
    case "Running":
      return { tone: "launched", kicker: status.kicker, canRetry: false }
    case "Problem":
      return {
        tone: "failed",
        kicker: status.kicker,
        reason: status.reason,
        canRetry: status.canRetry,
        ...(status.gameId ? { gameId: status.gameId } : {}),
      }
    default:
      return null
  }
}
