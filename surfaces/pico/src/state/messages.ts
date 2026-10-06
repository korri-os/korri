/**
 * Every fact that can change Pico's catalog navigation.
 *
 * Each is named for what happened ("PressedBack"), not for what should follow.
 * The view reports facts; `update` alone decides what they mean.
 */
import type { SurfaceAction, SurfaceIdentityDisposition } from "@contracts/surface/korri-surface"
import type { PicoOrder } from "../pico-library-view"
import type { PicoConfirmation } from "../pico-settings-view"
import type { PicoReply } from "./requests"

/** The host's buttons. They arrive through the host, not the DOM. */
export type PicoHostButton =
  | { readonly _tag: "PressedBack" }
  | { readonly _tag: "PressedSystem" }
  | { readonly _tag: "PressedOptions" }
  | { readonly _tag: "PressedMenu" }

export type PicoMessage =
  | PicoHostButton
  | PicoReply
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
  /** The cursor is on a MENU control with this name, or has left them. */
  | { readonly _tag: "AimedMenu"; readonly label: string | undefined }
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
  | { readonly _tag: "ConfirmedClear" }
  | { readonly _tag: "ClosedEditor" }
  // The text editor's keyboard
  | { readonly _tag: "TypedText"; readonly character: string }
  | { readonly _tag: "PressedTextBackspace" }
  | { readonly _tag: "ClearedText" }
  | { readonly _tag: "ToggledCapitals" }
  | { readonly _tag: "ToggledSymbols" }
  | { readonly _tag: "SavedText" }
  | { readonly _tag: "ChangedSetting"; readonly settingId: string; readonly value: string }
  | { readonly _tag: "DismissedSettingsProblem" }
  // The identity dialog. The form is in the model; a submission sends what it holds.
  | { readonly _tag: "ClosedIdentity" }
  | { readonly _tag: "EditedIdentity"; readonly field: "password" | "secret" | "bunkerUri"; readonly value: string }
  | { readonly _tag: "ChoseIdentityDisposition"; readonly disposition: SurfaceIdentityDisposition }
  | { readonly _tag: "CheckedIdentityConfirmation"; readonly confirmed: boolean }
  | { readonly _tag: "SubmittedIdentity" }

export const isHostButton = (message: PicoMessage): message is PicoHostButton =>
  message._tag === "PressedBack" || message._tag === "PressedSystem"
  || message._tag === "PressedOptions" || message._tag === "PressedMenu"
