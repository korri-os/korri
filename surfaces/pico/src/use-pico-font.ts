/**
 * The face this device chose in Settings, read once and kept by Pico
 * (pico-font-preference.ts). A change applies at once and is remembered.
 *
 * This is device preference, not navigation: it outlives both presentations,
 * so the root keeps it and both read it.
 */
import { useCallback, useState } from "react"
import { readPicoFont, rememberPicoFont } from "./pico-font-preference"
import type { PicoFontId } from "./pico-fonts"

export function usePicoFont(): readonly [PicoFontId, (font: PicoFontId) => void] {
  const [chosen, setChosen] = useState(readPicoFont)
  const choose = useCallback((font: PicoFontId) => {
    rememberPicoFont(font)
    setChosen(font)
  }, [])
  return [chosen, choose]
}
