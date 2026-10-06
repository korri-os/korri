/**
 * The gameplay overlay: Pico's pause menu over a running game.
 *
 * Back, Menu and System all dismiss: legacy bound RESUME to B, and the host's
 * menu and system buttons are how the overlay was opened, so pressing either
 * again closes it. Back withdraws a question before it dismisses anything.
 *
 * A range moves on every step, so the screen answers the thumb at once. Korri
 * is asked once per gesture: when a held direction is released, or shortly
 * after the last step when the input reports no release. A held d-pad repeats
 * many times a second, and asking Korri on every repeat would queue a stale
 * value behind each fresh one. The timings and the release matching are
 * Shift's (surfaces/shift/src/ui/molecules/ShiftSheetRange.tsx). A value Korri
 * republishes replaces the local one and drops any step not yet sent.
 */
import {
  type PicoOverlayControlView,
  type PicoOverlayRange,
  type PicoOverlayView,
  type PicoRangeRelease,
  type PicoRangeStepRequest,
  picoRangeStep,
} from "../pico-overlay-view"
import type { PicoHostButton } from "./messages"
import { ask, type PicoReply, type PicoRequest, type PicoStep, stay } from "./requests"

/** How long a step with no release edge waits before Korri is asked. */
export const RANGE_COMMIT_DELAY_MS = 180
/** How long a held direction waits for its release before Korri is asked anyway. */
export const RANGE_RELEASE_FALLBACK_MS = 2000

/** A range the player is moving, ahead of Korri. */
export interface RangeEdit {
  readonly value: number
  /** Korri's range and enabled state when the edit began. A change drops the edit. */
  readonly base: PicoOverlayRange
  readonly enabled: boolean
  /** The last step not yet sent, and the wait that will send it. */
  readonly pending: { readonly request: PicoRangeStepRequest; readonly token: number } | undefined
}

export interface PicoOverlayState {
  readonly asking: PicoOverlayControlView | undefined
  readonly ranges: Readonly<Record<string, RangeEdit>>
  readonly nextToken: number
}

export type PicoOverlayMessage =
  | PicoHostButton
  | PicoReply
  /** Korri published a new overlay or status. */
  | { readonly _tag: "KorriPublished" }
  | { readonly _tag: "AskedControl"; readonly control: PicoOverlayControlView }
  | { readonly _tag: "CancelledControl" }
  | { readonly _tag: "ConfirmedControl" }
  | { readonly _tag: "InvokedControl"; readonly control: PicoOverlayControlView }
  | { readonly _tag: "SteppedRange"; readonly control: PicoOverlayControlView; readonly request: PicoRangeStepRequest }
  | { readonly _tag: "ReleasedRange"; readonly control: PicoOverlayControlView; readonly ended: PicoRangeRelease }
  /** The cursor left a range. A step it holds is sent now. */
  | { readonly _tag: "LeftRange"; readonly control: PicoOverlayControlView }
  | { readonly _tag: "PressedRetry" }

export const initialOverlay: PicoOverlayState = { asking: undefined, ranges: {}, nextToken: 0 }

const invoke = (control: PicoOverlayControlView): PicoRequest =>
  ({ _tag: "InvokeGameplayControl", controlId: control.id, value: control.sends })

export function updateOverlay(
  state: PicoOverlayState,
  message: PicoOverlayMessage,
  overlay: PicoOverlayView,
): PicoStep<PicoOverlayState> {
  const result = step(state, message)
  const ranges = settleRanges(result.model.ranges, overlay)
  return { model: ranges === result.model.ranges ? result.model : { ...result.model, ranges }, requests: result.requests }
}

function step(state: PicoOverlayState, message: PicoOverlayMessage): PicoStep<PicoOverlayState> {
  switch (message._tag) {
    case "PressedBack":
      return state.asking !== undefined ? stay({ ...state, asking: undefined }) : dismiss(state)
    case "PressedMenu":
    case "PressedSystem":
      return dismiss(state)
    case "PressedOptions":
    case "KorriPublished":
    case "RenderedQr":
      return stay(state)
    case "AskedControl":
      return stay({ ...state, asking: message.control })
    case "CancelledControl":
      return stay({ ...state, asking: undefined })
    case "ConfirmedControl":
      return state.asking === undefined ? stay(state) : ask({ ...state, asking: undefined }, invoke(state.asking))
    case "InvokedControl":
      return ask(state, invoke(message.control))
    case "PressedRetry":
      return ask(state, { _tag: "Retry" })

    case "SteppedRange": {
      const { control, request } = message
      const range = control.range
      if (!control.enabled || range === undefined) return stay(state)
      const edit = state.ranges[control.id]
      const current = edit?.value ?? range.value
      const next = picoRangeStep({ ...range, value: current }, request.way)
      if (next === current) return stay(state)
      const token = state.nextToken
      return ask(
        {
          ...state,
          nextToken: token + 1,
          ranges: {
            ...state.ranges,
            [control.id]: { value: next, base: range, enabled: control.enabled, pending: { request, token } },
          },
        },
        { _tag: "Wait", ms: request.releaseExpected ? RANGE_RELEASE_FALLBACK_MS : RANGE_COMMIT_DELAY_MS, token },
      )
    }
    case "ReleasedRange": {
      const pending = state.ranges[message.control.id]?.pending?.request
      const ended = message.ended
      if (pending === undefined || pending.way !== ended.way
        || pending.gestureId !== ended.gestureId || pending.source !== ended.source) return stay(state)
      return flush(state, [message.control.id])
    }
    case "LeftRange":
      return flush(state, [message.control.id])
    case "Waited":
      return flush(state, Object.keys(state.ranges).filter(id => state.ranges[id]?.pending?.token === message.token))
  }
}

/** Leaving sends any step a range still holds, then asks to return to the game. */
function dismiss(state: PicoOverlayState): PicoStep<PicoOverlayState> {
  const sent = flush(state, Object.keys(state.ranges))
  return ask(sent.model, ...sent.requests, { _tag: "DismissGameplayOverlay" })
}

/** Send the held step of each named range. */
function flush(state: PicoOverlayState, ids: readonly string[]): PicoStep<PicoOverlayState> {
  const requests: PicoRequest[] = []
  let ranges = state.ranges
  for (const id of ids) {
    const edit = ranges[id]
    if (edit?.pending === undefined) continue
    ranges = { ...ranges, [id]: { ...edit, pending: undefined } }
    requests.push({ _tag: "InvokeGameplayControl", controlId: id, value: { kind: "range", value: edit.value } })
  }
  return requests.length === 0 ? stay(state) : ask({ ...state, ranges }, ...requests)
}

/** Drop the edit of every range Korri changed, disabled or stopped publishing. */
function settleRanges(
  ranges: Readonly<Record<string, RangeEdit>>,
  overlay: PicoOverlayView,
): Readonly<Record<string, RangeEdit>> {
  const ids = Object.keys(ranges)
  if (ids.length === 0) return ranges
  const controls = [...overlay.controls, ...overlay.groups.flatMap(group => group.controls)]
  const kept = ids.filter(id => {
    const edit = ranges[id]
    const control = controls.find(candidate => candidate.id === id)
    return edit !== undefined && control?.range !== undefined && control.enabled === edit.enabled
      && sameRange(control.range, edit.base)
  })
  return kept.length === ids.length ? ranges : Object.fromEntries(kept.map(id => [id, ranges[id]!]))
}

const sameRange = (a: PicoOverlayRange, b: PicoOverlayRange): boolean =>
  a.value === b.value && a.min === b.min && a.max === b.max && a.step === b.step

/** The value a range shows: the player's edit, or Korri's. */
export const rangeValue = (state: PicoOverlayState, control: PicoOverlayControlView): number | undefined =>
  state.ranges[control.id]?.value ?? control.range?.value
