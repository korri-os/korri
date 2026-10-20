/**
 * The option row's policy at its boundaries (src/plan/options-row.ts): the
 * stack threshold one art px either side, the 45 degree cell clip and its
 * cap, the cuts a fact takes when it does not fit, and the promise that
 * the plan's constants are the tokens Caliper turns.
 */
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import {
  isPathFact,
  optionRowStacked,
  ZONE_OPTION_CELL,
  ZONE_OPTION_ROW_STACK_AT,
  ZONE_OPTION_SLANT,
  ZONE_OPTIONS_BAND_INSET,
  ZONE_OPTIONS_NARROW,
  zoneCutMode,
  zoneCutValue,
  zoneLongestCut,
  zoneOptionClip,
  zoneWholeLine,
} from "../src/plan/options-row"
import { zoneStairClip } from "../src/plan/stair-clip"

describe("narrow: the row stacks its label over its value", () => {
  test("the threshold is the brief's 240-art-px screen less the band's inset each side", () => {
    expect(ZONE_OPTIONS_NARROW).toBe(240)
    expect(ZONE_OPTION_ROW_STACK_AT).toBe(240 - 2 * ZONE_OPTIONS_BAND_INSET)
  })
  test(`one art px either side of ${ZONE_OPTION_ROW_STACK_AT}`, () => {
    expect(optionRowStacked(ZONE_OPTION_ROW_STACK_AT - 1)).toBe(true)
    expect(optionRowStacked(ZONE_OPTION_ROW_STACK_AT)).toBe(true)
    expect(optionRowStacked(ZONE_OPTION_ROW_STACK_AT + 1)).toBe(false)
  })
  test("the sizes the brief names: 480x800 (240 wide) stacks, 640x480 (320 wide) does not", () => {
    expect(optionRowStacked(240 - 2 * ZONE_OPTIONS_BAND_INSET)).toBe(true)
    expect(optionRowStacked(320 - 2 * ZONE_OPTIONS_BAND_INSET)).toBe(false)
  })
  test("a row with no width yet (not laid out) is side by side", () => {
    expect(optionRowStacked(0)).toBe(false)
  })
})

describe("cells lean at 45 degrees, and no further when they wrap", () => {
  test("a one-line cell is the stair panel itself: its slant is its height less one", () => {
    expect(ZONE_OPTION_SLANT).toBe(ZONE_OPTION_CELL - 1)
    expect(zoneOptionClip(200, ZONE_OPTION_CELL)).toBe(zoneStairClip(200, ZONE_OPTION_CELL))
  })
  test("a taller cell keeps the one-line slant, stepped over its height", () => {
    const two = zoneOptionClip(200, 26)
    expect(two).toBe(zoneStairClip(2 * ZONE_OPTION_SLANT + 1, 26))
    expect(two).toContain(`calc(${ZONE_OPTION_SLANT} * var(--zone-px)) 0`)
    expect(two).not.toContain("calc(25 * var(--zone-px)) 0")
  })
  test("a flush panel (a problem, a warning) leans at its end only", () => {
    const flush = zoneOptionClip(200, ZONE_OPTION_CELL, { left: "straight", right: "slash" })
    expect(flush.startsWith("polygon(0 0")).toBe(true)
    expect(flush).toContain(`calc(100% - ${ZONE_OPTION_SLANT} * var(--zone-px))`)
  })
})

describe("a fact that does not fit is cut", () => {
  const build = "/nix/store/8n1x2-retroarch-1.19.1"
  test("paths cut in the middle, everything else at the end", () => {
    expect(isPathFact(build)).toBe(true)
    expect(isPathFact("~/roms")).toBe(true)
    expect(isPathFact("Configured")).toBe(false)
    expect(zoneCutMode(build)).toBe("middle")
    expect(zoneCutMode("the-very-long-network-name-xyz12")).toBe("end")
  })
  test("the end cut keeps the start", () => {
    expect(zoneCutValue("the-very-long-network-name-xyz12", 8, "end")).toBe("the-very…")
    expect(zoneCutValue("Configured", 10, "end")).toBe("Configured")
  })
  test("the middle cut keeps the last segment whole, and the start it has room for", () => {
    expect(zoneCutValue(build, 26, "middle")).toBe("/nix…8n1x2-retroarch-1.19.1")
    expect(zoneCutValue(build, 22, "middle")).toBe("…8n1x2-retroarch-1.19.1")
  })
  test("a last segment too long keeps its tail from a - or a ., so a name stays whole", () => {
    expect(zoneCutValue(build, 18, "middle")).toBe("…retroarch-1.19.1")
    expect(zoneCutValue(build, 15, "middle")).toBe("…1.19.1")
    expect(zoneCutValue("/a/abcdefghij", 4, "middle")).toBe("…ghij")
  })
  test("the longest cut that fits, by halving", () => {
    const fits = (limit: number) => (candidate: string) => [...candidate].length <= limit
    expect(zoneLongestCut(build, "middle", fits(100))).toBe(build)
    expect(zoneLongestCut(build, "middle", fits(18))).toBe("…retroarch-1.19.1")
    expect(zoneLongestCut("the-very-long-network-name-xyz12", "end", fits(9))).toBe("the-very…")
  })
  test("the whole line may wrap a path after each /, never inside a segment", () => {
    expect(zoneWholeLine("/run/media/roms")).toBe("/\u200brun/\u200bmedia/\u200broms")
    expect(zoneWholeLine("Configured")).toBe("Configured")
  })
})

describe("the plan's constants are the tokens", () => {
  const tokens = new Map(
    [...readFileSync(join(import.meta.dir, "..", "src", "zone-tokens.css"), "utf8")
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)]
      .map(([, name, value]) => [name ?? "", (value ?? "").trim()]),
  )
  test("the band's inset, the cell and its slant", () => {
    expect(tokens.get("--zone-options-band-inset")).toBe(`${ZONE_OPTIONS_BAND_INSET}`)
    expect(tokens.get("--zone-option-cell")).toBe(`${ZONE_OPTION_CELL}`)
    expect(tokens.get("--zone-option-slant")).toBe(`${ZONE_OPTION_SLANT}`)
  })
})
