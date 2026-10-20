/**
 * A program for a layer's part, with a pretend Korri behind it.
 *
 * A part shows a page answering its input, so it needs the real model and
 * the real `update` (src/state), and something to stand where Korri does:
 * every request is performed on the fixture host, which records it, and
 * `korri` says what Korri would publish next (a toggle flipped, a name
 * saved). Then the program dispatches `published`, as the binding does.
 *
 * Shapes never import src/state; parts reach it only through this fixture.
 * A ref-backed store read with useSyncExternalStore: no React state.
 */
import type {
  SurfaceGameplayControl,
  SurfaceGameplayOverlayPresentation,
  SurfaceModel,
  SurfaceStatus,
} from "@contracts/surface/korri-surface"
import {
  updateOnboarding,
  ZONE_ONBOARDING_START,
  type ZoneOnboardingMessage,
  type ZoneOnboardingModel,
} from "../state/onboarding"
import { updateOverlay, ZONE_OVERLAY_START, type ZoneOverlayMessage, type ZoneOverlayModel } from "../state/overlay"
import type { ZoneRequest, ZoneStep } from "../state/requests"
import { updateStatus, ZONE_STATUS_START, type ZoneStatusMessage, type ZoneStatusModel } from "../state/status"
import { performZoneRequest } from "../zone-host"
import { createFixtureHost, type FixtureHost } from "./fixture-host"

export interface LayerSnapshot<Model, Context> {
  readonly model: Model
  readonly context: Context
}

export interface LayerProgram<Model, Message, Context> {
  readonly host: FixtureHost
  readonly subscribe: (listener: () => void) => () => void
  readonly get: () => LayerSnapshot<Model, Context>
  readonly dispatch: (message: Message) => void
}

export function createLayerProgram<Model, Message, Context>({
  init,
  update,
  context,
  published,
  korri = (current) => current,
}: {
  readonly init: Model
  readonly update: (model: Model, message: Message, context: Context) => ZoneStep<Model>
  readonly context: Context
  /** The message the binding dispatches when Korri publishes. */
  readonly published: Message
  /** What Korri publishes after a request. */
  readonly korri?: (context: Context, request: ZoneRequest) => Context
}): LayerProgram<Model, Message, Context> {
  const host = createFixtureHost()
  const listeners = new Set<() => void>()
  let snapshot: LayerSnapshot<Model, Context> = {
    model: update(init, published, context).model,
    context,
  }

  const run = (message: Message) => {
    const step = update(snapshot.model, message, snapshot.context)
    let next = { model: step.model, context: snapshot.context }
    let changed = false
    for (const request of step.requests) {
      performZoneRequest(host, request)
      const context = korri(next.context, request)
      if (context !== next.context) {
        next = { ...next, context }
        changed = true
      }
    }
    if (changed) next = { ...next, model: update(next.model, published, next.context).model }
    if (next.model !== snapshot.model || next.context !== snapshot.context) {
      snapshot = next
      for (const listener of [...listeners]) listener()
    }
  }

  return {
    host,
    subscribe(listener) {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
    get: () => snapshot,
    dispatch: run,
  }
}

// ---- What the pretend Korri publishes ----

function applyControl(control: SurfaceGameplayControl, request: ZoneRequest): SurfaceGameplayControl {
  if (request._tag !== "InvokeGameplayControl" || request.controlId !== control.id || request.value === undefined) return control
  const value = request.value
  const interaction = control.interaction
  if (value.kind === "toggle" && interaction.kind === "toggle") return { ...control, interaction: { ...interaction, value: value.value } }
  if (value.kind === "choice" && interaction.kind === "choice") return { ...control, interaction: { ...interaction, value: value.value } }
  if (value.kind === "range" && interaction.kind === "range") return { ...control, interaction: { ...interaction, value: value.value } }
  return control
}

/** Korri applies a control's new value to the overlay it publishes. */
export function korriOverlay(overlay: SurfaceGameplayOverlayPresentation, request: ZoneRequest): SurfaceGameplayOverlayPresentation {
  if (request._tag !== "InvokeGameplayControl" || request.value === undefined) return overlay
  return {
    ...overlay,
    controls: overlay.controls.map(control => applyControl(control, request)),
    groups: overlay.groups.map(group => ({ ...group, controls: group.controls.map(control => applyControl(control, request)) })),
  }
}

/** Korri answers the status layer: a dismissal or a cancel returns to browsing, a retry starts again. */
export function korriStatus(status: SurfaceStatus, request: ZoneRequest): SurfaceStatus {
  switch (request._tag) {
    case "DismissProblem":
    case "RunAction":
      return { _tag: "Browsing" }
    case "Retry":
      return status._tag === "Problem"
        ? { _tag: "Busy", kicker: "Starting", ...(status.gameId === undefined ? {} : { gameId: status.gameId }) }
        : status
    default:
      return status
  }
}

/** Korri answers first boot: a join connects, a name is saved. */
export function korriOnboarding(model: SurfaceModel, request: ZoneRequest): SurfaceModel {
  switch (request._tag) {
    case "ConnectWifi": {
      const network = model.network
      const wifi = network?.wifi
      const joined = wifi?._tag === "Available" ? wifi.networks.find(item => item.id === request.networkId) : undefined
      if (network === undefined || joined === undefined) return model
      return { ...model, network: { ...network, connection: { _tag: "Wifi", ssid: joined.ssid, strengthPercent: joined.strengthPercent } } }
    }
    case "ChangeSetting":
      return {
        ...model,
        settings: model.settings.map(group => ({
          ...group,
          items: group.items.map(item => item.id === request.settingId ? { ...item, value: request.value } : item),
        })),
      }
    default:
      return model
  }
}

// ---- The three layer programs, as parts use them ----

/** The status layer over a page, Korri answering its requests. */
export const createStatusProgram = (status: SurfaceStatus) =>
  createLayerProgram<ZoneStatusModel, ZoneStatusMessage, SurfaceStatus>({
    init: ZONE_STATUS_START,
    update: updateStatus,
    context: status,
    published: { _tag: "KorriPublished" },
    korri: korriStatus,
  })

/** PAUSED, Korri applying every value it is sent. */
export const createOverlayProgram = (overlay: SurfaceGameplayOverlayPresentation, init: Partial<ZoneOverlayModel> = {}) =>
  createLayerProgram<ZoneOverlayModel, ZoneOverlayMessage, SurfaceGameplayOverlayPresentation>({
    init: { ...ZONE_OVERLAY_START, ...init },
    update: updateOverlay,
    context: overlay,
    published: { _tag: "KorriPublished" },
    korri: korriOverlay,
  })

/** First boot, Korri joining networks and saving the name. */
export const createOnboardingProgram = (model: SurfaceModel, init: Partial<ZoneOnboardingModel> = {}) =>
  createLayerProgram<ZoneOnboardingModel, ZoneOnboardingMessage, SurfaceModel>({
    init: { ...ZONE_ONBOARDING_START, ...init },
    update: updateOnboarding,
    context: model,
    published: { _tag: "KorriPublished" },
    korri: korriOnboarding,
  })

/** What the binding reads from Korri's model for first boot, for parts. */
export { canGoOn, deviceNameSetting } from "../state/onboarding"
