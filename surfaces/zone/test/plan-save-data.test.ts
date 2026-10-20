/**
 * The SAVE DATA page's plan: the slab's threshold one pixel either side,
 * the stylesheet's container query held to the same number, and the shapes
 * of the secret and of a shortened key.
 */
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import {
  planSaveData,
  secretLines,
  shortKey,
  ZONE_KEY_END,
  ZONE_SAVE_SLAB_BESIDE_ASPECT,
  ZONE_SECRET_GROUP,
  ZONE_SECRET_LINE,
} from "../src/plan/save-data"

describe("planSaveData", () => {
  const height = 400
  const edge = height * ZONE_SAVE_SLAB_BESIDE_ASPECT
  test("one pixel under the threshold the slab stands under the band", () => {
    expect(planSaveData(edge - 1, height).slab).toBe("under")
  })
  test("at the threshold and one pixel over it the slab stands beside the band", () => {
    expect(planSaveData(edge, height).slab).toBe("beside")
    expect(planSaveData(edge + 1, height).slab).toBe("beside")
  })
  test("the ladder: 1280x720 and 640x480 and 1280x300 beside, 480x800 under", () => {
    // Body sizes: the screen less the shell's strips.
    expect(planSaveData(1280, 720 - 74 - 58).slab).toBe("beside")
    expect(planSaveData(640, 480 - 56 - 46).slab).toBe("beside")
    expect(planSaveData(1280, 300 - 56 - 46).slab).toBe("beside")
    expect(planSaveData(480, 800 - 74 - 58).slab).toBe("under")
  })
  test("no height is no room to stand beside", () => {
    expect(planSaveData(800, 0).slab).toBe("under")
  })
  test("the page's container query holds the same number", () => {
    const css = readFileSync(join(import.meta.dir, "..", "src", "pages", "ZoneSaveData.css"), "utf8")
    const match = /@container zone-save-data \(min-aspect-ratio: (\d+) \/ (\d+)\)/.exec(css)
    expect(match).not.toBeNull()
    expect(Number(match?.[1]) / Number(match?.[2])).toBe(ZONE_SAVE_SLAB_BESIDE_ASPECT)
  })
})

describe("secretLines", () => {
  test("groups of four, thirty-two characters to a line, in order", () => {
    const secret = "abcdefghijklmnopqrstuvwxyz0123456789"
    const lines = secretLines(secret)
    expect(lines).toEqual([
      ["abcd", "efgh", "ijkl", "mnop", "qrst", "uvwx", "yz01", "2345"],
      ["6789"],
    ])
    expect(lines.flat().join("")).toBe(secret)
    expect(ZONE_SECRET_LINE % ZONE_SECRET_GROUP).toBe(0)
  })
  test("a 162-character backup is six lines, the last short", () => {
    const lines = secretLines("x".repeat(162))
    expect(lines.length).toBe(6)
    expect(lines.at(-1)?.join("")).toBe("xx")
  })
  test("an empty secret is no lines", () => {
    expect(secretLines("")).toEqual([])
  })
})

describe("shortKey", () => {
  test("the first and last eight characters", () => {
    const key = "npub1abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstu"
    expect(shortKey(key)).toBe(`${key.slice(0, ZONE_KEY_END)}…${key.slice(-ZONE_KEY_END)}`)
  })
  test("a key too short to shorten is shown whole", () => {
    expect(shortKey("abcdefghijklmnopq")).toBe("abcdefghijklmnopq")
    expect(shortKey("abcdefghijklmnopqr")).toBe("abcdefgh…klmnopqr")
  })
})
