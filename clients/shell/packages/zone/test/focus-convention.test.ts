/**
 * Contract 8.4's convention, on every scenario at the five sizes, through
 * logic.ts as the shell plays it:
 *  - every view entry with `focused` carries `focusRegion`;
 *  - at most one `focusActive` a region, and at most one focused entry;
 *  - `focusPlan` names exits only to regions with targets in that view;
 *  - the plan's structure (`screen`) follows the size, and the shell's
 *    Resized changes it.
 * Where a press goes is judged against what is drawn by
 * tests/zone_package.rs (and the shell's probe).
 */
import { describe, expect, test } from "bun:test"
import { SIZES, scenarioNames, Walk } from "./walk"

type Json = Record<string, unknown>

function entries(value: unknown, out: Json[] = []): Json[] {
  if (Array.isArray(value)) value.forEach(item => entries(item, out))
  else if (value !== null && typeof value === "object") {
    const entry = value as Json
    if ("focused" in entry) out.push(entry)
    Object.values(entry).forEach(inner => entries(inner, out))
  }
  return out
}

describe("focus regions and exits", () => {
  for (const name of scenarioNames()) {
    test(name, () => {
      for (const [width, height] of SIZES) {
        const walk = new Walk(name, width, height)
        for (const button of ["down", "right", "up", "left", "down", "down"] as const) {
          const view = walk.view()
          const all = entries(view)
          for (const entry of all) expect(typeof entry.focusRegion, `${name} ${width}x${height} ${JSON.stringify(entry).slice(0, 80)}`).toBe("string")
          expect(all.filter(entry => entry.focused === true).length, `${name} ${width}x${height}: focused entries`).toBeLessThanOrEqual(1)
          const regions = new Set(all.map(entry => entry.focusRegion as string))
          const active = all.filter(entry => entry.focusActive === true).map(entry => entry.focusRegion)
          expect(new Set(active).size, `${name}: one focusActive a region`).toBe(active.length)
          const plan = view.focusPlan as Record<string, Record<string, { to: string; entry: string }>>
          expect(plan).toBeDefined()
          for (const [region, exits] of Object.entries(plan)) {
            expect(regions.has(region), `${name} ${width}x${height}: ${region} has targets`).toBe(true)
            for (const exit of Object.values(exits)) {
              expect(regions.has(exit.to), `${name} ${width}x${height}: ${region} exits to ${exit.to}`).toBe(true)
              expect(["remembered", "active", "first"]).toContain(exit.entry)
            }
          }
          walk.press(button)
        }
      }
    })
  }
})

describe("Resized", () => {
  test("the plan follows the window", () => {
    const small = new Walk("games", 320, 240).view().screen as Json
    const wide = new Walk("games", 1920, 1080).view().screen as Json
    const tall = new Walk("games", 480, 800).view().screen as Json
    expect(small).toMatchObject({ a: 1, wide: false, tall: false, stripLevel: 2 })
    expect(wide).toMatchObject({ a: 4, wide: true, tall: false, stripLevel: 0 })
    expect(tall).toMatchObject({ a: 2, wide: false, tall: true, stripLevel: 3, tabsWrap: true })
  })

  test("a row of buttons wraps where the plan says, and the focus graph walks it as drawn", () => {
    const walk = new Walk("games-two-places", 480, 800)
    const actions = walk.view().actions as { label: string; line: number; focused: boolean }[]
    expect(actions.map(button => button.line)).toEqual([0, 1, 2])
    // Stacked: down walks the column; right has nothing that way.
    walk.press("right")
    expect((walk.view().actions as { focused: boolean }[]).findIndex(button => button.focused)).toBe(0)
    walk.press("down")
    expect((walk.view().actions as { focused: boolean }[]).findIndex(button => button.focused)).toBe(1)
  })
})
