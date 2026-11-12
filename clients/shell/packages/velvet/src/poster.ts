/**
 * The no-art card: what stands in a tile when Korri has no picture of a
 * game, and under every picture until it arrives (ADR 0065).
 *
 *   - A system Velvet knows (src/systems.ts, by `systemId`): a system
 *     poster. The title picks one of the machine's drawings and one of its
 *     shell colours, so neighbours of one system differ.
 *   - Any other system, or none: a pattern tile. Its motif and its hue come
 *     from the system id (the title when there is none), so one system's
 *     games read as a family; the title picks the drawing, the shell and
 *     the accent.
 *
 * The logic draws nothing and mixes no colour. Every drawing and every
 * palette is made at build time (drawings/build.ts) and stands in the
 * package as data (ui/drawings.slint). A game's card is here: which drawing,
 * which palette, and its title, fitted to the drawing's box. That is two
 * lookups and one fit, whether the card was ever seen before or not, and in
 * a view two numbers and the title.
 */
import { GELS, hash } from "./colour"
import { GEL_SHELLS, MOTIF_DRAWINGS, PALETTES, PATTERN_ACCENTS, PATTERN_PALETTES, PATTERN_SHELLS, SYSTEM_DRAWINGS, SYSTEM_PALETTES, VARIANTS, type Drawing } from "./drawings"
import { fitCapsOnce, type Fit } from "./fit"
import { onForget } from "./memo"
import { knownSystem, LANGUAGES } from "./systems"

export { KNOWN_SYSTEMS, knownSystem } from "./systems"
export { VARIANTS } from "./drawings"

export const CARD_W = 60
export const CARD_H = 90
/** Families a game with no system falls into (a hue and a motif each). */
const FAMILIES = 24

export interface Poster {
  /** The drawing and the palette, by their numbers in ui/drawings.slint. */
  readonly drawing: number
  readonly palette: number
  /** The title, fitted to the drawing's box: its lines and its size, in card units. */
  readonly lines: readonly string[]
  readonly size: number
  readonly ink: number
  /** The title is low on the card, so the focus badge sits at the top. */
  readonly badgeTop: boolean
  /** The card's ground colour. */
  readonly paper: number
  /** The card as a view sends it: made with the card, so every view of it sends the one object. */
  readonly view: PosterView
}

/** A machine's palette, by `choice`: one of its shells; with none of its own, a gel and one of the gel's shells. */
function systemPalette(id: string, choice: number): number {
  const range = SYSTEM_PALETTES[id]!
  if (LANGUAGES[id]!.shells !== undefined) return range.first + (choice % range.count)
  return range.first + (choice % GELS.length) * GEL_SHELLS + ((choice >>> 4) % GEL_SHELLS)
}

/** A pattern tile's palette: the family's gel; the title's choice of shell and accent. */
const patternPalette = (family: number, choice: number): number =>
  PATTERN_PALETTES + ((family % GELS.length) * PATTERN_SHELLS + (choice % PATTERN_SHELLS)) * PATTERN_ACCENTS + ((choice >>> 2) % PATTERN_ACCENTS)

// A card by its system (none is ""), then by its title: both whole.
const posters = new Map<string, Map<string, Poster>>()
onForget(() => posters.clear())

/** The no-art card of a game (or of a system, for the By system shelf: its name as the title). */
export function posterFor(title: string, systemId: string | undefined): Poster {
  // No system and an empty one are the same to everything below.
  const system = systemId ?? ""
  let byTitle = posters.get(system)
  const known = byTitle?.get(title)
  if (known !== undefined) return known
  const seed = hash(title)
  const variant = seed % VARIANTS
  const choice = seed >>> 8
  const language = knownSystem(systemId)
  let drawing: Drawing
  let palette: number
  if (language !== undefined) {
    drawing = SYSTEM_DRAWINGS[language]![variant]!
    palette = systemPalette(language, choice)
  } else {
    // One system, one family; a game with no system falls into one of a few by its title.
    const named = systemId !== undefined && systemId.trim() !== ""
    const family = hash(named ? `system|${systemId.trim().toLowerCase()}` : `title|${(seed >>> 4) % FAMILIES}`)
    drawing = MOTIF_DRAWINGS[(family >>> 4) % MOTIF_DRAWINGS.length]![variant]!
    palette = patternPalette(family, choice)
  }
  const colours = PALETTES[palette]!
  const fit: Fit = fitCapsOnce(title, drawing.w, drawing.h, drawing.max * CARD_H, 4.2)
  const ink = colours[1 + drawing.ink]!
  const poster: Poster = {
    drawing: drawing.index, palette, lines: fit.lines, size: fit.size, ink, badgeTop: drawing.badgeTop, paper: colours[0],
    view: { shown: true, drawing: drawing.index, palette, lines: fit.lines, size: fit.size, ink },
  }
  if (byTitle === undefined || byTitle.size > 600) {
    if (posters.size > 200) posters.clear()
    byTitle = new Map()
    posters.set(system, byTitle)
  }
  byTitle.set(title, poster)
  return poster
}

/** The poster as the view sends it: the drawing and the palette by number, and the title. */
export interface PosterView {
  readonly shown: boolean
  readonly drawing: number
  readonly palette: number
  readonly lines: readonly string[]
  readonly size: number
  readonly ink: number
}

export const NO_POSTER: PosterView = { shown: false, drawing: 0, palette: 0, lines: [], size: 0, ink: 0 }

/** A card as a view sends it. */
export const posterView = (poster: Poster): PosterView => poster.view
