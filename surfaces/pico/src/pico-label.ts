import { PICO_LABEL_COLORS, picoLuminance } from "./pico-palette"

/**
 * A cartridge label, derived from the game's id.
 *
 * Korri ships no art for most games, and a shelf of identical rectangles is the
 * single thing that made this surface look dead. Every game instead gets a
 * stable, distinct label: two palette colours and a dither step, hashed from
 * its id so the same game is the same colour on every boot and on every device
 * — a label that changed between visits would be worse than no label.
 */
export interface PicoLabel {
  /** Index into PICO_LABEL_COLORS for the ground. */
  readonly fill: number
  /** Index into PICO_LABEL_COLORS for the dither's second colour. */
  readonly accent: number
  /** Which of the dither's three cell sizes to use. */
  readonly dither: number
  /** Whether the fill needs light ink on top of it. */
  readonly ink: "light" | "dark"
}

/** FNV-1a. Small, stable across runtimes, and good enough to spread ids. */
function hash(input: string): number {
  let h = 2166136261
  for (let index = 0; index < input.length; index += 1) {
    h ^= input.charCodeAt(index)
    h = Math.imul(h, 16777619)
  }
  return h >>> 0
}

/** Below this, a fill is dark enough that black ink stops being readable. */
const INK_FLIP = 140

export function picoLabelFor(gameId: string): PicoLabel {
  const h = hash(gameId)
  const fill = h % PICO_LABEL_COLORS.length
  /* Unsigned shifts throughout: `>>` is signed, so any id hashing above 2^31
   * yields a negative offset — which lands the accent on the fill and draws a
   * flat rectangle, or indexes off the palette entirely. */
  const offset = 1 + ((h >>> 8) % (PICO_LABEL_COLORS.length - 1))
  /* Never the same colour twice: a dither of one colour is a flat rectangle,
   * which is what this exists to avoid. */
  const accent = (fill + offset) % PICO_LABEL_COLORS.length
  const fillColor = PICO_LABEL_COLORS[fill]
  return {
    fill,
    accent,
    dither: (h >>> 16) % 3,
    ink:
      fillColor !== undefined && picoLuminance(fillColor) < INK_FLIP
        ? "light"
        : "dark",
  }
}
