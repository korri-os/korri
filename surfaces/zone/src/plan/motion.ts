/**
 * Zone's moments as functions of time (REFINEMENT.md 2d): every move is a
 * pure `frame(t)` that returns where each element stands at `t` ms after
 * its trigger, in whole art px. `use-zone-moment.ts` runs the clock, writes
 * a frame's numbers as custom properties, and lets `window.seek(t)` hold
 * any frame, so a take is the same every run.
 *
 *     M1  card snap and focus travel: the band's track and the two cards
 *         that trade sizes, 220 ms, one overshoot
 *     M2  menu transition: the old ground stair-wipes away from the top
 *         right, the banner drops, the new rows slide in, 20 ms apart
 *     M3  title-card wipe: four bands from the top right, the name panel
 *         from the left, the ZONE bar from the right, the act disc in four
 *         steps; out, everything leaves toward the bottom left in 400 ms
 *     M5  the tally: rows 300 ms apart, each value counting up in 60 steps
 *         over 1200 ms with its bar
 *     M6  the idle attract: M3 in, hold, out, then the game's stage panning
 *         32 art px, 6000 ms a game, PRESS ANY BUTTON blinking
 *
 * Durations and curves are the brief's. Distances follow the container
 * (wArt, hArt); durations do not. Translates are rounded to whole art px
 * here, so no frame ever sits between the screen's art pixels, on the art
 * grid of the Blend and Retro looks; in Modern (`grid` "free") they fall
 * where the easing puts them, and the hard steps of the act disc are eased.
 */
import type { ZoneEdges, ZoneMotionGrid } from "./look"

/** A cubic Bézier easing, as CSS defines it: x is time, y is progress. */
export function cubicBezier(x1: number, y1: number, x2: number, y2: number): (t: number) => number {
  const at = (a: number, b: number, s: number) => 3 * a * s * (1 - s) ** 2 + 3 * b * s * s * (1 - s) + s ** 3
  const slope = (a: number, b: number, s: number) => 3 * a * (1 - s) ** 2 + 6 * (b - a) * s * (1 - s) + 3 * (1 - b) * s * s
  return (t: number) => {
    if (t <= 0) return 0
    if (t >= 1) return 1
    // Newton steps on x, then bisection if the slope is flat.
    let s = t
    for (let index = 0; index < 8; index += 1) {
      const error = at(x1, x2, s) - t
      const d = slope(x1, x2, s)
      if (Math.abs(error) < 1e-6) return at(y1, y2, s)
      if (Math.abs(d) < 1e-6) break
      s = Math.min(1, Math.max(0, s - error / d))
    }
    let low = 0
    let high = 1
    s = t
    for (let index = 0; index < 30; index += 1) {
      const x = at(x1, x2, s)
      if (Math.abs(x - t) < 1e-6) break
      if (x < t) low = s
      else high = s
      s = (low + high) / 2
    }
    return at(y1, y2, s)
  }
}

/** CSS `steps(n)` (jump-end): n flat runs, the last value reached only at the end. */
export const steps = (count: number) => (t: number): number => (t >= 1 ? 1 : t <= 0 ? 0 : Math.floor(t * count) / count)

/** Progress of a move that starts at `start` and runs `duration` ms, eased. */
export function along(t: number, start: number, duration: number, ease: (t: number) => number): number {
  if (duration <= 0) return t >= start ? 1 : 0
  return ease(Math.min(1, Math.max(0, (t - start) / duration)))
}

/**
 * A distance in art px: rounded to a whole art px on the art grid (Blend,
 * Retro), left where it falls in Modern, whose motion is sub-pixel.
 */
const art = (value: number, grid: ZoneMotionGrid = "art"): number => (grid === "art" ? Math.round(value) || 0 : value || 0)

/** A move in hard steps on the art grid, eased smooth in Modern. */
const stepped = (count: number, grid: ZoneMotionGrid, smooth: (t: number) => number) =>
  grid === "art" ? steps(count) : smooth

export const ZONE_EASE = {
  /** The one-bounce snap: focus travel, the banner. */
  snap: cubicBezier(0.34, 1.56, 0.64, 1),
  /** A panel landing with a softer overshoot. */
  land: cubicBezier(0.34, 1.4, 0.64, 1),
  /** Rows landing with the least overshoot. */
  rows: cubicBezier(0.34, 1.3, 0.64, 1),
  /** The title card's bands. */
  bands: cubicBezier(0.2, 0.9, 0.3, 1),
  /** Anything leaving: slow start, fast end. */
  leave: cubicBezier(0.5, 0, 0.9, 0.4),
  /** The ground's wipe. */
  wipe: cubicBezier(0.2, 0.8, 0.2, 1),
  linear: (t: number) => t,
} as const

// ---- M1 card snap ----

/** The band's snap: the track and the two cards trading sizes. */
export const ZONE_SNAP_MS = 220

/** M1 at `t`: 0 at the old focus, 1 at the new, overshooting once between. */
export const zoneSnapFrame = (t: number): number => along(t, 0, ZONE_SNAP_MS, ZONE_EASE.snap)

// ---- M2 menu transition ----

export const ZONE_MENU_MS = 500
const MENU_WIPE = { start: 60, duration: 280 } as const
const MENU_BANNER = { start: 160, duration: 200, rise: 8 } as const
const MENU_ROWS = { start: 160, duration: 240, stagger: 20, most: 6 } as const

export interface ZoneMenuFrame {
  /** How far the old ground's stair wipe has run, 0 to 1, from the top right. */
  readonly wipe: number
  /** The banner's drop, art px above its rest. */
  readonly bannerY: number
  /** Each of the first six rows' offset from its rest, art px to the right. */
  readonly rowsX: readonly number[]
}

/** M2 at `t` on a canvas `wArt` wide: the new page arriving. */
export function zoneMenuFrame(t: number, wArt: number, grid: ZoneMotionGrid = "art"): ZoneMenuFrame {
  return {
    wipe: along(t, MENU_WIPE.start, MENU_WIPE.duration, ZONE_EASE.wipe),
    bannerY: art(-MENU_BANNER.rise * (1 - along(t, MENU_BANNER.start, MENU_BANNER.duration, ZONE_EASE.snap)), grid),
    rowsX: Array.from({ length: MENU_ROWS.most }, (_, index) =>
      art(wArt * (1 - along(t, MENU_ROWS.start + index * MENU_ROWS.stagger, MENU_ROWS.duration, ZONE_EASE.rows)), grid)),
  }
}

/** GAME OPTS.' sheet arriving: M2's incoming rows, as one sheet from the right. */
export const ZONE_SHEET_MS = MENU_ROWS.duration

/** The sheet's offset from rest at `t`, art px to the right. */
export const zoneSheetFrame = (t: number, wArt: number, grid: ZoneMotionGrid = "art"): number =>
  art(wArt * (1 - along(t, 0, MENU_ROWS.duration, ZONE_EASE.rows)), grid)

/**
 * The stair the old ground keeps at wipe `share`: everything below and left
 * of a 45 degree line that has swept in from the top right corner. Rows of
 * 4 art px, so the wipe reads as stairs and the polygon stays short.
 * Smooth (Modern), the line itself, cut where it meets the box.
 */
export function zoneWipeClip(share: number, wArt: number, hArt: number, edges: ZoneEdges = "stairs"): string {
  if (share <= 0) return "none"
  if (share >= 1) return "polygon(0 0, 0 0, 0 0)"
  // Kept: every point with x + (hArt - y) at most `reach` from the bottom left.
  const reach = (wArt + hArt) * (1 - share)
  const step = 4
  const px = (value: number) => (value === 0 ? "0" : `calc(${Number(value.toFixed(4))} * var(--zone-px))`)
  if (edges === "smooth") {
    const at = (y: number) => Math.max(0, Math.min(wArt, reach - hArt + y))
    const rows = [0, hArt - reach, wArt + hArt - reach, hArt].filter(y => y >= 0 && y <= hArt).sort((a, b) => a - b)
    return `polygon(0 0, ${rows.map(y => `${px(at(y))} ${px(y)}`).join(", ")}, 0 ${px(hArt)})`
  }
  const points: string[] = ["0 0"]
  for (let y = 0; y < hArt; y += step) {
    const x = Math.max(0, Math.min(wArt, Math.floor(reach - hArt + y)))
    points.push(`${px(x)} ${px(y)}`, `${px(x)} ${px(Math.min(hArt, y + step))}`)
  }
  points.push(`0 ${px(hArt)}`)
  return `polygon(${points.join(", ")})`
}

// ---- M3 title card ----

export const ZONE_TITLE_IN_MS = 700
export const ZONE_TITLE_OUT_MS = 400
const TITLE_BANDS = { start: 0, duration: 320, stagger: 40, count: 4 } as const
const TITLE_NAME = { start: 200, duration: 260 } as const
const TITLE_BAR = { start: 260, duration: 260 } as const
const TITLE_DISC = { start: 420, duration: 160 } as const

export interface ZoneTitleFrame {
  /** The yellow ground and each of the four bands: offset from rest, art px. */
  readonly ground: readonly [number, number]
  readonly bands: readonly (readonly [number, number])[]
  /** The name panel's and the ZONE bar's offsets, art px. */
  readonly nameX: number
  readonly barX: number
  /** The act disc's scale, 0 to 1, in four steps. */
  readonly disc: number
}

/** M3 in at `t`: everything from the top right (bands), left (name) and right (bar). */
export function zoneTitleInFrame(t: number, wArt: number, hArt: number, grid: ZoneMotionGrid = "art"): ZoneTitleFrame {
  const band = (index: number): readonly [number, number] => {
    const left = 1 - along(t, TITLE_BANDS.start + index * TITLE_BANDS.stagger, TITLE_BANDS.duration, ZONE_EASE.bands)
    return [art(wArt * left, grid), art(-hArt * left, grid)]
  }
  const ground = 1 - along(t, 0, TITLE_BANDS.duration - 80, ZONE_EASE.bands)
  return {
    ground: [art(wArt * ground, grid), art(-hArt * ground, grid)],
    bands: Array.from({ length: TITLE_BANDS.count }, (_, index) => band(index)),
    nameX: art(-wArt * (1 - along(t, TITLE_NAME.start, TITLE_NAME.duration, ZONE_EASE.land)), grid),
    barX: art(wArt * (1 - along(t, TITLE_BAR.start, TITLE_BAR.duration, ZONE_EASE.land)), grid),
    disc: along(t, TITLE_DISC.start, TITLE_DISC.duration, stepped(4, grid, ZONE_EASE.snap)),
  }
}

/** M3 out at `t`: the reverse, the bands leaving toward the bottom left. */
export function zoneTitleOutFrame(t: number, wArt: number, hArt: number, grid: ZoneMotionGrid = "art"): ZoneTitleFrame {
  const gone = along(t, 0, ZONE_TITLE_OUT_MS, ZONE_EASE.leave)
  const band = (index: number): readonly [number, number] => {
    const share = along(t, index * TITLE_BANDS.stagger, ZONE_TITLE_OUT_MS - 3 * TITLE_BANDS.stagger, ZONE_EASE.leave)
    return [art(-wArt * share, grid), art(hArt * share, grid)]
  }
  return {
    ground: [art(-wArt * gone, grid), art(hArt * gone, grid)],
    bands: Array.from({ length: TITLE_BANDS.count }, (_, index) => band(index)),
    nameX: art(-wArt * gone, grid),
    barX: art(wArt * gone, grid),
    disc: 1 - along(t, 0, ZONE_TITLE_OUT_MS / 2, stepped(4, grid, ZONE_EASE.leave)),
  }
}

// ---- M5 tally ----

export const ZONE_TALLY_ROW_STAGGER_MS = 300
export const ZONE_TALLY_COUNT_MS = 1200
export const ZONE_TALLY_STEPS = 60
export const ZONE_TALLY_OUT_MS = 300
const TALLY_ROW_ENTER = { duration: 160, from: 40 } as const

/** The whole count-up for `rows` rows: the last row's start and its count. */
export const zoneTallyCountMs = (rows: number): number =>
  Math.max(0, rows - 1) * ZONE_TALLY_ROW_STAGGER_MS + ZONE_TALLY_COUNT_MS

export interface ZoneTallyRowFrame {
  /** Offset from rest, art px to the right; the row is hidden before it enters. */
  readonly x: number
  readonly shown: boolean
  /** How far the value and its bar have counted, 0 to 1, in 60 steps. */
  readonly count: number
}

/** M5 at `t` for row `index`. A skipped count-up is `t` at or past its end. */
export function zoneTallyRowFrame(t: number, index: number, grid: ZoneMotionGrid = "art"): ZoneTallyRowFrame {
  const start = index * ZONE_TALLY_ROW_STAGGER_MS
  return {
    x: art(TALLY_ROW_ENTER.from * (1 - along(t, start, TALLY_ROW_ENTER.duration, ZONE_EASE.rows)), grid),
    shown: t >= start,
    count: along(t, start, ZONE_TALLY_COUNT_MS, steps(ZONE_TALLY_STEPS)),
  }
}

/** The tally leaving at `t`: its panel slides down off the canvas. */
export const zoneTallyOutFrame = (t: number, hArt: number, grid: ZoneMotionGrid = "art"): number =>
  art(hArt * along(t, 0, ZONE_TALLY_OUT_MS, ZONE_EASE.leave), grid)

// ---- M6 idle attract ----

export const ZONE_ATTRACT_MS = 6000
const ATTRACT = { holdUntil: 2500, outUntil: 2900, panUntil: 5600, pan: 32, blink: 1000 } as const

export interface ZoneAttractFrame {
  /** The title card's frame; absent once it has gone. */
  readonly card?: ZoneTitleFrame
  /** The stage shows under the card from the moment the card starts out. */
  readonly stage: boolean
  /** The stage's pan, art px (0 to -32). */
  readonly panX: number
  /** PRESS ANY BUTTON's blink: shown in the first half of each second. */
  readonly prompt: boolean
}

/**
 * M6 at `t` ms into one game's turn: M3 in to 700 ms, hold to 2500, out to
 * 2900, then the stage pans linearly to 5600 and holds to 6000, when the
 * next game's turn starts at 0 with its own M3.
 */
export function zoneAttractFrame(t: number, wArt: number, hArt: number, grid: ZoneMotionGrid = "art"): ZoneAttractFrame {
  const at = Math.max(0, Math.min(ZONE_ATTRACT_MS, t))
  const stage = at >= ATTRACT.holdUntil
  const card = at < ATTRACT.holdUntil ? zoneTitleInFrame(at, wArt, hArt, grid)
    : at < ATTRACT.outUntil ? zoneTitleOutFrame(at - ATTRACT.holdUntil, wArt, hArt, grid) : undefined
  const pan = along(at, ATTRACT.outUntil, ATTRACT.panUntil - ATTRACT.outUntil, ZONE_EASE.linear)
  return {
    ...(card === undefined ? {} : { card }),
    stage,
    panX: art(-ATTRACT.pan * pan, grid),
    prompt: stage && Math.floor(at / (ATTRACT.blink / 2)) % 2 === 0,
  }
}

/** A title-card frame as the custom properties ZoneTitleCard reads. */
export function zoneTitleVars(frame: ZoneTitleFrame | undefined): Readonly<Record<string, number>> {
  if (frame === undefined) {
    return { "--zone-tc-ground-x": 0, "--zone-tc-ground-y": 0, "--zone-tc-name-x": 0, "--zone-tc-bar-x": 0, "--zone-tc-disc-scale": 1 }
  }
  return {
    "--zone-tc-ground-x": frame.ground[0],
    "--zone-tc-ground-y": frame.ground[1],
    ...Object.fromEntries(frame.bands.flatMap((band, index) => [
      [`--zone-tc-band-${index}-x`, band[0]],
      [`--zone-tc-band-${index}-y`, band[1]],
    ])),
    "--zone-tc-name-x": frame.nameX,
    "--zone-tc-bar-x": frame.barX,
    "--zone-tc-disc-scale": frame.disc,
  }
}
