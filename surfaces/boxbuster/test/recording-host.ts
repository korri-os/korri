/**
 * A whole treaty host in memory that records what the surface asked for.
 * Boxbuster must be testable with no Korri running.
 */
import type {
  SurfaceHost,
  SurfaceInputAction,
} from "@contracts/surface/korri-surface"

export interface RecordingHost extends SurfaceHost {
  /** Every command the surface issued, in order. */
  readonly calls: readonly string[]
  /** Deliver a semantic action the way the portal's input system does. */
  press(action: SurfaceInputAction): void
}

export function createRecordingHost(): RecordingHost {
  const calls: string[] = []
  const handlers = new Map<SurfaceInputAction, Set<() => void>>()
  const record = (call: string) => () => {
    calls.push(call)
  }
  return {
    calls,
    press(action) {
      for (const handler of handlers.get(action) ?? []) handler()
    },
    input: {
      on(action, handler) {
        const set = handlers.get(action) ?? new Set()
        set.add(handler)
        handlers.set(action, set)
        return () => {
          set.delete(handler)
        }
      },
    },
    launchGame: (gameId, launchLocationId) =>
      calls.push(`launch:${gameId}:${launchLocationId ?? ""}`),
    runAction: actionId => calls.push(`action:${actionId}`),
    changeSetting: settingId => calls.push(`setting:${settingId}`),
    dismissSettingsProblem: record("dismissSettingsProblem"),
    exportIdentityBackup: record("exportIdentityBackup"),
    switchIdentityFromBackup: record("switchIdentityFromBackup"),
    switchIdentityToNip46: record("switchIdentityToNip46"),
    deleteRetiredIdentity: record("deleteRetiredIdentity"),
    dismissIdentityStatus: record("dismissIdentityStatus"),
    gameActions: () => [],
    runGameAction: (gameId, actionId) =>
      calls.push(`gameAction:${gameId}:${actionId}`),
    invokeGameplayControl: controlId => calls.push(`control:${controlId}`),
    dismissGameplayOverlay: record("dismissGameplayOverlay"),
    retry: record("retry"),
    dismiss: record("dismiss"),
    reload: record("reload"),
  }
}
