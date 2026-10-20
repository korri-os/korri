/**
 * Korri surface treaty — the boundary between Korri and a presentation surface.
 *
 * A surface is a self-contained front end (Shift is the first). It renders what
 * Korri can currently do and asks Korri to do things; it never reaches into
 * korrid, the Linux shell, portal state, or any Rust-generated type. That
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
  /**
   * Present when this game needs anything downloaded to this device: its own
   * files, or a runtime it depends on. `bytes` is the total still to fetch;
   * absent when Korri does not know the size. `queued` while the download
   * waits behind another one. `progress` while it downloads, in the shape of
   * `Busy.progress`: no `total` means no known end. Surfaces format all of it;
   * Korri never ships a label. Absent when nothing is needed.
   */
  readonly download?: {
    readonly bytes?: number
    readonly queued?: true
    readonly progress?: { readonly done: number; readonly total?: number }
  }
}

/**
 * A non-game thing the user can do (open settings, cancel a launch, stop
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

export type SurfaceIdentityDisposition = "transfer" | "delete"

export type SurfaceIdentityStatus =
  | { readonly _tag: "Idle" }
  | { readonly _tag: "Working"; readonly operation: string }
  | { readonly _tag: "BackupReady"; readonly encryptedSecret: string }
  | { readonly _tag: "Problem"; readonly message: string }
  | { readonly _tag: "Switched"; readonly ownerPublicKey: string }

export interface SurfaceIdentityManagement {
  readonly localBackupAvailable: boolean
  readonly retiredPublicKeys: readonly string[]
  readonly status: SurfaceIdentityStatus
}

/** What Korri currently knows about the things that can be played. */
export type SurfaceCatalog =
  | { readonly _tag: "Loading" }
  | { readonly _tag: "Error"; readonly message: string }
  | { readonly _tag: "Empty" }
  | { readonly _tag: "Ready"; readonly games: readonly SurfaceGame[] }

/**
 * Where the current launch stands. A cancellable `Busy` state carries its
 * host-authorized actions. `Running` is informational; `Problem` offers recovery.
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
      /**
       * Measured progress, in bytes, when the work has a known position. A
       * surface draws a real bar from it; without it the work has no known
       * end and the surface shows only that it is under way. `total` is
       * absent when Korri knows how far it has got but not how far it goes.
       */
      readonly progress?: { readonly done: number; readonly total?: number }
      /** Actions supplied by the host for this work, such as real cancellation.
       * The surface renders them; it never infers cancellation from a label. */
      readonly actions?: readonly SurfaceAction[]
      /** The game this work belongs to, when it belongs to exactly one. */
      readonly gameId?: string
    }
  /**
   * Korri needs the user's say-so before it does something, such as a large
   * download. `kicker` is the short question ("Needs a download"); `detail`
   * names what and how much. The surface renders exactly the host's actions
   * and reports the chosen one through `runAction`; it never infers which
   * one agrees or declines from a label.
   */
  | {
      readonly _tag: "Confirm"
      readonly kicker: string
      readonly detail?: string
      readonly actions: readonly SurfaceAction[]
      /** The game this question belongs to, when it belongs to exactly one. */
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

/** Installed local runner facts from GameRoutes, not device/location choices.
 * Build identities are exact package paths, never inferred version labels. */
export interface SurfaceRunnerRoute {
  readonly runnerId: string
  readonly familyId?: string
  readonly systemId: string
  readonly runnerBuild: string
  readonly program: string
  readonly warnings: readonly string[]
  readonly actions: readonly SurfaceAction[]
}

export interface SurfaceRunnerChoiceContent {
  readonly gameTitle: string
  readonly message: string
  readonly routes: readonly SurfaceRunnerRoute[]
  readonly saved: readonly { readonly label: string; readonly runnerId: string }[]
  readonly warnings: readonly string[]
  /** IDs go unchanged to runAction. The host binds them to this route read. */
  readonly actions: readonly SurfaceAction[]
}

export type SurfaceRunnerChoice =
  | { readonly _tag: "Closed" }
  | ({ readonly _tag: "Loading" | "Ready" | "Stale" | "Conflict" | "Error" | "Busy" | "Warnings" } & SurfaceRunnerChoiceContent)

/**
 * How this device reaches the network right now. Grounded in legacy
 * `DeviceNetworkState` (`product/platform/device/device-facts.ts`: kind
 * wifi/ethernet, name, strengthPercent) and NetworkManager's primary
 * connection. `Wired` covers any non-Wi-Fi link NetworkManager reports as up.
 */
export type SurfaceNetworkConnection =
  | { readonly _tag: "Offline" }
  | { readonly _tag: "Wired" }
  | {
      readonly _tag: "Wifi"
      readonly ssid: string
      /** NetworkManager `AccessPoint.Strength`, 0 to 100. */
      readonly strengthPercent?: number
    }

/**
 * One Wi-Fi network as NetworkManager's scan reports it. Access points that
 * share an SSID appear once, at their strongest. `id` is opaque: hand it back
 * unchanged to `connectWifi`.
 */
export interface SurfaceWifiNetwork {
  readonly id: string
  readonly ssid: string
  /** NetworkManager `AccessPoint.Strength`, 0 to 100. */
  readonly strengthPercent: number
  /** True when the access point asks for a key (WPA/RSN flags set). */
  readonly secured: boolean
  /** True when NetworkManager already holds a saved profile for this SSID,
   * so connecting needs no password. */
  readonly known: boolean
}

/** The last connection attempt. Korri hands finished, user-facing copy. */
export type SurfaceWifiAttempt =
  | { readonly _tag: "Idle" }
  | { readonly _tag: "Connecting"; readonly ssid: string }
  | {
      readonly _tag: "Failed"
      readonly ssid: string
      readonly message: string
      /** NetworkManager rejected the key (state reason no-secrets or
       * supplicant failure). The surface may offer to type it again. */
      readonly passwordRejected: boolean
    }

export type SurfaceWifi =
  /** NetworkManager reports no Wi-Fi device on this machine. */
  | { readonly _tag: "NoAdapter" }
  | {
      readonly _tag: "Available"
      /** A scan Korri asked for is still running. */
      readonly scanning: boolean
      /** Strongest first. Empty when the last scan found nothing. */
      readonly networks: readonly SurfaceWifiNetwork[]
      readonly attempt: SurfaceWifiAttempt
    }

/** Present only when this host can read and change the device's network. */
export interface SurfaceNetwork {
  readonly connection: SurfaceNetworkConnection
  readonly wifi: SurfaceWifi
}

/**
 * A controller the host can see. Grounded in the native input stream's
 * `device-added` (`deviceId`, `name`, class `gamepad`).
 */
export interface SurfaceController {
  readonly id: string
  readonly name: string
  /** The host has received input from it since onboarding began. */
  readonly heard: boolean
}

/**
 * The Starter pack this device ships turned off, offered once during setup.
 * `message` is finished, user-facing copy from Korri.
 */
export type SurfaceStarterPack =
  | { readonly _tag: "Offered"; readonly actions: readonly SurfaceAction[] }
  | { readonly _tag: "Installing" }
  | { readonly _tag: "Installed" }
  | { readonly _tag: "Failed"; readonly message: string; readonly actions: readonly SurfaceAction[] }

/**
 * Present only while this device has not finished first-boot setup. The
 * surface owns the steps and their order; it ends setup with
 * `SurfaceHost.finishOnboarding`, after which Korri omits this field.
 */
export interface SurfaceOnboarding {
  readonly controllers: readonly SurfaceController[]
  /** Present while setup can offer the Starter pack. Absent when there is nothing to offer. */
  readonly starterPack?: SurfaceStarterPack
}

/** Everything Korri publishes to the surface. Replaced wholesale on change. */
export interface SurfaceModel {
  readonly presentation: SurfacePresentation
  readonly catalog: SurfaceCatalog
  readonly status: SurfaceStatus
  /** Absent in hosts that do not offer installed Linux runner selection. */
  readonly runnerChoice?: SurfaceRunnerChoice
  /** Preformatted local time. Absent when the surface should show no clock. */
  readonly clockLabel?: string
  /** Device-level actions for permissions and session control. May be empty. */
  readonly actions: readonly SurfaceAction[]
  /** Device facts and settings, grouped. Empty when Korri can state nothing. */
  readonly settings: readonly SurfaceSettingGroup[]
  readonly settingsStatus: SurfaceSettingsStatus
  /** Present only when this device can manage its person identity locally. */
  readonly identityManagement?: SurfaceIdentityManagement
  /** Free-form build/identity stamp a surface may display. */
  readonly buildLabel?: string
  /** Absent in hosts that cannot manage the device's network. */
  readonly network?: SurfaceNetwork
  /** Absent once first-boot setup is finished, and in hosts without it. */
  readonly onboarding?: SurfaceOnboarding
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
  exportIdentityBackup(password: string, retiredPublicKey?: string): void
  switchIdentityFromBackup(
    encryptedSecret: string,
    password: string,
    disposition: SurfaceIdentityDisposition,
    trustLossConfirmed: boolean,
  ): void
  switchIdentityToNip46(
    bunkerUri: string,
    disposition: SurfaceIdentityDisposition,
    trustLossConfirmed: boolean,
  ): void
  deleteRetiredIdentity(publicKey: string, backupConfirmed: boolean): void
  dismissIdentityStatus(): void
  /** Actions available for one game. Empty when Korri supports none yet. */
  gameActions(gameId: string): readonly SurfaceAction[]
  runGameAction(gameId: string, actionId: string): void
  /** Invoke a control from the current gameplay-overlay model. The host binds
   * this opaque request to its current launch identity before calling korrid. */
  invokeGameplayControl(
    controlId: string,
    value?: SurfaceGameplayControlValue,
  ): void
  /** Ask to return from the gameplay overlay. The host binds this to the
   * exact live launch; the surface never closes or thaws a session itself. */
  dismissGameplayOverlay(): void
  /** Try the failed thing again. Only meaningful while `Problem.canRetry`. */
  retry(): void
  /** Acknowledge a `Problem` and return to browsing. */
  dismiss(): void
  /** Ask Korri to re-read everything (used by error recovery). */
  reload(): void
  /** Ask for a fresh Wi-Fi scan. Results arrive in `model.network`. */
  scanWifi(): void
  /**
   * Join a listed network. `password` is required for a secured network that
   * is not `known`, and is passed straight to NetworkManager: Korri never
   * stores, logs, or republishes it.
   */
  connectWifi(networkId: string, password?: string): void
  /** Join a network that does not broadcast its name. With a password Korri
   * asks for WPA-PSK; without one it joins an open network. */
  connectHiddenWifi(ssid: string, password?: string): void
  /** Record that first-boot setup is finished on this device. */
  finishOnboarding(): void
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
