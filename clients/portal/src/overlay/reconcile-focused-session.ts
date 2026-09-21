import type { ActiveSession } from "@contracts/generated/korrid"
import type { KorridClient } from "../korrid/client"

export type FocusedSessionReconciliation =
  | "shown"
  | "not-active"
  | "unavailable"
  | "superseded"

export interface InspectionGeneration {
  begin(): () => boolean
  invalidate(): void
}

export function createInspectionGeneration(): InspectionGeneration {
  let generation = 0
  return {
    begin() {
      const inspection = ++generation
      return () => inspection === generation
    },
    invalidate() {
      generation += 1
    },
  }
}

export async function reconcileFocusedSession({
  korrid,
  allowFrozen = true,
  allowFocusFailed = true,
  expectedLaunchId,
  onActive,
  attempts = 4,
  retryDelayMs = 75,
  sleep = delay,
  isCurrent = () => true,
}: {
  readonly korrid: KorridClient
  readonly allowFrozen?: boolean
  readonly allowFocusFailed?: boolean
  readonly expectedLaunchId?: string
  readonly onActive: (active: ActiveSession) => void
  readonly attempts?: number
  readonly retryDelayMs?: number
  readonly sleep?: (milliseconds: number) => Promise<void>
  readonly isCurrent?: () => boolean
}): Promise<FocusedSessionReconciliation> {
  const boundedAttempts = Math.max(1, attempts)
  for (let attempt = 0; attempt < boundedAttempts; attempt += 1) {
    if (!isCurrent()) return "superseded"
    const status = await korrid.sessionStatus(2_000)
    if (!isCurrent()) return "superseded"
    if (status._tag === "Ok") {
      const active = status.payload.overlay
      if (
        active !== undefined &&
        (expectedLaunchId === undefined || active.launchId === expectedLaunchId) &&
        ((allowFrozen && active.phase === "frozen") ||
          (allowFocusFailed && active.phase === "focus-failed"))
      ) {
        onActive(active)
        return "shown"
      }
      return "not-active"
    }
    if (attempt + 1 < boundedAttempts) {
      await sleep(retryDelayMs)
      if (!isCurrent()) return "superseded"
    }
  }
  return isCurrent() ? "unavailable" : "superseded"
}

function delay(milliseconds: number): Promise<void> {
  return new Promise(resolve => window.setTimeout(resolve, milliseconds))
}
