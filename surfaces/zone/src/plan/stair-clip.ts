/**
 * Zone's 45 degree edges as pixel stairs: one pure helper for every panel,
 * tail and wedge (docs/design/surfaces/zone/REFINEMENT.md, 2b).
 *
 * A shape is a stack of rows one art px tall. Each row spans from a left
 * end to a right end; a 45 degree edge moves its end by one art px a row,
 * so the edge is a stair with one step per art px, never a smooth line.
 * The result is a CSS `polygon()` in which every length is a whole number
 * of `--zone-px`: lengths on the far side are written from 100%, so the
 * shape follows its box's width without knowing it, and the last row ends
 * at 100% of the height, so a box a fraction taller than its art rows still
 * closes.
 *
 * Edges, read top to bottom:
 *
 *  - `straight`   the end does not move.
 *  - `slash`      `/`: the end moves one art px left a row. Both side edges
 *                 `slash` is Mania's parallelogram, its top leading.
 *  - `backslash`  `\`: the end moves one art px right a row.
 *  - `point`      `<` on the left, `>` on the right: the end moves out one
 *                 art px a row to the middle row, then back in. The
 *                 banner's two points.
 *
 * Every helper takes the look's edges (src/plan/look.ts): `stairs` draws
 * the stair above (Blend, Retro); `smooth` draws the same shape with each
 * slanted run as one straight line between the stair's first and last
 * rows (Modern), so a look changes the edge and never the box.
 */
import type { ZoneEdges } from "./look"

export type ZoneStairEdge = "straight" | "slash" | "backslash" | "point"

/** A length along one axis: art px from the near side, or from the far side (100%). */
interface Length {
  readonly from: "start" | "end"
  readonly art: number
}

const start = (art: number): Length => ({ from: "start", art })
const end = (art: number): Length => ({ from: "end", art })

const css = (length: Length): string => {
  if (length.from === "start") return length.art === 0 ? "0" : `calc(${length.art} * var(--zone-px))`
  return length.art === 0 ? "100%" : `calc(100% - ${length.art} * var(--zone-px))`
}

const same = (a: Length, b: Length): boolean => a.from === b.from && a.art === b.art

type Point = readonly [x: Length, y: Length]

/** The polygon round a stack of rows: down the left ends, up the right ends. */
function polygonOfRows(rows: readonly (readonly [left: Length, right: Length])[], top: (row: number) => Length): string {
  const points: Point[] = []
  const push = (point: Point) => {
    const last = points.at(-1)
    if (last !== undefined && same(last[0], point[0]) && same(last[1], point[1])) return
    points.push(point)
  }
  rows.forEach(([left], row) => {
    push([left, top(row)])
    push([left, top(row + 1)])
  })
  for (let row = rows.length - 1; row >= 0; row -= 1) {
    const right = rows[row]![1]
    push([right, top(row + 1)])
    push([right, top(row)])
  }
  // Drop the middle of three points on one line: a stair has none, a straight edge does.
  const kept = points.filter((point, index) => {
    const before = points[(index - 1 + points.length) % points.length]!
    const after = points[(index + 1) % points.length]!
    const vertical = same(before[0], point[0]) && same(point[0], after[0])
    const horizontal = same(before[1], point[1]) && same(point[1], after[1])
    return !vertical && !horizontal
  })
  return `polygon(${kept.map(([x, y]) => `${css(x)} ${css(y)}`).join(", ")})`
}

/**
 * The smooth outline round the same rows: down the left ends, up the right
 * ends, one point where a side starts and ends and one at each row where
 * its run turns (a point's tip), so each slanted run is one straight line.
 */
function smoothOfRows(rows: readonly (readonly [left: Length, right: Length])[], top: (row: number) => Length): string {
  const last = rows.length - 1
  const turns = (side: 0 | 1): number[] => {
    const at: number[] = []
    for (let row = 1; row < last; row += 1) {
      const before = rows[row]![side].art - rows[row - 1]![side].art
      const after = rows[row + 1]![side].art - rows[row]![side].art
      if (before !== after) at.push(row)
    }
    return at
  }
  const middle = (row: number): Length => ({ from: "start", art: row + 0.5 })
  const points: Point[] = [[rows[0]![0], top(0)]]
  for (const row of turns(0)) points.push([rows[row]![0], middle(row)])
  points.push([rows[last]![0], top(last + 1)], [rows[last]![1], top(last + 1)])
  for (const row of turns(1).reverse()) points.push([rows[row]![1], middle(row)])
  points.push([rows[0]![1], top(0)])
  const kept = points.filter((point, index) => {
    const before = points[(index - 1 + points.length) % points.length]!
    return !(same(before[0], point[0]) && same(before[1], point[1]))
  })
  return `polygon(${kept.map(([x, y]) => `${css(x)} ${css(y)}`).join(", ")})`
}

/** Row `row` of `rows`, from the top, as a length from the top; the last row ends at 100%. */
const fromTop = (rows: number) => (row: number): Length => (row >= rows ? end(0) : start(row))

/**
 * A panel `hArt` art px tall whose side edges are 45 degree stairs (or
 * straight). The run of each slanted edge is the panel's height less one,
 * so the top and bottom rows are each one step: slant = height, as all of
 * Mania's panels lean. `wArt` bounds the run: a panel too narrow for two
 * full slants leans as far as half its width.
 */
export function zoneStairClip(
  wArt: number,
  hArt: number,
  edges: { readonly left?: ZoneStairEdge; readonly right?: ZoneStairEdge } = {},
  drawn: ZoneEdges,
): string {
  const left = edges.left ?? "slash"
  const right = edges.right ?? "slash"
  const rows = Math.max(1, Math.round(hArt))
  const slanted = (left === "straight" ? 0 : 1) + (right === "straight" ? 0 : 1)
  const run = Math.max(0, Math.min(rows - 1, slanted === 0 ? 0 : Math.floor(Math.max(0, wArt - 1) / slanted)))
  const step = (row: number): number => Math.max(0, Math.min(run, run - Math.round((row * run) / Math.max(1, rows - 1))))
  // A point reaches its tip at the middle row: one art px a row, half the height deep.
  const half = Math.min(run, Math.floor((rows - 1) / 2))
  const point = (row: number): number => Math.max(0, half - Math.min(row, rows - 1 - row))
  const leftAt = (row: number): Length => {
    switch (left) {
      case "straight": return start(0)
      case "slash": return start(step(row))
      case "backslash": return start(run - step(row))
      case "point": return start(point(row))
    }
  }
  const rightAt = (row: number): Length => {
    switch (right) {
      case "straight": return end(0)
      case "slash": return end(run - step(row))
      case "backslash": return end(step(row))
      case "point": return end(point(row))
    }
  }
  const stack = Array.from({ length: rows }, (_, row) => [leftAt(row), rightAt(row)] as const)
  return drawn === "smooth" ? smoothOfRows(stack, fromTop(rows)) : polygonOfRows(stack, fromTop(rows))
}

/** A box corner a wedge stands in. */
export type ZoneStairCorner = "top-left" | "top-right" | "bottom-left" | "bottom-right"

/**
 * A right-angled wedge `sizeArt` art px along each leg, standing in one
 * corner of its box, its long edge a 45 degree stair: the dialog's black
 * wedges and PAUSED's wedge cut.
 */
export function zoneStairWedge(sizeArt: number, corner: ZoneStairCorner, drawn: ZoneEdges): string {
  const size = Math.max(1, Math.round(sizeArt))
  const top = corner.startsWith("top")
  const leftSide = corner.endsWith("left")
  if (drawn === "smooth") {
    // The right-angled triangle itself: its two legs along the box's edges.
    const x = (art: number): Length => (leftSide ? start(art) : end(art))
    const y = (art: number): Length => (top ? start(art) : end(art))
    const corners: Point[] = [[x(0), y(0)], [x(size), y(0)], [x(0), y(size)]]
    return `polygon(${corners.map(([px, py]) => `${css(px)} ${css(py)}`).join(", ")})`
  }
  // Row 0 is the row on the box's edge, which is the wedge's full width.
  const width = (row: number) => size - row
  const rows = Array.from({ length: size }, (_, row) => {
    const reach = width(top ? row : size - 1 - row)
    return leftSide ? [start(0), start(reach)] as const : [end(reach), end(0)] as const
  })
  const y = (row: number): Length => (top ? start(row) : row >= size ? end(0) : end(size - row))
  return polygonOfRows(rows, y)
}

/**
 * A pixel circle `diameterArt` art px across, as a polygon in shares of its
 * own box: a box that many art px square draws every stair on an art px.
 * Each row spans the pixels whose centres fall inside the circle.
 */
export function zoneStairCircle(diameterArt: number, drawn: ZoneEdges): string {
  if (drawn === "smooth") return "circle(50%)"
  const d = Math.max(1, Math.round(diameterArt))
  const r = d / 2
  const share = (n: number): string => (n === 0 ? "0" : n === d ? "100%" : `${Number(((n / d) * 100).toFixed(4))}%`)
  const spans = Array.from({ length: d }, (_, row) => {
    const y = row + 0.5 - r
    const half = Math.sqrt(Math.max(0, r * r - y * y))
    const reach = Math.round(r - half)
    return [reach, d - reach] as const
  })
  const points: string[] = []
  const push = (x: number, y: number) => {
    const point = `${share(x)} ${share(y)}`
    if (points.at(-1) !== point) points.push(point)
  }
  spans.forEach(([left], row) => {
    push(left, row)
    push(left, row + 1)
  })
  for (let row = d - 1; row >= 0; row -= 1) {
    const right = spans[row]![1]
    push(right, row + 1)
    push(right, row)
  }
  return `polygon(${points.join(", ")})`
}
