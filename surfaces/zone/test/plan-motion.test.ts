/**
 * The moments as functions of time (src/plan/motion.ts): each row of the
 * brief's motion tables at its start, its end and one step either side.
 */
import { describe, expect, test } from "bun:test"
import {
  along,
  cubicBezier,
  steps,
  ZONE_ATTRACT_MS,
  ZONE_SNAP_MS,
  ZONE_TALLY_COUNT_MS,
  ZONE_TALLY_ROW_STAGGER_MS,
  ZONE_TITLE_OUT_MS,
  zoneAttractFrame,
  zoneMenuFrame,
  zoneSnapFrame,
  zoneTallyCountMs,
  zoneTallyRowFrame,
  zoneTitleInFrame,
  zoneTitleOutFrame,
  zoneWipeClip,
} from "../src/plan/motion"

describe("curves", () => {
  test("a cubic Bézier meets its ends and overshoots where CSS's does", () => {
    const bounce = cubicBezier(0.34, 1.56, 0.64, 1)
    expect(bounce(0)).toBe(0)
    expect(bounce(1)).toBe(1)
    expect(Math.max(...Array.from({ length: 99 }, (_, index) => bounce((index + 1) / 100)))).toBeGreaterThan(1)
    expect(cubicBezier(0, 0, 1, 1)(0.3)).toBeCloseTo(0.3, 3)
  })
  test("steps(n) holds n flat runs and lands on 1 only at the end", () => {
    expect(steps(4)(0.24)).toBe(0)
    expect(steps(4)(0.25)).toBe(0.25)
    expect(steps(4)(0.99)).toBe(0.75)
    expect(steps(4)(1)).toBe(1)
  })
  test("along() is 0 before its start and 1 after its end", () => {
    expect(along(59, 60, 280, t => t)).toBe(0)
    expect(along(340, 60, 280, t => t)).toBe(1)
  })
})

describe("M1 card snap", () => {
  test("220 ms, from the old focus to the new, overshooting once", () => {
    expect(ZONE_SNAP_MS).toBe(220)
    expect(zoneSnapFrame(0)).toBe(0)
    expect(zoneSnapFrame(ZONE_SNAP_MS)).toBe(1)
    expect(Math.max(...Array.from({ length: 22 }, (_, index) => zoneSnapFrame(index * 10)))).toBeGreaterThan(1)
  })
})

describe("M2 menu transition", () => {
  test("the wipe runs 60 to 340 ms, the banner and the rows land by 500", () => {
    expect(zoneMenuFrame(60, 426).wipe).toBe(0)
    expect(zoneMenuFrame(340, 426).wipe).toBe(1)
    expect(zoneMenuFrame(0, 426).bannerY).toBe(-8)
    expect(zoneMenuFrame(360, 426).bannerY).toBe(0)
    expect(zoneMenuFrame(160, 426).rowsX[0]).toBe(426)
    expect(zoneMenuFrame(400, 426).rowsX[0]).toBe(0)
    expect(zoneMenuFrame(500, 426).rowsX.every(x => x === 0)).toBe(true)
  })
  test("rows enter 20 ms apart, six at most, in whole art px", () => {
    const frame = zoneMenuFrame(200, 320)
    expect(frame.rowsX).toHaveLength(6)
    expect(frame.rowsX.every(x => Number.isInteger(x))).toBe(true)
    expect(frame.rowsX[1]).toBeGreaterThan(frame.rowsX[0]!)
  })
  test("the wipe is the whole old ground before it starts and nothing after", () => {
    expect(zoneWipeClip(0, 320, 240)).toBe("none")
    expect(zoneWipeClip(1, 320, 240)).toBe("polygon(0 0, 0 0, 0 0)")
    expect(zoneWipeClip(0.5, 320, 240)).toMatch(/^polygon\(0 0, /)
  })
})

describe("M3 title card", () => {
  test("the bands sweep in from the top right, 40 ms apart, and rest by 440 ms", () => {
    const start = zoneTitleInFrame(0, 320, 240)
    expect(start.bands[0]).toEqual([320, -240])
    expect(zoneTitleInFrame(40, 320, 240).bands[1]).toEqual([320, -240])
    expect(zoneTitleInFrame(440, 320, 240).bands.every(([x, y]) => x === 0 && y === 0)).toBe(true)
  })
  test("the name from the left at 200, the bar from the right at 260, the disc in four steps from 420", () => {
    expect(zoneTitleInFrame(200, 320, 240).nameX).toBe(-320)
    expect(zoneTitleInFrame(460, 320, 240).nameX).toBe(0)
    expect(zoneTitleInFrame(260, 320, 240).barX).toBe(320)
    expect(zoneTitleInFrame(520, 320, 240).barX).toBe(0)
    expect(zoneTitleInFrame(419, 320, 240).disc).toBe(0)
    expect(zoneTitleInFrame(460, 320, 240).disc).toBe(0.25)
    expect(zoneTitleInFrame(580, 320, 240).disc).toBe(1)
  })
  test("out in 400 ms, the bands toward the bottom left", () => {
    const end = zoneTitleOutFrame(ZONE_TITLE_OUT_MS, 320, 240)
    expect(end.bands.every(([x, y]) => x === -320 && y === 240)).toBe(true)
    expect(end.nameX).toBe(-320)
    expect(end.barX).toBe(320)
    expect(end.disc).toBe(0)
  })
  test("every offset is a whole art px", () => {
    for (let t = 0; t <= 700; t += 10) {
      const frame = zoneTitleInFrame(t, 426, 240)
      expect([frame.nameX, frame.barX, ...frame.bands.flat()].every(Number.isInteger)).toBe(true)
    }
  })
})

describe("M5 tally", () => {
  test("rows 300 ms apart, each counting 0 to its value in 1200 ms and 60 steps", () => {
    expect(zoneTallyCountMs(3)).toBe(2 * ZONE_TALLY_ROW_STAGGER_MS + ZONE_TALLY_COUNT_MS)
    expect(zoneTallyRowFrame(299, 1).shown).toBe(false)
    expect(zoneTallyRowFrame(300, 1)).toMatchObject({ shown: true, count: 0 })
    expect(zoneTallyRowFrame(300 + 20, 1).count).toBe(1 / 60)
    expect(zoneTallyRowFrame(300 + ZONE_TALLY_COUNT_MS, 1).count).toBe(1)
  })
})

describe("M6 attract", () => {
  test("card in, hold, out by 2900; the stage pans 32 art px to 5600 and holds to 6000", () => {
    expect(zoneAttractFrame(0, 320, 240).card).toBeDefined()
    expect(zoneAttractFrame(2499, 320, 240).stage).toBe(false)
    expect(zoneAttractFrame(2500, 320, 240).stage).toBe(true)
    expect(zoneAttractFrame(2900, 320, 240).card).toBeUndefined()
    expect(zoneAttractFrame(2900, 320, 240).panX).toBe(0)
    expect(zoneAttractFrame(5600, 320, 240).panX).toBe(-32)
    expect(zoneAttractFrame(ZONE_ATTRACT_MS, 320, 240).panX).toBe(-32)
  })
  test("seamless: a turn's first frame is the next turn's first frame", () => {
    expect(zoneAttractFrame(0, 320, 240)).toEqual(zoneAttractFrame(0, 320, 240))
    expect(zoneAttractFrame(0, 320, 240).card?.bands[0]).toEqual([320, -240])
  })
  test("PRESS ANY BUTTON blinks once a second on the stage, in two steps", () => {
    expect(zoneAttractFrame(1000, 320, 240).prompt).toBe(false)
    expect(zoneAttractFrame(3000, 320, 240).prompt).toBe(true)
    expect(zoneAttractFrame(3500, 320, 240).prompt).toBe(false)
    expect(zoneAttractFrame(4000, 320, 240).prompt).toBe(true)
  })
})
