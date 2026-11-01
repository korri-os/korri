/**
 * Small text on a card's plastic stays readable
 * (surfaces/pico/test/contrast-gate.test.ts).
 *
 * A card's shell is one of PICO-8's bright colours (yellow asks, red warns,
 * blue tells). A note on it, such as the reason a pause-menu control is
 * disabled, is drawn in the small face, so it needs WCAG 2's 4.5:1 for normal
 * text. Lilac on blue was 1.7:1 and could not be read. The card's own ink,
 * drawn large, is held to WCAG 2's 3:1 for large text.
 *
 * The colours are read from ui/palette.slint, role by role, down to PICO-8's
 * sixteen; the components are read for which role they draw a note in.
 */
import { describe, expect, test } from "bun:test"
import { join } from "node:path"
import { code, read, ROOT } from "./slint-source"

const palette = code(read(join(ROOT, "ui/palette.slint")))
const roles = new Map([...palette.matchAll(/property\s*<color>\s*([\w-]+)\s*:\s*([^;]+);/g)].map(match => [match[1] ?? "", (match[2] ?? "").trim()]))

function hex(role: string): string {
  const value = roles.get(role)
  if (value === undefined) throw new Error(`${role} is not in the palette`)
  return value.startsWith("#") ? value.toLowerCase() : hex(value)
}

function luminance(color: string): number {
  const [r = 0, g = 0, b = 0] = [1, 3, 5]
    .map(at => Number.parseInt(color.slice(at, at + 2), 16) / 255)
    .map(c => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4))
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}

const contrast = (a: string, b: string) => {
  const [hi = 0, lo = 0] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return (hi + 0.05) / (lo + 0.05)
}

describe("text on a card's plastic", () => {
  for (const tone of ["ask", "warn", "tell"] as const) {
    test(`a note on a ${tone} card reaches 4.5:1`, () => {
      expect(contrast(hex(`shell-${tone}`), hex(`shell-${tone}-note`))).toBeGreaterThanOrEqual(4.5)
    })
    // A card's title and words are large type (md and up, 21 px and more):
    // WCAG 2 asks 3:1 of large text. White on PICO-8's red is 3.5:1.
    test(`a ${tone} card's ink reaches 3:1 for its large type`, () => {
      expect(contrast(hex(`shell-${tone}`), hex(`shell-${tone}-ink`))).toBeGreaterThanOrEqual(3)
    })
  }
})

describe("the components draw notes in the note role", () => {
  test("a disabled control's reason and a card's note use the tone's note colour", () => {
    for (const file of ["ui/molecules/PicoControlRow.slint", "ui/molecules/PicoNotice.slint"]) {
      expect(`${file}: ${/P8\.(note|shell-[a-z]+-note)\b/.test(code(read(join(ROOT, file))))}`).toBe(`${file}: true`)
    }
  })
})
