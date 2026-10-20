/**
 * The status layer: what Zone draws over any page while Korri's status is
 * not Browsing, as one value, and the one function that changes it.
 *
 *     shown    which layer is up: none, busy, running or problem. It follows
 *              Korri's status on every message, so it can never be stale.
 *     focus    the index of the lit button in the layer: one of Korri's busy
 *              actions, or DISMISS (0) and RETRY (1) in the problem dialog.
 *
 * Focus moves into the layer when it opens with a button to hold it (busy
 * with actions, problem) and goes back to where it was when it closes. The
 * index lives here; the DOM element to go back to is a mechanic the layer
 * keeps in a ref (src/use-zone-layer-focus.ts).
 *
 * Korri's SurfaceStatus is not stored here. `update` receives it beside this
 * value, as navigation receives the whole model.
 */
import type { SurfaceStatus } from "@contracts/surface/korri-surface"
import { ask, stay, type ZoneStep } from "./requests"

export type ZoneStatusShown = "none" | "busy" | "running" | "problem"

export interface ZoneStatusModel {
  readonly shown: ZoneStatusShown
  readonly focus: number
}

export const ZONE_STATUS_START: ZoneStatusModel = { shown: "none", focus: 0 }

/** The problem dialog's buttons, left to right. DISMISS is lit first. */
export type ZoneProblemButton = "dismiss" | "retry"

export type ZoneStatusMessage =
  /** Korri published a model. The layer follows its status. */
  | { readonly _tag: "KorriPublished" }
  /** The host's Back. A problem is dismissed; busy work is never cancelled by Back. */
  | { readonly _tag: "PressedBack" }
  /** The cursor is on a button of the layer. */
  | { readonly _tag: "FocusedStatusButton"; readonly index: number }
  /** A on one of Korri's busy actions. */
  | { readonly _tag: "ChoseBusyAction"; readonly actionId: string }
  /** A on DISMISS or RETRY. */
  | { readonly _tag: "ChoseProblemButton"; readonly button: ZoneProblemButton }

/** Which layer Korri's status calls for. */
export function shownFor(status: SurfaceStatus): ZoneStatusShown {
  switch (status._tag) {
    case "Browsing":
      return "none"
    case "Busy":
      return "busy"
    case "Running":
      return "running"
    case "Problem":
      return "problem"
  }
}

/** The problem dialog's buttons for this status: RETRY only when Korri can. */
export function problemButtons(status: SurfaceStatus): readonly ZoneProblemButton[] {
  return status._tag === "Problem" && status.canRetry ? ["dismiss", "retry"] : ["dismiss"]
}

/** How many buttons the layer holds now. */
export function statusButtonCount(status: SurfaceStatus): number {
  switch (status._tag) {
    case "Busy":
      return status.actions?.length ?? 0
    case "Problem":
      return problemButtons(status).length
    case "Browsing":
    case "Running":
      return 0
  }
}

/** Whether focus belongs in the layer: it has a button to hold it. */
export const statusTakesFocus = (status: SurfaceStatus): boolean => statusButtonCount(status) > 0

export function updateStatus(
  model: ZoneStatusModel,
  message: ZoneStatusMessage,
  status: SurfaceStatus,
): ZoneStep<ZoneStatusModel> {
  const result = step(model, message, status)
  return { model: settle(result.model, status), requests: result.requests }
}

function step(model: ZoneStatusModel, message: ZoneStatusMessage, status: SurfaceStatus): ZoneStep<ZoneStatusModel> {
  switch (message._tag) {
    case "KorriPublished":
      return stay(model)
    case "PressedBack":
      return status._tag === "Problem" ? ask(model, { _tag: "DismissProblem" }) : stay(model)
    case "FocusedStatusButton":
      return stay({ ...model, focus: Math.max(0, message.index) })
    case "ChoseBusyAction": {
      if (status._tag !== "Busy") return stay(model)
      const action = status.actions?.find(candidate => candidate.id === message.actionId)
      return action?.enabled === true ? ask(model, { _tag: "RunAction", actionId: action.id }) : stay(model)
    }
    case "ChoseProblemButton":
      if (status._tag !== "Problem") return stay(model)
      if (message.button === "retry") return status.canRetry ? ask(model, { _tag: "Retry" }) : stay(model)
      return ask(model, { _tag: "DismissProblem" })
  }
}

/**
 * The layer follows Korri. A layer that changes opens with its first button
 * lit (DISMISS before RETRY); a focus past the buttons Korri now offers is
 * brought back in.
 */
function settle(model: ZoneStatusModel, status: SurfaceStatus): ZoneStatusModel {
  const shown = shownFor(status)
  const count = statusButtonCount(status)
  const focus = shown !== model.shown ? 0 : Math.min(model.focus, Math.max(0, count - 1))
  return shown === model.shown && focus === model.focus ? model : { shown, focus }
}
