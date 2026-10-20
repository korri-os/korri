/**
 * The look this device shows Zone in (src/plan/look.ts). Zone keeps it, not
 * Korri: the treaty has no setting for it. Read once at start
 * (zone-look-preference.ts, by the runtime), the way Elm reads flags, and
 * stored again through a request whenever the person steps it on OPTIONS'
 * LOOK row, as Pico keeps its face (surfaces/pico/src/state/font.ts).
 */
import { zoneLookStep, type ZoneLook } from "../plan/look"
import { ask, stay, type ZoneStep } from "./requests"

export type ZoneLookMessage =
  /** Left or right on the LOOK row; A steps it right, as on every choice. */
  | { readonly _tag: "SteppedLook"; readonly direction: -1 | 1 }

export function updateLook(look: ZoneLook, message: ZoneLookMessage): ZoneStep<ZoneLook> {
  switch (message._tag) {
    case "SteppedLook": {
      const next = zoneLookStep(look, message.direction)
      return next === look ? stay(look) : ask(next, { _tag: "RememberLook", look: next })
    }
  }
}
