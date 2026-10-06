/**
 * Every fact that can change Pico's catalog navigation.
 *
 * Each is named for what happened ("PressedBack"), not for what should follow.
 * The view reports facts; `update` alone decides what they mean.
 */
import type { SurfaceAction, SurfaceIdentityDisposition } from "@contracts/surface/korri-surface"
import type { PicoOrder } from "../pico-library-view"
import type { PicoConfirmation } from "../pico-settings-view"

/** The host's buttons. They arrive through the host, not the DOM. */
export type PicoHostButton =
  | { readonly _tag: "PressedBack" }
  | { readonly _tag: "PressedSystem" }
  | { readonly _tag: "PressedOptions" }
  | { readonly _tag: "PressedMenu" }

export type PicoMessage =
  | PicoHostButton
  /** Korri published a model with a different catalog, status or runner choice. */
  | { readonly _tag: "KorriPublished" }
  /** The screen was left alone for PICO_ATTRACT_AFTER_MS. */
  | { readonly _tag: "IdleElapsed" }
  /** A key, pointer or click on the screen. It wakes attract. */
  | { readonly _tag: "PressedScreen" }
  /** The host moved the cursor with focus alone. It keeps the screen awake
   * but does not wake attract: the surface cannot swallow a focus move. */
  | { readonly _tag: "MovedFocus" }
  // The runner picker
  | { readonly _tag: "ChoseRunnerAction"; readonly actionId: string }
  // Home
  | { readonly _tag: "ToggledHomeMenu" }
  | { readonly _tag: "ChoseMenuFind" }
  | { readonly _tag: "ChoseMenuSettings" }
  | { readonly _tag: "ChoseMenuView" }
  /** Home put the cursor back where `focusOnReturn` asked. */
  | { readonly _tag: "ReturnedFocus" }
  | { readonly _tag: "SelectedCart"; readonly gameId: string }
  /** A cart on home, or a result in Find, was opened. */
  | { readonly _tag: "OpenedGame"; readonly gameId: string }
  | { readonly _tag: "PressedRetry" }
  | { readonly _tag: "PressedDismiss" }
  /** One of the actions Korri published for its work under way, such as Cancel. */
  | { readonly _tag: "PressedStatusAction"; readonly actionId: string }
  // Find
  | { readonly _tag: "TypedCharacter"; readonly character: string }
  | { readonly _tag: "PressedBackspace" }
  | { readonly _tag: "ClearedQuery" }
  | { readonly _tag: "ChoseSection"; readonly section: string }
  | { readonly _tag: "ChoseOrder"; readonly order: PicoOrder }
  // A game's own screen
  | { readonly _tag: "PressedPlay" }
  | { readonly _tag: "ChoseLocation"; readonly locationId: string }
  | { readonly _tag: "PressedGameAction"; readonly action: SurfaceAction }
  | { readonly _tag: "ConfirmedGameAction" }
  | { readonly _tag: "CancelledGameAction" }
  // Settings
  | { readonly _tag: "ChoseSettingsGroup"; readonly group: number }
  | { readonly _tag: "PressedSettingAction"; readonly actionId: string }
  | { readonly _tag: "AskedSettingConfirmation"; readonly actionId: string; readonly confirmation: PicoConfirmation }
  | { readonly _tag: "ConfirmedSettingAction" }
  | { readonly _tag: "CancelledSettingAction" }
  | { readonly _tag: "OpenedEditor"; readonly settingId: string }
  | { readonly _tag: "AskedClear" }
  | { readonly _tag: "CancelledClear" }
  | { readonly _tag: "ClosedEditor" }
  | { readonly _tag: "ChangedSetting"; readonly settingId: string; readonly value: string }
  | { readonly _tag: "DismissedSettingsProblem" }
  // The identity dialog. Its form is the dialog's own; only submissions arrive.
  | { readonly _tag: "ClosedIdentity" }
  | { readonly _tag: "SubmittedIdentityExport"; readonly password: string; readonly retiredPublicKey?: string }
  | {
      readonly _tag: "SubmittedIdentityFromBackup"
      readonly encryptedSecret: string
      readonly password: string
      readonly disposition: SurfaceIdentityDisposition
      readonly trustLossConfirmed: boolean
    }
  | {
      readonly _tag: "SubmittedIdentityToNip46"
      readonly bunkerUri: string
      readonly disposition: SurfaceIdentityDisposition
      readonly trustLossConfirmed: boolean
    }
  | { readonly _tag: "SubmittedRetiredIdentityDeletion"; readonly publicKey: string; readonly backupConfirmed: boolean }
  | { readonly _tag: "DismissedIdentityStatus" }

export const isHostButton = (message: PicoMessage): message is PicoHostButton =>
  message._tag === "PressedBack" || message._tag === "PressedSystem"
  || message._tag === "PressedOptions" || message._tag === "PressedMenu"
