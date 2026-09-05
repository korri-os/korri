/**
 * Korri surface treaty — the boundary between Korri and a presentation surface.
 *
 * A surface is a self-contained front end (Shift is the first). It renders what
 * Korri can currently do and asks Korri to do things; it never reaches into
 * korrid, the Android bridge, portal state, or any Rust-generated type. That
 * one-way dependency is what lets a surface live in its own repository — or
 * eventually ship as an installable theme — without dragging Korri with it.
 *
 * Rules this file exists to hold:
 *   - Korri hands the surface a MODEL (what is true right now) and a HOST
 *     (what the surface may ask for). Nothing else crosses.
 *   - The model carries only facts Korri can actually produce today. A surface
 *     may render less than the model offers, never more than Korri knows.
 *   - Absent data is `undefined`, never invented. A surface decides how to
 *     present the gap (Shift draws a title monogram in place of cover art).
 *   - Commands are fire-and-forget. Korri publishes the consequence as a new
 *     model; the surface never awaits a result or holds Korri's state.
 *
 * This file imports nothing. It is the whole contract.
 */

/**
 * Semantic input a surface may consume directly. Directional movement and
 * confirmation are deliberately absent: those are DOM focus and clicks, which
 * the host's input system already drives, so a surface never re-implements
 * navigation or learns which physical device produced an action.
 */
export type SurfaceInputAction = "back" | "options" | "menu" | "system"

export interface SurfaceInput {
  /** Subscribe to a semantic action. Returns an unsubscribe function. */
  on(action: SurfaceInputAction, handler: () => void): () => void
}

/**
 * One playable thing. `id` is opaque to the surface — it is handed back
 * verbatim when asking Korri to launch or act on it.
 */
export interface SurfaceLaunchLocation {
  /** Opaque copy id. A surface returns it unchanged when the user chooses. */
  readonly id: string
  /** Human-readable host name, such as "This device" or "zao". */
  readonly label: string
}

export interface SurfaceGame {
  readonly id: string
  readonly title: string
  /**
   * Places that can launch this game. Present only when there is a real choice;
   * surfaces must ask the user rather than silently selecting or falling back.
   * Korri orders the local device first, followed by stable remote host order.
   */
  readonly launchLocations?: readonly SurfaceLaunchLocation[]
  /**
   * Grouping caption ("Continue", "This device", a peer device's name). Games
   * sharing a section arrive consecutively; the surface groups on change.
   */
  readonly section?: string
  /** Short provenance line, e.g. "GBA · This device". */
  readonly subtitle?: string
  /** Cover art. Absent when Korri has none — surfaces must have a fallback. */
  readonly coverArtUrl?: string
  /** Wide/background art. Absent when Korri has none. */
  readonly wideArtUrl?: string
  /** True when confirming continues an existing session rather than starting. */
  readonly resumable?: boolean
  /**
   * Newest recorded session end, as UTC epoch milliseconds. Absent when Korri
   * has never recorded a session for this game. Surfaces format it; Korri
   * never ships a label.
   */
  readonly lastPlayedAt?: number
  /** Recorded sessions. Absent together with lastPlayedAt. */
  readonly playCount?: number
  /** Summed recorded session length. Absent together with lastPlayedAt. */
  readonly totalPlaytimeSeconds?: number
}

/**
 * A non-game thing the user can do (pair a device, grant storage access, stop
 * the running game). Korri owns which of these exist and whether they apply;
 * the surface only renders and reports selection.
 */
export interface SurfaceAction {
  readonly id: string
  readonly label: string
  /** One-line explanation. Surfaces may show it or not. */
  readonly description?: string
  /** Present but inert. Surfaces render it dimmed rather than hiding it. */
  readonly enabled: boolean
  /** Destructive/irreversible. Surfaces may mark it. */
  readonly destructive?: boolean
}

/** One value offered by a choice setting. Values are opaque to the surface. */
export interface SurfaceSettingChoice {
  readonly value: string
  readonly label: string
}

/** How a user may interact with a setting. Absent means read-only. */
export type SurfaceSettingInteraction =
  | {
      readonly kind: "action"
      /** Setting-local command id. Pass to `SurfaceHost.runAction` unchanged. */
      readonly actionId: string
      /** Destructive actions should be confirmed by the surface before firing. */
      readonly destructive?: boolean
      readonly confirmation?: {
        readonly title: string
        readonly message: string
        readonly confirmLabel: string
      }
    }
  | {
      readonly kind: "choice"
      readonly choices: readonly SurfaceSettingChoice[]
    }
  | {
      readonly kind: "text"
      readonly placeholder?: string
      readonly maxLength?: number
    }
  | {
      readonly kind: "sensitiveText"
      readonly placeholder?: string
      readonly maxLength?: number
      readonly clearLabel?: string
    }

/** One device fact or setting. Korri owns truth and allowed interactions. */
export interface SurfaceSettingItem {
  readonly id: string
  readonly label: string
  /** Current state as display text: "usu", "Granted", "2 devices". */
  readonly value?: string
  /** One-line explanation. Surfaces may show it or not. */
  readonly description?: string
  /** Absent means this row is genuinely read-only. */
  readonly interaction?: SurfaceSettingInteraction
}

/** A titled run of settings items. Korri omits groups it has nothing to say
 * about, so a surface never renders an empty heading. */
export interface SurfaceSettingGroup {
  readonly title: string
  readonly items: readonly SurfaceSettingItem[]
}

export type SurfaceSettingsStatus =
  | { readonly _tag: "Idle" }
  | { readonly _tag: "Saving"; readonly settingId: string }
  | {
      readonly _tag: "Problem"
      readonly settingId: string
      readonly message: string
    }

/** What Korri currently knows about the things that can be played. */
export type SurfaceCatalog =
  | { readonly _tag: "Loading" }
  | { readonly _tag: "Error"; readonly message: string }
  | { readonly _tag: "Empty" }
  | { readonly _tag: "Ready"; readonly games: readonly SurfaceGame[] }

/**
 * Where the current launch stands. `Busy` and `Running` are informational;
 * `Problem` is the only state that offers the user a decision.
 */
export type SurfaceStatus =
  /** Nothing is happening. The surface shows its normal browsing presentation. */
  | { readonly _tag: "Browsing" }
  /** Work is under way. `kicker` is a short, calm headline ("Starting…"). */
  | {
      readonly _tag: "Busy"
      readonly kicker: string
      /** Optional progress detail. Never an error code or raw output. */
      readonly detail?: string
      /** The game this work belongs to, when it belongs to exactly one. */
      readonly gameId?: string
    }
  /** A launch is live. `gameId` is present when Korri knows which one. */
  | {
      readonly _tag: "Running"
      readonly kicker: string
      readonly gameId?: string
    }
  /**
   * Something did not work, stated in plain language. `reason` is already
   * user-facing copy — a surface must not attempt to interpret failure codes.
   */
  | {
      readonly _tag: "Problem"
      readonly kicker: string
      readonly reason: string
      readonly canRetry: boolean
      /**
       * The game the failure belongs to, when it belongs to exactly one. A
       * surface must state the problem against this game rather than whatever
       * it happens to be showing.
       */
      readonly gameId?: string
      /** That game's title, so the surface can name it without a lookup. */
      readonly gameTitle?: string
    }

/** Presentation-safe gameplay interaction. Integration effects and protected
 * platform instructions deliberately have no representation in this treaty. */
export type SurfaceGameplayControlInteraction =
  | { readonly kind: "command" }
  | {
      readonly kind: "toggle"
      readonly value: boolean
      readonly trueLabel: string
      readonly falseLabel: string
    }
  | {
      readonly kind: "choice"
      readonly value: string
      readonly options: readonly SurfaceSettingChoice[]
    }
  | {
      readonly kind: "range"
      readonly value: number
      readonly min: number
      readonly max: number
      readonly step: number
    }

export interface SurfaceGameplayControl {
  /** Opaque within the current gameplay-overlay model. */
  readonly id: string
  readonly label: string
  readonly description?: string
  readonly enabled: boolean
  readonly disabledReason?: string
  readonly destructive: boolean
  readonly dismissOnSuccess: boolean
  readonly interaction: SurfaceGameplayControlInteraction
}

/** A plugin-owned run of controls. Korri omits empty groups. */
export interface SurfaceGameplayControlGroup {
  readonly id: string
  readonly label: string
  readonly controls: readonly SurfaceGameplayControl[]
}

export interface SurfaceGameplayOverlayPresentation {
  readonly kind: "gameplay-overlay"
  readonly title?: string
  /** Overlay-owned controls such as Resume, always before plugin groups. */
  readonly controls: readonly SurfaceGameplayControl[]
  readonly groups: readonly SurfaceGameplayControlGroup[]
}

export type SurfacePresentation =
  | { readonly kind: "catalog" }
  | SurfaceGameplayOverlayPresentation

export type SurfaceGameplayControlValue =
  | { readonly kind: "toggle"; readonly value: boolean }
  | { readonly kind: "choice"; readonly value: string }
  | { readonly kind: "range"; readonly value: number }

/** Everything Korri publishes to the surface. Replaced wholesale on change. */
export interface SurfaceModel {
  readonly presentation: SurfacePresentation
  readonly catalog: SurfaceCatalog
  readonly status: SurfaceStatus
  /** Preformatted local time. Absent when the surface should show no clock. */
  readonly clockLabel?: string
  /** Device-level actions for permissions and session control. May be empty. */
  readonly actions: readonly SurfaceAction[]
  /** Device facts and settings, grouped. Empty when Korri can state nothing. */
  readonly settings: readonly SurfaceSettingGroup[]
  readonly settingsStatus: SurfaceSettingsStatus
  /** Free-form build/identity stamp a surface may display. */
  readonly buildLabel?: string
}

/** Everything the surface may ask Korri to do. */
export interface SurfaceHost {
  readonly input: SurfaceInput
  /**
   * Start or resume the game. A location id is required when the corresponding
   * SurfaceGame publishes launchLocations; otherwise it is absent.
   */
  launchGame(gameId: string, launchLocationId?: string): void
  /** Run an action id Korri published, either device-level or setting-local. */
  runAction(actionId: string): void
  /** Change an editable setting. Korri validates and republishes the result. */
  changeSetting(settingId: string, value: string): void
  /** Dismiss the current settings failure without changing its value. */
  dismissSettingsProblem(): void
  /** Actions available for one game. Empty when Korri supports none yet. */
  gameActions(gameId: string): readonly SurfaceAction[]
  runGameAction(gameId: string, actionId: string): void
  /** Invoke a control from the current gameplay-overlay model. The host binds
   * this opaque request to its current launch identity before calling korrid. */
  invokeGameplayControl(
    controlId: string,
    value?: SurfaceGameplayControlValue,
  ): void
  /** Close the gameplay overlay locally; no brain call is required. */
  dismissGameplayOverlay(): void
  /** Try the failed thing again. Only meaningful while `Problem.canRetry`. */
  retry(): void
  /** Acknowledge a `Problem` and return to browsing. */
  dismiss(): void
  /** Ask Korri to re-read everything (used by error recovery). */
  reload(): void
}

/**
 * A mounted surface. Korri pushes new models in and tears the surface down
 * through the same handle, so hosting a surface never requires knowing which
 * UI framework it was built with.
 */
export interface SurfaceInstance {
  update(model: SurfaceModel): void
  unmount(): void
}

/** The single entry point a surface package must export. */
export interface KorriSurface {
  readonly id: string
  readonly title: string
  mount(
    container: HTMLElement,
    model: SurfaceModel,
    host: SurfaceHost,
  ): SurfaceInstance
}
