import { PICO_DEFAULT_FONT, PICO_FONT_IDS, type PicoFontId } from "./pico-fonts"

/**
 * Which face this device shows Pico in.
 *
 * One key, `pico.font`, holding one face id and nothing else (contract 3.6).
 * It is declared with the mark `device`: a face suits a panel, not a person.
 * Korri stores it for Pico (`RememberPreference`) and hands it back to `init`.
 *
 * A missing or unknown value means the default face. A tampered value can do
 * no more than pick another of Pico's own faces.
 */
export const PICO_FONT_KEY = "pico.font"

export function isPicoFont(value: string | null | undefined): value is PicoFontId {
  return (PICO_FONT_IDS as readonly (string | null | undefined)[]).includes(value)
}

/** The face a stored value names, or the default. */
export function picoFontFrom(stored: string | undefined): PicoFontId {
  return isPicoFont(stored) ? stored : PICO_DEFAULT_FONT
}
