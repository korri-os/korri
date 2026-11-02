/**
 * Words measured as .slint draws them (ui/atoms/ZoneMeasure.slint), so the
 * plan can decide where words stand (contract A2): a row of buttons that
 * wraps, tabs that wrap. Advances and kerning come from src/plan/faces.ts,
 * written from the fonts the package ships.
 *
 * A face is sized so its cap is the caps' cap (ui/tokens.slint): Zone Caps'
 * em is its cap; Anton's cap is 1760 of 2048 units; Jersey 10's 750 of 1400.
 * So a cap of `cap` px sets Zone Caps at `cap`, Anton at cap x 2048 / 1760
 * and Jersey 10 at cap x 1400 / 750, and one font unit is cap / 1000,
 * cap / 1760 or cap / 750 px.
 *
 * A character a face does not hold is drawn by another font, whose width
 * is not known here (`missing` stands in for it): Zone's own words keep to
 * the faces' characters; tests/zone_package.rs holds the two equal.
 */
import { ZONE_FACES, type ZoneFace } from "./faces"
import type { ZoneLook } from "./look"

/** Zone's words (caps), Korri's facts, and Korri's names at the caps' size (ZoneMeasure's `face`). */
export type ZoneWordFace = "caps" | "fact" | "korri"

/** The font a look draws a face in, and the font units in one cap. */
function fontOf(face: ZoneWordFace, look: ZoneLook): { readonly font: ZoneFace; readonly unitsPerCap: number } {
  const anton = { font: ZONE_FACES.anton, unitsPerCap: 1760 }
  const caps = { font: ZONE_FACES.zoneCaps, unitsPerCap: 1000 }
  const jersey = { font: ZONE_FACES.jersey, unitsPerCap: 750 }
  switch (face) {
    case "caps":
      return look === "modern" ? anton : caps
    case "fact":
      return look === "retro" ? jersey : anton
    case "korri":
      return look === "retro" ? caps : anton
  }
}

/** The words as a face draws them: capitals for Zone's caps (and Korri's names in Retro). */
export function shownWords(text: string, face: ZoneWordFace, look: ZoneLook): string {
  return face === "caps" || (face === "korri" && look === "retro") ? text.toUpperCase() : text
}

/** The width of `text` at a cap of `cap` px, as ZoneMeasure's preferred-width. */
export function measureWords(text: string, cap: number, face: ZoneWordFace, look: ZoneLook): number {
  const { font, unitsPerCap } = fontOf(face, look)
  const chars = [...shownWords(text, face, look)]
  let units = 0
  chars.forEach((ch, index) => {
    units += font.advances[ch] ?? font.missing
    const next = chars[index + 1]
    if (next !== undefined) units += font.kerning[ch + next] ?? 0
  })
  // .slint's preferred width is the drawn run rounded up to a whole px.
  return Math.ceil((units * cap) / unitsPerCap - 1e-6)
}
