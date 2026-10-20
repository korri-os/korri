/**
 * First boot: where the player is in Zone's setup, as one value, and the
 * one function that changes it.
 *
 *     step       CONTROLLERS, WI-FI, NAME or IDENTITY, in that order
 *     focus      the control the cursor is on in the step, by target id
 *     entry      the keyboard over the NAME step: the draft, its caret,
 *                the layer and the lit key; absent while the keyboard is down
 *     started    START was pressed on the NEW SAVE card; FINISH is reachable
 *     finishing  FINISH was pressed; setup waits for Korri to drop it
 *     join       a secured network the player chose that needs a password:
 *                the hand-off to the OPTIONS page's join flow
 *
 * Korri says whether setup is still to do (`SurfaceModel.onboarding`) and
 * owns every fact a step shows; `update` receives its model beside this
 * value. Nothing here is stored: a restart begins at CONTROLLERS.
 *
 * The treaty asks for no identity call at first boot: a device starts with
 * its own new identity, so START on the NEW SAVE card records the player's
 * choice here and sends nothing. Restoring a save belongs to SAVE DATA.
 */
import type { SurfaceModel, SurfaceSettingItem } from "@contracts/surface/korri-surface"
import { ask, stay, type ZoneStep } from "./requests"

export type ZoneOnboardingStep = "controllers" | "wifi" | "name" | "identity"

/** The steps in strip order. */
export const ZONE_ONBOARDING_STEPS: readonly ZoneOnboardingStep[] = ["controllers", "wifi", "name", "identity"]

/** The keyboard's layers, as the keyboard names them. */
export type ZoneEntryLayer = "letters" | "shift" | "symbols" | "uri"

/** A key the keyboard reports, as the keyboard names it. */
export type ZoneEntryKey =
  | { readonly kind: "char"; readonly char: string }
  | { readonly kind: "space" }
  | { readonly kind: "delete" }
  | { readonly kind: "done" }
  | { readonly kind: "layer"; readonly layer: ZoneEntryLayer }

export interface ZoneNameEntry {
  readonly draft: string
  readonly caret: number
  readonly layer: ZoneEntryLayer
  /** The lit key's id, as the keyboard names keys. */
  readonly focusedKey: string
}

export interface ZoneOnboardingModel {
  readonly step: ZoneOnboardingStep
  readonly focus: string
  readonly entry: ZoneNameEntry | undefined
  readonly started: boolean
  readonly finishing: boolean
  readonly join: string | undefined
}

/** The focus targets that are not a network row. */
export const TARGET = {
  next: "next",
  finish: "finish",
  save: "save",
  connection: "connection",
  scan: "scan",
  skip: "skip",
  name: "name",
} as const

/** A network row's focus target. */
export const networkTarget = (networkId: string): string => `net:${networkId}`

export const ZONE_ONBOARDING_START: ZoneOnboardingModel = {
  step: "controllers",
  focus: TARGET.next,
  entry: undefined,
  started: false,
  finishing: false,
  join: undefined,
}

export type ZoneOnboardingMessage =
  | { readonly _tag: "KorriPublished" }
  /** The host's Back: the keyboard goes down, else setup goes back one step. */
  | { readonly _tag: "PressedBack" }
  | { readonly _tag: "FocusedTarget"; readonly target: string }
  /** A on NEXT. */
  | { readonly _tag: "ChoseNext" }
  /** A on the SKIP row of the WI-FI step. */
  | { readonly _tag: "ChoseSkip" }
  /** A on the SCAN row of the WI-FI step. */
  | { readonly _tag: "ChoseScan" }
  /** A on a network row. */
  | { readonly _tag: "OnboardingJoin"; readonly networkId: string }
  /** A on the DEVICE NAME row. */
  | { readonly _tag: "OpenedName" }
  /** A on a key of the keyboard. */
  | { readonly _tag: "PressedKey"; readonly key: ZoneEntryKey }
  /** A on the NEW SAVE card. */
  | { readonly _tag: "ChoseStart" }
  /** A on FINISH. */
  | { readonly _tag: "ChoseFinish" }

/** What the WI-FI step's network row dispatches. */
export const onboardingJoin = (networkId: string): ZoneOnboardingMessage => ({ _tag: "OnboardingJoin", networkId })

/** The label Korri gives the device's name, compared without case. */
export const DEVICE_NAME_LABEL = "device name"

/**
 * Korri's device-name setting. The treaty names no setting ids, so Zone
 * finds it by its label: the first item, in any group, whose label reads
 * "Device name" (case and outer space ignored) and which takes text. Korri
 * publishes it today with the id `device-name`; Zone does not rely on that.
 * Absent, the NAME row shows read-only and A on it does nothing.
 */
export function deviceNameSetting(korri: SurfaceModel): SurfaceSettingItem | undefined {
  return korri.settings.flatMap(group => group.items)
    .find(item => item.label.trim().toLowerCase() === DEVICE_NAME_LABEL && item.interaction?.kind === "text")
}

/** At least one controller has been heard since setup began. */
export const anyHeard = (korri: SurfaceModel): boolean =>
  (korri.onboarding?.controllers ?? []).some(controller => controller.heard)

/** The device is online, or Korri cannot manage its network at all. */
export function wifiSettled(korri: SurfaceModel): boolean {
  return korri.network === undefined || korri.network.connection._tag !== "Offline"
}

/** Whether NEXT leads on from this step. */
export function canGoOn(step: ZoneOnboardingStep, korri: SurfaceModel): boolean {
  switch (step) {
    case "controllers":
      return anyHeard(korri)
    case "wifi":
      return wifiSettled(korri)
    case "name":
      return true
    case "identity":
      return false
  }
}

/** The step's focus targets, top to bottom. */
export function onboardingTargets(model: ZoneOnboardingModel, korri: SurfaceModel): readonly string[] {
  switch (model.step) {
    case "controllers":
      return [TARGET.next]
    case "wifi": {
      const network = korri.network
      const wifi = network?.wifi
      const networks = wifi?._tag === "Available" ? wifi.networks.map(item => networkTarget(item.id)) : []
      return [
        ...(network === undefined ? [] : [TARGET.connection]),
        ...networks,
        ...(wifi?._tag === "Available" ? [TARGET.scan] : []),
        TARGET.skip,
        TARGET.next,
      ]
    }
    case "name":
      return [TARGET.name, TARGET.next]
    case "identity":
      return [TARGET.save, TARGET.finish]
  }
}

/** Where the cursor lands when a step opens. */
function firstTarget(model: ZoneOnboardingModel, korri: SurfaceModel): string {
  switch (model.step) {
    case "controllers":
      return TARGET.next
    case "wifi": {
      const wifi = korri.network?.wifi
      const first = wifi?._tag === "Available" ? wifi.networks[0] : undefined
      return first !== undefined ? networkTarget(first.id) : wifi?._tag === "Available" ? TARGET.scan : TARGET.skip
    }
    case "name":
      return TARGET.name
    case "identity":
      return model.started ? TARGET.finish : TARGET.save
  }
}

const go = (model: ZoneOnboardingModel, step: ZoneOnboardingStep, korri: SurfaceModel): ZoneOnboardingModel => {
  const moved = { ...model, step, entry: undefined }
  return { ...moved, focus: firstTarget(moved, korri) }
}

const nextStep = (step: ZoneOnboardingStep): ZoneOnboardingStep =>
  ZONE_ONBOARDING_STEPS[Math.min(ZONE_ONBOARDING_STEPS.indexOf(step) + 1, ZONE_ONBOARDING_STEPS.length - 1)] ?? step

const previousStep = (step: ZoneOnboardingStep): ZoneOnboardingStep =>
  ZONE_ONBOARDING_STEPS[Math.max(ZONE_ONBOARDING_STEPS.indexOf(step) - 1, 0)] ?? step

/** The id the keyboard names a key by, so the lit key follows the press. */
export function entryKeyId(key: ZoneEntryKey): string {
  switch (key.kind) {
    case "char":
      return key.char
    case "space":
    case "delete":
    case "done":
      return key.kind
    case "layer":
      return `layer:${key.layer}`
  }
}

export function updateOnboarding(
  model: ZoneOnboardingModel,
  message: ZoneOnboardingMessage,
  korri: SurfaceModel,
): ZoneStep<ZoneOnboardingModel> {
  const result = model.finishing ? stay(model) : step(model, message, korri)
  return { model: settle(result.model, korri), requests: result.requests }
}

function step(model: ZoneOnboardingModel, message: ZoneOnboardingMessage, korri: SurfaceModel): ZoneStep<ZoneOnboardingModel> {
  switch (message._tag) {
    case "KorriPublished":
      return stay(model)
    case "PressedBack":
      if (model.entry !== undefined) return stay({ ...model, entry: undefined, focus: TARGET.name })
      return model.step === "controllers" ? stay(model) : stay(go(model, previousStep(model.step), korri))
    case "FocusedTarget":
      return model.entry !== undefined ? stay(model) : stay({ ...model, focus: message.target })
    case "ChoseNext":
      return canGoOn(model.step, korri) ? stay(go(model, nextStep(model.step), korri)) : stay(model)
    case "ChoseSkip":
      return model.step === "wifi" ? stay(go(model, "name", korri)) : stay(model)
    case "ChoseScan":
      return model.step === "wifi" && korri.network?.wifi._tag === "Available"
        ? ask({ ...model, focus: TARGET.scan }, { _tag: "ScanWifi" }) : stay(model)
    case "OnboardingJoin":
      return chooseNetwork(model, message.networkId, korri)
    case "OpenedName": {
      const setting = deviceNameSetting(korri)
      if (model.step !== "name" || setting === undefined) return stay(model)
      const draft = setting.value ?? ""
      return stay({ ...model, focus: TARGET.name, entry: { draft, caret: draft.length, layer: "letters", focusedKey: "done" } })
    }
    case "PressedKey":
      return pressKey(model, message.key, korri)
    case "ChoseStart":
      return model.step === "identity" ? stay({ ...model, started: true, focus: TARGET.finish }) : stay(model)
    case "ChoseFinish":
      return model.step === "identity" && model.started
        ? ask({ ...model, finishing: true, focus: TARGET.finish }, { _tag: "FinishOnboarding" }) : stay(model)
  }
}

/**
 * A network row. One that needs no password (saved, or open) is joined at
 * once; a secured one Korri has no key for is handed to the OPTIONS join
 * flow through `join`, which asks for the password.
 */
function chooseNetwork(model: ZoneOnboardingModel, networkId: string, korri: SurfaceModel): ZoneStep<ZoneOnboardingModel> {
  const wifi = korri.network?.wifi
  const network = wifi?._tag === "Available" ? wifi.networks.find(item => item.id === networkId) : undefined
  if (model.step !== "wifi" || network === undefined) return stay(model)
  const focused = { ...model, focus: networkTarget(network.id) }
  return network.known || !network.secured
    ? ask({ ...focused, join: undefined }, { _tag: "ConnectWifi", networkId: network.id })
    : stay({ ...focused, join: network.id })
}

/** A key on the NAME step's keyboard. DONE sends the name when it changed. */
function pressKey(model: ZoneOnboardingModel, key: ZoneEntryKey, korri: SurfaceModel): ZoneStep<ZoneOnboardingModel> {
  const entry = model.entry
  const setting = deviceNameSetting(korri)
  if (entry === undefined || setting === undefined) return stay(model)
  const lit = { ...entry, focusedKey: entryKeyId(key) }
  const maxLength = setting.interaction?.kind === "text" ? setting.interaction.maxLength : undefined
  const { draft, caret } = entry
  switch (key.kind) {
    case "char":
    case "space": {
      if (maxLength !== undefined && draft.length >= maxLength) return stay({ ...model, entry: lit })
      const char = key.kind === "space" ? " " : key.char
      return stay({ ...model, entry: { ...lit, draft: draft.slice(0, caret) + char + draft.slice(caret), caret: caret + 1 } })
    }
    case "delete":
      return caret === 0 ? stay({ ...model, entry: lit })
        : stay({ ...model, entry: { ...lit, draft: draft.slice(0, caret - 1) + draft.slice(caret), caret: caret - 1 } })
    case "layer":
      return stay({ ...model, entry: { ...lit, layer: key.layer } })
    case "done": {
      const closed = { ...model, entry: undefined, focus: TARGET.name }
      return draft === (setting.value ?? "") ? stay(closed)
        : ask(closed, { _tag: "ChangeSetting", settingId: setting.id, value: draft })
    }
  }
}

/**
 * The step follows Korri: a cursor on a row Korri no longer publishes (a
 * network that left the scan) goes to the step's first row. The keyboard
 * goes down if the name setting goes away.
 */
function settle(model: ZoneOnboardingModel, korri: SurfaceModel): ZoneOnboardingModel {
  const entry = model.entry !== undefined && deviceNameSetting(korri) === undefined ? undefined : model.entry
  const focus = onboardingTargets(model, korri).includes(model.focus) ? model.focus : firstTarget(model, korri)
  return entry === model.entry && focus === model.focus ? model : { ...model, entry, focus }
}
