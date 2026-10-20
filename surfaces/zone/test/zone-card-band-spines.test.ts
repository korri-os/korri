/**
 * Section dividers take width in the band. The band counts the most a side
 * of the focus ever holds, and its stylesheet's fit leaves that room, so on
 * a narrow screen the focused card and both neighbours are never cut.
 */
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import {
  plan,
  ZONE_BAND_MARGIN,
  ZONE_CARD_FOCUS_MAX,
  ZONE_CARD_RATIO,
  ZONE_CARD_SIDE_SCALE,
  ZONE_TRIANGLE_GAP,
} from "../src/plan/zone-select"
import { bandSpinesPerSide } from "../src/ui/molecules/ZoneCardBand"

const cards = (opens: readonly number[], count = 12) =>
  Array.from({ length: count }, (_, index) => (opens.includes(index) ? { section: `s${index}` } : {}))

describe("bandSpinesPerSide", () => {
  test("no sections, no spines", () => {
    expect(bandSpinesPerSide(cards([]), 3)).toBe(0)
    expect(bandSpinesPerSide(cards([]), 5)).toBe(0)
  })
  test("the first card's divider counts on the left of the focus", () => {
    // With the first card focused its divider is on the left of the focus.
    expect(bandSpinesPerSide(cards([0]), 3)).toBe(1)
  })
  test("three visible: a side never holds more than one", () => {
    expect(bandSpinesPerSide(cards([0, 1, 2]), 3)).toBe(1)
    expect(bandSpinesPerSide(cards([5]), 3)).toBe(1)
  })
  test("five visible: two in a row on one side count two", () => {
    expect(bandSpinesPerSide(cards([0, 1, 6]), 5)).toBe(2)
    expect(bandSpinesPerSide(cards([0, 6]), 5)).toBe(1)
  })
  test("the leftmost visible card's own divider lies outside the view", () => {
    // Sections open at 3 and 6 only: with 5 visible no side holds both.
    expect(bandSpinesPerSide(cards([3, 6]), 5)).toBe(1)
  })
})

/*
 * The band's sums, written out from the stylesheet with the plan's
 * constants: where the outer edge of each neighbour lands when a hairline
 * spine stands on both sides of the focus.
 */
describe("the narrow band at 640x480 keeps the focus and its neighbours whole", () => {
  // Art px: the plan and the tokens are both on the art grid.
  const HAIRLINE = 3
  const GAP = 7
  const TRI = 12
  test("with spines counted, both neighbours end inside the band", () => {
    const p = plan(640, 480)
    expect(p.visible).toBe(3)
    const width = p.wArt
    const height = p.bandH
    const spines = 1
    const fixed = 2 * GAP + 2 * (TRI + ZONE_TRIANGLE_GAP) + 2 * ZONE_BAND_MARGIN + 2 * spines * (GAP + HAIRLINE)
    const fit = (width - fixed) / (ZONE_CARD_RATIO * (1 + 2 * ZONE_CARD_SIDE_SCALE))
    const h = Math.min(ZONE_CARD_FOCUS_MAX, height - 26, fit)
    const focusW = h * ZONE_CARD_RATIO
    const sideW = h * ZONE_CARD_SIDE_SCALE * ZONE_CARD_RATIO
    const reach = focusW / 2 + TRI + ZONE_TRIANGLE_GAP + spines * (HAIRLINE + GAP) + GAP + sideW
    expect(width / 2 - reach).toBeGreaterThanOrEqual(0)
    expect(width / 2 + reach).toBeLessThanOrEqual(width)
  })
  test("the stylesheet counts the spines in the fit and in the centring", () => {
    const css = readFileSync(join(import.meta.dir, "..", "src", "ui", "molecules", "ZoneCardBand.css"), "utf8")
    expect(css).toMatch(/--zone-band-fixed: calc\([^;]*2 \* var\(--zone-band-spines, 0\) \* \(var\(--zone-band-gap\) \+ var\(--zone-band-spine-fixed\)\)/)
    expect(css).toMatch(/--zone-band-fit: calc\([^;]*2 \* var\(--zone-band-spines, 0\) \* var\(--zone-band-spine-share\)/)
    expect(css).toMatch(/--zone-band-section-w: calc\(var\(--zone-card-h\) \* var\(--zone-band-spine-share\) \+ var\(--zone-band-spine-fixed\)\)/)
    expect(css).toMatch(/\.zone-card-band\[data-visible="3"\] \{[^}]*--zone-band-spine-fixed: calc\(var\(--zone-section-hairline\) \* var\(--zone-px\)\)/)
  })
})
