/**
 * Pico, Korri's fantasy-console surface, as a native surface package
 * (docs/briefs/2026-10-31-native-surfaces-contract.md, section 3.1).
 *
 * `main.slint` draws every pixel; this file is bundled to `logic.js` and
 * runs in the shell's QuickJS. The behaviour is Pico's own, in src/state;
 * src/native adapts it to the contract (see src/native/program.ts).
 *
 * Rebuild after any change: `bun run build` (see README.md).
 */
import type { KorriModel, SurfaceMessage, SurfacePreferenceDeclarations } from "./src/model"
import { PICO_DEFAULT_FONT } from "./src/pico-fonts"
import { PICO_FONT_KEY } from "./src/pico-font-preference"
import { init, overlay, update, view } from "./src/native/program"
import type { State } from "./src/native/types"

const preferences: SurfacePreferenceDeclarations = {
  // A face suits a panel, not a person: it stays on this device.
  [PICO_FONT_KEY]: { mark: "device", initial: PICO_DEFAULT_FONT },
}

;(globalThis as { surface?: unknown }).surface = {
  // As the portal registered Pico: it draws on every display it is given.
  displays: "all",
  preferences,
  init: (model: KorriModel, stored: Readonly<Record<string, string>>): State => init(model, stored),
  update: (state: State, message: SurfaceMessage, model: KorriModel) => update(state, message, model),
  view: (state: State, model: KorriModel) => view(state, model),
  overlay: (state: State, model: KorriModel) => overlay(state, model),
}
