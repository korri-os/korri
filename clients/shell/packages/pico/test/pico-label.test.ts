/**
 * Cartridge plastics. The property that matters is not which colour a game
 * gets but that it always gets the same one, that a no-art sticker never
 * vanishes into its own shell, and that the ink on the sticker can be read.
 */
import { describe, expect, test } from "bun:test"
import { picoLabelFor } from "../src/pico-label"
import { PICO_SHELL_COLORS } from "../src/pico-palette"

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
  test.each(IDS)("never puts %p's sticker on a shell of its own colour", (id) => {
    const label = picoLabelFor(id)
    expect(label.sticker).not.toBe(label.shell)
  })

  test.each(IDS)("keeps %p inside the shell colours", (id) => {
    const label = picoLabelFor(id)
    expect(label.shell).toBeGreaterThanOrEqual(0)
    expect(label.shell).toBeLessThan(PICO_SHELL_COLORS.length)
    expect(label.sticker).toBeGreaterThanOrEqual(0)
    expect(label.sticker).toBeLessThan(PICO_SHELL_COLORS.length)
  })

  test("spreads a realistic library across the shells", () => {
    // One colour for everything would read as a row of identical rectangles.
    const shells = new Set(
      Array.from({ length: 40 }, (_, index) => picoLabelFor(`game-${index}`).shell),
    )
    expect(shells.size).toBeGreaterThanOrEqual(5)
  })
})
