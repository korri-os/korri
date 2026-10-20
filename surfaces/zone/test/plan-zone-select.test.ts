/**
 * The layout policy at its boundaries: one art pixel either side of every
 * threshold, the yield when the visible set does not fit, and the promise
 * that the plan's constants are the tokens Caliper turns. Every threshold
 * and length is in art px (src/plan/art-px.ts); `planArt` takes the art
 * grid directly and `plan` divides a CSS size by its art pixel first.
 */
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { ZONE_ART_ROWS, zoneArt, zoneArtPx } from "../src/plan/art-px"
import {
  plan,
  planArt,
  ZONE_ACTION_STRIP,
  ZONE_BAND_MARGIN,
  ZONE_BAND_PAD,
  ZONE_CARD_FOCUS_MAX,
  ZONE_CARD_GAP,
  ZONE_CARD_RATIO,
  ZONE_CARD_SIDE_SCALE,
  ZONE_SHORT_HEIGHT,
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
    const narrow = planArt(ZONE_WIDE_WIDTH - 1, 240)
    expect(narrow.band).toBe("narrow")
    expect(narrow.visible).toBe(3)
    expect(narrow.tri).toBe(ZONE_TRIANGLE.narrow)
    expect(narrow.gap).toBe(ZONE_CARD_GAP.narrow)
    expect(narrow.tabFs).toBe(ZONE_TYPE_TAB.narrow)
    expect(narrow.hintFs).toBe(ZONE_TYPE_HINT.narrow)
  })

  test(`at ${ZONE_WIDE_WIDTH} shows five, wide triangles and gaps`, () => {
    const wide = planArt(ZONE_WIDE_WIDTH, 240)
    expect(wide.band).toBe("wide")
    expect(wide.visible).toBe(5)
    expect(wide.tri).toBe(ZONE_TRIANGLE.wide)
    expect(wide.gap).toBe(ZONE_CARD_GAP.wide)
    expect(wide.tabFs).toBe(ZONE_TYPE_TAB.wide)
    expect(wide.hintFs).toBe(ZONE_TYPE_HINT.wide)
  })

  test("width never changes the strips", () => {
    expect(planArt(ZONE_WIDE_WIDTH - 1, 240).tabH).toBe(planArt(ZONE_WIDE_WIDTH, 240).tabH)
    expect(planArt(ZONE_WIDE_WIDTH - 1, 240).footFs).toBe(planArt(ZONE_WIDE_WIDTH, 240).footFs)
  })
})

describe("height decides the strips", () => {
  test(`one pixel below ${ZONE_TALL_HEIGHT} takes the short strips and type`, () => {
    const short = planArt(426, ZONE_TALL_HEIGHT - 1)
    expect(short.strips).toBe("short")
    expect(short.tabH).toBe(ZONE_TAB_STRIP.short)
    expect(short.actH).toBe(ZONE_ACTION_STRIP.short)
    expect(short.footH).toBe(ZONE_FOOT_STRIP.short)
    expect(short.btnFs).toBe(ZONE_TYPE_BUTTON.short)
    expect(short.footFs).toBe(ZONE_TYPE_FOOT.short)
  })

  test(`at ${ZONE_TALL_HEIGHT} takes the tall strips and type`, () => {
    const tall = planArt(426, ZONE_TALL_HEIGHT)
    expect(tall.strips).toBe("tall")
    expect(tall.tabH).toBe(ZONE_TAB_STRIP.tall)
    expect(tall.actH).toBe(ZONE_ACTION_STRIP.tall)
    expect(tall.footH).toBe(ZONE_FOOT_STRIP.tall)
    expect(tall.btnFs).toBe(ZONE_TYPE_BUTTON.tall)
    expect(tall.footFs).toBe(ZONE_TYPE_FOOT.tall)
  })

  test("height never changes how many cards show", () => {
    expect(planArt(426, ZONE_TALL_HEIGHT - 1).visible).toBe(planArt(426, ZONE_TALL_HEIGHT).visible)
  })

  test("the band is what the strips leave", () => {
    const short = plan(1280, 720)
    expect(short.bandH).toBe(240 - ZONE_TAB_STRIP.short - ZONE_ACTION_STRIP.short - ZONE_FOOT_STRIP.short)
  })
})

describe("the focused card", () => {
  test(`never grows past ${ZONE_CARD_FOCUS_MAX}`, () => {
    const tall = planArt(1000, 400)
    expect(tall.focusH).toBe(ZONE_CARD_FOCUS_MAX)
    expect(tall.scale).toBe(1)
  })

  test("one art pixel below the cap takes the band less its padding", () => {
    const bandH = ZONE_CARD_FOCUS_MAX + ZONE_BAND_PAD * 2 - 1
    const height = bandH + ZONE_TAB_STRIP.short + ZONE_ACTION_STRIP.short + ZONE_FOOT_STRIP.short
    expect(planArt(1000, height).focusH).toBe(ZONE_CARD_FOCUS_MAX - 1)
  })

  test("keeps the card ratio and the side scale", () => {
    const tall = planArt(1000, 400)
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
    return p.focusW + (p.visible - 1) * p.sideW + fixed <= p.wArt + p.visible
  }

  test("at the two devices the plan is Mania's canvas: five cards shrunk to fit 426 art px, three shrunk a hair to fit 320", () => {
    const wide = plan(1280, 720)
    expect([wide.px, wide.wArt, wide.hArt]).toEqual([3, 426, 240])
    expect([wide.focusH, wide.focusW, wide.sideH, wide.sideW]).toEqual([110, 86, 75, 58])
    expect(wide.scale).toBeCloseTo(0.8281, 3)
    const narrow = plan(640, 480)
    expect([narrow.px, narrow.wArt, narrow.hArt]).toEqual([2, 320, 240])
    expect([narrow.focusH, narrow.focusW, narrow.sideH, narrow.sideW]).toEqual([132, 103, 89, 69])
    expect(narrow.scale).toBeCloseTo(0.9918, 3)
    expect(fits(1280, 720)).toBe(true)
    expect(fits(640, 480)).toBe(true)
  })

  test("a generous screen fits without shrinking", () => {
    expect(planArt(600, 400).scale).toBe(1)
    expect(fits(2400, 1600)).toBe(true)
  })

  test("a narrow tall column shrinks every card by one factor and keeps three", () => {
    const column = plan(480, 800)
    expect([column.px, column.wArt, column.strips]).toEqual([2, 240, "tall"])
    expect(column.visible).toBe(3)
    expect(column.scale).toBeLessThan(1)
    expect(column.scale).toBeGreaterThan(0)
    expect(fits(480, 800)).toBe(true)
    const unscaled = planArt(1000, 400)
    expect(column.focusH / column.sideH).toBeCloseTo(unscaled.focusH / unscaled.sideH, 1)
  })

  test("a wide short strip draws at one CSS px per art px, keeps five cards and the short strips", () => {
    const strip = plan(1280, 300)
    expect(strip.px).toBe(1)
    expect(strip.visible).toBe(5)
    expect(strip.strips).toBe("short")
    expect(strip.focusH).toBeGreaterThan(0)
    expect(fits(1280, 300)).toBe(true)
  })

  test("a box too small for any band still answers, with nothing negative", () => {
    const tiny = planArt(60, 60)
    expect(tiny.bandH).toBe(0)
    expect(tiny.focusH).toBe(0)
    expect(tiny.focusW).toBe(0)
    expect(tiny.visible).toBe(3)
  })
})

describe("height and width decide where the GAMES facts line stands", () => {
  test(`one art pixel below ${ZONE_SHORT_HEIGHT} on a wide screen, it joins the action strip`, () => {
    expect(planArt(ZONE_WIDE_WIDTH, ZONE_SHORT_HEIGHT - 1).facts).toBe("inline")
  })
  test(`at ${ZONE_SHORT_HEIGHT} it keeps its own row`, () => {
    expect(planArt(ZONE_WIDE_WIDTH, ZONE_SHORT_HEIGHT).facts).toBe("row")
  })
  test("a narrow screen keeps the row however short, since its width cannot pay for both", () => {
    expect(planArt(ZONE_WIDE_WIDTH - 1, ZONE_SHORT_HEIGHT - 1).facts).toBe("row")
  })
  test(`one art pixel below ${ZONE_SHORT_HEIGHT} the find keyboard rises over the screen, at any width`, () => {
    expect(planArt(ZONE_WIDE_WIDTH, ZONE_SHORT_HEIGHT - 1).keys).toBe("screen")
    expect(planArt(ZONE_WIDE_WIDTH - 1, ZONE_SHORT_HEIGHT - 1).keys).toBe("screen")
  })
  test(`at ${ZONE_SHORT_HEIGHT} it rises inside the body`, () => {
    expect(planArt(ZONE_WIDE_WIDTH, ZONE_SHORT_HEIGHT).keys).toBe("body")
  })
  test("a CSS strip shorter than 240 px holds fewer than 240 art rows and takes the inline facts", () => {
    expect(plan(1280, 200).facts).toBe("inline")
  })
})

describe("the art pixel", () => {
  test.each([
    [640, 480, 2, 320, 240],
    [1280, 720, 3, 426, 240],
    [1920, 1080, 4, 480, 270],
    [480, 800, 2, 240, 400],
    [1280, 300, 1, 1280, 300],
  ] as const)("%dx%d stands on %d CSS px per art px, a %dx%d canvas", (width, height, px, wArt, hArt) => {
    expect(zoneArt(width, height)).toEqual({ px, wArt, hArt })
  })

  test(`one CSS px either side of each step of ${ZONE_ART_ROWS} rows`, () => {
    expect(zoneArtPx(2000, ZONE_ART_ROWS * 2 - 1)).toBe(1)
    expect(zoneArtPx(2000, ZONE_ART_ROWS * 2)).toBe(2)
    expect(zoneArtPx(ZONE_ART_ROWS * 3 - 1, 2000)).toBe(2)
    expect(zoneArtPx(ZONE_ART_ROWS * 3, 2000)).toBe(3)
  })

  test("never fractional and never under one, even for an empty box", () => {
    expect(zoneArtPx(0, 0)).toBe(1)
    expect(zoneArtPx(100, 100)).toBe(1)
    expect(Number.isInteger(zoneArtPx(1366, 768))).toBe(true)
  })
})

describe("the plan's constants are the tokens", () => {
  const tokens = new Map(
    [...readFileSync(join(import.meta.dir, "..", "src", "zone-tokens.css"), "utf8")
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)]
      .map(([, name, value]) => [name ?? "", (value ?? "").trim()]),
  )
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
  ] as const)("%s is %d art px", (name, value) => {
    expect(tokens.get(name)).toBe(`${value}`)
  })

  test.each([
    ["--zone-card-ratio", ZONE_CARD_RATIO],
    ["--zone-card-side-scale", ZONE_CARD_SIDE_SCALE],
    ["--zone-type-card-share", ZONE_TYPE_CARD_SHARE],
    ["--zone-type-side-share", ZONE_TYPE_SIDE_SHARE],
  ] as const)("%s is %f", (name, value) => {
    expect(number(name)).toBe(value)
  })

  test("the thresholds are named in the tokens too, in art px", () => {
    expect(tokens.get("--zone-wide-width")).toBe(`${ZONE_WIDE_WIDTH}`)
    expect(tokens.get("--zone-tall-height")).toBe(`${ZONE_TALL_HEIGHT}`)
    expect(tokens.get("--zone-short-height")).toBe(`${ZONE_SHORT_HEIGHT}`)
  })

  test("the art rows and the least art pixel are the tokens zone.css derives a part's pixel from", () => {
    expect(number("--zone-art-rows")).toBe(ZONE_ART_ROWS)
    expect(tokens.get("--zone-px-least")).toBe("1px")
    const screen = readFileSync(join(import.meta.dir, "..", "src", "zone.css"), "utf8")
    expect(screen).toContain("round(down, calc(min(100cqw, 100cqh) / var(--zone-art-rows)), var(--zone-px-least))")
  })
})
