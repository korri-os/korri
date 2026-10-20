/**
 * First boot's controller cards as a pure function of the room they have.
 *
 * COMPETITION shows its seats in one row. Zone keeps them in one row while
 * they fit at a readable size, and otherwise tries fewer columns, choosing
 * the number of columns that gives each card the most width: width decides
 * how many go across, height decides how many rows can stand. Every card
 * stays on screen; only below the least width does the stage scroll.
 *
 * The constants are tokens too (zone-tokens.css), held equal by
 * test/zone-onboarding.test.tsx.
 */

/** A card's widest. */
export const ZONE_CONTROLLER_CARD_MAX = 220
/** A card's narrowest; below it the stage scrolls rather than shrink a name past reading. */
export const ZONE_CONTROLLER_CARD_MIN = 72
/** The room between cards, across and down: the band's wide card gap. */
export const ZONE_CONTROLLER_CARD_GAP = 20
/** How much taller than wide a card stands: the square body plus the name bar and frame. */
export const ZONE_CONTROLLER_CARD_TALL = 1.5

export interface ZoneControllerGrid {
  readonly columns: number
  readonly rows: number
  /** Whole pixels, so a card's edges stay crisp. */
  readonly cardWidth: number
}

export function planControllerCards(width: number, height: number, count: number): ZoneControllerGrid {
  const cards = Math.max(1, count)
  let best: ZoneControllerGrid = { columns: 1, rows: cards, cardWidth: -1 }
  for (let columns = cards; columns >= 1; columns -= 1) {
    const rows = Math.ceil(cards / columns)
    const across = (width - (columns - 1) * ZONE_CONTROLLER_CARD_GAP) / columns
    const down = (height - (rows - 1) * ZONE_CONTROLLER_CARD_GAP) / rows / ZONE_CONTROLLER_CARD_TALL
    const cardWidth = Math.min(ZONE_CONTROLLER_CARD_MAX, across, down)
    // More columns win a tie: one row reads as COMPETITION does.
    if (cardWidth > best.cardWidth) best = { columns, rows, cardWidth }
  }
  return { ...best, cardWidth: Math.max(ZONE_CONTROLLER_CARD_MIN, Math.floor(best.cardWidth)) }
}
