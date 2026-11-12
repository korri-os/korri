/**
 * The plan (src/plan.ts): Velvet's layout as a pure function of the window.
 * One plan drives the views and the focus. Proven here at the six named
 * sizes and over a sweep from 320 x 240 to 2560 x 1440:
 *
 *  - the same window gives the same plan; nothing reads anything else;
 *  - every number is finite and no size is negative;
 *  - text never goes under its floor (a fact at 12 px, a button at 15 px);
 *  - what the plan places stays inside the window: the home's strip and
 *    shelf, a set's columns, the keyboard's five rows of ten, the sheet, the
 *    curtain, the overlay's panel;
 *  - a set shows at least two columns and never more pictures than the
 *    pixel budget holds;
 *  - the named thresholds decide the structure: narrow under 0.9, the panel
 *    beside from 1.25.
 */
import { describe, expect, test } from "bun:test"
import { decodedBytes, logoBox, LOGO_USUAL, PIXEL_BUDGET } from "../src/art"
import { FOCUS_SCALE, KEY_COLUMNS, KEY_ROWS, MOST_COLUMNS, NARROW_RATIO, PANEL_BESIDE_RATIO, plan, SHELF_SLOTS, TILE_RATIO } from "../src/plan"
import { SIZES } from "./korri"

const sweep: [number, number][] = []
for (let width = 320; width <= 2560; width += 80) for (let height = 240; height <= 1440; height += 80) sweep.push([width, height])
const all: [number, number][] = [...SIZES.map(([w, h]) => [w, h] as [number, number]), ...sweep]

describe("the plan", () => {
  test("pure: the same window, the same plan", () => {
    for (const [width, height] of SIZES) {
      const first = JSON.stringify(plan(width, height))
      plan(width + 1, height)
      expect(JSON.stringify(plan(width, height))).toBe(first)
    }
  })

  test("every number is finite and no size is negative", () => {
    for (const [width, height] of all) {
      for (const [key, value] of Object.entries(plan(width, height))) {
        if (typeof value !== "number") continue
        expect(Number.isFinite(value), `${key} at ${width}x${height}`).toBe(true)
        expect(value, `${key} at ${width}x${height}`).toBeGreaterThanOrEqual(0)
      }
    }
  })

  test("text keeps its floor", () => {
    for (const [width, height] of all) {
      const p = plan(width, height)
      const at = `${width}x${height}`
      expect(p.fact, at).toBeGreaterThanOrEqual(12)
      expect(p.detailFact, at).toBeGreaterThanOrEqual(12)
      expect(p.small, at).toBeGreaterThanOrEqual(12)
      expect(p.label, at).toBeGreaterThanOrEqual(13)
      expect(p.body, at).toBeGreaterThanOrEqual(14)
      expect(p.button, at).toBeGreaterThanOrEqual(15)
      expect(p.head, at).toBeGreaterThanOrEqual(22)
    }
  })

  test("the home stays inside the window", () => {
    for (const [width, height] of all) {
      const p = plan(width, height)
      const at = `${width}x${height}`
      expect(p.stripY + p.stripH, at).toBeLessThanOrEqual(height)
      expect(p.rowY + Math.round(p.rowH * FOCUS_SCALE), at).toBeLessThanOrEqual(height)
      expect(p.rowMarkY + p.rowMarkH, at).toBeLessThanOrEqual(p.rowY)
      expect(p.heroX + p.heroW, at).toBeLessThanOrEqual(width)
      expect(p.textX + p.textW, at).toBeLessThanOrEqual(width)
      expect(p.posterX, at).toBeGreaterThanOrEqual(0)
      expect(p.posterX + p.posterH * TILE_RATIO, at).toBeLessThanOrEqual(width)
      expect(p.rowMarkW, at).toBeLessThanOrEqual(width - p.padX)
    }
  })

  test("a set: two columns or more, inside the window, never more pictures than the budget", () => {
    const poster = decodedBytes(600, 900)
    for (const [width, height] of all) {
      const p = plan(width, height)
      const at = `${width}x${height}`
      expect(p.columns, at).toBeGreaterThanOrEqual(2)
      expect(p.columns, at).toBeLessThanOrEqual(MOST_COLUMNS)
      expect(2 * p.padX + p.columns * p.tileW + (p.columns - 1) * p.gridGap, at).toBeLessThanOrEqual(width)
      expect(p.tileW, at).toBeGreaterThanOrEqual(60)
      expect(p.tileH / p.tileW, at).toBeCloseTo(1 / TILE_RATIO, 1)
      expect(p.gridRows, at).toBeGreaterThanOrEqual(1)
      expect(p.poolRows, at).toBeGreaterThan(p.gridRows)
      // The focused row always has its pictures: one row of posters fits several times over.
      expect(p.columns * poster * 2, at).toBeLessThanOrEqual(PIXEL_BUDGET)
    }
    expect(SHELF_SLOTS * poster * 2).toBeLessThanOrEqual(PIXEL_BUDGET)
  })

  test("the keyboard: five rows of ten inside the window", () => {
    for (const [width, height] of all) {
      const p = plan(width, height)
      const at = `${width}x${height}`
      expect(KEY_COLUMNS * p.keyW + (KEY_COLUMNS - 1) * p.keyGap, at).toBeLessThanOrEqual(width - 2 * p.padX)
      expect(p.keysY + KEY_ROWS * p.keyH + (KEY_ROWS - 1) * p.keyGap, at).toBeLessThanOrEqual(height)
      expect(p.keysY, at).toBeGreaterThan(height * 0.2)
      expect(p.keyW, at).toBeGreaterThanOrEqual(24)
      expect(p.keyH, at).toBeGreaterThanOrEqual(22)
    }
  })

  test("sheets, curtains and panels fit", () => {
    for (const [width, height] of all) {
      const p = plan(width, height)
      const at = `${width}x${height}`
      expect(p.sheetW, at).toBeLessThanOrEqual(width)
      expect(p.curtainW, at).toBeLessThanOrEqual(width - 2 * p.padX)
      expect(p.pauseW, at).toBeLessThanOrEqual(width)
      expect(p.pageW, at).toBeLessThanOrEqual(width - 2 * p.padX)
      expect(p.panelW, at).toBeGreaterThan(0)
      expect(p.detailW, at).toBeLessThanOrEqual(width - p.padX)
      expect(p.detailLogoW, at).toBeLessThanOrEqual(width - p.padX)
    }
  })

  test("the named thresholds decide the structure", () => {
    for (const [width, height] of all) {
      const p = plan(width, height)
      expect(p.narrow, `${width}x${height}`).toBe(width / height < NARROW_RATIO)
      expect(p.stack, `${width}x${height}`).toBe(p.narrow)
      if (p.panelBeside) expect(width / height, `${width}x${height}`).toBeGreaterThanOrEqual(PANEL_BESIDE_RATIO)
    }
    expect(plan(480, 800).narrow).toBe(true)
    expect(plan(640, 480).narrow).toBe(false)
    expect(plan(1240, 1080).narrow).toBe(false)
  })

  test("a window one pixel wider never loses a column's worth of room at once", () => {
    for (let width = 320; width < 2560; width += 7) {
      const a = plan(width, 720), b = plan(width + 1, 720)
      expect(Math.abs(a.columns - b.columns), `at ${width}`).toBeLessThanOrEqual(1)
      if (a.columns === b.columns) expect(Math.abs(a.tileW - b.tileW), `at ${width}`).toBeLessThanOrEqual(1)
    }
  })
})

describe("a logo's box", () => {
  // The shapes stored logos come in (the coordinator's list), widest first.
  const shapes: [number, number][] = [[2364, 704], [1554, 542], [830, 355], [785, 338], [770, 304], [1936, 1243]]
  test("no logo leaves its box, at any of the sizes", () => {
    for (const [width, height] of SIZES) {
      const p = plan(width, height)
      for (const [boxW, boxH] of [[p.heroW * 0.92, Math.round(p.markH * 1.25)], [p.detailLogoW * 0.92, p.detailLogoH], [p.rowMarkW * 0.92, p.rowMarkH * 0.6]] as [number, number][]) {
        for (const [w, h] of shapes) {
          const box = logoBox(w, h, boxW, boxH)
          expect(box.width, `${w}x${h} in ${boxW}x${boxH}`).toBeLessThanOrEqual(Math.ceil(boxW))
          expect(box.height, `${w}x${h} in ${boxW}x${boxH}`).toBeLessThanOrEqual(Math.ceil(boxH))
          expect(Math.abs(box.width / box.height - w / h), `${w}x${h} keeps its shape`).toBeLessThan(0.06)
        }
      }
    }
  })
  test("none is tiny: every shape takes at least 55 % of the area the usual one takes", () => {
    for (const [width, height] of SIZES) {
      const p = plan(width, height)
      const boxW = p.heroW * 0.92, boxH = Math.round(p.markH * 1.25)
      const usual = logoBox(LOGO_USUAL * 1000, 1000, boxW, boxH)
      for (const [w, h] of shapes) {
        const box = logoBox(w, h, boxW, boxH)
        expect((box.width * box.height) / (usual.width * usual.height), `${w}x${h} at ${width}x${height}`).toBeGreaterThanOrEqual(0.55)
      }
    }
  })
  test("a picture's cost is its size as the shell hands it over: longer side 1920 at most", () => {
    expect(decodedBytes(1920, 620)).toBe(1920 * 620 * 4)
    expect(decodedBytes(3840, 1240)).toBe(1920 * 620 * 4)
    expect(decodedBytes(600, 900)).toBe(600 * 900 * 4)
    expect(decodedBytes(3611, 1877)).toBe(1920 * 998 * 4)
  })
})
