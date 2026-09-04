import type {
  LaunchSpec as GeneratedLaunchSpec,
  MoonlightLaunchSpec as GeneratedMoonlightLaunchSpec,
  PlatformInstruction,
} from "../generated/korrid"

/**
 * Treaty between the portal (TS, in the WebView) and the Android shell
 * (Kotlin, KorriShellActivity). This file is the source of truth for every
 * value that crosses the JS <-> Kotlin bridge.
 *
 * Rules:
 * - This directory imports nothing outside `contracts/`.
 * - Shapes only — no logic, no I/O.
 * - The Kotlin implementation mirrors these shapes by hand and cites this
 *   file. When the two sides disagree, this file wins.
 * - Changes are additive or bump BRIDGE_VERSION.
 *
 * Transport:
 * - JS -> Kotlin: methods on the injected `window.KorriNative` object.
 *   Methods take/return JSON strings encoding the types below.
 * - Kotlin -> JS: input events call `window.__korriInput(json)` with an
 *   encoded `BridgeInputEvent`. The portal registers that global.
 */

// 2 and 3 are skipped: the shipped shell reported 3 while this file lagged
// at 1. 4 introduced the session lifecycle; 5 adds the per-server korrid
// capability required to protect localhost session-control RPCs. 6 adds the
// launcher-neutral local launch instruction. 10 removes direct Android app
// enumeration/launch and makes notification prompt semantics explicit. 11 adds
// the shell-owned Android/app identity used by System information. 12 seals
// Moonlight startup behind korrid's signed, one-use launch instruction. 13 adds
// the honest gameplay-overlay accessibility grant/settings seam. 14 adds the
// receipt-based asynchronous game-folder picker. 15 adds opaque local cover
// asset URL resolution. 16 adds picker single-flight Busy results. 17 removes
// the obsolete Moonlight trust ceremony. 18 removes public pairing state after
// korrid-owned provisioning becomes the only trust path. 19 adds the Android
// person-signer and verified owner-binding lifecycle.
export const BRIDGE_VERSION = 19

// ── Local launches (JS -> Kotlin) ───────────────────────────────────────

/**
 * Launcher-neutral instruction produced by korrid. Its Rust/Typeshare shape is
 * the single source of truth for both the HTTP response and native bridge.
 */
export type LocalLaunchSpec = GeneratedLaunchSpec

/** Result of `KorriNative.launchLocal(specJson)`. */
export type LaunchLocalResult =
  | { readonly _tag: "Launched" }
  | {
      readonly _tag: "LaunchFailed"
      readonly reason:
        | "UnsupportedLauncher"
        | "InvalidSpec"
        | "NotInstalled"
        | "ProvisionFailed"
        | "StartFailed"
      readonly message: string
    }

export type LocalGameAssetUrlResult =
  | { readonly _tag: "Resolved"; readonly url: string }
  | { readonly _tag: "Absent" }

// ── Streaming (JS -> Kotlin) ────────────────────────────────────────────

/** A stream host known to the shell. Trust is provisioned when needed. */
export interface StreamHost {
  readonly uuid: string
  readonly name: string
}

/** Result of `KorriNative.queryStreamHosts()`. */
export type QueryStreamHostsResult =
  | { readonly _tag: "StreamHosts"; readonly items: readonly StreamHost[] }
  | { readonly _tag: "QueryFailed"; readonly message: string }

/** A streamable app on a host, from the shell's cached app list. */
export interface StreamApp {
  readonly id: number
  readonly name: string
}

/** Result of `KorriNative.queryStreamApps(hostUuid)`. */
export type QueryStreamAppsResult =
  | { readonly _tag: "StreamApps"; readonly items: readonly StreamApp[] }
  | { readonly _tag: "QueryFailed"; readonly message: string }

/** Signed Moonlight startup instruction produced and consumed by korrid. */
export type MoonlightLaunchSpec = GeneratedMoonlightLaunchSpec

/** Result of `KorriNative.startStream(specJson)`. */
export type StartStreamResult =
  | { readonly _tag: "StreamStarted" }
  | {
      readonly _tag: "StreamFailed"
      readonly reason:
        | "HostUnreachable"
        | "HostCertificateRejected"
        | "InvalidCertificate"
        | "ProvisioningFailed"
        | "ProvisioningCancelled"
        | "ProvisioningChanged"
        | "ProvisioningRepairRequired"
        | "AppListFailed"
        | "AppNotFound"
        | "StartInProgress"
        | "StartFailed"
      readonly message: string
    }

// ── Input (Kotlin -> JS) ────────────────────────────────────────────────

/**
 * Semantic input vocabulary. The shell owns all hardware truth (key codes,
 * button indices, controller quirks) and emits only these. The portal never
 * sees a key code.
 *
 * This mirrors the portal's internal `InputAction` model (harvested from
 * legacy `product/platform/input/types.ts`); the portal's native adapter
 * converts wire events into that model at the seam.
 */
export type BridgeDirection = "up" | "down" | "left" | "right"

export type BridgeInputEvent =
  | {
      readonly type: "direction"
      readonly direction: BridgeDirection
      /** True after the hardware edge's initial directional press. */
      readonly repeat?: boolean
      readonly releaseExpected?: never
      readonly gestureId?: never
      readonly source: "gamepad"
    }
  | {
      readonly type: "direction"
      readonly direction: BridgeDirection
      /** True after the hardware edge's initial directional press. */
      readonly repeat?: boolean
      /** Android will publish one matching release edge. */
      readonly releaseExpected: true
      /** Opaque process-local gesture identity; never a device or key identity. */
      readonly gestureId: number
      readonly source: "gamepad"
    }
  | {
      readonly type: "direction-end"
      readonly direction: BridgeDirection
      readonly gestureId: number
      readonly source: "gamepad"
    }
  | { readonly type: "confirm"; readonly source: "gamepad" }
  | { readonly type: "back"; readonly source: "gamepad" }
  | { readonly type: "menu"; readonly source: "gamepad" }
  | { readonly type: "options"; readonly source: "gamepad" }
  /** Guide/system remains semantic; its Android key code never crosses. */
  | { readonly type: "system"; readonly source: "gamepad" }

// ── Dedicated gameplay-overlay entry and messages ──────────────────────

/** Query treaty selecting the global gameplay-overlay portal composition. */
export const GAMEPLAY_OVERLAY_SCREEN_PARAM = "screen"
export const GAMEPLAY_OVERLAY_SCREEN_VALUE = "gameplay-overlay"

/** Synthetic HTTPS origin served only from APK assets by WebViewAssetLoader. */
export const KORRI_ASSET_ORIGIN = "https://appassets.androidplatform.net"
export const GAMEPLAY_OVERLAY_URL =
  `${KORRI_ASSET_ORIGIN}/assets/portal/index.html?${GAMEPLAY_OVERLAY_SCREEN_PARAM}=${GAMEPLAY_OVERLAY_SCREEN_VALUE}`

/** AndroidX WebMessageListener object. It has only postMessage(String). */
export const GAMEPLAY_OVERLAY_MESSAGE_OBJECT = "KorriOverlay"
/** The one encoded Android -> JS receiver installed by the overlay entry. */
export const GAMEPLAY_OVERLAY_RECEIVER = "__korriOverlayMessage"

export interface GameplayOverlayConfig {
  readonly korridPort: number
  readonly korridCapability: string
  readonly launchId: string
}

export type GameplayOverlayInstructionResult =
  | { readonly _tag: "Executed" }
  | { readonly _tag: "Unavailable"; readonly message: string }
  | { readonly _tag: "Rejected"; readonly message: string }

/** The complete JS -> native overlay vocabulary. No shell powers are inherited. */
export type GameplayOverlayToNativeMessage =
  | { readonly type: "ready" }
  | { readonly type: "dismiss" }
  | { readonly type: "refresh-authority" }
  | {
      readonly type: "execute-protected-instruction"
      readonly requestId: string
      readonly instruction: PlatformInstruction
    }

/** The complete native -> JS overlay vocabulary. */
export type GameplayOverlayToPortalMessage =
  | { readonly type: "config"; readonly payload: GameplayOverlayConfig }
  | { readonly type: "input"; readonly payload: BridgeInputEvent }
  | {
      readonly type: "instruction-result"
      readonly requestId: string
      readonly outcome: GameplayOverlayInstructionResult
    }

export interface KorriOverlayMessageSurface {
  postMessage(messageJson: string): void
}

// ── Shell surface (JS -> Kotlin) ────────────────────────────────────────

/**
 * The full `window.KorriNative` surface the shell injects. Spike-era
 * methods (streaming, settings, korrid RPC) are intentionally not part of
 * the treaty yet; they join it when a slice formalizes them.
 */
export type PersonSignerState =
  | { readonly _tag: "Unavailable"; readonly message: string }
  | { readonly _tag: "Pending"; readonly message: string }
  | { readonly _tag: "Approved"; readonly message: string }
  | { readonly _tag: "Denied"; readonly message: string }
  | { readonly _tag: "InvalidResponse"; readonly message: string }
  | { readonly _tag: "Defect"; readonly message: string }

export type DeviceIdentityState =
  | { readonly _tag: "Unowned"; readonly devicePublicKey: string }
  | {
      readonly _tag: "Owned" | "Revoked"
      readonly devicePublicKey: string
      readonly ownerPublicKey: string
      readonly eventId: string
      readonly createdAt: number
    }
  | { readonly _tag: "Invalid"; readonly reason?: string }

export interface OwnerBindingSnapshot {
  readonly identity: DeviceIdentityState
  readonly personSigner: PersonSignerState
  readonly deviceFingerprint?: string
  readonly requestedAction: string
  readonly bindingUri?: string
  readonly signerRequirement?: string
}

/** Emitted after an asynchronous signer result changes owner state. */
export const OWNER_BINDING_CHANGED_EVENT = "korri-owner-binding-changed"

export interface KorriNativeBridgeSurface {
  /** Public device identity plus the person-signer lifecycle. */
  ownerBindingSnapshot(): string
  /** Starts account selection/signing and returns the immediate snapshot. */
  startOwnerBinding(): string
  /** Returns JSON-encoded `LaunchLocalResult`. */
  launchLocal(specJson: string): string
  /** Returns JSON-encoded `LocalGameAssetUrlResult`. */
  localGameAssetUrl(assetId: string): string
  /** Returns JSON-encoded `QueryStreamHostsResult`. */
  queryStreamHosts(): string
  /** Returns JSON-encoded `QueryStreamAppsResult`. */
  queryStreamApps(hostUuid: string): string
  /**
   * Verifies and consumes korrid's signed Moonlight launch instruction before
   * starting the stream Activity. The portal cannot submit raw host/app data.
   */
  startStream(specJson: string): string
  /**
   * Port of the embedded korrid server on 127.0.0.1, or -1 when it is not
   * running. The portal builds its brain base URL from this; hardware and
   * process lifecycle stay on the Kotlin side of the treaty.
   */
  korridPort(): number
  /**
   * Unguessable capability for this embedded korrid server lifetime. The
   * portal sends it as a bearer token; it must never be persisted.
   */
  korridCapability(): string
  /**
   * Whether Korri may read and write the user-visible storage its settings,
   * plugins, and local-game files live in. Returns a JSON-encoded
   * `StorageAccessResult`.
   *
   * The portal must treat `Denied` as a normal, recoverable state rather than
   * an error: it is one toggle in system settings, the user can revoke it at
   * any time, and without it Korri cannot read its own configuration.
   */
  storageAccess(): string
  /**
   * Open the system screen where the user grants Korri file access, returning
   * a JSON-encoded `OpenStorageSettingsResult`.
   *
   * The shell cannot grant the permission itself and no result means it was
   * granted — it only takes the user there. The portal re-checks
   * `storageAccess()` on `korri-shell-resumed`.
   */
  openStorageAccessSettings(): string
  /** Actual accessibility-service grant state, re-read on shell resume. */
  overlayPermission(): string
  /** Open Android's accessibility details. Opened never means granted. */
  openOverlaySettings(): string
  /**
   * Whether the user can see that Korri is running in the background.
   * Returns a JSON-encoded `BackgroundNoticeResult`.
   *
   * Korri keeps its brain alive while games run, and Android's bargain for
   * that is a notice the user can see and act on. The notice may be hidden
   * without stopping the brain, so this reports what the user can actually
   * see rather than whether the service is running.
   */
  backgroundNotice(): string
  /**
   * Ask Android to let Korri show the notice, returning a JSON-encoded
   * `RequestBackgroundNoticeResult`.
   *
   * Only ever prompts. Android refuses to prompt again once the user has
   * declined twice, so `Unprompted` is a normal answer and the portal should
   * fall back to `openNotificationSettings()`.
   */
  requestBackgroundNotice(): string
  /**
   * Open the system screen where the user shows or hides Korri's notice,
   * returning a JSON-encoded `OpenNotificationSettingsResult`.
   *
   * The shell cannot hide its own background notice — Android reserves that
   * for the user — so turning it off always means going here.
   */
  openNotificationSettings(): string
  /** Android and app identity for the read-only System information group. */
  systemInfo(): string
  /** Open Android's asynchronous folder picker for game locations. */
  openGameFolderPicker(): string
  /** Re-read the generation-tagged picker result. */
  gameFolderPickerSnapshot(): string
  /** Acknowledge one definitive picker result generation. */
  acknowledgeGameFolderPicker(generation: string): string
  /** Returns `BRIDGE_VERSION` of the shell build. */
  bridgeVersion(): number
}

// ── Game folder picker (v12) ────────────────────────────────────────

export type OpenGameFolderPickerResult =
  | { readonly _tag: "Opened"; readonly generation: string }
  | {
      readonly _tag: "Busy"
      readonly generation: string
      readonly state: "Choosing" | "Selected"
    }
  | { readonly _tag: "Unavailable"; readonly message: string }

export type GameFolderPickerState =
  | { readonly _tag: "Idle" }
  | { readonly _tag: "Choosing" }
  | { readonly _tag: "Selected"; readonly receipt: string }
  | { readonly _tag: "Cancelled" }
  | { readonly _tag: "Problem"; readonly code: string; readonly message: string }

export interface GameFolderPickerSnapshot {
  readonly version: 1
  readonly generation: string
  readonly state: GameFolderPickerState
}

export type AcknowledgeGameFolderPickerResult =
  | { readonly _tag: "Acknowledged"; readonly generation: string }
  | { readonly _tag: "Stale"; readonly generation: string }

// ── System information (v11) ────────────────────────────────────────

export interface AndroidSystemInfo {
  readonly device: string
  readonly manufacturer: string
  readonly androidRelease: string
  readonly sdk: number
  readonly appVersion: string
}

export type SystemInfoResult =
  | { readonly _tag: "SystemInfo"; readonly payload: AndroidSystemInfo }
  | { readonly _tag: "Unavailable"; readonly message: string }

// ── Background notice (v10) ──────────────────────────────────────────

/**
 * What the user can see of Korri running in the background.
 *
 * `Hidden` is not a failure: the brain still runs, the user simply has no
 * visible sign of it and no quick way to stop it.
 */
export type BackgroundNoticeResult =
  | { readonly _tag: "Visible" }
  | { readonly _tag: "Hidden" }

/** Outcome of asking Android for permission to show the notice. */
export type RequestBackgroundNoticeResult =
  | { readonly _tag: "Granted" }
  | { readonly _tag: "Denied" }
  /** Android launched the asynchronous permission dialog; re-read on resume. */
  | { readonly _tag: "Prompted" }
  /** Android declined to prompt; only system settings can change it now. */
  | { readonly _tag: "Unprompted" }

/** Outcome of sending the user to the system notification screen. */
export type OpenNotificationSettingsResult =
  | { readonly _tag: "Opened" }
  | { readonly _tag: "Unavailable"; readonly message: string }

// ── Gameplay overlay permission (v13) ───────────────────────────────

export type OverlayPermissionResult =
  | { readonly _tag: "Enabled" }
  | { readonly _tag: "Disabled" }
  | { readonly _tag: "RestrictedOrUnavailable" }

export type OpenOverlaySettingsResult =
  | { readonly _tag: "Opened" }
  | { readonly _tag: "Unavailable"; readonly message: string }

// ── User-visible storage access (v7) ─────────────────────────────────

/**
 * Korri keeps settings, plugins, and local-game files where the user can find
 * them in a file manager. On Android that requires a permission the user
 * grants manually and may revoke at any time; on platforms with no such
 * concept the answer is `NotRequired`.
 */
export type StorageAccessResult =
  | { readonly _tag: "Granted" }
  | { readonly _tag: "NotRequired" }
  /** Korri cannot read or write its own files until this is granted. */
  | { readonly _tag: "Denied" }
  | { readonly _tag: "QueryFailed"; readonly message: string }

/** Result of asking the shell to open the grant screen. */
export type OpenStorageSettingsResult =
  /** The settings screen was opened. The user may still decline. */
  | { readonly _tag: "Opened" }
  /** No settings screen exists to open on this platform or device. */
  | { readonly _tag: "Unavailable"; readonly message: string }

// ── Stream session lifecycle (v4) ────────────────────────────────────

/**
 * Korri-initiated streams narrate their pre-stream lifecycle to a
 * portal-origin overlay WebView inside the stream Activity. The overlay is
 * the same bundled portal app booted on the session screen: the asset-loader
 * origin URL plus `?SESSION_SCREEN_PARAM=SESSION_SCREEN_VALUE`.
 *
 * Contract shape is pull-then-push, mirroring `__korriInput`:
 * - Pull: on boot the overlay calls `KorriSession.lifecycleSnapshot()` and
 *   folds the returned event log. This closes the race where stages fire
 *   before the overlay's JS is ready.
 * - Push: the shell then calls `window.__korriSessionEvent(json)` with each
 *   new JSON-encoded `StreamLifecycleEvent`. Events may overlap with the
 *   snapshot; consumers must treat replayed/duplicate stage events as
 *   idempotent and never regress the timeline.
 *
 * Only Korri-initiated streams (launched via `startStream`) inject
 * `KorriSession` and show the overlay; stock Artemis entry points are
 * untouched.
 */

/** Query parameter that boots the bundled portal on the session screen. */
export const SESSION_SCREEN_PARAM = "screen"
export const SESSION_SCREEN_VALUE = "session"

/**
 * Semantic connection stages. Kotlin owns the mapping from raw Moonlight
 * stage strings (moonlight-common-c `getStageName` values plus the
 * app-launch stage, which is named after the app) into these ids; the raw
 * native string rides along as `detail` for display only and is never a
 * contract value.
 */
export type StreamStageId =
  /** Host is launching or resuming the requested app. */
  | "launching-app"
  /** Platform initialization and name resolution. */
  | "initializing"
  /** RTSP handshake with the host. */
  | "handshaking"
  /** Control/video/audio/input stream bring-up. */
  | "establishing-streams"

/**
 * Tagged failure vocabulary for pre-stream failures and terminations.
 * Kotlin derives these from stage names, error codes, and port-test
 * results; the numeric `errorCode` rides along for diagnostics.
 */
export type StreamFailureReason =
  | "AppLaunchFailed"
  | "HostUnreachable"
  | "PermissionDenied"
  | "DecoderInitFailed"
  | "NoVideoTraffic"
  | "ConnectionLost"
  | "Unknown"

/**
 * One step in the stream session lifecycle, pushed by the shell as it
 * happens and replayed in order by `lifecycleSnapshot()`.
 *
 * - `stage-starting` / `stage-complete` mirror Moonlight's stage callbacks.
 * - `connected` mirrors `connectionStarted()`: frames are imminent, the
 *   shell removes the overlay. Terminal for the overlay's happy path.
 * - `failed` is a pre-stream stage failure; the overlay renders the tagged
 *   reason and offers a way back to the portal.
 * - `terminated` is a connection ending after establishment. `graceful`
 *   distinguishes the user quitting from an error termination.
 */
export type StreamLifecycleEvent =
  | {
      readonly type: "stage-starting"
      readonly stage: StreamStageId
      readonly detail?: string
    }
  | {
      readonly type: "stage-complete"
      readonly stage: StreamStageId
      readonly detail?: string
    }
  | { readonly type: "connected" }
  | {
      readonly type: "failed"
      readonly reason: StreamFailureReason
      readonly stage: StreamStageId
      readonly errorCode: number
      readonly detail?: string
    }
  | {
      readonly type: "terminated"
      readonly graceful: boolean
      readonly reason: StreamFailureReason
      readonly errorCode: number
    }

/** Result of `KorriSession.lifecycleSnapshot()`: the event log so far. */
export interface StreamLifecycleSnapshot {
  readonly events: readonly StreamLifecycleEvent[]
}

/**
 * The `window.KorriSession` surface injected only into the session-screen
 * overlay WebView inside the stream Activity. The launcher WebView never
 * sees it; the overlay uses its absence to detect browser dev and render a
 * fixture timeline instead.
 */
export interface KorriSessionBridgeSurface {
  /** Returns JSON-encoded `StreamLifecycleSnapshot`. */
  lifecycleSnapshot(): string
  /**
   * User acknowledged a failure (or backed out): finish the stream
   * Activity and return to the portal Activity beneath it.
   */
  exitToPortal(): void
}

// ── Lifecycle (Kotlin -> JS) ────────────────────────────────────────────

/**
 * When the shell Activity resumes (e.g. returning from a stream), it
 * dispatches `window.dispatchEvent(new Event("korri-shell-resumed"))`.
 * The portal treats this as "your state may be stale; re-query".
 */
export const SHELL_RESUMED_EVENT = "korri-shell-resumed"

/** Android completed a background Moonlight app-list repair. */
export const STREAM_APPS_CHANGED_EVENT = "korri-stream-apps-changed"
