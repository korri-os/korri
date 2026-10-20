/**
 * The zone select's layout as a pure function of its container.
 *
 * Ported from the mockup (docs/design/surfaces/mania/index.html). Width
 * decides content: how many cards show, how big the cursor triangles are,
 * how wide the gaps. Height decides structure: how tall the three strips are
 * and how much the band gets. The two axes never trade. Nothing is removed
 * at any size: below the wide threshold three cards show instead of five,
 * and when even those do not fit, every card scales down by one factor so
 * the full set still fits.
 *
 * Every constant is also a token in zone-tokens.css, so Caliper can turn
 * it; test/plan-zone-select.test.ts holds the two equal.
 */

/** Five cards show at or above this width; three below. */
export const ZONE_WIDE_WIDTH = 1000
/** The strips take their tall heights at or above this height. */
export const ZONE_TALL_HEIGHT = 600
/**
 * Below this height, on a wide screen, the GAMES page's facts line joins the
 * action strip instead of taking a row of its own: the band gets the row
 * back, and the width that is plentiful there pays for it.
 */
export const ZONE_FACTS_INLINE_HEIGHT = 400

export const ZONE_VISIBLE_CARDS = { wide: 5, narrow: 3 } as const
export const ZONE_TAB_STRIP = { tall: 74, short: 56 } as const
export const ZONE_ACTION_STRIP = { tall: 66, short: 52 } as const
export const ZONE_FOOT_STRIP = { tall: 58, short: 46 } as const
export const ZONE_TRIANGLE = { wide: 36, narrow: 26 } as const
export const ZONE_CARD_GAP = { wide: 20, narrow: 14 } as const
export const ZONE_TYPE_TAB = { wide: 24, narrow: 17 } as const
export const ZONE_TYPE_HINT = { wide: 20, narrow: 15 } as const
export const ZONE_TYPE_BUTTON = { tall: 24, short: 19 } as const
export const ZONE_TYPE_FOOT = { tall: 28, short: 22 } as const

/** The focused card never grows past this height. */
export const ZONE_CARD_FOCUS_MAX = 400
/** Width over height of a card. */
export const ZONE_CARD_RATIO = 0.78
/** A side card's height as a share of the focused card's. */
export const ZONE_CARD_SIDE_SCALE = 0.68
/** Room above and below the band for the pink edge and the drop edge. */
export const ZONE_BAND_PAD = 26
/** Horizontal margin either side of the band. */
export const ZONE_BAND_MARGIN = 24
/** The room between a cursor triangle and the card it points at. */
export const ZONE_TRIANGLE_GAP = 16
/** The focused card's title as a share of its width; a side card's likewise. */
export const ZONE_TYPE_CARD_SHARE = 0.125
export const ZONE_TYPE_SIDE_SHARE = 0.115

export type ZoneBand = "wide" | "narrow"
export type ZoneStrips = "tall" | "short"
export type ZoneFacts = "row" | "inline"

export interface ZonePlan {
  readonly band: ZoneBand
  readonly strips: ZoneStrips
  /** Where the GAMES page's facts line stands: its own row, or in the action strip. */
  readonly facts: ZoneFacts
  /** Cards on screen, the focused one in the middle. */
  readonly visible: 3 | 5
  readonly tabH: number
  readonly actH: number
  readonly footH: number
  /** The band's height: what is left between the strips. */
  readonly bandH: number
  readonly tri: number
  readonly gap: number
  readonly focusH: number
  readonly focusW: number
  readonly sideH: number
  readonly sideW: number
  /** The factor every card shrank by so the visible set fits; 1 when it fit. */
  readonly scale: number
  readonly tabFs: number
  readonly hintFs: number
  readonly btnFs: number
  readonly footFs: number
  readonly cardFs: number
  readonly sideFs: number
}

export function plan(width: number, height: number): ZonePlan {
  const band: ZoneBand = width >= ZONE_WIDE_WIDTH ? "wide" : "narrow"
  const strips: ZoneStrips = height >= ZONE_TALL_HEIGHT ? "tall" : "short"
  const facts: ZoneFacts = height < ZONE_FACTS_INLINE_HEIGHT && band === "wide" ? "inline" : "row"
  const visible = ZONE_VISIBLE_CARDS[band]
  const tabH = ZONE_TAB_STRIP[strips]
  const actH = ZONE_ACTION_STRIP[strips]
  const footH = ZONE_FOOT_STRIP[strips]
  const tri = ZONE_TRIANGLE[band]
  const gap = ZONE_CARD_GAP[band]
  const bandH = Math.max(0, height - tabH - actH - footH)

  let focusH = Math.max(0, Math.min(ZONE_CARD_FOCUS_MAX, bandH - ZONE_BAND_PAD * 2))
  let focusW = Math.round(focusH * ZONE_CARD_RATIO)
  let sideH = Math.round(focusH * ZONE_CARD_SIDE_SCALE)
  let sideW = Math.round(sideH * ZONE_CARD_RATIO)

  // The width the cards do not get: gaps, the two triangles, the margins.
  const fixed = (visible - 1) * gap + 2 * (tri + ZONE_TRIANGLE_GAP) + ZONE_BAND_MARGIN * 2
  const cards = focusW + (visible - 1) * sideW
  const scale = cards > 0 ? Math.max(0, Math.min(1, (width - fixed) / cards)) : 1
  focusH = Math.round(focusH * scale)
  focusW = Math.round(focusW * scale)
  sideH = Math.round(sideH * scale)
  sideW = Math.round(sideW * scale)

  return {
    band,
    strips,
    facts,
    visible,
    tabH,
    actH,
    footH,
    bandH,
    tri,
    gap,
    focusH,
    focusW,
    sideH,
    sideW,
    scale,
    tabFs: ZONE_TYPE_TAB[band],
    hintFs: ZONE_TYPE_HINT[band],
    btnFs: ZONE_TYPE_BUTTON[strips],
    footFs: ZONE_TYPE_FOOT[strips],
    cardFs: Math.round(focusW * ZONE_TYPE_CARD_SHARE),
    sideFs: Math.round(sideW * ZONE_TYPE_SIDE_SHARE),
  }
}
