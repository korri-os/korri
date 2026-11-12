/**
 * Velvet's logic for the native shell (docs/briefs/2026-10-31-native-surfaces-contract.md,
 * section 3.1). Bundled to logic.js:
 *
 *     bun build logic.ts --outfile logic.js --format iife --target browser --minify
 *
 * A thin shell around src/: `init` reads the stored preference, `update`
 * routes a message (src/update.ts), `view` and `overlay` draw the state
 * (src/view.ts). The same state and model give the last view back, so a
 * call that changed nothing costs only its JSON.
 *
 *   preferences  velvet.motion, mark `device`: "full" or "reduced". A device
 *                that cannot hold 60 frames a second, or a person who wants
 *                stillness, sets it on that device; it does not follow them.
 *   displays     "main": Velvet is one stage.
 *
 * Pure: no timers, no Date.now, no randomness. "Now" is the model's
 * `publishedAt`; idle time is the shell's Tick.
 */
import type { SurfaceMessage as Message, SurfaceModel as Model, SurfaceRequest as Request } from "../../../../contracts/generated/surface"
import { onForget } from "./src/memo"
import type { State } from "./src/types"
import { MOTION_KEY, start, update as step } from "./src/update"
import { overlayView, view as draw } from "./src/view"

export type { State }

export function init(model: Model, stored: Readonly<Record<string, string>>): State {
  return start(model, stored)
}

export function update(state: State, message: Message, model: Model): { state: State; requests: Request[] } {
  return step(state, message, model)
}

// The view of the very state and model drawn last (src/memo.ts has the rule for everything kept).
let lastView: { state: State; model: Model; view: ReturnType<typeof draw> } | undefined
let lastOverlay: { state: State; model: Model; view: ReturnType<typeof overlayView> } | undefined
onForget(() => {
  lastView = undefined
  lastOverlay = undefined
})

export function view(state: State, model: Model) {
  if (lastView?.state === state && lastView.model === model) return lastView.view
  lastView = { state, model, view: draw(state, model) }
  return lastView.view
}

export function overlay(state: State, model: Model) {
  if (lastOverlay?.state === state && lastOverlay.model === model) return lastOverlay.view
  lastOverlay = { state, model, view: overlayView(state, model) }
  return lastOverlay.view
}

// Motion stays on the device (ADR 0018's rule by kind): what a screen and a
// person's eyes can take is a fact about that device and that room.
const preferences = { [MOTION_KEY]: { mark: "device", initial: "full" } } as const

const displays = "main"

globalThis.surface = { displays, preferences, init, update, view, overlay }

declare global {
  // The one global the contract allows.
  var surface: {
    displays: typeof displays
    preferences: typeof preferences
    init: typeof init
    update: typeof update
    view: typeof view
    overlay: typeof overlay
  }
}
