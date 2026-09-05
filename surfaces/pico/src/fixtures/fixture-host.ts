/**
 * A host made of nothing.
 *
 * Pico must be developable and testable with no Korri running, so this
 * implements the whole treaty in memory and records what was asked for. If Pico
 * ever needs something this cannot provide, that is the signal the treaty — not
 * the fixture — has to change.
 */
import type {
  SurfaceGameplayOverlayPresentation,
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
    invokeGameplayControl(controlId, value) {
      calls.push(
        value === undefined
          ? `gameplayControl:${controlId}`
          : `gameplayControl:${controlId}:${value.kind}:${String(value.value)}`,
      )
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
/**
 * A gameplay overlay as korrid publishes one: Korri's own Resume first, then a
 * plugin's group carrying every interaction kind the treaty allows, one of them
 * disabled with a reason, one destructive. Everything the overlay must draw.
 */
export const fixtureOverlay: SurfaceGameplayOverlayPresentation = {
  kind: "gameplay-overlay",
  title: "Hollow Knight",
  controls: [
    {
      id: "resume",
      label: "Continue playing",
      enabled: true,
      destructive: false,
      dismissOnSuccess: true,
      interaction: { kind: "command" },
    },
    {
      id: "quit",
      label: "Quit game",
      description: "Unsaved progress is lost.",
      enabled: true,
      destructive: true,
      dismissOnSuccess: true,
      interaction: { kind: "command" },
    },
  ],
  groups: [
    {
      id: "mgba",
      label: "mGBA",
      controls: [
        {
          id: "save",
          label: "Save state",
          enabled: true,
          destructive: false,
          dismissOnSuccess: false,
          interaction: { kind: "command" },
        },
        {
          id: "load",
          label: "Load state",
          enabled: false,
          disabledReason: "No save yet",
          destructive: false,
          dismissOnSuccess: true,
          interaction: { kind: "command" },
        },
        {
          id: "ff",
          label: "Fast forward",
          enabled: true,
          destructive: false,
          dismissOnSuccess: false,
          interaction: { kind: "toggle", value: false, trueLabel: "On", falseLabel: "Off" },
        },
        {
          id: "shader",
          label: "Shader",
          enabled: true,
          destructive: false,
          dismissOnSuccess: false,
          interaction: {
            kind: "choice",
            value: "none",
            options: [
              { value: "none", label: "None" },
              { value: "crt", label: "CRT" },
              { value: "lcd", label: "LCD" },
            ],
          },
        },
        {
          id: "vol",
          label: "Volume",
          enabled: true,
          destructive: false,
          dismissOnSuccess: false,
          interaction: { kind: "range", value: 80, min: 0, max: 100, step: 10 },
        },
      ],
    },
  ],
}

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
        playCount: 3,
        totalPlaytimeSeconds: 7_800,
        lastPlayedAt: 1_757_000_000_000,
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
  /* Real shapes from korrid: a fact, an editable name, a two-value choice,
   * a described fact, and an action a permission is granted through. */
  settings: [
    {
      title: "Device",
      items: [
        {
          id: "device-name",
          label: "Name",
          value: "usu",
          interaction: { kind: "text", maxLength: 64 },
        },
        { id: "software", label: "Software", value: "korrid 0.4.1" },
      ],
    },
    {
      title: "Plugins",
      items: [
        {
          id: "@korri:mgba",
          label: "mGBA",
          value: "On",
          interaction: {
            kind: "choice",
            choices: [
              { value: "true", label: "On" },
              { value: "false", label: "Off" },
            ],
          },
        },
      ],
    },
    {
      title: "Permissions",
      items: [
        {
          id: "file-access",
          label: "File access",
          value: "Granted",
          description: "Managed by Android",
          interaction: { kind: "action", actionId: "storage-access" },
        },
        {
          id: "reset",
          label: "Forget this device",
          interaction: {
            kind: "action",
            actionId: "factory-reset",
            destructive: true,
            confirmation: {
              title: "FORGET EVERYTHING?",
              message: "Every game, save and setting on this device is removed.",
              confirmLabel: "FORGET",
            },
          },
        },
      ],
    },
  ],
  settingsStatus: { _tag: "Idle" },
  buildLabel: "pico-dev",
}
