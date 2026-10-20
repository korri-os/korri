/**
 * zoneStairClip and zoneStairWedge: the points of a 45 degree edge, one
 * stair per art px, every length a whole number of --zone-px.
 */
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { zoneStairCircle, zoneStairClip, zoneStairWedge } from "../src/plan/stair-clip"

const SRC = join(import.meta.dir, "..", "src")
const flat = (text: string) => text.replace(/\s+/g, " ").replace(/\( /g, "(").replace(/ \)/g, ")")

/** The polygon as short points: `3` is 3 art px from the near side, `-3` is 3 from the far side (100%). */
function points(polygon: string): string[] {
  const body = polygon.match(/^polygon\((.*)\)$/)?.[1]
  if (body === undefined) throw new Error(`not a polygon: ${polygon}`)
  const length = (value: string): string => {
    if (value === "0") return "0"
    if (value === "100%") return "-0"
    const near = value.match(/^calc\((\d+) \* var\(--zone-px\)\)$/)
    if (near !== null) return near[1]!
    const far = value.match(/^calc\(100% - (\d+) \* var\(--zone-px\)\)$/)
    if (far !== null) return `-${far[1]}`
    throw new Error(`not a whole number of art px: ${value}`)
  }
  return body.split(/,\s*(?![^()]*\))/).map(point => {
    const parts = point.trim().match(/^(calc\([^)]*\)\)|calc\([^)]*\)|\S+)\s+(.+)$/)
    if (parts === null) throw new Error(`bad point ${point}`)
    return `${length(parts[1]!)},${length(parts[2]!)}`
  })
}

describe("zoneStairClip", () => {
  test("a panel 3 art px tall leans 2 px each side, one stair per art px", () => {
    expect(points(zoneStairClip(10, 3))).toEqual([
      "2,0", "2,1", "1,1", "1,2", "0,2", "0,-0",
      "-2,-0", "-2,2", "-1,2", "-1,1", "-0,1", "-0,0",
    ])
  })

  test("the slant is the height: a 16-px panel's top row starts 15 px in and every step is 1 px", () => {
    const pts = points(zoneStairClip(100, 16))
    expect(pts[0]).toBe("15,0")
    // Down the left edge: x falls by one each time y rises by one.
    const left = pts.slice(0, 32)
    for (let i = 0; i < 15; i += 1) {
      expect(left[i * 2]).toBe(`${15 - i},${i}`)
      expect(left[i * 2 + 1]).toBe(`${15 - i},${i + 1}`)
    }
    expect(pts).toContain("0,-0")
  })

  test("every length is a whole number of art px or a share of the box, never a smooth line", () => {
    for (const polygon of [zoneStairClip(40, 12), zoneStairClip(40, 12, { left: "straight" }), zoneStairClip(7, 9, { right: "backslash" })]) {
      expect(() => points(polygon)).not.toThrow()
    }
  })

  test("a straight left edge and a slashed right edge: the keycap's tail", () => {
    expect(points(zoneStairClip(20, 3, { left: "straight", right: "slash" }))).toEqual([
      "0,0", "0,-0", "-2,-0", "-2,2", "-1,2", "-1,1", "-0,1", "-0,0",
    ])
  })

  test("backslash edges lean the other way", () => {
    expect(points(zoneStairClip(20, 3, { left: "backslash", right: "straight" }))).toEqual([
      "0,0", "0,1", "1,1", "1,2", "2,2", "2,-0", "-0,-0", "-0,0",
    ])
  })

  test("a panel too narrow for two full slants leans half its width", () => {
    const pts = points(zoneStairClip(5, 10))
    expect(pts[0]).toBe("2,0")
  })

  test("two straight edges are the box", () => {
    expect(points(zoneStairClip(10, 4, { left: "straight", right: "straight" }))).toEqual(["0,0", "0,-0", "-0,-0", "-0,0"])
  })
})

describe("zoneStairWedge", () => {
  test("a 3-px wedge in the top-left corner: full along the top, one stair a row", () => {
    expect(points(zoneStairWedge(3, "top-left"))).toEqual([
      "0,0", "0,3", "1,3", "1,2", "2,2", "2,1", "3,1", "3,0",
    ])
  })

  test("a 3-px wedge in the bottom-right corner, measured from the far sides", () => {
    expect(points(zoneStairWedge(3, "bottom-right"))).toEqual([
      "-1,-3", "-1,-2", "-2,-2", "-2,-1", "-3,-1", "-3,-0", "-0,-0", "-0,-3",
    ])
  })
})

describe("the shapes the stylesheets hold are the helper's", () => {
  const tokens = readFileSync(join(SRC, "zone-tokens.css"), "utf8")
  const token = (name: string) => tokens.match(new RegExp(`${name}: ([^;]+);`))?.[1]

  test("the keycap's rim and disc are pixel circles 15 and 13 art px across", () => {
    expect(token("--zone-keycap-rim-shape")).toBe(zoneStairCircle(15))
    expect(token("--zone-keycap-disc-shape")).toBe(zoneStairCircle(13))
  })

  test("the keycap's tail is a 12-px bar with a straight start and a 45 degree stair end", () => {
    const css = flat(readFileSync(join(SRC, "ui", "atoms", "ZoneKeycap.css"), "utf8"))
    expect(css).toContain(`clip-path: ${zoneStairClip(100, 12, { left: "straight", right: "slash" })};`)
  })

  test("a pixel circle's rows are symmetric and every point is a share of whole art px", () => {
    const circle = zoneStairCircle(15)
    const shares = [...circle.matchAll(/([\d.]+)%/g)].map(match => Number(match[1]) * 15 / 100)
    for (const share of shares) expect(Math.abs(share - Math.round(share))).toBeLessThan(1e-3)
  })
})
