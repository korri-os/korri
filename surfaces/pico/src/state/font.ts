/**
 * The face this device shows. Pico keeps it, not Korri: the treaty has no
 * setting for it. Read once at start (pico-font-preference.ts), the way Elm
 * reads flags, and stored again through a request when the player changes it.
 */
import type { PicoFontId } from "../pico-fonts"
import { ask, type PicoStep } from "./requests"

export type PicoFontMessage = { readonly _tag: "ChoseFont"; readonly font: PicoFontId }

export function updateFont(_font: PicoFontId, message: PicoFontMessage): PicoStep<PicoFontId> {
  return ask(message.font, { _tag: "RememberFont", font: message.font })
}
