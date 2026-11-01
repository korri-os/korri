/**
 * The palette is written down twice — as colours for main.slint
 * (palette.slint) and as data for the cart plastics (pico-palette.ts) — so
 * this makes drift impossible rather than unlikely.
 */
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { PICO_PALETTE, PICO_SHELL_COLORS } from "../src/pico-palette"

const palette = readFileSync(join(import.meta.dir, "..", "ui", "palette.slint"), "utf8")

describe("the palette in TypeScript and the palette in Slint", () => {
  test.each(PICO_PALETTE.map((color) => [color.name, color.hex]))(
    "%s is %s in both",
    (name, hex) => {
      const declared = palette.match(new RegExp(`property <color> ${name}:\\s*(#[0-9a-fA-F]{6})`))?.[1]
      expect(declared?.toLowerCase()).toBe(hex.toLowerCase())
    },
  )

  test("shell plastics are listed in the order the module lists them", () => {
    // The cart selects a colour by index, so order is the contract.
    const body = palette.slice(palette.indexOf("function plastic("))
    const names = [...body.slice(0, body.indexOf("}")).matchAll(/index == (\d+) \? (\w+)/g)]
    for (const [, index, name] of names) expect(PICO_SHELL_COLORS[Number(index)]?.name).toBe(name ?? "")
    expect(PICO_SHELL_COLORS[PICO_SHELL_COLORS.length - 1]?.name).toBe(body.match(/: (\w+)\s*\n\s*}/)?.[1] ?? "")
  })
})
