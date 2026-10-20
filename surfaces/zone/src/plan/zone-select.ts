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
 * Every length and threshold is in art px (src/plan/art-px.ts): the plan
 * divides the container by its art pixel first, so 640x480 is planned as a
 * 320x240 canvas and 1280x720 as 426x240, and a stylesheet multiplies each
 * answer by `--zone-px`. Every constant is also a token in zone-tokens.css,
 * so Caliper can turn it; test/plan-zone-select.test.ts holds the two
 * equal. The thresholds are first guesses on the art grid, re-measured for
 * the refinement (docs/design/surfaces/zone/REFINEMENT.md, 2a).
 */
import { zoneArt } from "./art-px"

/** Five cards show at or above this width in art px; three below (1000 CSS px on the old grid, 333 art px at 720p). */
export const ZONE_WIDE_WIDTH = 333
/** The strips take their tall heights at or above this height in art px: a portrait screen. Every landscape screen holds 240 to 479 art rows. */
export const ZONE_TALL_HEIGHT = 320
/**
 * Below this height in art px the GAMES page's body is too short for its
 * rows. On a wide screen the facts line joins the action strip instead of
 * taking a row of its own: the band gets the row back, and the plentiful
 * width pays for it. On any screen the find keyboard rises over the whole
 * screen below the tabs instead of inside the body, since the body cannot
 * hold its keys. Every screen whose short side is 240 CSS px or more holds
 * 240 art rows, so only a box shorter than that (at one CSS px per art px)
 * falls under it.
 */
export const ZONE_SHORT_HEIGHT = 240

export const ZONE_VISIBLE_CARDS = { wide: 5, narrow: 3 } as const
export const ZONE_TAB_STRIP = { tall: 36, short: 32 } as const
export const ZONE_ACTION_STRIP = { tall: 30, short: 26 } as const
export const ZONE_FOOT_STRIP = { tall: 24, short: 22 } as const
export const ZONE_TRIANGLE = { wide: 14, narrow: 12 } as const
export const ZONE_CARD_GAP = { wide: 10, narrow: 7 } as const
/** Type sizes are em sizes in art px. Zone Caps' em is its cap, so 10 is caps-1. */
export const ZONE_TYPE_TAB = { wide: 10, narrow: 10 } as const
export const ZONE_TYPE_HINT = { wide: 10, narrow: 10 } as const
export const ZONE_TYPE_BUTTON = { tall: 10, short: 10 } as const
export const ZONE_TYPE_FOOT = { tall: 10, short: 10 } as const

/** The focused card never grows past this height. */
export const ZONE_CARD_FOCUS_MAX = 133
/** Width over height of a card. */
export const ZONE_CARD_RATIO = 0.78
/** A side card's height as a share of the focused card's. */
export const ZONE_CARD_SIDE_SCALE = 0.68
/** Room above and below the band for the pink edge and the drop edge. */
export const ZONE_BAND_PAD = 13
/** Horizontal margin either side of the band. */
export const ZONE_BAND_MARGIN = 12
/** The room between a cursor triangle and the card it points at. */
export const ZONE_TRIANGLE_GAP = 8
/** The focused card's title as a share of its width; a side card's likewise. */
export const ZONE_TYPE_CARD_SHARE = 0.125
export const ZONE_TYPE_SIDE_SHARE = 0.115

export type ZoneBand = "wide" | "narrow"
export type ZoneStrips = "tall" | "short"
export type ZoneFacts = "row" | "inline"
export type ZoneKeys = "body" | "screen"

export interface ZonePlan {
  /** CSS px per art px; every other length here is in art px. */
  readonly px: number
  /** The container in whole art px. */
  readonly wArt: number
  readonly hArt: number
  readonly band: ZoneBand
  readonly strips: ZoneStrips
  /** Where the GAMES page's facts line stands: its own row, or in the action strip. */
  readonly facts: ZoneFacts
  /** Where the find keyboard rises: in the body, or over the screen below the tabs. */
  readonly keys: ZoneKeys
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

/** The plan for a container measured in CSS px: its art pixel, then the plan on the art grid. */
export function plan(width: number, height: number): ZonePlan {
  const art = zoneArt(width, height)
  return planArt(art.wArt, art.hArt, art.px)
}

/** The plan on the art grid: every threshold and length in art px. */
export function planArt(width: number, height: number, px = 1): ZonePlan {
  const band: ZoneBand = width >= ZONE_WIDE_WIDTH ? "wide" : "narrow"
  const strips: ZoneStrips = height >= ZONE_TALL_HEIGHT ? "tall" : "short"
  const facts: ZoneFacts = height < ZONE_SHORT_HEIGHT && band === "wide" ? "inline" : "row"
  const keys: ZoneKeys = height < ZONE_SHORT_HEIGHT ? "screen" : "body"
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
    px,
    wArt: width,
    hArt: height,
    band,
    strips,
    facts,
    keys,
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
