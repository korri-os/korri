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
import { applyTopStrip } from "../src/use-zone-top-strip"

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
  // Measured in Chromium in art px (Zone Caps, caps-1 tabs and hints, the
  // caps-2 banner): one row needs 483 at plan size, 446 at the least gaps
  // and 339 with the keycaps alone.
  const ONE_ROW_NEEDS = { inline: 483, tight: 446, keys: 339 } as const
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
      }
    })
  }
  test("720p keeps one row with keycaps alone; 640x480 and the narrow frames take two rows with words", () => {
    expect(planTopStrip(426)).toMatchObject({ level: "keys", rows: 1 })
    expect(planTopStrip(360)).toMatchObject({ level: "keys", rows: 1 })
    expect(planTopStrip(320)).toMatchObject({ level: "stacked", rows: 2, words: true })
    expect(planTopStrip(240)).toMatchObject({ level: "stacked", rows: 2, words: true })
    expect(planTopStrip(160)).toMatchObject({ level: "stacked", rows: 2, words: false })
  })
})

describe("the shell writes the plan; its stylesheet follows", () => {
  test("the attribute names each answer, and a box with no width keeps none", () => {
    const element = document.createElement("div")
    // A test DOM computes no --zone-px, so one CSS px is one art px.
    applyTopStrip(element, 640)
    expect(element.dataset.top).toBe("inline")
    applyTopStrip(element, 400)
    expect(element.dataset.top).toBe("keys")
    applyTopStrip(element, 150)
    expect(element.dataset.top).toBe("stacked-keys")
    applyTopStrip(element, 0)
    expect(element.dataset.top).toBeUndefined()
  })
  test("every attribute value has a rule in the shell's stylesheet", () => {
    const css = readFileSync(join(import.meta.dir, "..", "src", "ui", "templates", "ZoneShell.css"), "utf8")
    const values = new Set([640, 460, 400, 300, 150].map(width => topStripAttribute(planTopStrip(width))))
    expect([...values].sort()).toEqual(["inline", "keys", "stacked", "stacked-keys", "tight"])
    for (const value of values) {
      if (value === "inline") continue
      expect(css).toContain(`[data-top="${value}"]`)
    }
  })
  test("dropped words hide through the keycap's own stylesheet, not the shell's", () => {
    const keycap = readFileSync(join(import.meta.dir, "..", "src", "ui", "atoms", "ZoneKeycap.css"), "utf8")
    expect(keycap).toContain("display: var(--zone-keycap-words, inline-flex)")
  })
})
