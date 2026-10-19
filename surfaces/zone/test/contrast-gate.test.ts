/**
 * Every lettering pair Zone draws stays readable.
 *
 * Zone has eight colours and draws words in a handful of pairs: white caps on
 * a black panel, yellow on the blue banner, white on a blue or red tag, black
 * on a yellow tag, black on the mint ground for the build string, yellow and
 * mint on black in the foot. Anton caps at Zone's sizes are large text, so the
 * WCAG 2 floor is 3:1; the two small readouts (the build string and the
 * badge's SSID) need 4.5:1.
 *
 * The colours are read from zone-tokens.css through their role names, so a
 * palette edit that breaks a pair fails here, not on a device.
 */
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"

const SRC = join(import.meta.dir, "..", "src")
const read = (file: string) => readFileSync(join(SRC, file), "utf8").replace(/\/\*[\s\S]*?\*\//g, "")

/** Every custom property declared anywhere in the stylesheet. */
function declarations(source: string): Map<string, string> {
  const found = new Map<string, string>()
  for (const [, name, value] of source.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)) {
    found.set(name ?? "", (value ?? "").trim())
  }
  return found
}

const tokens = declarations(read("zone-tokens.css"))

function resolve(value: string): string {
  const reference = value.match(/^var\((--[a-z0-9-]+)\)$/)
  if (reference === null) return value
  const next = tokens.get(reference[1] ?? "")
  if (next === undefined) throw new Error(`${reference[1]} is not declared`)
  return resolve(next)
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

const LARGE = 3
const SMALL = 4.5

/** Ink on ground, by role, and the floor the pair must reach. */
const PAIRS: readonly (readonly [ink: string, ground: string, floor: number])[] = [
  ["--zone-cap", "--zone-panel", LARGE],
  ["--zone-banner-ink", "--zone-banner", LARGE],
  ["--zone-tag-ink", "--zone-tag-system", LARGE],
  ["--zone-tag-ink", "--zone-tag-remote", LARGE],
  ["--zone-tag-here-ink", "--zone-tag-here", LARGE],
  ["--zone-timer-label", "--zone-panel", LARGE],
  ["--zone-badge-strength", "--zone-panel", LARGE],
  ["--zone-key-b-ink", "--zone-panel", LARGE],
  ["--zone-key-a-ink", "--zone-panel", LARGE],
  ["--zone-build-ink", "--zone-ground", SMALL],
  ["--zone-cap", "--zone-panel", SMALL],
]

describe("lettering on Zone's panels and ground", () => {
  for (const [ink, ground, floor] of PAIRS) {
    test(`${ink} on ${ground} reaches ${floor}:1`, () => {
      expect(contrast(resolve(`var(${ink})`), resolve(`var(${ground})`))).toBeGreaterThanOrEqual(floor)
    })
  }
})
