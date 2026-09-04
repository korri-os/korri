/**
 * A host made of nothing.
 *
 * Pico must be developable and testable with no Korri running, so this
 * implements the whole treaty in memory and records what was asked for. If Pico
 * ever needs something this cannot provide, that is the signal the treaty — not
 * the fixture — has to change.
 */
import type {
  SurfaceAction,
  SurfaceHost,
  SurfaceInputAction,
  SurfaceModel,
} from "@contracts/surface/korri-surface"

export interface FixtureHost extends SurfaceHost {
  /** Every command the surface issued, in order. */
  readonly calls: readonly string[]
  /** Deliver a semantic action as the real host's input system would. */
  press(action: SurfaceInputAction): void
}

export function createFixtureHost(): FixtureHost {
  const calls: string[] = []
  const listeners = new Map<SurfaceInputAction, Set<() => void>>()

  return {
    calls,
    press(action) {
      for (const listener of [...(listeners.get(action) ?? [])]) listener()
    },
    input: {
      on(action, handler) {
        const set = listeners.get(action) ?? new Set()
        set.add(handler)
        listeners.set(action, set)
        return () => set.delete(handler)
      },
    },
    launchGame(gameId, launchLocationId) {
      calls.push(
        launchLocationId === undefined
          ? `launch:${gameId}`
          : `launch:${gameId}:${launchLocationId}`,
      )
    },
    runAction(actionId) {
      calls.push(`action:${actionId}`)
    },
    changeSetting(settingId, value) {
      calls.push(`setting:${settingId}:${value}`)
    },
    dismissSettingsProblem() {
      calls.push("dismissSettingsProblem")
    },
    gameActions(): readonly SurfaceAction[] {
      return []
    },
    runGameAction(gameId, actionId) {
      calls.push(`gameAction:${gameId}:${actionId}`)
    },
    invokeGameplayControl(controlId) {
      calls.push(`gameplayControl:${controlId}`)
    },
    dismissGameplayOverlay() {
      calls.push("dismissGameplayOverlay")
    },
    retry() {
      calls.push("retry")
    },
    dismiss() {
      calls.push("dismiss")
    },
    reload() {
      calls.push("reload")
    },
  }
}

/**
 * A representative library: one game with art, one that resumes, one with a
 * real launch choice, and one with no art at all — the four shapes the home
 * screen has to hold at once, so a preview shows the awkward cases rather than
 * a tidy row that proves nothing.
 */
export const fixtureModel: SurfaceModel = {
  presentation: { kind: "catalog" },
  catalog: {
    _tag: "Ready",
    games: [
      {
        id: "celeste",
        title: "Celeste Classic",
        subtitle: "PICO-8 · This device",
      },
      {
        id: "hollow",
        title: "Hollow Knight",
        subtitle: "Switch · This device",
        resumable: true,
      },
      {
        id: "tetris",
        title: "Tetris",
        subtitle: "GB · zao",
        launchLocations: [
          { id: "local", label: "This device" },
          { id: "zao", label: "zao" },
        ],
      },
      {
        id: "spelunky",
        title: "Spelunky",
        subtitle: "PC · This device",
      },
    ],
  },
  status: { _tag: "Browsing" },
  clockLabel: "10:24",
  actions: [],
  settings: [],
  settingsStatus: { _tag: "Idle" },
  buildLabel: "pico-dev",
}
