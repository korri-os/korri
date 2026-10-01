import { PICO_DEFAULT_FONT, PICO_FONT_IDS, type PicoFontId } from "./pico-fonts"

/**
 * Which face this device shows Pico in, kept by Pico itself.
 *
 * The storage seam, stated plainly: one key, `pico.font`, holding one face id
 * and nothing else. It is a display preference — no identity, no capability, no
 * secret — so it lives in the browser's local storage, which the device keeps
 * across restarts, the same way the portal keeps its `korri.surface`. Korri is
 * never told: a face is Pico's business, and the treaty has no word for it.
 *
 * A missing, unreadable or unknown value means the default face. A tampered
 * value can do no more than pick another of Pico's own faces.
 */
export const PICO_FONT_KEY = "pico.font"

export function isPicoFont(value: string | null | undefined): value is PicoFontId {
  return (PICO_FONT_IDS as readonly (string | null | undefined)[]).includes(value)
}

function storage(): Storage | undefined {
  try {
    return typeof window === "undefined" ? undefined : window.localStorage
  } catch {
    /* Reading `localStorage` itself throws where storage is blocked. */
    return undefined
  }
}

export function readPicoFont(): PicoFontId {
  try {
    const stored = storage()?.getItem(PICO_FONT_KEY)
    return isPicoFont(stored) ? stored : PICO_DEFAULT_FONT
  } catch {
    /* A face is not worth failing a screen over: unreadable means default. */
    return PICO_DEFAULT_FONT
  }
}

export function rememberPicoFont(id: PicoFontId): void {
  try {
    storage()?.setItem(PICO_FONT_KEY, id)
  } catch {
    /* Full or disabled storage: the face still applies until Pico reloads. */
  }
}
