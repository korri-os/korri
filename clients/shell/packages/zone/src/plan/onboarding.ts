/**
 * First boot's controller cards as a pure function of the room they have.
 *
 * COMPETITION shows its seats in one row. Zone keeps them in one row while
 * they fit at a readable size, and otherwise tries fewer columns, choosing
 * the number of columns that gives each card the most width: width decides
 * how many go across, height decides how many rows can stand. Every card
 * stays on screen; only below the least width does the stage scroll.
 *
 * Every length is in art px (src/plan/art-px.ts): the page divides the
 * stage by its art pixel first and multiplies the card's width back. The
 * constants are tokens too (zone-tokens.css), held equal by
 * test/zone-onboarding.test.tsx.
 */

/** A card's widest. */
export const ZONE_CONTROLLER_CARD_MAX = 110
/** A card's narrowest; below it the stage scrolls rather than shrink a name past reading. */
export const ZONE_CONTROLLER_CARD_MIN = 36
/** The room between cards, across and down: the band's wide card gap. */
export const ZONE_CONTROLLER_CARD_GAP = 10
/** How much taller than wide a card stands: the square body plus the name bar and frame. */
export const ZONE_CONTROLLER_CARD_TALL = 1.5
/** The narrowest card that keeps the two triangles beside its name; a narrower one gives their room to the name. */
export const ZONE_CONTROLLER_ARROWS_LEAST = 90
/** COMPETITION's VS panel between two seats: its width in a row (a token too, --zone-vs-width). */
export const ZONE_CONTROLLER_VS = 80
/** The VS panel's height when the two seats stand one over the other. */
export const ZONE_CONTROLLER_VS_TALL = 28

export interface ZoneControllerGrid {
  readonly columns: number
  readonly rows: number
  /** Whole pixels, so a card's edges stay crisp. */
  readonly cardWidth: number
  /** Where VS stands, when it does: between the two cards in a row, or between them in a column. */
  readonly versus?: "row" | "column"
}

/**
 * `versus`: exactly two seats, both heard: VS stands between them, and
 * takes its room from the axis the cards are laid along.
 */
export function planControllerCards(width: number, height: number, count: number, versus = false): ZoneControllerGrid {
  const cards = Math.max(1, count)
  const vs = versus && cards === 2
  let best: ZoneControllerGrid = { columns: 1, rows: cards, cardWidth: -1 }
  for (let columns = cards; columns >= 1; columns -= 1) {
    const rows = Math.ceil(cards / columns)
    const acrossVs = vs && columns === 2 ? ZONE_CONTROLLER_VS + ZONE_CONTROLLER_CARD_GAP : 0
    const downVs = vs && rows === 2 ? ZONE_CONTROLLER_VS_TALL + ZONE_CONTROLLER_CARD_GAP : 0
    const across = (width - (columns - 1) * ZONE_CONTROLLER_CARD_GAP - acrossVs) / columns
    const down = (height - (rows - 1) * ZONE_CONTROLLER_CARD_GAP - downVs) / rows / ZONE_CONTROLLER_CARD_TALL
    const cardWidth = Math.min(ZONE_CONTROLLER_CARD_MAX, across, down)
    // More columns win a tie: one row reads as COMPETITION does.
    if (cardWidth > best.cardWidth) {
      best = vs ? { columns, rows, cardWidth, versus: columns === 2 ? "row" : "column" } : { columns, rows, cardWidth }
    }
  }
  return { ...best, cardWidth: Math.max(ZONE_CONTROLLER_CARD_MIN, Math.floor(best.cardWidth)) }
}
