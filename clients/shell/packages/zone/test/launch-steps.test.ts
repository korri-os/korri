/**
 * Amendment A4: a launch's named steps reach the busy strip as a count, the
 * step running now, and its line, at every size.
 */
import { describe, expect, test } from "bun:test"
import { zoneSteps } from "../src/view"
import { SIZES, Walk } from "./walk"

type Busy = { shown: boolean; steps: number; step: number; stepWords: string }

describe("a launch's named steps", () => {
  test("the scenario's steps reach the busy strip at every size", () => {
    for (const [width, height] of SIZES) {
      const strip = new Walk("status-steps", width, height).view().busy as Busy
      expect(strip.shown).toBe(true)
      expect(strip.steps).toBe(4)
      expect(strip.step).toBe(2)
      expect(strip.stepWords).toBe("Step 3 of 4 · Getting the game ready")
    }
  })

  test("no steps draw no cells; a current step past the end is the last", () => {
    expect(zoneSteps(undefined)).toEqual({ steps: 0, step: 0, stepWords: "" })
    expect(zoneSteps({ labels: [], current: 0 })).toEqual({ steps: 0, step: 0, stepWords: "" })
    expect(zoneSteps({ labels: ["Getting the game ready", "Starting the game"], current: 9 }))
      .toEqual({ steps: 2, step: 1, stepWords: "Step 2 of 2 · Starting the game" })
  })

  test("a busy without steps keeps its strip as it was", () => {
    const strip = new Walk("games-busy", 640, 480).view().busy as Busy
    expect(strip.shown).toBe(true)
    expect(strip.steps).toBe(0)
  })
})
