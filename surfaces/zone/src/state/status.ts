/**
 * The status layer: what Zone draws over any page while Korri's status is
 * not Browsing, as one value, and the one function that changes it.
 *
 *     shown    which layer is up: none, busy, running, problem, confirm,
 *              paused or ended. It
 *              follows Korri's status on every message, so it can never be
 *              stale.
 *     focus    the index of the lit button in the layer: one of Korri's busy
 *              actions, DISMISS (0) and RETRY (1) in the problem dialog,
 *              one of Korri's confirm or paused actions in the host's
 *              order, or DISMISS (0) when a stream session ended.
 *
 * Focus moves into the layer when it opens with a button to hold it (busy
 * with actions, problem, confirm) and goes back to where it was when it closes. The
 * index lives here; the DOM element to go back to is a mechanic the layer
 * keeps in a ref (src/use-zone-layer-focus.ts).
 *
 * Korri's SurfaceStatus is not stored here. `update` receives it beside this
 * value, as navigation receives the whole model.
 */
import type { SurfaceAction, SurfaceStatus } from "@contracts/surface/korri-surface"
import { ask, stay, type ZoneStep } from "./requests"

export type ZoneStatusShown = "none" | "busy" | "running" | "problem" | "confirm" | "paused" | "ended"

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
  /**
   * The host's Back. A problem, and a stream session that ended, are
   * dismissed; busy work is never cancelled by Back, a question is never
   * answered by it, and a paused stream is never left by it: Korri's actions
   * are the only ways out, and Zone never infers which one declines.
   */
  | { readonly _tag: "PressedBack" }
  /** The cursor is on a button of the layer. */
  | { readonly _tag: "FocusedStatusButton"; readonly index: number }
  /** A on one of Korri's busy actions. */
  | { readonly _tag: "ChoseBusyAction"; readonly actionId: string }
  /** A on DISMISS or RETRY. */
  | { readonly _tag: "ChoseProblemButton"; readonly button: ZoneProblemButton }
  /** A on one of Korri's answers to its question. */
  | { readonly _tag: "ChoseConfirmAction"; readonly actionId: string }
  /** A on one of Korri's actions while the device holding the stream paused it. */
  | { readonly _tag: "ChosePausedAction"; readonly actionId: string }
  /** A on DISMISS after the stream session this device played on ended. */
  | { readonly _tag: "ChoseEndedDismiss" }

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
    case "Confirm":
    case "ChooseSave":
      return "confirm"
    case "Paused":
      return "paused"
    case "Ended":
      return "ended"
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
    case "Confirm":
      return status.actions.length
    case "ChooseSave":
      // One answer per side, then Korri's own (src/zone-save-split.ts).
      return status.sides.length + status.actions.length
    case "Paused":
      return status.actions.length
    case "Ended":
      return 1
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
      return status._tag === "Problem" || status._tag === "Ended" ? ask(model, { _tag: "DismissProblem" }) : stay(model)
    case "FocusedStatusButton":
      return stay({ ...model, focus: Math.max(0, message.index) })
    case "ChoseBusyAction":
      return status._tag === "Busy" ? runOffered(model, status.actions, message.actionId) : stay(model)
    case "ChoseProblemButton":
      if (status._tag !== "Problem") return stay(model)
      if (message.button === "retry") return status.canRetry ? ask(model, { _tag: "Retry" }) : stay(model)
      return ask(model, { _tag: "DismissProblem" })
    case "ChoseConfirmAction":
      return status._tag === "Confirm" ? runOffered(model, status.actions, message.actionId) : stay(model)
    case "ChosePausedAction":
      return status._tag === "Paused" ? runOffered(model, status.actions, message.actionId) : stay(model)
    case "ChoseEndedDismiss":
      return status._tag === "Ended" ? ask(model, { _tag: "DismissProblem" }) : stay(model)
  }
}

/** Run one of Korri's actions by its id, only while Korri still offers it,
 * enabled, in the layer up now. */
function runOffered(model: ZoneStatusModel, actions: readonly SurfaceAction[] | undefined, actionId: string): ZoneStep<ZoneStatusModel> {
  const action = actions?.find(candidate => candidate.id === actionId)
  return action?.enabled === true ? ask(model, { _tag: "RunAction", actionId: action.id }) : stay(model)
}

/**
 * The button a layer opens on: the first. Korri's question, and a paused
 * stream's actions, open on the first enabled action, since an inert button cannot hold the cursor; with
 * none enabled, on the first.
 */
function opening(status: SurfaceStatus): number {
  if (status._tag !== "Confirm" && status._tag !== "Paused") return 0
  return Math.max(0, status.actions.findIndex(action => action.enabled))
}

/**
 * The layer follows Korri. A layer that changes opens with its first button
 * lit (DISMISS before RETRY, Korri's first enabled answer); a focus past the
 * buttons Korri now offers is brought back in.
 */
function settle(model: ZoneStatusModel, status: SurfaceStatus): ZoneStatusModel {
  const shown = shownFor(status)
  const count = statusButtonCount(status)
  const focus = shown !== model.shown ? opening(status) : Math.min(model.focus, Math.max(0, count - 1))
  return shown === model.shown && focus === model.focus ? model : { shown, focus }
}
