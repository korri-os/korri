/**
 * Cartridge labels. The property that matters is not which colour a game gets
 * but that it always gets the same one, that it is never a flat rectangle, and
 * that the ink on it can be read.
 */
import { describe, expect, test } from "bun:test"
import { picoLabelFor } from "../src/pico-label"
import { PICO_LABEL_COLORS, picoLuminance } from "../src/pico-palette"

const IDS = [
  "celeste",
  "hollow",
  "tetris",
  "spelunky",
  "a",
  "",
  "very-long-identifier-with-dashes",
  "🎮",
]

describe("picoLabelFor", () => {
  test("gives the same game the same label every time", () => {
    // A label that changed between visits would be worse than no label.
    expect(picoLabelFor("celeste")).toEqual(picoLabelFor("celeste"))
  })

  test.each(IDS)("never dithers %p against itself", (id) => {
    // fill === accent is a flat rectangle, which is what this exists to avoid.
    const label = picoLabelFor(id)
    expect(label.fill).not.toBe(label.accent)
  })

  test.each(IDS)("keeps %p inside the palette", (id) => {
    const label = picoLabelFor(id)
    expect(label.fill).toBeGreaterThanOrEqual(0)
    expect(label.fill).toBeLessThan(PICO_LABEL_COLORS.length)
    expect(label.accent).toBeGreaterThanOrEqual(0)
    expect(label.accent).toBeLessThan(PICO_LABEL_COLORS.length)
    expect([0, 1, 2]).toContain(label.dither)
  })

  test.each(IDS)("puts readable ink on %p", (id) => {
    const label = picoLabelFor(id)
    const fill = PICO_LABEL_COLORS[label.fill]
    expect(fill).toBeDefined()
    const bright = picoLuminance(fill as (typeof PICO_LABEL_COLORS)[number]) >= 140
    expect(label.ink).toBe(bright ? "dark" : "light")
  })

  test("spreads a realistic library across the palette", () => {
    // One colour for everything would look exactly like the flat shelf this
    // replaced, so a spread is the actual requirement.
    const fills = new Set(
      Array.from({ length: 40 }, (_, index) => picoLabelFor(`game-${index}`).fill),
    )
    expect(fills.size).toBeGreaterThanOrEqual(5)
  })
})
