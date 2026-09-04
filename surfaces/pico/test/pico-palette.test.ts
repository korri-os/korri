/**
 * The palette is written down twice — as custom properties for the stylesheet
 * and as channels for the quantiser — so this makes drift impossible rather
 * than unlikely. Without it, a re-palette would silently leave every quantised
 * image mapping to the old sixteen.
 */
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { PICO_LABEL_COLORS, PICO_PALETTE } from "../src/pico-palette"

const tokens = readFileSync(
  join(import.meta.dir, "..", "src", "pico-tokens.css"),
  "utf8",
)

describe("the palette in TypeScript and the palette in CSS", () => {
  test.each(PICO_PALETTE.map((color) => [color.name, color.hex]))(
    "--p8-%s is %s in both",
    (name, hex) => {
      const declared = tokens.match(
        new RegExp(`--p8-${name}:\\s*(#[0-9a-fA-F]{6})`),
      )?.[1]
      expect(declared?.toLowerCase()).toBe(hex.toLowerCase())
    },
  )

  test("every label role points at a colour the label set actually contains", () => {
    const names = new Set(PICO_LABEL_COLORS.map((color) => color.name))
    const roles = [...tokens.matchAll(/--pico-label-\d+:\s*var\(--p8-(\w+)\)/g)]

    expect(roles.length).toBe(PICO_LABEL_COLORS.length)
    for (const [, name] of roles) expect(names.has(name ?? "")).toBe(true)
  })

  test("label roles are declared in the same order the module lists them", () => {
    // The cart selects a colour by index, so order is the contract.
    const roles = [...tokens.matchAll(/--pico-label-(\d+):\s*var\(--p8-(\w+)\)/g)]
    for (const [, index, name] of roles) {
      expect(PICO_LABEL_COLORS[Number(index)]?.name).toBe(name ?? "")
    }
  })
})
