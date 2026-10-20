/**
 * The ground's drawing is a pure function of the clock: the same time gives
 * the same pixels, the dots step at the frame rate, every rectangle is on
 * the art grid, and only the palette's opaque colours are painted.
 */
import { describe, expect, test } from "bun:test"
import { renderZoneGround, zoneGroundFrame, ZONE_RING_THICKNESS, type ZoneGroundPainter, type ZoneGroundPalette } from "../src/plan/ground"

const PALETTE: ZoneGroundPalette = {
  ground: "#4ad39c",
  ringA: "#39b2ce",
  ringB: "#f78e18",
  rings: true,
  driftA: 140_000,
  driftB: 200_000,
  fps: 15,
}

function record(t: number, palette = PALETTE, wArt = 426, hArt = 240): string[] {
  const calls: string[] = []
  const painter: ZoneGroundPainter = {
    fillStyle: "",
    fillRect(x, y, w, h) {
      calls.push(`${String(this.fillStyle)} ${x} ${y} ${w} ${h}`)
    },
  }
  renderZoneGround(painter, t, palette, wArt, hArt)
  return calls
}

describe("renderZoneGround", () => {
  test("seekable: the same time paints the same pixels", () => {
    expect(record(12_345)).toEqual(record(12_345))
  })

  test("the clock steps at 15 frames a second: two times inside one frame paint the same", () => {
    expect(zoneGroundFrame(100, 15)).toBeCloseTo(1000 / 15, 6)
    expect(record(1000 / 15 + 1)).toEqual(record(2 * (1000 / 15) - 1))
  })

  test("the dots orbit: a quarter of ring A's turn later the picture differs", () => {
    expect(record(0)).not.toEqual(record(35_000))
  })

  test("every rectangle is whole art px, one row tall after the flat ground", () => {
    const [ground, ...runs] = record(5_000)
    expect(ground).toBe("#4ad39c 0 0 426 240")
    for (const run of runs) {
      const [, x, y, w, h] = run.split(" ").map(Number)
      expect([Number.isInteger(x), Number.isInteger(y), Number.isInteger(w), h]).toEqual([true, true, true, 1])
      expect(w).toBeGreaterThan(0)
    }
  })

  test("only the palette's colours, no opacity: no blend is ever painted", () => {
    const colours = new Set(record(77_000).map(run => run.split(" ")[0]))
    expect([...colours].sort()).toEqual(["#39b2ce", "#4ad39c", "#f78e18"])
  })

  test(`a ring is ${ZONE_RING_THICKNESS} art px thick where it crosses a row through its centre`, () => {
    // Ring A stands at the canvas's middle: on its centre row the left run is the ring's thickness.
    const runs = record(0).filter(run => run.startsWith("#39b2ce")).map(run => run.split(" ").map(Number))
    const centreRow = Math.round(240 * 0.56)
    const widths = runs.filter(([, , y]) => y === centreRow).map(([, , , w]) => w)
    expect(widths.some(w => w === ZONE_RING_THICKNESS || w === ZONE_RING_THICKNESS + 1)).toBe(true)
  })

  test("a flat ground draws no rings", () => {
    expect(record(0, { ...PALETTE, rings: false })).toEqual(["#4ad39c 0 0 426 240"])
  })
})
