/**
 * Pico's authoring rules in .slint (surfaces/pico/test/authoring-gate.test.ts):
 *
 * - no raw colour outside ui/palette.slint, and the palette is PICO-8's
 *   sixteen and nothing else;
 * - no colour blend of any kind: no gradient, no opacity, no rgb() or hsl();
 * - no pixel literal outside ui/tokens.slint: every size is a whole number of
 *   Tokens.p, the virtual pixel, or a share of the box it is given;
 * - the virtual pixel is derived from the window's width and height.
 */
import { describe, expect, test } from "bun:test"
import { PICO_PALETTE } from "../src/pico-palette"
import { code, read, rel, slintFiles, stringsBlanked } from "./slint-source"
import { join } from "node:path"
import { ROOT } from "./slint-source"

const PALETTE = "ui/palette.slint"
const TOKENS = "ui/tokens.slint"
const files = slintFiles()

describe("colour", () => {
  test("no raw colour outside the palette", () => {
    const raw = files.filter(path => rel(path) !== PALETTE).flatMap(path =>
      [...stringsBlanked(read(path)).matchAll(/#[0-9a-fA-F]{3,8}\b/g)].map(match => `${rel(path)}: ${match[0]}`))
    expect(raw).toEqual([])
  })

  test("the palette is PICO-8's sixteen and nothing else", () => {
    const hexes = [...new Set([...code(read(join(ROOT, PALETTE))).matchAll(/#[0-9a-fA-F]{6}\b/g)].map(match => (match[0] ?? "").toLowerCase()))].sort()
    expect(hexes).toEqual(PICO_PALETTE.map(color => color.hex.toLowerCase()).sort())
  })

  test("no blend: no gradient, opacity, rgb(), rgba() or hsl()", () => {
    const blends = files.flatMap(path =>
      [...stringsBlanked(read(path)).matchAll(/@linear-gradient|@radial-gradient|@conic-gradient|\bopacity\s*:|\brgba?\s*\(|\bhsla?\s*\(|\.with-alpha\(|\.mix\(|\.transparentize\(|\.darker\(|\.brighter\(/g)]
        .map(match => `${rel(path)}: ${match[0]}`))
    expect(blends).toEqual([])
  })
})

describe("size", () => {
  test("no pixel literal outside the tokens", () => {
    const literals = files.filter(path => rel(path) !== TOKENS).flatMap(path =>
      [...stringsBlanked(read(path)).matchAll(/(?<![\w.])(\d+(?:\.\d+)?)\s*(px|phx)\b/g)]
        .filter(match => Number(match[1]) !== 0)
        .map(match => `${rel(path)}: ${match[0]}`))
    expect(literals).toEqual([])
  })

  test("the virtual pixel derives from the window's width and height", () => {
    const tokens = code(read(join(ROOT, TOKENS)))
    expect(tokens).toMatch(/property\s*<length>\s*p\s*:[^;]*min\(\s*width\s*,\s*height\s*\)/)
    expect(tokens).toMatch(/pixel-rows\s*:\s*360/)
  })
})
