/**
 * The plan (src/plan.ts): Velvet's layout as a pure function of the window.
 * One plan drives the views and the focus. Proven here at the named sizes
 * (test/korri.ts), at a ladder from a 3840 x 2160 television to a 480 x 320
 * pane, and over a sweep from 320 x 240 to 3840 x 2160:
 *
 *  - the same window gives the same plan; nothing reads anything else;
 *  - every number is finite and no size is negative;
 *  - text never goes under its floor (a fact at 15 px, a button at 17 px);
 *  - what the plan places stays inside the window: the home's strip and
 *    shelf, a set's columns, the keyboard's five rows of ten, the sheet, the
 *    curtain, the overlay's panel;
 *  - a set shows at least two columns, and its rows fill the width from pad
 *    to pad: no empty band at either side;
 *  - nothing has a ceiling in pixels: type, tiles, pads and gaps grow with
 *    the window on each axis, and a window twice the size is the same
 *    composition twice the size;
 *  - the named thresholds decide the structure: narrow under 0.9, the panel
 *    beside from 1.25.
 */
import { describe, expect, test } from "bun:test"
import { decodedBytes, logoBox, LOGO_USUAL } from "../src/art"
import { FOCUS_SCALE, KEY_COLUMNS, KEY_ROWS, NARROW_RATIO, PANEL_BESIDE_RATIO, plan, ROW_FACT_LINES, SET_FIT_GROW, SET_LEAST, TILE_RATIO } from "../src/plan"
import { SIZES } from "./korri"

/**
 * The ladder: IBUKI's television (3840 x 2160), 1920 x 1080, the Thor's Floor (1240 x
 * 1080), 1280 x 720, two narrow-tall columns, a wide-short strip, and two small panes.
 */
const LADDER: [number, number][] = [[3840, 2160], [1920, 1080], [1240, 1080], [1280, 720], [480, 800], [720, 1600], [1920, 400], [640, 480], [480, 320]]
const sweep: [number, number][] = []
for (let width = 320; width <= 3840; width += 160) for (let height = 240; height <= 2160; height += 120) sweep.push([width, height])
const all: [number, number][] = [...SIZES.map(([w, h]) => [w, h] as [number, number]), ...LADDER, ...sweep]

describe("the plan", () => {
  test("pure: the same window, the same plan", () => {
    for (const [width, height] of SIZES) {
      const first = JSON.stringify(plan(width, height))
      plan(width + 1, height)
      expect(JSON.stringify(plan(width, height))).toBe(first)
    }
  })

  test("the Floor's own window is an input of its own: the memory never hands one for the other", () => {
    for (const [width, height] of SIZES) {
      const whole = JSON.stringify(plan(width, height))
      const floor = JSON.stringify(plan(width, height, true))
      expect(floor).not.toBe(whole)
      // In turn, as the Stage and the Floor ask: each gets its own every time.
      for (let i = 0; i < 3; i++) {
        expect(JSON.stringify(plan(width, height, true))).toBe(floor)
        expect(JSON.stringify(plan(width, height))).toBe(whole)
        plan(width + 1, height, true)
      }
    }
  })

  test("the Floor's own window: no band for a marquee; the row under the pad, the library under it", () => {
    for (const [width, height] of all) {
      const p = plan(width, height, true)
      const one = plan(width, height)
      const at = `${width}x${height}`
      expect(p.rowMarkH, at).toBe(0)
      expect(p.rowY, at).toBeLessThanOrEqual(one.rowY)
      expect(p.rowY - Math.ceil(p.rowH * (FOCUS_SCALE - 1)), at).toBeGreaterThanOrEqual(p.padY)
      expect(p.rowY + Math.round(p.rowH * FOCUS_SCALE), at).toBeLessThanOrEqual(height)
      if (one.peekY > 0) expect(p.peekY, at).toBeGreaterThan(0)
      // Everything else is the window's plan.
      expect(p.rowNameH, at).toBe(0)
      expect(p.rowNameGap, at).toBe(0)
      const rest = { rowY: 0, rowH: 0, rowGap: 0, rowMarkH: 0, rowNameGap: 0, rowNameH: 0, peekY: 0 }
      expect({ ...p, ...rest }, at).toEqual({ ...one, ...rest })
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
      expect(p.fact, at).toBeGreaterThanOrEqual(15)
      expect(p.detailFact, at).toBeGreaterThanOrEqual(15)
      expect(p.small, at).toBeGreaterThanOrEqual(14)
      expect(p.label, at).toBeGreaterThanOrEqual(15)
      expect(p.body, at).toBeGreaterThanOrEqual(16)
      expect(p.button, at).toBeGreaterThanOrEqual(17)
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

  // The name in one place (owner, 2026-11-20): over the row only its facts, under the focused tile its name.
  test("the row: a band for its facts over it, its name line under it, both clear of the focus growth and the library's peek", () => {
    for (const [width, height] of all) {
      const p = plan(width, height)
      const at = `${width}x${height}`
      const facts = Math.round(p.fact * 1.7 * ROW_FACT_LINES)
      // The band holds the facts and no title: the row stands just under it.
      expect(p.rowMarkH, at).toBeGreaterThanOrEqual(facts)
      expect(p.rowMarkH, at).toBeLessThanOrEqual(facts + 1)
      // The focused tile grows up from its foot and stays clear of the facts.
      expect(p.rowMarkY + p.rowMarkH, at).toBeLessThanOrEqual(p.rowY - Math.ceil(p.rowH * (FOCUS_SCALE - 1)))
      // The name line: under the row's foot, one line of the label size, inside the window.
      const nameTop = p.rowY + p.rowH + p.rowNameGap
      expect(p.rowNameGap, at).toBeGreaterThan(0)
      expect(p.rowNameH, at).toBeGreaterThanOrEqual(Math.round(p.label * 1.5))
      expect(nameTop + p.rowNameH, at).toBeLessThanOrEqual(height)
      // The library's peek stands under the name line with a gap, or not at all.
      if (p.peekY > 0) {
        expect(p.peekY - (nameTop + p.rowNameH), at).toBeGreaterThanOrEqual(p.rowGap)
        expect(p.peekY, at).toBeLessThan(height - 24)
      }
    }
  })

  test("a set: two columns or more, inside the window, rows that a finger and an eye can read", () => {
    for (const [width, height] of all) {
      const p = plan(width, height)
      const at = `${width}x${height}`
      expect(p.columns, at).toBeGreaterThanOrEqual(2)
      expect(2 * p.padX + p.columns * p.tileW + (p.columns - 1) * p.gridGap, at).toBeLessThanOrEqual(width)
      expect(p.tileW, at).toBeGreaterThanOrEqual(SET_LEAST)
      expect(p.tileH / p.tileW, at).toBeCloseTo(1 / TILE_RATIO, 1)
      expect(p.gridRows, at).toBeGreaterThanOrEqual(1)
      expect(p.poolRows, at).toBeGreaterThan(p.gridRows)
    }
  })

  // The photo from IBUKI (3840 x 2160): five columns of 300 px in the left third, the rest black.
  test("a set's rows fill the width: the right margin is the left one, at every size, on either window", () => {
    for (const [width, height] of all) {
      for (const floor of [false, true]) {
        const p = plan(width, height, floor)
        const at = `${width}x${height}${floor ? " floor" : ""}`
        const used = p.columns * p.tileW + (p.columns - 1) * p.gridGap
        const right = width - p.padX - used
        // What is left over is the rounding of each column's width: under one px a column.
        expect(right - p.padX, at).toBeGreaterThanOrEqual(0)
        expect(right - p.padX, at).toBeLessThan(p.columns)
      }
    }
    expect(plan(3840, 2160).columns).toBeGreaterThan(5)
  })

  test("a set's tiles are the lobby's, given or taken a little to fill the row (owner, 2026-11-13)", () => {
    for (const [width, height] of all) {
      for (const floor of [false, true]) {
        const p = plan(width, height, floor)
        const at = `${width}x${height}${floor ? " floor" : ""}`
        const lobby = Math.max(p.shelfH, 90) * TILE_RATIO
        // Never more than SET_FIT_GROW larger than the lobby's, unless one more column would be under SET_LEAST.
        const more = (width - 2 * p.padX - p.columns * p.gridGap) / (p.columns + 1)
        if (more >= SET_LEAST) expect(p.tileW, at).toBeLessThanOrEqual(Math.ceil(lobby * (1 + SET_FIT_GROW)))
        // As many as stand at the lobby's size, or one more where those would grow past SET_FIT_GROW.
        const tileOf = (count: number) => (width - 2 * p.padX - (count - 1) * p.gridGap) / count
        if (p.columns > 2 && more >= SET_LEAST) {
          const asMany = tileOf(p.columns) >= lobby - 1 && tileOf(p.columns + 1) < lobby
          const oneMore = tileOf(p.columns - 1) > lobby * (1 + SET_FIT_GROW) - 1
          expect(asMany || oneMore, at).toBe(true)
        }
      }
    }
    // The Thor's Floor keeps its five across.
    expect(plan(1240, 1080, true).columns).toBe(5)
  })

  test("no ceiling: a window twice the size is the same composition twice the size", () => {
    // From the reference stage up (`k` and `kt` 1 and over), where type and space scale alike.
    for (const [width, height] of [[1920, 1080], [1600, 900], [2200, 1000], [1600, 1800], [1700, 900]] as [number, number][]) {
      const one = plan(width, height), two = plan(width * 2, height * 2)
      const at = `${width}x${height}`
      expect(two.columns, at).toBe(one.columns)
      expect(two.gridRows, at).toBe(one.gridRows)
      for (const key of ["padX", "padY", "title", "head", "body", "fact", "button", "shelfH", "tileW", "tileH", "rowH", "stripH", "posterH", "rowHeight", "pageW", "keyW", "keyH", "sheetW", "curtainW", "pauseW", "detailLogoH", "detailTitle"] as const) {
        // Where the smaller window sits on a floor, the larger one is at least the floor's twice.
        if (one[key] * 2 < 60) continue
        expect(Math.abs(two[key] - one[key] * 2), `${key} at ${at}: ${one[key]} then ${two[key]}`).toBeLessThanOrEqual(Math.max(3, one[key] * 0.04))
      }
    }
  })

  test("type and tiles grow with the window, on each axis, and never shrink when it grows", () => {
    const keys = ["title", "head", "body", "fact", "label", "button", "small", "kicker", "detailFact", "shelfH", "rowHeight"] as const
    for (const [width, height] of sweep) {
      const p = plan(width, height)
      for (const [w, h] of [[width + 160, height], [width, height + 120]] as [number, number][]) {
        const q = plan(w, h)
        // A tile's size may step down where the structure changes (narrow, under NARROW_RATIO); type never does.
        for (const key of keys) if (p.narrow === q.narrow || !(key === "shelfH" || key === "rowHeight")) expect(q[key], `${key}: ${width}x${height} to ${w}x${h}`).toBeGreaterThanOrEqual(p[key])
      }
    }
    // The ladder, largest to smallest at one shape: every size of type larger at the larger window.
    const tv = plan(3840, 2160), hd = plan(1920, 1080), small = plan(1280, 720)
    for (const key of ["title", "head", "body", "fact", "button", "tileW"] as const) {
      expect(tv[key], key).toBeGreaterThan(hd[key])
      expect(hd[key], key).toBeGreaterThan(small[key])
    }
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
      for (const [boxW, boxH] of [[p.heroW * 0.92, Math.round(p.markH * 1.25)], [p.detailLogoW * 0.92, p.detailLogoH]] as [number, number][]) {
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
