/**
 * Zone's own emblems are drawings on a 16 by 16 grid (REFINEMENT.md 4,
 * SAVE DATA 2): every row 16 pixels, every filled pixel inside the outline's
 * one-pixel margin, every ink a token, and the outline drawn from the
 * drawing itself.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { cleanup, render } from "@testing-library/react"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { ZONE_EMBLEM_SIDE, ZONE_EMBLEMS, ZoneEmblem, zoneEmblemPaths, type ZoneEmblemName } from "../src/ui/atoms/ZoneEmblem"

afterEach(cleanup)

const names = Object.keys(ZONE_EMBLEMS) as ZoneEmblemName[]
const css = readFileSync(join(import.meta.dir, "..", "src", "ui", "atoms", "ZoneEmblem.css"), "utf8")
const tokens = readFileSync(join(import.meta.dir, "..", "src", "zone-tokens.css"), "utf8")

/** Every pixel a path covers, as "x,y". */
function pixels(d: string): Set<string> {
  const covered = new Set<string>()
  for (const [, x, y, run] of d.matchAll(/M(\d+) (\d+)h(\d+)/g)) {
    for (let at = 0; at < Number(run); at += 1) covered.add(`${Number(x) + at},${Number(y)}`)
  }
  return covered
}

describe("ZoneEmblem", () => {
  test("the four the brief names, and the pad, are drawn", () => {
    expect(names.sort()).toEqual(["key", "link", "pad", "restore", "retired"])
  })

  for (const name of names) {
    test(`${name}: 16 by 16, drawn inside a one-pixel margin for its outline`, () => {
      const grid = ZONE_EMBLEMS[name]
      expect(grid).toHaveLength(ZONE_EMBLEM_SIDE)
      for (const row of grid) expect(row).toHaveLength(ZONE_EMBLEM_SIDE)
      const edge = grid.flatMap((row, y) => [...row].flatMap((ink, x) =>
        ink !== "." && (x === 0 || y === 0 || x === ZONE_EMBLEM_SIDE - 1 || y === ZONE_EMBLEM_SIDE - 1) ? [`${x},${y}`] : []))
      expect(edge).toEqual([])
    })

    test(`${name}: the paths cover the drawing exactly, and every empty pixel beside it is black`, () => {
      const grid = ZONE_EMBLEMS[name]
      const paths = zoneEmblemPaths(grid)
      const byInk = new Map(paths.map(path => [path.ink, pixels(path.d)]))
      const filled = (x: number, y: number) => (grid[y]?.[x] ?? ".") !== "."
      for (let y = 0; y < ZONE_EMBLEM_SIDE; y += 1) {
        for (let x = 0; x < ZONE_EMBLEM_SIDE; x += 1) {
          const own = grid[y]?.[x] ?? "."
          const beside = filled(x - 1, y) || filled(x + 1, y) || filled(x, y - 1) || filled(x, y + 1)
          const expected = own !== "." ? own : beside ? "k" : undefined
          const found = [...byInk].filter(([, set]) => set.has(`${x},${y}`)).map(([ink]) => ink)
          expect(found).toEqual(expected === undefined ? [] : [expected])
        }
      }
    })
  }

  test("every ink a drawing uses has a rule, and every rule reads a token", () => {
    const inks = new Set(names.flatMap(name => zoneEmblemPaths(ZONE_EMBLEMS[name]).map(path => path.ink)))
    for (const ink of inks) {
      expect(css).toContain(`[data-ink="${ink}"]`)
      expect(tokens).toContain(`--zone-emblem-${ink}:`)
    }
  })

  test("an emblem is decoration: hidden from a reader, one path per ink", () => {
    render(<ZoneEmblem name="key" />)
    const svg = document.querySelector("svg.zone-emblem")!
    expect(svg.getAttribute("aria-hidden")).toBe("true")
    expect(svg.getAttribute("viewBox")).toBe("0 0 16 16")
    const inks = [...svg.querySelectorAll("path")].map(path => path.getAttribute("data-ink"))
    expect(new Set(inks).size).toBe(inks.length)
  })
})
