/**
 * The layout policy at its boundaries: one pixel either side of every
 * threshold, the yield when the visible set does not fit, and the promise
 * that the plan's constants are the tokens Caliper turns.
 */
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import {
  plan,
  ZONE_ACTION_STRIP,
  ZONE_BAND_MARGIN,
  ZONE_BAND_PAD,
  ZONE_CARD_FOCUS_MAX,
  ZONE_CARD_GAP,
  ZONE_CARD_RATIO,
  ZONE_CARD_SIDE_SCALE,
  ZONE_FOOT_STRIP,
  ZONE_TAB_STRIP,
  ZONE_TALL_HEIGHT,
  ZONE_TRIANGLE,
  ZONE_TRIANGLE_GAP,
  ZONE_TYPE_BUTTON,
  ZONE_TYPE_CARD_SHARE,
  ZONE_TYPE_FOOT,
  ZONE_TYPE_HINT,
  ZONE_TYPE_SIDE_SHARE,
  ZONE_TYPE_TAB,
  ZONE_WIDE_WIDTH,
} from "../src/plan/zone-select"

describe("width decides how many cards", () => {
  test(`one pixel below ${ZONE_WIDE_WIDTH} shows three, narrow triangles and gaps`, () => {
    const narrow = plan(ZONE_WIDE_WIDTH - 1, 720)
    expect(narrow.band).toBe("narrow")
    expect(narrow.visible).toBe(3)
    expect(narrow.tri).toBe(ZONE_TRIANGLE.narrow)
    expect(narrow.gap).toBe(ZONE_CARD_GAP.narrow)
    expect(narrow.tabFs).toBe(ZONE_TYPE_TAB.narrow)
    expect(narrow.hintFs).toBe(ZONE_TYPE_HINT.narrow)
  })

  test(`at ${ZONE_WIDE_WIDTH} shows five, wide triangles and gaps`, () => {
    const wide = plan(ZONE_WIDE_WIDTH, 720)
    expect(wide.band).toBe("wide")
    expect(wide.visible).toBe(5)
    expect(wide.tri).toBe(ZONE_TRIANGLE.wide)
    expect(wide.gap).toBe(ZONE_CARD_GAP.wide)
    expect(wide.tabFs).toBe(ZONE_TYPE_TAB.wide)
    expect(wide.hintFs).toBe(ZONE_TYPE_HINT.wide)
  })

  test("width never changes the strips", () => {
    expect(plan(ZONE_WIDE_WIDTH - 1, 720).tabH).toBe(plan(ZONE_WIDE_WIDTH, 720).tabH)
    expect(plan(ZONE_WIDE_WIDTH - 1, 720).footFs).toBe(plan(ZONE_WIDE_WIDTH, 720).footFs)
  })
})

describe("height decides the strips", () => {
  test(`one pixel below ${ZONE_TALL_HEIGHT} takes the short strips and type`, () => {
    const short = plan(1280, ZONE_TALL_HEIGHT - 1)
    expect(short.strips).toBe("short")
    expect(short.tabH).toBe(ZONE_TAB_STRIP.short)
    expect(short.actH).toBe(ZONE_ACTION_STRIP.short)
    expect(short.footH).toBe(ZONE_FOOT_STRIP.short)
    expect(short.btnFs).toBe(ZONE_TYPE_BUTTON.short)
    expect(short.footFs).toBe(ZONE_TYPE_FOOT.short)
  })

  test(`at ${ZONE_TALL_HEIGHT} takes the tall strips and type`, () => {
    const tall = plan(1280, ZONE_TALL_HEIGHT)
    expect(tall.strips).toBe("tall")
    expect(tall.tabH).toBe(ZONE_TAB_STRIP.tall)
    expect(tall.actH).toBe(ZONE_ACTION_STRIP.tall)
    expect(tall.footH).toBe(ZONE_FOOT_STRIP.tall)
    expect(tall.btnFs).toBe(ZONE_TYPE_BUTTON.tall)
    expect(tall.footFs).toBe(ZONE_TYPE_FOOT.tall)
  })

  test("height never changes how many cards show", () => {
    expect(plan(1280, ZONE_TALL_HEIGHT - 1).visible).toBe(plan(1280, ZONE_TALL_HEIGHT).visible)
  })

  test("the band is what the strips leave", () => {
    const tall = plan(1280, 720)
    expect(tall.bandH).toBe(720 - ZONE_TAB_STRIP.tall - ZONE_ACTION_STRIP.tall - ZONE_FOOT_STRIP.tall)
  })
})

describe("the focused card", () => {
  test(`never grows past ${ZONE_CARD_FOCUS_MAX}`, () => {
    const tall = plan(1920, 1080)
    expect(tall.focusH).toBe(ZONE_CARD_FOCUS_MAX)
    expect(tall.scale).toBe(1)
  })

  test("one pixel below the cap takes the band less its padding", () => {
    const bandH = ZONE_CARD_FOCUS_MAX + ZONE_BAND_PAD * 2 - 1
    const height = bandH + ZONE_TAB_STRIP.tall + ZONE_ACTION_STRIP.tall + ZONE_FOOT_STRIP.tall
    expect(plan(1920, height).focusH).toBe(ZONE_CARD_FOCUS_MAX - 1)
  })

  test("keeps the card ratio and the side scale", () => {
    const tall = plan(1920, 1080)
    expect(tall.focusW).toBe(Math.round(ZONE_CARD_FOCUS_MAX * ZONE_CARD_RATIO))
    expect(tall.sideH).toBe(Math.round(ZONE_CARD_FOCUS_MAX * ZONE_CARD_SIDE_SCALE))
    expect(tall.cardFs).toBe(Math.round(tall.focusW * ZONE_TYPE_CARD_SHARE))
    expect(tall.sideFs).toBe(Math.round(tall.sideW * ZONE_TYPE_SIDE_SHARE))
  })
})

describe("nothing is cut: the visible set always fits", () => {
  const fits = (width: number, height: number) => {
    const p = plan(width, height)
    const fixed = (p.visible - 1) * p.gap + 2 * (p.tri + ZONE_TRIANGLE_GAP) + ZONE_BAND_MARGIN * 2
    return p.focusW + (p.visible - 1) * p.sideW + fixed <= width + p.visible
  }

  test("at the mockup's two sizes the plan is the mockup's: shrunk a little so five, then three, fit", () => {
    const wide = plan(1280, 720)
    expect([wide.focusH, wide.focusW, wide.sideH, wide.sideW]).toEqual([361, 282, 246, 192])
    expect(wide.scale).toBeCloseTo(0.9034, 3)
    const narrow = plan(640, 480)
    expect([narrow.focusH, narrow.focusW, narrow.sideH, narrow.sideW]).toEqual([261, 204, 177, 138])
    expect(narrow.scale).toBeCloseTo(0.9524, 3)
    expect(fits(1280, 720)).toBe(true)
    expect(fits(640, 480)).toBe(true)
  })

  test("a generous screen fits without shrinking", () => {
    expect(plan(1920, 1080).scale).toBe(1)
    expect(fits(1920, 1080)).toBe(true)
  })

  test("a narrow tall column shrinks every card by one factor and keeps three", () => {
    const column = plan(420, 900)
    expect(column.visible).toBe(3)
    expect(column.scale).toBeLessThan(1)
    expect(column.scale).toBeGreaterThan(0)
    expect(fits(420, 900)).toBe(true)
    const unscaled = plan(1920, 900)
    expect(column.focusH / column.sideH).toBeCloseTo(unscaled.focusH / unscaled.sideH, 1)
  })

  test("a wide short strip keeps five cards and the short strips", () => {
    const strip = plan(1280, 300)
    expect(strip.visible).toBe(5)
    expect(strip.strips).toBe("short")
    expect(strip.focusH).toBeGreaterThan(0)
    expect(fits(1280, 300)).toBe(true)
  })

  test("a box too small for any band still answers, with nothing negative", () => {
    const tiny = plan(100, 100)
    expect(tiny.bandH).toBe(0)
    expect(tiny.focusH).toBe(0)
    expect(tiny.focusW).toBe(0)
    expect(tiny.visible).toBe(3)
  })
})

describe("the plan's constants are the tokens", () => {
  const tokens = new Map(
    [...readFileSync(join(import.meta.dir, "..", "src", "zone-tokens.css"), "utf8")
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)]
      .map(([, name, value]) => [name ?? "", (value ?? "").trim()]),
  )
  const px = (name: string) => `${tokens.get(name)}`
  const number = (name: string) => Number(tokens.get(name))

  test.each([
    ["--zone-tab-strip-tall", ZONE_TAB_STRIP.tall],
    ["--zone-tab-strip-short", ZONE_TAB_STRIP.short],
    ["--zone-action-strip-tall", ZONE_ACTION_STRIP.tall],
    ["--zone-action-strip-short", ZONE_ACTION_STRIP.short],
    ["--zone-foot-strip-tall", ZONE_FOOT_STRIP.tall],
    ["--zone-foot-strip-short", ZONE_FOOT_STRIP.short],
    ["--zone-card-focus-max", ZONE_CARD_FOCUS_MAX],
    ["--zone-band-pad", ZONE_BAND_PAD],
    ["--zone-band-margin", ZONE_BAND_MARGIN],
    ["--zone-card-gap-wide", ZONE_CARD_GAP.wide],
    ["--zone-card-gap-narrow", ZONE_CARD_GAP.narrow],
    ["--zone-triangle-wide", ZONE_TRIANGLE.wide],
    ["--zone-triangle-narrow", ZONE_TRIANGLE.narrow],
    ["--zone-triangle-gap", ZONE_TRIANGLE_GAP],
    ["--zone-type-tab-wide", ZONE_TYPE_TAB.wide],
    ["--zone-type-tab-narrow", ZONE_TYPE_TAB.narrow],
    ["--zone-type-hint-wide", ZONE_TYPE_HINT.wide],
    ["--zone-type-hint-narrow", ZONE_TYPE_HINT.narrow],
    ["--zone-type-button-tall", ZONE_TYPE_BUTTON.tall],
    ["--zone-type-button-short", ZONE_TYPE_BUTTON.short],
    ["--zone-type-foot-tall", ZONE_TYPE_FOOT.tall],
    ["--zone-type-foot-short", ZONE_TYPE_FOOT.short],
  ] as const)("%s is %dpx", (name, value) => {
    expect(px(name)).toBe(`${value}px`)
  })

  test.each([
    ["--zone-card-ratio", ZONE_CARD_RATIO],
    ["--zone-card-side-scale", ZONE_CARD_SIDE_SCALE],
    ["--zone-type-card-share", ZONE_TYPE_CARD_SHARE],
    ["--zone-type-side-share", ZONE_TYPE_SIDE_SHARE],
  ] as const)("%s is %f", (name, value) => {
    expect(number(name)).toBe(value)
  })

  test("the two thresholds are named in the tokens too", () => {
    expect(px("--zone-wide-width")).toBe(`${ZONE_WIDE_WIDTH}px`)
    expect(px("--zone-tall-height")).toBe(`${ZONE_TALL_HEIGHT}px`)
  })
})
