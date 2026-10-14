/**
 * Where the player is in first-boot setup, as one value.
 *
 * Korri says whether setup is still to do (`SurfaceModel.onboarding`), and
 * owns every fact a step shows: the network, the controllers it hears, the
 * device's name, the identity, the library. This value holds only what Pico
 * decides: which step is on screen, what the player is typing, a question
 * the player opened, and whether the player chose to go on offline. None of
 * it is stored; a restart begins at Hello.
 *
 * Secrets typed here (a Wi-Fi password, a backup's password) sit in the
 * model only while their editor or form is open. A password goes when its
 * editor closes and when a connect is sent. Nothing may log this value or a
 * message that carries one.
 */
import type { SurfaceWifiAttempt, SurfaceWifiNetwork } from "@contracts/surface/korri-surface"
import type { PicoTextDraft } from "../pico-text-draft"
import type { IdentityQr } from "./navigation"
import type { PicoReply } from "./requests"
import type { WifiEntry } from "./wifi"

/** The steps, in order. */
export type OnboardingStep = "hello" | "wifi" | "name" | "you" | "pads" | "games" | "play"

export const ONBOARDING_STEPS: readonly { readonly id: OnboardingStep; readonly label: string }[] = [
  { id: "hello", label: "HELLO" },
  { id: "wifi", label: "WI-FI" },
  { id: "name", label: "NAME" },
  { id: "you", label: "YOU" },
  { id: "pads", label: "PADS" },
  { id: "games", label: "GAMES" },
  { id: "play", label: "PLAY" },
]

/**
 * Something Pico asked of the network, until Korri's model shows the answer.
 * Korri may publish its answer without the in-between state (a scan that ends
 * before `scanning` was ever seen), so a pending ends on either: the busy
 * state seen and gone, or the facts changed from `before`.
 */
export type WifiPending =
  | { readonly _tag: "Scan"; readonly saw: boolean; readonly before: readonly SurfaceWifiNetwork[] }
  | { readonly _tag: "Join"; readonly ssid: string; readonly saw: boolean; readonly before: SurfaceWifiAttempt }

export type WifiQuestion =
  | { readonly _tag: "None" }
  /** The keyboard is up: a password, or a hidden network's name. */
  | { readonly _tag: "Entry"; readonly entry: WifiEntry }
  /** "Continue offline?", asked after no network worked. */
  | { readonly _tag: "Warning" }

export interface WifiStep {
  /** `List`: the player asked for the network list while online, or after a
   * failure. `Auto`: show what Korri's model says. */
  readonly showing: "Auto" | "List"
  readonly question: WifiQuestion
  readonly pending: WifiPending | undefined
  /** A failed attempt the player moved on from. Korri reports the last
   * attempt until the next one, so this one is no longer news. */
  readonly seenFailure: SurfaceWifiAttempt | undefined
}

export interface NameStep {
  /** What is typed. Seeded from Korri's device name the first time the step
   * opens; kept when the player goes back and forth. */
  readonly draft: PicoTextDraft | undefined
  /** NEXT sent the name and setup waits for Korri's save: `Sent`, then
   * `Seen` once Korri said it was saving. */
  readonly saving: "No" | "Sent" | "Seen"
}

export type IdentityOrigin = "created" | "restored" | "signer"

/** One field of the restore form open on the keyboard. */
export interface RestoreField {
  readonly field: "backup" | "password"
  readonly draft: PicoTextDraft
  readonly shown: boolean
}

export type IdentityStep =
  | { readonly _tag: "Choose" }
  | { readonly _tag: "Ready"; readonly origin: IdentityOrigin }
  /** `sent`: Korri was asked to switch, and its status now speaks for it. */
  | {
      readonly _tag: "Restore"
      readonly backup: string
      readonly password: string
      readonly editing: RestoreField | undefined
      readonly sent: boolean
    }
  | { readonly _tag: "Signer"; readonly uri: string; readonly editing: PicoTextDraft | undefined; readonly sent: boolean }

/** The backup dialog, over the identity step. */
export interface BackupDialog {
  readonly password: string
  readonly qr: IdentityQr | undefined
}

export interface OnboardingNav {
  readonly step: OnboardingStep
  readonly wifi: WifiStep
  /** The player chose to go on with no connection: after no network worked,
   * or on a device with no Wi-Fi. A later connection makes it moot. */
  readonly continuedOffline: boolean
  readonly name: NameStep
  readonly identity: IdentityStep
  readonly backup: BackupDialog | undefined
  /** GO TO GAMES was pressed. Home appears once Korri drops `onboarding`. */
  readonly finishing: boolean
}

export const WIFI_STEP: WifiStep = { showing: "Auto", question: { _tag: "None" }, pending: undefined, seenFailure: undefined }

export const initialOnboarding = (): OnboardingNav => ({
  step: "hello",
  wifi: WIFI_STEP,
  continuedOffline: false,
  name: { draft: undefined, saving: "No" },
  identity: { _tag: "Choose" },
  backup: undefined,
  finishing: false,
})

/** Every fact that can change setup, named for what happened. */
export type OnboardingMessage =
  | PicoReply
  /** The host's Back. Setup listens to no other host button. */
  | { readonly _tag: "PressedBack" }
  /** Korri published a model. */
  | { readonly _tag: "KorriPublished" }
  | { readonly _tag: "Started" }
  /** NEXT on a step that is done. */
  | { readonly _tag: "Continued" }
  // Wi-Fi
  | { readonly _tag: "PickedNetwork"; readonly networkId: string }
  | { readonly _tag: "ChoseHiddenNetwork" }
  | { readonly _tag: "ScannedNetworks" }
  | { readonly _tag: "TriedAgain" }
  /** Away from a failure, or from the network in use, to the list. */
  | { readonly _tag: "PickedAnotherNetwork" }
  /** "Stay on …" on a failure while another connection works. */
  | { readonly _tag: "StayedConnected" }
  /** "Set up Wi-Fi too" while a cable is online. */
  | { readonly _tag: "ChoseWifiToo" }
  /** "Continue offline" where no network worked: asks first. */
  | { readonly _tag: "ChoseOffline" }
  /** "Continue offline" on the question. */
  | { readonly _tag: "ConfirmedOffline" }
  /** "Back to Wi-Fi" on the question. */
  | { readonly _tag: "ReturnedToWifi" }
  // The keyboard
  | { readonly _tag: "TypedText"; readonly character: string }
  | { readonly _tag: "PressedTextBackspace" }
  | { readonly _tag: "ClearedText" }
  | { readonly _tag: "ToggledCapitals" }
  | { readonly _tag: "ToggledSymbols" }
  | { readonly _tag: "ToggledShown" }
  /** The keyboard's confirming action: CONNECT, NEXT, OK. */
  | { readonly _tag: "SubmittedText" }
  /** OK on a settings refusal Korri reported: the name, the seats, a rescan. */
  | { readonly _tag: "DismissedSettingsProblem" }
  // Identity
  | { readonly _tag: "ChoseIdentity"; readonly how: IdentityOrigin }
  | { readonly _tag: "EditedIdentityField"; readonly field: "backup" | "password" | "uri" }
  | { readonly _tag: "SubmittedIdentity" }
  | { readonly _tag: "OpenedBackup" }
  | { readonly _tag: "EditedBackupPassword"; readonly password: string }
  | { readonly _tag: "SubmittedBackup" }
  | { readonly _tag: "ClosedBackup" }
  // Pads
  | { readonly _tag: "ChangedSetting"; readonly settingId: string; readonly value: string }
  // Games
  | { readonly _tag: "ScannedGames" }
  // Play
  | { readonly _tag: "Finished" }
