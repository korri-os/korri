/**
 * Setting a title in the display face (Big Shoulders ExtraBold, caps) so it
 * fits a box: wrapped by words, as large as the box allows. The logic cannot
 * ask Slint how wide a word is, so it carries the face's advances
 * (src/measure.ts, written from the font file) and Slint draws exactly the
 * lines it is given. A word wider than the box sets the size.
 */
import { DISPLAY_ADVANCES, STRONG_ADVANCES, TEXT_ADVANCES } from "./measure"
import { caps } from "./words"

/** Kerning and rounding room: Slint shapes the line, the table only adds advances. */
const SAFETY = 1.03
/** Line pitch of the display face, in font sizes. */
export const DISPLAY_LINE = 0.9
/** Where the caps' top stands under a Text's own top, and how tall they are, in font sizes. */
export const DISPLAY_CAP_TOP = 0.186
export const DISPLAY_CAP_HEIGHT = 0.8

const FALLBACK_ADVANCE = 480
const widths = new Map<string, number>()

/** The width of `text` set in caps, in font sizes. */
export function capsWidth(text: string): number {
  const known = widths.get(text)
  if (known !== undefined) return known
  let total = 0
  for (const c of caps(text)) total += DISPLAY_ADVANCES[c.codePointAt(0)!] ?? FALLBACK_ADVANCE
  const width = (total / 1000) * SAFETY
  if (widths.size > 4000) widths.clear()
  widths.set(text, width)
  return width
}

export interface Fit {
  /** The font size, in the box's units. */
  readonly size: number
  /** The lines, in caps. */
  readonly lines: readonly string[]
}

function wrap(words: readonly string[], size: number, width: number): string[] {
  const lines: string[] = []
  let line = ""
  for (const word of words) {
    const tried = line === "" ? word : `${line} ${word}`
    if (line === "" || capsWidth(tried) * size <= width) line = tried
    else {
      lines.push(line)
      line = word
    }
  }
  if (line !== "") lines.push(line)
  return lines
}

const fits = new Map<string, Fit>()

/**
 * The largest size up to `max` at which `text`, wrapped by words, stands
 * inside `width` x `height`. Never under `min`: a title that cannot fit at
 * `min` is cut to the lines that do, the last one ending in an ellipsis.
 */
export function fitCaps(text: string, width: number, height: number, max: number, min = 1): Fit {
  const key = `${text}|${width.toFixed(1)}|${height.toFixed(1)}|${max.toFixed(1)}|${min}`
  const known = fits.get(key)
  if (known !== undefined) return known
  const words = text.trim().split(/\s+/).filter(word => word !== "")
  const holds = (size: number, lines: readonly string[]) =>
    lines.every(line => capsWidth(line) * size <= width) && lines.length * size * DISPLAY_LINE <= height
  let low = min
  let high = Math.max(min, max)
  let best: Fit | undefined
  if (holds(high, wrap(words, high, width))) best = { size: high, lines: wrap(words, high, width).map(caps) }
  else {
    for (let i = 0; i < 12; i++) {
      const size = (low + high) / 2
      const lines = wrap(words, size, width)
      if (holds(size, lines)) {
        best = { size, lines: lines.map(caps) }
        low = size
      } else high = size
    }
  }
  if (best === undefined) {
    // Not even at the smallest size. A word wider than the box breaks where the box
    // ends; then the lines that fit are kept, and the last one says more was there.
    const pieces = words.flatMap(word => split(word, min, width))
    const lines = wrap(pieces, min, width)
    const room = Math.max(1, Math.floor(height / (min * DISPLAY_LINE)))
    const kept = lines.slice(0, room)
    if (lines.length > room && kept.length > 0) kept[kept.length - 1] = cut(`${kept[kept.length - 1]}…`, min, width, true)
    best = { size: min, lines: kept.map(caps) }
  }
  if (fits.size > 2000) fits.clear()
  fits.set(key, best)
  return best
}

/** A word in pieces no wider than `width` at `size`: whole when it fits. */
function split(word: string, size: number, width: number): string[] {
  if (capsWidth(word) * size <= width) return [word]
  const pieces: string[] = []
  let piece = ""
  for (const c of word) {
    if (piece !== "" && capsWidth(piece + c) * size > width) {
      pieces.push(piece)
      piece = c
    } else piece += c
  }
  if (piece !== "") pieces.push(piece)
  return pieces
}

/** `line` cut to `width` at `size`, ending in an ellipsis when it was cut (or already ends in one). */
function cut(line: string, size: number, width: number, marked = false): string {
  if (capsWidth(line) * size <= width) return line
  let text = marked ? line.slice(0, -1) : line
  while (text.length > 1 && capsWidth(`${text}…`) * size > width) text = text.slice(0, -1)
  return `${text.trimEnd()}…`
}

/** The height a fitted title takes: its caps from the first line's top to the last line's foot. */
export const fitHeight = (fit: Fit): number =>
  fit.lines.length === 0 ? 0 : (fit.lines.length - 1) * fit.size * DISPLAY_LINE + fit.size * DISPLAY_CAP_HEIGHT

const textWidths = new Map<string, number>()

/** The width of `text` in the text face (Archivo), in font sizes. */
export function textWidth(text: string, strong = false): number {
  const key = strong ? `\u0001${text}` : text
  const known = textWidths.get(key)
  if (known !== undefined) return known
  const table = strong ? STRONG_ADVANCES : TEXT_ADVANCES
  let total = 0
  for (const c of text) total += table[c.codePointAt(0)!] ?? 560
  const width = (total / 1000) * SAFETY
  if (textWidths.size > 4000) textWidths.clear()
  textWidths.set(key, width)
  return width
}

/**
 * Words in the text face broken into lines no wider than `width` at `size`.
 * A word wider than the line stands alone on it (Slint cuts it with an ellipsis).
 */
export function breakLines(text: string, size: number, width: number, strong = false): string[] {
  const out: string[] = []
  for (const paragraph of text.split("\n")) {
    let line = ""
    for (const word of paragraph.split(/\s+/).filter(part => part !== "")) {
      const tried = line === "" ? word : `${line} ${word}`
      if (line === "" || textWidth(tried, strong) * size <= width) line = tried
      else {
        out.push(line)
        line = word
      }
    }
    out.push(line)
  }
  return out
}
