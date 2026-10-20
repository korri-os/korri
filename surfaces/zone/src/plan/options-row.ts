/**
 * How one option row lays itself out: Mania's OPTIONS row (a label panel,
 * a value panel) on the art grid. Pure, so the row's measuring hook
 * (src/use-zone-option-row.ts) and the tests agree on every threshold.
 *
 *  - The row reflows label over value at or under ZONE_OPTION_ROW_STACK_AT
 *    art px wide. That is a 240-art-px screen (the brief's narrow) less the
 *    band's inset on each side: a row measures itself, not the screen.
 *  - Its panels lean at 45 degrees while they hold one line (slant = the
 *    cell's height); a cell that wraps keeps the one-line slant, stepped
 *    over its whole height, so its words never meet the edge.
 *  - A fact that does not fit is cut: a path in the middle, keeping its
 *    last segment whole for as long as it can, anything else at the end.
 */
import { zoneStairClip, type ZoneStairEdge } from "./stair-clip"

/** The band's inset on each side of the rows. Art px. Held equal to `--zone-options-band-inset`. */
export const ZONE_OPTIONS_BAND_INSET = 8

/** The brief's narrow screen: 240 art px wide or less. */
export const ZONE_OPTIONS_NARROW = 240

/** A row this wide or narrower (art px) stacks its label over its value. */
export const ZONE_OPTION_ROW_STACK_AT = ZONE_OPTIONS_NARROW - 2 * ZONE_OPTIONS_BAND_INSET

/** A row cell's height for one line: a 10-art-px cap and 2 art px over and under it. Held equal to `--zone-option-cell`. */
export const ZONE_OPTION_CELL = 14

/** The slant of a one-line cell: 45 degrees, its height less one row. */
export const ZONE_OPTION_SLANT = ZONE_OPTION_CELL - 1

/** Whether a row `wArt` art px wide stacks its label over its value. */
export function optionRowStacked(wArt: number): boolean {
  return wArt > 0 && wArt <= ZONE_OPTION_ROW_STACK_AT
}

/**
 * A cell's stair polygon: a 45 degree lean for one line, and no more than
 * the one-line slant for a taller cell. `zoneStairClip` bounds its run by
 * the width it is told, and writes the far side from 100%, so telling it a
 * width of one slant per slanted edge caps the run without changing the
 * shape's width.
 */
export function zoneOptionClip(
  wArt: number,
  hArt: number,
  edges: { readonly left?: ZoneStairEdge; readonly right?: ZoneStairEdge } = {},
): string {
  const slanted = (edges.left === "straight" ? 0 : 1) + (edges.right === "straight" ? 0 : 1)
  return zoneStairClip(Math.min(wArt, slanted * ZONE_OPTION_SLANT + 1), hArt, edges)
}

/** A fact that reads as a path ("/nix/store/…", "~/roms"): cut in its middle. */
export const isPathFact = (text: string): boolean => /^[/~]/.test(text)

/** How a fact that does not fit is cut. */
export type ZoneCutMode = "end" | "middle"

export const zoneCutMode = (text: string): ZoneCutMode => (isPathFact(text) ? "middle" : "end")

/**
 * `text` cut to keep at most `keep` of its characters, with "…" where it
 * was cut. `end` keeps the start. `middle` keeps the last path segment
 * whole and as much of the start as is left ("/nix/st…8n1x2-retroarch-1.19.1");
 * when even the last segment is too long, it keeps that segment's tail
 * from a `-` or `.` in it, so a name stays whole ("…retroarch-1.19.1"), and
 * only when no such tail fits does it cut inside one. `keep` at or over
 * the length is the text itself.
 */
export function zoneCutValue(text: string, keep: number, mode: ZoneCutMode): string {
  const chars = [...text]
  const kept = Math.max(0, Math.floor(keep))
  if (kept >= chars.length) return text
  if (mode === "end") return `${chars.slice(0, kept).join("")}…`
  const slash = text.lastIndexOf("/")
  const tail = slash < 0 ? chars.length : [...text.slice(slash + 1)].length
  if (kept <= tail) {
    const segment = chars.slice(chars.length - tail)
    const whole = segment
      .map((char, at) => (at > 0 && /[-.]/.test(segment[at - 1] ?? "") ? segment.length - at : 0))
      .filter(length => length > 0 && length <= kept)
    const length = whole.length > 0 ? Math.max(...whole) : kept
    return `…${chars.slice(chars.length - length).join("")}`
  }
  return `${chars.slice(0, kept - tail).join("")}…${chars.slice(chars.length - tail).join("")}`
}

/**
 * The longest cut of `text` that `fits`, by halving. `fits` is a
 * measurement (the row asks the DOM); the search asks it about log2 of the
 * length times. The whole text is returned when it fits.
 */
export function zoneLongestCut(text: string, mode: ZoneCutMode, fits: (candidate: string) => boolean): string {
  if (fits(text)) return text
  let low = 0
  let high = [...text].length - 1
  while (low < high) {
    const mid = Math.ceil((low + high) / 2)
    if (fits(zoneCutValue(text, mid, mode))) low = mid
    else high = mid - 1
  }
  return zoneCutValue(text, low, mode)
}

/**
 * The whole fact as the focused row's whole line writes it: a path may
 * wrap after each `/`, so a long one breaks between its segments rather
 * than inside one.
 */
export const zoneWholeLine = (text: string): string => (isPathFact(text) ? text.replaceAll("/", "/\u200b") : text)
