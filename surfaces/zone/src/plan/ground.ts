/**
 * The ground's drawing as a pure function of the clock (docs/design/
 * surfaces/zone/REFINEMENT.md, 2b and M7).
 *
 * Two opaque pixel rings, 6 art px thick, stair-stepped on the art grid,
 * each carrying filled "planet" dots that orbit it: ring A's once in
 * `driftA` ms, ring B's once in `driftB` the other way. No bands (they
 * belong to the title card), no opacity. The clock steps at `fps` frames a
 * second, and every dot lands on a whole art px, so a frame is a pure
 * function of `t`: seek to any time and the same pixels come out.
 *
 * `renderZoneGround` paints onto anything with a `fillStyle` and a
 * `fillRect`, one rectangle per run of pixels, in art px. The ground
 * organism hands it a canvas `wArt` by `hArt` that CSS scales by whole
 * `--zone-px` with `image-rendering: pixelated`; the tests hand it a
 * recorder.
 *
 * The Modern look (src/plan/look.ts, rings "smooth") draws the same rings
 * and dots with `renderZoneGroundSmooth`: antialiased arcs on a canvas of
 * CSS px, `scale` CSS px an art px, each dot where its orbit puts it,
 * never rounded to an art px.
 */

export interface ZoneGroundPalette {
  readonly ground: string
  readonly ringA: string
  readonly ringB: string
  /** Whether this ground draws its rings at all (the title card's and PAUSED's do not). */
  readonly rings: boolean
  /** One turn of ring A's dots, in ms. */
  readonly driftA: number
  /** One turn of ring B's dots, in ms. */
  readonly driftB: number
  /** Frames a second: the dots step, never glide. */
  readonly fps: number
}

export interface ZoneGroundPainter {
  fillStyle: string | CanvasGradient | CanvasPattern
  fillRect(x: number, y: number, w: number, h: number): void
}

/** A ring's thickness in art px. */
export const ZONE_RING_THICKNESS = 6

interface Ring {
  readonly cx: number
  readonly cy: number
  readonly r: number
  /** Each dot: its radius, and its start angle as a share of a turn. */
  readonly dots: readonly { readonly r: number; readonly phase: number }[]
  /** 1 turns clockwise, -1 the other way. */
  readonly turn: 1 | -1
}

/**
 * Where the rings stand, as shares of the canvas, so a wide strip and a
 * tall column each see a part of them. Ring A is the big ring across the
 * middle; ring B the smaller one to the upper right.
 */
export function zoneGroundRings(wArt: number, hArt: number): { readonly a: Ring; readonly b: Ring } {
  const short = Math.min(wArt, hArt)
  return {
    a: {
      cx: Math.round(wArt * 0.5),
      cy: Math.round(hArt * 0.56),
      r: Math.round(short * 0.46),
      dots: [{ r: 10, phase: 0.75 }, { r: 6, phase: 0.08 }],
      turn: 1,
    },
    b: {
      cx: Math.round(wArt * 0.8),
      cy: Math.round(hArt * 0.36),
      r: Math.round(short * 0.34),
      dots: [{ r: 8, phase: 0.5 }],
      turn: -1,
    },
  }
}

/** The clock stepped to whole frames. */
export function zoneGroundFrame(t: number, fps: number): number {
  const step = 1000 / Math.max(1, fps)
  return Math.floor(Math.max(0, t) / step) * step
}

/** One pixel row's spans of a disc or an annulus, centred on (cx, cy), in art px. */
function spans(cx: number, cy: number, outer: number, inner: number, y: number): readonly (readonly [number, number])[] {
  const dy = y + 0.5 - cy
  if (Math.abs(dy) > outer) return []
  const out = Math.sqrt(outer * outer - dy * dy)
  const left = Math.round(cx - out)
  const right = Math.round(cx + out)
  if (inner <= 0 || Math.abs(dy) >= inner) return right > left ? [[left, right]] : []
  const into = Math.sqrt(inner * inner - dy * dy)
  const innerLeft = Math.round(cx - into)
  const innerRight = Math.round(cx + into)
  const both: readonly (readonly [number, number])[] = [[left, innerLeft], [innerRight, right]]
  return both.filter(([from, to]) => to > from)
}

function fillRing(paint: ZoneGroundPainter, cx: number, cy: number, outer: number, inner: number, hArt: number): void {
  const top = Math.max(0, Math.floor(cy - outer))
  const bottom = Math.min(hArt, Math.ceil(cy + outer))
  for (let y = top; y < bottom; y += 1) {
    for (const [from, to] of spans(cx, cy, outer, inner, y)) paint.fillRect(from, y, to - from, 1)
  }
}

/** Where a dot stands at time `t`: on its ring's centre line, on a whole art px (or anywhere, `whole` false). */
export function zoneGroundDot(
  ring: Ring,
  dot: Ring["dots"][number],
  t: number,
  drift: number,
  whole: boolean,
): { readonly x: number; readonly y: number } {
  const angle = (dot.phase + (ring.turn * t) / Math.max(1, drift)) * Math.PI * 2
  const x = ring.cx + ring.r * Math.cos(angle)
  const y = ring.cy + ring.r * Math.sin(angle)
  return whole ? { x: Math.round(x), y: Math.round(y) } : { x, y }
}

/**
 * Paint the ground at time `t` (ms) on a canvas `wArt` by `hArt` art px:
 * the flat ground, ring B, ring A over it, then each ring's dots.
 */
export function renderZoneGround(paint: ZoneGroundPainter, t: number, palette: ZoneGroundPalette, wArt: number, hArt: number): void {
  paint.fillStyle = palette.ground
  paint.fillRect(0, 0, wArt, hArt)
  if (!palette.rings || wArt <= 0 || hArt <= 0) return
  const frame = zoneGroundFrame(t, palette.fps)
  const { a, b } = zoneGroundRings(wArt, hArt)
  const half = ZONE_RING_THICKNESS / 2
  for (const [ring, colour, drift] of [[b, palette.ringB, palette.driftB], [a, palette.ringA, palette.driftA]] as const) {
    paint.fillStyle = colour
    fillRing(paint, ring.cx, ring.cy, ring.r + half, ring.r - half, hArt)
    for (const dot of ring.dots) {
      const at = zoneGroundDot(ring, dot, frame, drift, true)
      fillRing(paint, at.x + 0.5, at.y + 0.5, dot.r, 0, hArt)
    }
  }
}

/** What a smooth ground paints with: a canvas's paths, in art px after `setTransform`. */
export interface ZoneGroundSmoothPainter extends ZoneGroundPainter {
  setTransform(a: number, b: number, c: number, d: number, e: number, f: number): void
  beginPath(): void
  arc(x: number, y: number, radius: number, startAngle: number, endAngle: number, counterclockwise?: boolean): void
  fill(fillRule?: "nonzero" | "evenodd"): void
}

/**
 * The Modern ground at time `t` (ms): the same flat ground, ring B, ring A
 * and their dots as `renderZoneGround`, drawn as antialiased arcs on a
 * canvas `scale` px an art px, the dots gliding between frames instead of
 * standing on whole art px.
 */
export function renderZoneGroundSmooth(
  paint: ZoneGroundSmoothPainter,
  t: number,
  palette: ZoneGroundPalette,
  wArt: number,
  hArt: number,
  scale: number,
): void {
  paint.setTransform(scale, 0, 0, scale, 0, 0)
  paint.fillStyle = palette.ground
  paint.fillRect(0, 0, wArt, hArt)
  if (!palette.rings || wArt <= 0 || hArt <= 0) return
  const frame = zoneGroundFrame(t, palette.fps)
  const { a, b } = zoneGroundRings(wArt, hArt)
  const half = ZONE_RING_THICKNESS / 2
  const turn = Math.PI * 2
  for (const [ring, colour, drift] of [[b, palette.ringB, palette.driftB], [a, palette.ringA, palette.driftA]] as const) {
    paint.fillStyle = colour
    paint.beginPath()
    paint.arc(ring.cx, ring.cy, ring.r + half, 0, turn)
    paint.arc(ring.cx, ring.cy, Math.max(0, ring.r - half), 0, turn, true)
    paint.fill("evenodd")
    for (const dot of ring.dots) {
      const at = zoneGroundDot(ring, dot, frame, drift, false)
      paint.beginPath()
      paint.arc(at.x + 0.5, at.y + 0.5, dot.r, 0, turn)
      paint.fill()
    }
  }
}
