/**
 * The stage plates of the no-art cards (owner, 2026-11-13, #423: "the card
 * lights the room"). A game with no picture lights the stage the way a
 * picture does: its own card, scaled up and blurred past recognition. Velvet
 * draws no blur and the logic holds no paths, so a plate is made here, once
 * per drawing, as a few soft ellipses: one for each shape the blur would
 * spread, at its place, with its palette slot and its share of the area. The
 * logic sets them as an SVG with a Gaussian blur (src/plate.ts) and the shell
 * renders it like any other picture.
 *
 * Two kinds:
 *   - a system poster: the card itself, in card units (the logic crops it to
 *     the stage the way a cover crop would);
 *   - a pattern tile: three of the card's own parts, large and apart, in its
 *     paper, its light tone and its rule's colour, on its ink, in a 160 x 90
 *     stage box. A pattern blurred as it is averages to one flat colour.
 */
import { hash, pick, rng } from "../src/colour"
import { SLOT } from "../src/drawings-slots"
import type { Language } from "../src/systems"
import { FIELD_H, partShapes, type Motif, type Shape } from "./draw"

export interface Ellipse {
  readonly slot: number
  readonly cx: number
  readonly cy: number
  readonly rx: number
  readonly ry: number
  readonly o: number
}

/** The numbers of an SVG path, command by command (M, L, A, H, V, Z: what the drawings use). */
function* commands(d: string): Generator<{ c: string; n: number[] }> {
  const re = /([MLAHVZ])([^MLAHVZ]*)/gi
  for (const m of d.matchAll(re)) {
    const n = (m[2]!.match(/-?\d*\.?\d+(?:e-?\d+)?/gi) ?? []).map(Number)
    yield { c: m[1]!.toUpperCase(), n }
  }
}

/** Each sub-path of `d` as one ellipse of its bounds and its area. */
function ellipses(shape: Shape): Ellipse[] {
  const out: Ellipse[] = []
  let points: [number, number][] = []
  let arcR = 0
  let x = 0
  let y = 0
  const flush = () => {
    if (points.length === 0) return
    const xs = points.map(p => p[0])
    const ys = points.map(p => p[1])
    let x0 = Math.min(...xs), x1 = Math.max(...xs), y0 = Math.min(...ys), y1 = Math.max(...ys)
    let area = 0
    for (let i = 0; i < points.length; i++) {
      const [ax, ay] = points[i]!
      const [bx, by] = points[(i + 1) % points.length]!
      area += ax * by - bx * ay
    }
    area = Math.abs(area) / 2
    // A sub-path drawn with arcs alone is a circle: its two end points say nothing of its size.
    if (arcR > 0 && x1 - x0 < arcR * 2.2 && y1 - y0 < arcR * 2.2) {
      const cx = (x0 + x1) / 2
      const cy = (y0 + y1) / 2
      x0 = cx - arcR; x1 = cx + arcR; y0 = cy - arcR; y1 = cy + arcR
      area = Math.PI * arcR * arcR
    }
    const w = x1 - x0
    const h = y1 - y0
    if (w > 0.2 && h > 0.2 && area > 0.05) {
      // The ellipse of the bounds, shrunk to the shape's own area.
      const s = Math.min(1.15, Math.sqrt((4 * area) / (Math.PI * w * h)))
      out.push({ slot: shape.c, cx: (x0 + x1) / 2, cy: (y0 + y1) / 2, rx: (w / 2) * s, ry: (h / 2) * s, o: shape.o ?? 1 })
    }
    points = []
    arcR = 0
  }
  for (const { c, n } of commands(shape.d)) {
    if (c === "M") {
      flush()
      x = n[0]!; y = n[1]!
      points.push([x, y])
    } else if (c === "L") {
      for (let i = 0; i + 1 < n.length; i += 2) { x = n[i]!; y = n[i + 1]!; points.push([x, y]) }
    } else if (c === "H") {
      x = n[0]!; points.push([x, y])
    } else if (c === "V") {
      y = n[0]!; points.push([x, y])
    } else if (c === "A") {
      for (let i = 0; i + 6 < n.length; i += 7) { arcR = Math.max(arcR, n[i]!, n[i + 1]!); x = n[i + 5]!; y = n[i + 6]!; points.push([x, y]) }
    } else if (c === "Z") {
      flush()
    }
  }
  flush()
  return out
}

/** A system poster's plate: its field's shapes as ellipses, in card units; the paper under them. */
export function posterPlate(shapes: readonly Shape[]): Ellipse[] {
  // A shape too small or too faint to show through the blur (a texture's hairline) is left out.
  return shapes.filter(shape => shape.c !== SLOT.paper).flatMap(ellipses).filter(e => e.cy - e.ry < FIELD_H + 2 && Math.PI * e.rx * e.ry * e.o >= 6)
}

/** The box a pattern plate is set in: a 16:9 stage. */
export const PLATE_W = 160
export const PLATE_H = 90

/** A pattern tile's plate: three of its parts, large and apart, on its ink. */
export function patternPlate(generic: Language, kind: Motif, variant: number): Ellipse[] {
  const next = rng(hash(`${variant}|plate|${kind.join("+")}`))
  const spots: readonly (readonly [number, number])[] = [[0.3, 0.34], [0.72, 0.28], [0.56, 0.78], [0.18, 0.8], [0.86, 0.7]]
  const used: number[] = []
  const at = () => {
    let i: number
    do i = Math.floor(next() * spots.length)
    while (used.includes(i))
    used.push(i)
    return spots[i]!
  }
  const slots = [SLOT.paper, SLOT.lite, SLOT.acc]
  const names = [kind[0], kind[1], pick(next, generic.parts)]
  return names.flatMap((name, i) => {
    const [x, y] = at()
    const scale = 1.5 + next() * 0.9
    const turn = Math.round((next() - 0.5) * 50)
    return partShapes(generic, name, { x: x * PLATE_W, y: y * PLATE_H, s: scale, r: turn }, slots[i]!).flatMap(ellipses)
  })
}

