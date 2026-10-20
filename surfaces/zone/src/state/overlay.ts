/**
 * PAUSED: the gameplay overlay's own state, as one value, and the one
 * function that changes it.
 *
 *     focusedId  the control the cursor is on; the first control when the
 *                overlay opens, so focus moves into the wedge at once
 *     asking     a destructive control waiting on NO / YES, with the value
 *                it would send; NO is lit first
 *     answer     the lit button of that question: 0 is NO, 1 is YES
 *     moving     a range's value ahead of Korri, shown while it moves
 *     leaving    a control that closes the overlay on success was invoked;
 *                further presses wait for Korri's next model
 *
 * Korri owns the overlay (`SurfaceGameplayOverlayPresentation`); `update`
 * receives it beside this value and never stores it. Every change of value
 * is invoked as it happens: a range is asked once per step.
 *
 * `dismissOnSuccess` is the host's to act on. The portal's overlay
 * controller returns to the game itself after a control with it succeeds
 * (clients/portal/src/overlay/live-overlay-controller.ts), so Zone sends no
 * DismissGameplayOverlay of its own; a failure stays on screen as Korri's
 * Problem instead of closing the overlay under it.
 */
import type {
  SurfaceGameplayControl,
  SurfaceGameplayControlValue,
  SurfaceGameplayOverlayPresentation,
} from "@contracts/surface/korri-surface"
import { ask, stay, type ZoneRequest, type ZoneStep } from "./requests"

export interface ZoneOverlayQuestion {
  readonly controlId: string
  readonly value?: SurfaceGameplayControlValue
}

export interface ZoneOverlayModel {
  readonly focusedId: string | undefined
  readonly asking: ZoneOverlayQuestion | undefined
  readonly answer: 0 | 1
  readonly moving: Readonly<Record<string, number>>
  readonly leaving: boolean
}

export const ZONE_OVERLAY_START: ZoneOverlayModel = {
  focusedId: undefined,
  asking: undefined,
  answer: 0,
  moving: {},
  leaving: false,
}

export type ZoneOverlayMessage =
  /** Korri published a model: a range it caught up with stops moving. */
  | { readonly _tag: "KorriPublished" }
  /** The host's Back: withdraws a question, else asks to return to the game. */
  | { readonly _tag: "PressedBack" }
  | { readonly _tag: "FocusedControl"; readonly controlId: string }
  /** A on a row. */
  | { readonly _tag: "PressedControl"; readonly controlId: string }
  /** Left or right on a toggle, choice or range row. */
  | { readonly _tag: "SteppedControl"; readonly controlId: string; readonly direction: -1 | 1 }
  /** The cursor is on NO (0) or YES (1). */
  | { readonly _tag: "FocusedAnswer"; readonly index: 0 | 1 }
  /** A on NO or YES. */
  | { readonly _tag: "ChoseAnswer"; readonly yes: boolean }

/** Every control in the overlay, flat ones first, then each group's. */
export function overlayControls(overlay: SurfaceGameplayOverlayPresentation): readonly SurfaceGameplayControl[] {
  return [...overlay.controls, ...overlay.groups.flatMap(group => group.controls)]
}

const findControl = (overlay: SurfaceGameplayOverlayPresentation, id: string | undefined) =>
  id === undefined ? undefined : overlayControls(overlay).find(control => control.id === id)

/** The number of decimals a step needs, so 0.1 + 0.2 shows 0.3. */
function placesOf(step: number): number {
  const text = String(step)
  const dot = text.indexOf(".")
  return dot === -1 ? 0 : text.length - dot - 1
}

/** One step of a range, kept within min and max. */
export function rangeStep(value: number, min: number, max: number, step: number, direction: -1 | 1): number {
  const next = Number((value + direction * step).toFixed(placesOf(step)))
  return Math.min(max, Math.max(min, next))
}

/** The value a range shows: the player's while it moves, else Korri's. */
export function shownRangeValue(model: ZoneOverlayModel, control: SurfaceGameplayControl): number | undefined {
  return control.interaction.kind === "range" ? model.moving[control.id] ?? control.interaction.value : undefined
}

/** What a step sends for this control, or nothing when the step changes nothing. */
function stepValue(
  model: ZoneOverlayModel,
  control: SurfaceGameplayControl,
  direction: -1 | 1,
): SurfaceGameplayControlValue | undefined {
  const interaction = control.interaction
  switch (interaction.kind) {
    case "command":
      return undefined
    case "toggle":
      return { kind: "toggle", value: !interaction.value }
    case "choice": {
      const count = interaction.options.length
      if (count < 2) return undefined
      const at = interaction.options.findIndex(option => option.value === interaction.value)
      const next = interaction.options[((at === -1 ? 0 : at + direction) % count + count) % count]
      return next === undefined ? undefined : { kind: "choice", value: next.value }
    }
    case "range": {
      const from = model.moving[control.id] ?? interaction.value
      const next = rangeStep(from, interaction.min, interaction.max, interaction.step, direction)
      return next === from ? undefined : { kind: "range", value: next }
    }
  }
}

/** Invoke a control with its value. A range's value shows at once. */
function invoke(
  model: ZoneOverlayModel,
  control: SurfaceGameplayControl,
  value: SurfaceGameplayControlValue | undefined,
): ZoneStep<ZoneOverlayModel> {
  const request: ZoneRequest = value === undefined
    ? { _tag: "InvokeGameplayControl", controlId: control.id }
    : { _tag: "InvokeGameplayControl", controlId: control.id, value }
  const moving = value?.kind === "range" ? { ...model.moving, [control.id]: value.value } : model.moving
  return ask({ ...model, moving, leaving: model.leaving || control.dismissOnSuccess }, request)
}

/** A change to a control: asked first when it is destructive, else sent. */
function change(
  model: ZoneOverlayModel,
  control: SurfaceGameplayControl,
  value: SurfaceGameplayControlValue | undefined,
): ZoneStep<ZoneOverlayModel> {
  if (control.destructive) {
    return stay({ ...model, asking: value === undefined ? { controlId: control.id } : { controlId: control.id, value }, answer: 0 })
  }
  return invoke(model, control, value)
}

export function updateOverlay(
  model: ZoneOverlayModel,
  message: ZoneOverlayMessage,
  overlay: SurfaceGameplayOverlayPresentation,
): ZoneStep<ZoneOverlayModel> {
  const result = step(model, message, overlay)
  return { model: settle(result.model, overlay), requests: result.requests }
}

function step(
  model: ZoneOverlayModel,
  message: ZoneOverlayMessage,
  overlay: SurfaceGameplayOverlayPresentation,
): ZoneStep<ZoneOverlayModel> {
  switch (message._tag) {
    case "KorriPublished":
      return stay({ ...model, leaving: false })
    case "PressedBack":
      return model.asking !== undefined
        ? stay({ ...model, asking: undefined, answer: 0 })
        : ask(model, { _tag: "DismissGameplayOverlay" })
    case "FocusedControl": {
      if (model.asking !== undefined) return stay(model)
      // The cursor leaving a range hands its value back to Korri's.
      const left = model.focusedId !== undefined && model.focusedId !== message.controlId
        && model.moving[model.focusedId] !== undefined
      const moving = left ? omit(model.moving, model.focusedId!) : model.moving
      return stay({ ...model, focusedId: message.controlId, moving })
    }
    case "PressedControl":
      return press(model, message.controlId, overlay)
    case "SteppedControl": {
      const control = findControl(overlay, message.controlId)
      if (control === undefined || !control.enabled || model.leaving || model.asking !== undefined) return stay(model)
      if (control.interaction.kind === "command") return stay(model)
      const value = stepValue(model, control, message.direction)
      return value === undefined ? stay(model) : change({ ...model, focusedId: control.id }, control, value)
    }
    case "FocusedAnswer":
      return model.asking === undefined ? stay(model) : stay({ ...model, answer: message.index })
    case "ChoseAnswer": {
      const question = model.asking
      if (question === undefined) return stay(model)
      const closed = { ...model, asking: undefined, answer: 0 as const }
      const control = findControl(overlay, question.controlId)
      if (!message.yes || control === undefined || !control.enabled) return stay(closed)
      return invoke(closed, control, question.value)
    }
  }
}

/** A on a row. */
function press(
  model: ZoneOverlayModel,
  controlId: string,
  overlay: SurfaceGameplayOverlayPresentation,
): ZoneStep<ZoneOverlayModel> {
  const control = findControl(overlay, controlId)
  if (model.asking !== undefined || control === undefined) return stay(model)
  // A disabled row takes the cursor and nothing else: its reason is under it.
  if (!control.enabled || model.leaving) return stay({ ...model, focusedId: control.id })
  const focused = { ...model, focusedId: control.id }
  switch (control.interaction.kind) {
    case "command":
      return change(focused, control, undefined)
    // A on a toggle flips it and A on a choice steps it forward, as right does.
    case "toggle":
    case "choice":
      return change(focused, control, stepValue(focused, control, 1))
    // There is no one right answer to "press Volume": A on a range does nothing.
    case "range":
      return stay(focused)
  }
}

function omit(record: Readonly<Record<string, number>>, key: string): Readonly<Record<string, number>> {
  const { [key]: _gone, ...rest } = record
  return rest
}

/**
 * The overlay follows Korri. The cursor stays on a control Korri still
 * publishes, else goes to the first one. A question about a control that
 * is gone or disabled is withdrawn. A range stops moving once Korri shows
 * its value, or when Korri stops offering it.
 */
function settle(model: ZoneOverlayModel, overlay: SurfaceGameplayOverlayPresentation): ZoneOverlayModel {
  const controls = overlayControls(overlay)
  const focusedId = controls.some(control => control.id === model.focusedId) ? model.focusedId : controls[0]?.id
  const askedControl = findControl(overlay, model.asking?.controlId)
  const asking = askedControl?.enabled === true ? model.asking : undefined
  let moving = model.moving
  for (const id of Object.keys(model.moving)) {
    const control = findControl(overlay, id)
    const caughtUp = control?.interaction.kind === "range" && control.interaction.value === model.moving[id]
    if (control === undefined || !control.enabled || control.interaction.kind !== "range" || caughtUp) {
      moving = omit(moving, id)
    }
  }
  const same = focusedId === model.focusedId && asking === model.asking && moving === model.moving
  return same ? model : { ...model, focusedId, asking, answer: asking === undefined ? 0 : model.answer, moving }
}
