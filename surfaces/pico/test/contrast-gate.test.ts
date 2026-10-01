/**
 * Small text on a card's plastic stays readable.
 *
 * A card's shell is one of PICO-8's bright colours (yellow asks, red warns,
 * blue tells). A note on it, such as the reason a pause-menu control is
 * disabled, is drawn in the small face, so it needs the WCAG 2 contrast for
 * normal text, 4.5:1. Lilac on blue was 1.7:1 and could not be read.
 *
 * The colours are read from the stylesheets, through their custom
 * properties, down to PICO-8's sixteen hex values in pico-tokens.css.
 */
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"

const SRC = join(import.meta.dir, "..", "src")
const read = (file: string) => readFileSync(join(SRC, file), "utf8").replace(/\/\*[\s\S]*?\*\//g, "")

/** Every custom property declared in a block whose selector matches. */
function declarations(source: string, selector: (text: string) => boolean): Map<string, string> {
  const found = new Map<string, string>()
  for (const [, text, body] of source.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    if (!selector((text ?? "").trim())) continue
    for (const [, name, value] of (body ?? "").matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)) {
      found.set(name ?? "", (value ?? "").trim())
    }
  }
  return found
}

const tokens = declarations(read("pico-tokens.css"), () => true)
const card = read("ui/molecules/PicoCard.css")
const base = declarations(card, (text) => text === ".pico-card")

function resolve(value: string, scope: Map<string, string>): string {
  const reference = value.match(/^var\((--[a-z0-9-]+)(?:,\s*(.+))?\)$/)
  if (reference === null) return value
  const [, name, fallback] = reference
  const next = scope.get(name ?? "") ?? tokens.get(name ?? "") ?? fallback
  if (next === undefined) throw new Error(`${name} is not declared`)
  return resolve(next, scope)
}

function luminance(hex: string): number {
  const channels = [1, 3, 5].map((at) => Number.parseInt(hex.slice(at, at + 2), 16) / 255)
  const [r = 0, g = 0, b = 0] = channels.map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4))
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}

const contrast = (a: string, b: string) => {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return ((hi ?? 0) + 0.05) / ((lo ?? 0) + 0.05)
}

/** The colour a disabled control's reason is drawn in, as its stylesheet says. */
const reasonColour = declarations(read("ui/molecules/PicoControlRow.css").replace(/color:/g, "--colour:"),
  (text) => text === ".pico-control-row-reason").get("--colour") ?? ""

describe("notes on a card's plastic", () => {
  for (const tone of ["ask", "warn", "tell"] as const) {
    test(`a disabled control's reason on a ${tone} card reaches 4.5:1`, () => {
      const toned = new Map([...base, ...declarations(card, (text) => text === `.pico-card[data-tone="${tone}"]`)])
      if (tone === "ask") toned.set("--pico-card-shell", base.get("--pico-card-shell") ?? "")
      const shell = resolve("var(--pico-card-shell)", toned)
      const note = resolve(reasonColour, toned)
      expect(contrast(shell, note)).toBeGreaterThanOrEqual(4.5)
    })
  }
})
