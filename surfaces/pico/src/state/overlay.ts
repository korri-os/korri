/**
 * The gameplay overlay: Pico's pause menu over a running game.
 *
 * Its only state is the question a destructive control asks before it runs.
 * Back, Menu and System all dismiss: legacy bound RESUME to B, and the host's
 * menu and system buttons are how the overlay was opened, so pressing either
 * again closes it. Back withdraws a question before it dismisses anything.
 */
import type { PicoOverlayControlView } from "../pico-overlay-view"
import type { PicoHostButton } from "./messages"
import { ask, type PicoRequest, type PicoStep, stay } from "./requests"

export interface PicoOverlayState {
  readonly asking: PicoOverlayControlView | undefined
}

export type PicoOverlayMessage =
  | PicoHostButton
  | { readonly _tag: "AskedControl"; readonly control: PicoOverlayControlView }
  | { readonly _tag: "CancelledControl" }
  | { readonly _tag: "ConfirmedControl" }
  | { readonly _tag: "InvokedControl"; readonly control: PicoOverlayControlView }
  /** A range settled on a new value. */
  | { readonly _tag: "AdjustedControl"; readonly control: PicoOverlayControlView; readonly value: number }
  | { readonly _tag: "PressedRetry" }

export const initialOverlay: PicoOverlayState = { asking: undefined }

const invoke = (control: PicoOverlayControlView): PicoRequest =>
  ({ _tag: "InvokeGameplayControl", controlId: control.id, value: control.sends })

export function updateOverlay(state: PicoOverlayState, message: PicoOverlayMessage): PicoStep<PicoOverlayState> {
  switch (message._tag) {
    case "PressedBack":
      return state.asking !== undefined ? stay(initialOverlay) : ask(state, { _tag: "DismissGameplayOverlay" })
    case "PressedMenu":
    case "PressedSystem":
      return ask(state, { _tag: "DismissGameplayOverlay" })
    case "PressedOptions":
      return stay(state)
    case "AskedControl":
      return stay({ asking: message.control })
    case "CancelledControl":
      return stay(initialOverlay)
    case "ConfirmedControl":
      return state.asking === undefined ? stay(state) : ask(initialOverlay, invoke(state.asking))
    case "InvokedControl":
      return ask(state, invoke(message.control))
    case "AdjustedControl":
      return ask(state, {
        _tag: "InvokeGameplayControl",
        controlId: message.control.id,
        value: { kind: "range", value: message.value },
      })
    case "PressedRetry":
      return ask(state, { _tag: "Retry" })
  }
}
