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
import { view } from "../logic"

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

describe("the per-press cost (RG353M benchmark)", () => {
  test("a Tick that changes nothing gives the same state back, and the same view", () => {
    const walk = new Walk("games", 640, 480)
    const before = walk.state
    const view = walk.view()
    walk.deliver({ _tag: "Tick", idleMs: 1_000 })
    expect(walk.state).toBe(before)
    expect(walk.view()).toBe(view)
  })

  test("a huge idleMs is one step, not one per turn", () => {
    const walk = new Walk("games-empty", 640, 480)
    const started = performance.now()
    walk.deliver({ _tag: "Tick", idleMs: 4_294_967_295 })
    walk.deliver({ _tag: "Tick", idleMs: 4_294_967_295 })
    expect(performance.now() - started).toBeLessThan(5)
  })

  test("moving along the band changes one card in and one out: the others are the same objects", () => {
    const walk = new Walk("games", 640, 480)
    const before = walk.view().cards as unknown[]
    const marks = JSON.stringify(walk.view().cardMarks)
    walk.press("right")
    const after = walk.view().cards as unknown[]
    const changed = after.filter((card, index) => card !== before[index]).length
    expect(changed).toBeLessThanOrEqual(2)
    expect(JSON.stringify(walk.view().cardMarks)).not.toBe(marks)
  })

  test("the view sends whole cards only for the band's pool and the attract's game", () => {
    const walk = new Walk("games", 640, 480)
    const shown = walk.view()
    const cards = shown.cards as Record<string, unknown>[]
    const at = shown.focusedCard as number
    const whole = cards.flatMap((card, index) => ("title" in card ? [index] : []))
    // 640x480 stands three cards: the focus and one each side, and the next one each way.
    expect(whole).toEqual([at - 2, at - 1, at, at + 1, at + 2].filter(index => index >= 0 && index < cards.length))
  })

  test("the attract's next game gives the view a full rebuild would", () => {
    const walk = new Walk("games", 640, 480)
    walk.deliver({ _tag: "Tick", idleMs: 30_000 })
    for (let step = 1; step < 14; step++) {
      walk.deliver({ _tag: "Tick", idleMs: 30_000 + step * 6_000 })
      const stepped = JSON.stringify(walk.view())
      const fresh = JSON.stringify(view({ ...walk.state, size: { ...walk.state.size } }, walk.model))
      expect(stepped).toBe(fresh)
    }
  })
})
