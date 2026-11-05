// Ported from surfaces/zone/test/plan-top-strip.test.ts at the cut; the checks that held the plan to zone-tokens.css now hold it to ui/tokens.slint (plan-tokens.test.ts).
/**
 * The shell's top strip: planTopStrip one pixel either side of every
 * threshold, the ladder the part states show, and the stylesheet following
 * the attribute the shell writes.
 */
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import {
  planTopStrip,
  topStripAttribute,
  ZONE_TOP_INLINE_WIDTH,
  ZONE_TOP_KEYS_WIDTH,
  ZONE_TOP_STACK_WORDS_WIDTH,
  ZONE_TOP_TIGHT_WIDTH,
} from "../src/plan/top-strip"

const level = (width: number) => planTopStrip(width).level

describe("planTopStrip at each threshold", () => {
  test("inline at and over the inline width, tight one pixel under", () => {
    expect(level(ZONE_TOP_INLINE_WIDTH + 1)).toBe("inline")
    expect(level(ZONE_TOP_INLINE_WIDTH)).toBe("inline")
    expect(level(ZONE_TOP_INLINE_WIDTH - 1)).toBe("tight")
  })
  test("tight at the tight width, keys one pixel under", () => {
    expect(level(ZONE_TOP_TIGHT_WIDTH)).toBe("tight")
    expect(level(ZONE_TOP_TIGHT_WIDTH - 1)).toBe("keys")
  })
  test("keys at the keys width, two rows one pixel under", () => {
    expect(level(ZONE_TOP_KEYS_WIDTH)).toBe("keys")
    expect(level(ZONE_TOP_KEYS_WIDTH - 1)).toBe("stacked")
  })
  test("two rows keep the hints' words down to their width, then keycaps alone", () => {
    expect(planTopStrip(ZONE_TOP_STACK_WORDS_WIDTH).words).toBe(true)
    expect(planTopStrip(ZONE_TOP_STACK_WORDS_WIDTH - 1).words).toBe(false)
    expect(planTopStrip(ZONE_TOP_STACK_WORDS_WIDTH - 1).rows).toBe(2)
  })
  test("the thresholds descend, so the ladder only ever yields more", () => {
    expect(ZONE_TOP_INLINE_WIDTH).toBeGreaterThan(ZONE_TOP_TIGHT_WIDTH)
    expect(ZONE_TOP_TIGHT_WIDTH).toBeGreaterThan(ZONE_TOP_KEYS_WIDTH)
    expect(ZONE_TOP_KEYS_WIDTH).toBeGreaterThan(ZONE_TOP_STACK_WORDS_WIDTH)
  })
})

describe("the yield order: space, then the hints' words, then a second row", () => {
  test("gaps and type reach their least before a word goes", () => {
    const tight = planTopStrip(ZONE_TOP_TIGHT_WIDTH)
    expect(tight).toEqual({ level: "tight", rows: 1, words: true, least: true })
  })
  test("words go before the strip takes a second row", () => {
    expect(planTopStrip(ZONE_TOP_KEYS_WIDTH)).toEqual({ level: "keys", rows: 1, words: false, least: true })
  })
  test("plan size above the inline width", () => {
    expect(planTopStrip(640)).toEqual({ level: "inline", rows: 1, words: true, least: false })
  })
})

describe("the part ladder: hints and tabs never share a row they do not fit", () => {
  // Measured in art px (Zone Caps, caps-1 tabs and hints, the
  // caps-2 banner, each hint's column its own width): one row needs 472 at
  // plan size, 414 at the least gaps and tab padding and 307 with the
  // keycaps alone; the two hints with their words need 147.
  const ONE_ROW_NEEDS = { inline: 472, tight: 414, keys: 307 } as const
  const HINT_WORDS_NEED = 147
  // The devices and frames, in art px: 1280x720 is 426 wide, 640x480 320,
  // the 480-wide frame 240 (two CSS px an art px), the 360- and 260-wide
  // frames 360 and 260 (one).
  for (const width of [426, 320, 240, 360, 260]) {
    test(`${width} art px wide`, () => {
      const plan = planTopStrip(width)
      if (plan.rows === 1) {
        const need = ONE_ROW_NEEDS[plan.level as keyof typeof ONE_ROW_NEEDS]
        expect(width).toBeGreaterThanOrEqual(need)
      } else {
        expect(plan.level).toBe("stacked")
        if (plan.words) expect(width).toBeGreaterThanOrEqual(HINT_WORDS_NEED)
      }
    })
  }
  test("720p keeps the hints' words in one row; 640x480 one row of keycaps; 480x800 two rows with words", () => {
    expect(planTopStrip(426)).toMatchObject({ level: "tight", rows: 1, words: true })
    expect(planTopStrip(360)).toMatchObject({ level: "keys", rows: 1 })
    expect(planTopStrip(320)).toMatchObject({ level: "keys", rows: 1 })
    expect(planTopStrip(240)).toMatchObject({ level: "stacked", rows: 2, words: true })
    expect(planTopStrip(160)).toMatchObject({ level: "stacked", rows: 2, words: true })
    expect(planTopStrip(140)).toMatchObject({ level: "stacked", rows: 2, words: false })
  })
})

