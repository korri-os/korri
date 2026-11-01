/**
 * Contrast: every lettering pair Zone draws stays readable, judged on the
 * token values in ui/tokens.slint (ported from Zone's contrast gate).
 *
 * Zone Caps and Anton at caps-1 and up are large text (WCAG 3:1); the
 * small readouts (the build string, the badge's SSID) need 4.5:1. Outlined
 * lettering is judged face against its ring, and a two-tone face's shade
 * against the ring too. Each screen group's ground is judged on its own.
 * The looks change faces, never colours, so one judgement holds for all
 * three; Modern's flat face draws the shade in the face's colour, which is
 * judged as the face.
 */
import { describe, expect, test } from "bun:test"
import { read, stripComments } from "./slint-source"

const LARGE = 3
const SMALL = 4.5

const tokens = new Map<string, string>()
for (const [, name, value] of stripComments(read("ui/tokens.slint")).matchAll(/property\s*<color>\s*([\w-]+)\s*:\s*(#[0-9a-fA-F]{6})\b/g)) {
  tokens.set(name!, value!)
}

function luminance(hex: string): number {
  const channel = (at: number) => {
    const c = Number.parseInt(hex.slice(at, at + 2), 16) / 255
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
  }
  return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5)
}

function contrast(a: string, b: string): number {
  const one = tokens.get(a)
  const two = tokens.get(b)
  if (one === undefined || two === undefined) throw new Error(`no colour token ${one === undefined ? a : b}`)
  const [hi, lo] = [luminance(one), luminance(two)].sort((x, y) => y - x) as [number, number]
  return (hi + 0.05) / (lo + 0.05)
}

/** Ink on the surface it is drawn on. */
const PAIRS: readonly (readonly [ink: string, ground: string, floor: number])[] = [
  ["white", "black", LARGE],
  ["white", "black", SMALL],
  ["banner-yellow", "banner-blue", LARGE],
  ["white", "blue", LARGE],
  ["white", "red", LARGE],
  ["black", "yellow", LARGE],
  ["black", "orange", LARGE],
  ["black", "white", LARGE],
  ["black", "mint", LARGE],
  ["black", "stripe-light", LARGE],
  ["yellow", "black", LARGE],
  ["key-b", "key-disc", LARGE],
  ["key-a", "key-disc", LARGE],
  ["white-shade", "black", LARGE],
  ["steel", "black", LARGE],
  ["danger", "black", LARGE],
  ["danger-shade", "black", LARGE],
]

/** Outlined inks: the face and its shade against the ring. */
const OUTLINED: readonly (readonly [face: string, ring: string, floor: number])[] = [
  ["white", "black", LARGE],
  ["white-shade", "black", LARGE],
  ["yellow", "hud-edge", LARGE],
  ["banner-yellow", "black", LARGE],
  ["banner-white", "black", LARGE],
  ["danger", "black", LARGE],
  ["danger-shade", "black", LARGE],
]

/** The grounds of every screen group: the build string's black stands on each. */
const GROUNDS = ["ground-games", "ground-options", "ground-savedata", "ground-gameopts", "ground-paused", "ground-titlecard", "ground-tally"]

describe("contrast", () => {
  test("every screen group has its ground", () => {
    expect(GROUNDS.filter(name => !tokens.has(name))).toEqual([])
  })
  for (const [ink, ground, floor] of PAIRS) {
    test(`${ink} on ${ground} reaches ${floor}:1`, () => expect(contrast(ink, ground)).toBeGreaterThanOrEqual(floor))
  }
  for (const [face, ring, floor] of OUTLINED) {
    test(`${face} inside its ${ring} ring reaches ${floor}:1`, () => expect(contrast(face, ring)).toBeGreaterThanOrEqual(floor))
  }
  for (const ground of GROUNDS) {
    test(`the build string's black on ${ground} reaches ${SMALL}:1`, () => expect(contrast("black", ground)).toBeGreaterThanOrEqual(SMALL))
  }
})
