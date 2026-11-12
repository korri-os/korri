/**
 * Contrast (WCAG 2): Velvet is a dark house, and its words must stand on
 * it wherever the light falls.
 *
 *  - House light on House down, and ink on House light (the one filled
 *    button, the focused row): 12 to 1 or more.
 *  - Usher (facts) on House down: 7 to 1. Exit (what cannot be undone, what
 *    went wrong) on House down and on a sheet's ground: 4.5 to 1.
 *  - The quiet ink on House light: 4.5 to 1.
 *  - The stage light, for any art colour and any title: House light on the
 *    light's core keeps 4.5 to 1, and Usher on the light where facts can
 *    stand (the light at 42 %) keeps 4.5 to 1. So a logo-less title and its
 *    facts can be read on any game's light.
 *  - The tokens in ui/tokens.slint and the logic's copies (src/colour.ts)
 *    are the same colours.
 */
import { describe, expect, test } from "bun:test"
import { contrast, GELS, HOUSE_DOWN, HOUSE_LIGHT, hsl, LIGHT_CONTRAST, LIGHT_CORE, mix, stageLight, toHsl, USHER } from "../src/colour"
import { read, stripComments } from "./slint-source"

const tokens = new Map<string, number>()
for (const match of stripComments(read("ui/tokens.slint")).matchAll(/out property <color> ([\w-]+): #([0-9a-fA-F]{6})\b;/g)) tokens.set(match[1]!, parseInt(match[2]!, 16))
const token = (name: string): number => {
  const value = tokens.get(name)
  if (value === undefined) throw new Error(`no token ${name}`)
  return value
}

describe("contrast", () => {
  test("the logic's house colours are the tokens'", () => {
    expect(token("house-down")).toBe(HOUSE_DOWN)
    expect(token("house-light")).toBe(HOUSE_LIGHT)
    expect(token("usher")).toBe(USHER)
  })

  test("the house's pairs", () => {
    expect(contrast(token("house-light"), token("house-down"))).toBeGreaterThanOrEqual(12)
    expect(contrast(token("ink"), token("house-light"))).toBeGreaterThanOrEqual(12)
    expect(contrast(token("usher"), token("house-down"))).toBeGreaterThanOrEqual(7)
    expect(contrast(token("exit"), token("house-down"))).toBeGreaterThanOrEqual(4.5)
    expect(contrast(token("ink-quiet"), token("house-light"))).toBeGreaterThanOrEqual(4.5)
  })

  test("Exit is the one signal colour: no other token is a saturated hue", () => {
    const loud = [...tokens].filter(([, colour]) => { const [, s, l] = toHsl(colour); return s > 0.6 && l > 0.3 && l < 0.85 }).map(([name]) => name)
    expect(loud).toEqual(["exit"])
  })

  test("any game's light: a title on its core and facts on its fall-off can be read", () => {
    const colours: (string | undefined)[] = [undefined, "#000000", "#ffffff", "#808080"]
    for (let hue = 0; hue < 360; hue += 5) for (const s of [0.15, 0.5, 1]) for (const l of [0.08, 0.3, 0.5, 0.7, 0.92]) colours.push(`#${hsl(hue, s, l).toString(16).padStart(6, "0")}`)
    for (const colour of colours) {
      for (const title of ["Ember Road", "Tide Pool", "µCity", "X", ""]) {
        const light = stageLight(colour, title)
        expect(contrast(HOUSE_LIGHT, mix(HOUSE_DOWN, light, LIGHT_CORE)), `${colour} ${title}`).toBeGreaterThanOrEqual(LIGHT_CONTRAST - 0.01)
        expect(contrast(USHER, mix(HOUSE_DOWN, light, 0.42)), `${colour} ${title}`).toBeGreaterThanOrEqual(4.5)
      }
    }
  })

  test("a light keeps its colour: it is never grey, whatever the art", () => {
    for (const colour of [undefined, "#000000", "#777777", "#b0452a", "#2a7a8c", "#e8e000"]) {
      const [, saturation] = toHsl(stageLight(colour, "Ember Road"))
      expect(saturation, String(colour)).toBeGreaterThanOrEqual(0.29)
    }
  })

  test("the twelve gels are twelve hues, and none is a corporate blue at full strength", () => {
    expect(new Set(GELS).size).toBe(12)
    for (const title of ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n"]) {
      const [, saturation] = toHsl(stageLight(undefined, title))
      expect(saturation).toBeLessThanOrEqual(0.5)
    }
  })
})
