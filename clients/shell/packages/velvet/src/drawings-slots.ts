/**
 * A no-art card's palette: sixteen colours, by place. A drawing
 * (ui/drawings.slint, made by drawings/build.ts) names these places and no
 * colour; the logic fills them for each game (src/poster.ts).
 */
export const SLOT = {
  /** The card's ground. */
  paper: 0,
  /** The darkest: outlines, the title's plate. */
  ink: 1,
  /** A tone under the paper: a part drawn tone on tone. */
  deep: 2,
  /** A tone over the paper. */
  lite: 3,
  /** Up to four accents, from this place on: buttons, a stripe. */
  acc: 4,
  /** A screen's four colours, from this place on. */
  screen: 8,
  white: 12,
  black: 13,
  /** The one red a power light takes. */
  signal: 14,
  /** A pattern's second tone. */
  other: 15,
} as const

export const PALETTE_LENGTH = 16

/**
 * How a card's title takes its colour from the palette:
 *   paper   on the paper: the ink on a light paper, warm white on a dark one
 *   label   on a light label: the ink, unless it does not read there
 *   band    on the ink band: the paper or the light tone, unless it does not read there
 */
export type InkRule = "paper" | "label" | "band"
