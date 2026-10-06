/**
 * What Pico's `update` functions may ask the outside world to do.
 *
 * An update never calls the host. It returns these as data, and
 * src/pico-host.ts, the one door to Korri, performs them. A test drives an
 * update with plain values and reads the requests it returned.
 */
import type { SurfaceGameplayControlValue, SurfaceIdentityDisposition } from "@contracts/surface/korri-surface"
import type { PicoFontId } from "../pico-fonts"

export type PicoRequest =
  | { readonly _tag: "LaunchGame"; readonly gameId: string; readonly locationId?: string }
  | { readonly _tag: "RunAction"; readonly actionId: string }
  | { readonly _tag: "RunGameAction"; readonly gameId: string; readonly actionId: string }
  | { readonly _tag: "ChangeSetting"; readonly settingId: string; readonly value: string }
  | { readonly _tag: "DismissSettingsProblem" }
  | { readonly _tag: "ExportIdentityBackup"; readonly password: string; readonly retiredPublicKey?: string }
  | {
      readonly _tag: "SwitchIdentityFromBackup"
      readonly encryptedSecret: string
      readonly password: string
      readonly disposition: SurfaceIdentityDisposition
      readonly trustLossConfirmed: boolean
    }
  | {
      readonly _tag: "SwitchIdentityToNip46"
      readonly bunkerUri: string
      readonly disposition: SurfaceIdentityDisposition
      readonly trustLossConfirmed: boolean
    }
  | { readonly _tag: "DeleteRetiredIdentity"; readonly publicKey: string; readonly backupConfirmed: boolean }
  | { readonly _tag: "DismissIdentityStatus" }
  | { readonly _tag: "InvokeGameplayControl"; readonly controlId: string; readonly value?: SurfaceGameplayControlValue }
  /** Leave the gameplay overlay. A range's held step is sent first. */
  | { readonly _tag: "DismissGameplayOverlay" }
  | { readonly _tag: "Retry" }
  | { readonly _tag: "DismissProblem" }
  | { readonly _tag: "Reload" }
  /** Pico's own setting: stored by Pico on this device, never sent to Korri. */
  | { readonly _tag: "ChooseFont"; readonly font: PicoFontId }
  /** Store the chosen face on this device. */
  | { readonly _tag: "RememberFont"; readonly font: PicoFontId }
  /** Draw `text` as a QR code. Replies with RenderedQr. */
  | { readonly _tag: "RenderQr"; readonly text: string }
  /** Wait `ms`. Replies with Waited and the same token, so a newer wait can
   * make an older one stale. */
  | { readonly _tag: "Wait"; readonly ms: number; readonly token: number }

/** What a request answers with, delivered to the program as a message. */
export type PicoReply =
  | { readonly _tag: "RenderedQr"; readonly text: string; readonly dataUrl: string }
  | { readonly _tag: "Waited"; readonly token: number }

/** One step of a program: the next model, and what to ask for. */
export interface PicoStep<Model> {
  readonly model: Model
  readonly requests: readonly PicoRequest[]
}

export const stay = <Model>(model: Model): PicoStep<Model> => ({ model, requests: [] })
export const ask = <Model>(model: Model, ...requests: PicoRequest[]): PicoStep<Model> => ({ model, requests })
