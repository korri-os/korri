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
 *    When it starts left of the B BACK hint's width, it rests above the
 *    hint instead of on it.
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

/** The B BACK hint's room at the bottom-left: its width with the page's margin... */
export const ZONE_PAUSED_HINT_WIDTH = 80
/** ...and its height with the margin under it. A column that starts left of the hint's width rests above it. */
export const ZONE_PAUSED_HINT_RISE = 25

/** The most steps the rows take down-left; rows past it stand in line. */
export const ZONE_PAUSED_STEPS_MOST = 4

export interface ZonePausedPlan {
  /** The band's height, art px. */
  readonly bandArt: number
  /** The wedge's leg (width and height of its triangle), art px. */
  readonly wedgeArt: number
  /** Where the rows' column starts from the page's left, art px. */
  readonly rowsLeftArt: number
  /** How far above the page's foot the rows rest: clear of B BACK when the column reaches over it, art px. */
  readonly rowsFootArt: number
}

const clamp = (value: number, least: number, most: number) => Math.max(least, Math.min(most, value))

export function planPaused(wArt: number, hArt: number): ZonePausedPlan {
  const w = Math.max(0, Math.floor(wArt))
  const h = Math.max(0, Math.floor(hArt))
  const bandArt = clamp(Math.round(h * ZONE_PAUSED_BAND_SHARE), ZONE_PAUSED_BAND_LEAST, ZONE_PAUSED_BAND_MOST)
  const wedgeArt = Math.max(1, h - ZONE_PAUSED_WEDGE_TOP)
  const fromWedge = Math.max(0, w - wedgeArt) + ZONE_PAUSED_ROWS_INSET
  const rowsLeftArt = Math.max(0, Math.min(fromWedge, w - Math.min(w, ZONE_PAUSED_ROWS_LEAST)))
  const rowsFootArt = rowsLeftArt < ZONE_PAUSED_HINT_WIDTH ? ZONE_PAUSED_HINT_RISE : 0
  return { bandArt, wedgeArt, rowsLeftArt, rowsFootArt }
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

/*
 * The wedge's list, once its rows are laid out. Two decisions, both on
 * measured boxes, in any one unit:
 *
 *  - The gaps yield before the list scrolls. Between the rows, and over
 *    each group's tag, the gap is GAP_MOST art px while the rows fit; when
 *    they do not, it gives one art px at a time down to GAP_LEAST, and only
 *    then does the list scroll. A gap is space, not information: it never
 *    keeps its size while a row goes out of view.
 *  - Where a list that scrolls stands. The focused row with its pink drop
 *    stands whole; each edge of the view either has nothing past it or cuts
 *    a line of words through the middle of its letters, so the cut row says more is
 *    there with letters a reader can see. A cut through a slab's foot
 *    under its words (a black sliver) or through the gap between two rows
 *    (no cut at all) is never chosen while another place will do. Of the
 *    places that will do, the one nearest where the list stands now wins,
 *    so the list moves as little as it can and keeps its place across a
 *    resize.
 */

/** The gap between the wedge's rows while they fit (held equal to `--zone-paused-row-gap`). */
export const ZONE_PAUSED_GAP_MOST = 5
/** The least the gap yields to before the list scrolls. */
export const ZONE_PAUSED_GAP_LEAST = 2

/** The gaps the wedge tries, largest first, one art px apart. */
export function zonePausedGaps(): readonly number[] {
  return Array.from({ length: ZONE_PAUSED_GAP_MOST - ZONE_PAUSED_GAP_LEAST + 1 }, (_, index) => ZONE_PAUSED_GAP_MOST - index)
}

/** A measured span down the list's content, from its top. */
export interface ZonePausedSpan {
  readonly top: number
  readonly bottom: number
}

export interface ZonePausedScrollInput {
  /** The list's own height: what shows. */
  readonly view: number
  /** The height of everything in it (its scroll height). */
  readonly end: number
  /** Where it stands now (its scroll top). */
  readonly at: number
  /** Every row and group tag. */
  readonly rows: readonly ZonePausedSpan[]
  /** Every line of words in them, label, tag, value or note. */
  readonly lines: readonly ZonePausedSpan[]
  /** The focused row with its drop, when one is focused. */
  readonly keep?: ZonePausedSpan
}

/** Less than this past an edge is not past it: boxes are measured in fractions. */
const EDGE_SLACK = 0.5

/**
 * How far from a line's middle an edge may cut it, as a share of the line's
 * box. A line's box is the face's ascent and descent, about 1.6 of its cap,
 * with the cap in the middle: an eighth of the box either side of the
 * middle leaves at least 30% of the cap shown and 30% hidden.
 */
export const ZONE_PAUSED_CUT_SHARE = 1 / 8

/** An edge at `y` cuts `line` through the middle of its letters. */
const cutsLetters = (line: ZonePausedSpan, y: number): boolean =>
  Math.abs(y - (line.top + line.bottom) / 2) <= (line.bottom - line.top) * ZONE_PAUSED_CUT_SHARE

/**
 * How well an edge of the view at `y` says what is past it: 2 when nothing
 * is past it or it cuts a line of words through the middle of its letters;
 * 1 when it cuts a row whose shown side still holds most of a line of
 * words (the row is cut, no letter is lost, but the cut misses the
 * letters); 0 when the shown side of every row it cuts is a letterless
 * sliver, or it cuts no row at all and says nothing.
 */
function edgeLevel(y: number, side: "top" | "bottom", rows: readonly ZonePausedSpan[], lines: readonly ZonePausedSpan[]): 0 | 1 | 2 {
  const past = side === "top" ? rows.some(row => row.top < y - EDGE_SLACK) : rows.some(row => row.bottom > y + EDGE_SLACK)
  if (!past || lines.some(line => cutsLetters(line, y))) return 2
  const mid = (line: ZonePausedSpan) => (line.top + line.bottom) / 2
  const shows = rows.some(row => row.top < y - EDGE_SLACK && row.bottom > y + EDGE_SLACK && lines.some(line =>
    mid(line) > row.top && mid(line) < row.bottom && (side === "top" ? mid(line) >= y : mid(line) <= y)))
  return shows ? 1 : 0
}

/**
 * Where the list's scroll top stands: 0 when it fits. Else, of the places
 * that keep the focused row whole (each line of words at either edge, the
 * focused row at either edge, the ends, and where it stands now), the one
 * whose worse edge is best, then whose edges are best together, then the
 * nearest to `at`. A focused row taller than the view stands at the top.
 */
export function zonePausedScroll({ view, end, at, rows, lines, keep }: ZonePausedScrollInput): number {
  const most = Math.max(0, end - view)
  if (most <= EDGE_SLACK) return 0
  const within = (value: number) => Math.min(most, Math.max(0, value))
  const whole = (top: number) => keep === undefined
    || (keep.top >= top - EDGE_SLACK && keep.bottom <= top + view + EDGE_SLACK)

  const mid = (line: ZonePausedSpan) => (line.top + line.bottom) / 2
  const places = [
    0,
    most,
    at,
    ...(keep === undefined ? [] : [keep.top, keep.bottom - view]),
    ...lines.map(mid),
    ...lines.map(line => mid(line) - view),
  ].map(within).filter(whole)
  if (places.length === 0) return within(keep === undefined ? at : keep.top)

  const scored = places.map(place => {
    const top = edgeLevel(place, "top", rows, lines)
    const bottom = edgeLevel(place + view, "bottom", rows, lines)
    return { place, worse: Math.min(top, bottom), both: top + bottom, away: Math.abs(place - at) }
  })
  scored.sort((a, b) => b.worse - a.worse || b.both - a.both || a.away - b.away || a.place - b.place)
  return scored[0]?.place ?? 0
}
