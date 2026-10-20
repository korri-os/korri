/**
 * What Zone's `update` may ask the outside world to do.
 *
 * An update never calls the host. It returns these as data, and
 * src/zone-host.ts, the one door to Korri, performs them. One request per
 * SurfaceHost method; `gameActions` is a read, not a request. A test drives
 * an update with plain values and reads the requests it returned.
 */
import type { SurfaceGameplayControlValue, SurfaceIdentityDisposition } from "@contracts/surface/korri-surface"
import type { ZoneLook } from "../plan/look"

export type ZoneRequest =
  | { readonly _tag: "LaunchGame"; readonly gameId: string; readonly locationId?: string }
  | { readonly _tag: "RunAction"; readonly actionId: string }
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
  | { readonly _tag: "RunGameAction"; readonly gameId: string; readonly actionId: string }
  | { readonly _tag: "InvokeGameplayControl"; readonly controlId: string; readonly value?: SurfaceGameplayControlValue }
  | { readonly _tag: "DismissGameplayOverlay" }
  | { readonly _tag: "Retry" }
  | { readonly _tag: "DismissProblem" }
  | { readonly _tag: "Reload" }
  | { readonly _tag: "ScanWifi" }
  /** A password goes straight to the host and nowhere else: never logged. */
  | { readonly _tag: "ConnectWifi"; readonly networkId: string; readonly password?: string }
  | { readonly _tag: "ConnectHiddenWifi"; readonly ssid: string; readonly password?: string }
  | { readonly _tag: "FinishOnboarding" }
  /** Zone's own: keep the look on this device. Korri is never told. */
  | { readonly _tag: "RememberLook"; readonly look: ZoneLook }

/** One step of a program: the next model, and what to ask for. */
export interface ZoneStep<Model> {
  readonly model: Model
  readonly requests: readonly ZoneRequest[]
}

export const stay = <Model>(model: Model): ZoneStep<Model> => ({ model, requests: [] })
export const ask = <Model>(model: Model, ...requests: ZoneRequest[]): ZoneStep<Model> => ({ model, requests })
