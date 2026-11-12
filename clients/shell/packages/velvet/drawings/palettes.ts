/**
 * The no-art cards' palettes, made at build time (drawings/build.ts): every
 * set of sixteen colours a card can take, and for each the title's colour
 * under each of the three ink rules.
 *
 *   - A machine with shell colours of its own: one palette a shell.
 *   - A machine with none (its colours come from a gel): one a gel and a
 *     shell, twelve gels by three shells.
 *   - A pattern tile: one a gel, a shell and an accent, twelve by four by
 *     four.
 */
import { contrast, GELS, hsl, mix, toHsl } from "../src/colour"
import { PALETTE_LENGTH, SLOT, type InkRule } from "../src/drawings-slots"
import { LANGUAGES, shell, type Shell } from "../src/systems"

const WARM_WHITE = 0xf4efe6
const WHITE = 0xffffff
const BLACK = 0x000000
/** The one red a power light takes, on every machine. */
const SIGNAL = 0xe0303a

export const GEL_SHELLS = 3
export const PATTERN_SHELLS = 4
export const PATTERN_ACCENTS = 4

const isLight = (colour: number): boolean => toHsl(colour)[2] > 0.55

/** A title colour that reads on `ground`: `wanted` when it does, else the better of the card's ink and warm white. */
function readable(wanted: number, ground: number, ink: number): number {
  if (contrast(wanted, ground) >= 4.5) return wanted
  return contrast(ink, ground) >= contrast(WARM_WHITE, ground) ? ink : WARM_WHITE
}

/** The title's colour on a card, by its drawing's rule. */
export function inkOf(rule: InkRule, palette: readonly number[]): number {
  const paper = palette[SLOT.paper]!, ink = palette[SLOT.ink]!, lite = palette[SLOT.lite]!
  switch (rule) {
    case "paper":
      return isLight(paper) ? ink : WARM_WHITE
    case "label":
      return readable(ink, lite, ink)
    case "band":
      return readable(isLight(paper) ? paper : lite, ink, WARM_WHITE)
  }
}

/** A palette from its parts: every place filled, so a drawing never names a colour that is not there. */
function paletteOf(s: Shell, accents: readonly number[], screen: readonly number[], other: number): number[] {
  const palette: number[] = new Array<number>(PALETTE_LENGTH).fill(s.lite)
  palette[SLOT.paper] = s.paper
  palette[SLOT.ink] = s.ink
  palette[SLOT.deep] = s.deep
  palette[SLOT.lite] = s.lite
  for (let i = 0; i < 4; i++) {
    palette[SLOT.acc + i] = accents[i] ?? s.lite
    palette[SLOT.screen + i] = screen[i] ?? s.ink
  }
  palette[SLOT.white] = WHITE
  palette[SLOT.black] = BLACK
  palette[SLOT.signal] = SIGNAL
  palette[SLOT.other] = other
  return palette
}

/** A machine's palettes: one a shell colour; or, with none of its own, one a gel and a shell (gel first). */
export function systemPalettes(id: string): number[][] {
  const language = LANGUAGES[id]!
  if (language.shells !== undefined) return language.shells.map(s => paletteOf(s, language.acc, language.screen, s.lite))
  return GELS.flatMap(h => [
    shell(hsl(h, 0.26, 0.22), hsl(h, 0.4, 0.08), hsl(h, 0.24, 0.27), hsl(h, 0.3, 0.78)),
    shell(hsl(h, 0.22, 0.72), hsl(h, 0.34, 0.12), hsl(h, 0.18, 0.66), hsl(h, 0.3, 0.88)),
    shell(hsl(h, 0.38, 0.36), hsl(h, 0.4, 0.1), hsl(h, 0.34, 0.31), hsl(h, 0.4, 0.8)),
  ].map(s => paletteOf(s, [hsl(h + 180, 0.6, 0.64), 0xd9e4ee, hsl(h + 40, 0.7, 0.6)], [hsl(h, 0.4, 0.1), hsl(h, 0.5, 0.35), hsl(h + 180, 0.6, 0.62), 0xe8eef3], s.lite)))
}

/** The pattern tiles' palettes: gel, then shell, then accent. */
export function patternPalettes(): number[][] {
  return GELS.flatMap(hue => {
    const shells = [
      shell(hsl(hue, 0.34, 0.27), hsl(hue, 0.4, 0.09), hsl(hue, 0.36, 0.2), hsl(hue, 0.36, 0.8)),
      shell(hsl(hue, 0.3, 0.4), hsl(hue, 0.42, 0.1), hsl(hue, 0.3, 0.33), hsl(hue, 0.4, 0.86)),
      shell(hsl(hue, 0.26, 0.68), hsl(hue, 0.36, 0.12), hsl(hue, 0.24, 0.6), hsl(hue, 0.3, 0.9)),
      shell(hsl(hue + 24, 0.3, 0.22), hsl(hue, 0.4, 0.07), hsl(hue + 24, 0.3, 0.3), hsl(hue, 0.36, 0.82)),
    ]
    return shells.flatMap(s => [150, 180, 210, 40].map(turn => paletteOf(s, [hsl(hue + turn, 0.62, 0.62)], [], isLight(s.paper) ? s.lite : mix(s.paper, s.lite, 0.25))))
  })
}
