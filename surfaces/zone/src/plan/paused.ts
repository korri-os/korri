/**
 * PAUSED's layout on the art grid (docs/design/surfaces/zone/REFINEMENT.md,
 * 4 "PAUSED"): Mania's pause screen as a pure function of the page's size
 * in art px. Every length is a whole number of art px, so the band's and the
 * wedge's 45 degree edges are stairs of one art px.
 *
 *  - The red band across the top: a quarter of the height, never under
 *    room for the title slab at caps-2 and never over its caps-3 room.
 *  - The yellow wedge: a right-angled triangle in the bottom-right corner,
 *    its long edge a 45 degree stair `/`. Its legs reach from the bottom to
 *    a few rows under the top, as Mania's does; on a page taller than wide
 *    the wedge reaches past the left edge, so the bottom is all yellow.
 *  - The rows' column: from a little inside the wedge's bottom-left corner
 *    to the right margin, at least ROWS_LEAST wide (all of a narrow page).
 *  - The steps: each row stands one cap further left than the row above
 *    it, down to the last row, over at most STEPS_MOST steps; a longer
 *    list's upper rows stand in line with the top step, so the stair is
 *    always the list's foot (where a short list rests) and a long list
 *    never walks off the page.
 *
 * Thresholds are named constants in art px, guesses until measured on a
 * device (each is tested one art px either side).
 */

/** The band is a quarter of the height... */
export const ZONE_PAUSED_BAND_SHARE = 0.25
/** ...never less than the title slab at caps-2 with its margins... */
export const ZONE_PAUSED_BAND_LEAST = 40
/** ...and never more than caps-3's slab with its margins. */
export const ZONE_PAUSED_BAND_MOST = 72

/** How far under the page's top the wedge's point stops. */
export const ZONE_PAUSED_WEDGE_TOP = 8

/** How far inside the wedge's bottom-left corner the rows' column starts. */
export const ZONE_PAUSED_ROWS_INSET = 8
/** The rows' column is never narrower than this (or the page, when it is narrower). */
export const ZONE_PAUSED_ROWS_LEAST = 200

/** The most steps the rows take down-left; rows past it stand in line. */
export const ZONE_PAUSED_STEPS_MOST = 4

export interface ZonePausedPlan {
  /** The band's height, art px. */
  readonly bandArt: number
  /** The wedge's leg (width and height of its triangle), art px. */
  readonly wedgeArt: number
  /** Where the rows' column starts from the page's left, art px. */
  readonly rowsLeftArt: number
}

const clamp = (value: number, least: number, most: number) => Math.max(least, Math.min(most, value))

export function planPaused(wArt: number, hArt: number): ZonePausedPlan {
  const w = Math.max(0, Math.floor(wArt))
  const h = Math.max(0, Math.floor(hArt))
  const bandArt = clamp(Math.round(h * ZONE_PAUSED_BAND_SHARE), ZONE_PAUSED_BAND_LEAST, ZONE_PAUSED_BAND_MOST)
  const wedgeArt = Math.max(1, h - ZONE_PAUSED_WEDGE_TOP)
  const fromWedge = Math.max(0, w - wedgeArt) + ZONE_PAUSED_ROWS_INSET
  const rowsLeftArt = Math.max(0, Math.min(fromWedge, w - Math.min(w, ZONE_PAUSED_ROWS_LEAST)))
  return { bandArt, wedgeArt, rowsLeftArt }
}

/** How many steps a list of `count` rows takes: one fewer than its rows, at most STEPS_MOST. */
export function zonePausedStairs(count: number): number {
  return clamp(Math.floor(count) - 1, 0, ZONE_PAUSED_STEPS_MOST)
}

/**
 * The step down-left of row `index` (0 at the top) in a list of `count`:
 * the last row stands on the last step, each row above it one step less,
 * none less than 0.
 */
export function zonePausedStep(index: number, count: number): number {
  const stairs = zonePausedStairs(count)
  return clamp(stairs - (Math.floor(count) - 1 - Math.floor(index)), 0, stairs)
}
