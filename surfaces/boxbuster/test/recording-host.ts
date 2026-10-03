/**
 * A whole treaty host in memory that records what the surface asked for.
 * Boxbuster must be testable with no Korri running.
 */
import type { SurfaceHost } from "@contracts/surface/korri-surface"

export interface RecordingHost extends SurfaceHost {
  /** Every command the surface issued, in order. */
  readonly calls: readonly string[]
}

export function createRecordingHost(): RecordingHost {
  const calls: string[] = []
  const record = (call: string) => () => {
    calls.push(call)
  }
  return {
    calls,
    input: { on: () => () => {} },
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
