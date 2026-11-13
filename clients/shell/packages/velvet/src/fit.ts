/**
 * Setting a title in the display face (Big Shoulders ExtraBold, caps) so it
 * fits a box: wrapped by words, as large as the box allows. The logic cannot
 * ask Slint how wide a word is, so it carries the face's advances
 * (src/measure.ts, written from the font file) and Slint draws exactly the
 * lines it is given. A word wider than the box sets the size.
 */
import { DISPLAY_ADVANCES, STRONG_ADVANCES, TEXT_ADVANCES } from "./measure"
import { onForget } from "./memo"
import { caps } from "./words"

/** Kerning and rounding room: Slint shapes the line, the table only adds advances. */
const SAFETY = 1.03
/** Line pitch of the display face, in font sizes. */
export const DISPLAY_LINE = 0.98
/** Where the caps' top stands under a Text's own top, and how tall they are, in font sizes. */
export const DISPLAY_CAP_TOP = 0.186
export const DISPLAY_CAP_HEIGHT = 0.8

const FALLBACK_ADVANCE = 480
const widths = new Map<string, number>()

/** The width of `text` set in caps, in font sizes. */
export function capsWidth(text: string): number {
  const known = widths.get(text)
  if (known !== undefined) return known
  const width = upperWidth(caps(text))
  if (widths.size > 4000) widths.clear()
  widths.set(text, width)
  return width
}

const upperWidths = new Map<string, number>()

/** The width of a word that is in caps already, in font sizes. */
function upperWidth(upper: string): number {
  const known = upperWidths.get(upper)
  if (known !== undefined) return known
  let total = 0
  for (let i = 0; i < upper.length; i++) total += DISPLAY_ADVANCES[upper.charCodeAt(i)] ?? FALLBACK_ADVANCE
  const width = (total / 1000) * SAFETY
  if (upperWidths.size > 4000) upperWidths.clear()
  upperWidths.set(upper, width)
  return width
}

/** White space as `\s` has it, by code: a title is cut into words for every card on its way in, and a pattern is slow there. */
function isSpace(code: number): boolean {
  return code === 32 || (code >= 9 && code <= 13) || code === 0xa0 || (code > 0x167f && (code === 0x1680 || (code >= 0x2000 && code <= 0x200a) || code === 0x2028 || code === 0x2029 || code === 0x202f || code === 0x205f || code === 0x3000 || code === 0xfeff))
}

/** The words of `text`. */
function wordsOf(text: string): string[] {
  const words: string[] = []
  let start = -1
  for (let i = 0; i < text.length; i++) {
    if (isSpace(text.charCodeAt(i))) {
      if (start >= 0) {
        words.push(text.slice(start, i))
        start = -1
      }
    } else if (start < 0) start = i
  }
  if (start >= 0) words.push(text.slice(start))
  return words
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

// A fit by its text, then held to its box exactly: two boxes that differ by any amount are two fits.
interface Fitted {
  readonly width: number
  readonly height: number
  readonly max: number
  readonly min: number
  readonly most: number
  readonly fit: Fit
}
const fits = new Map<string, Fitted[]>()

/**
 * The largest size up to `max` at which `text`, wrapped by words, stands
 * inside `width` x `height`. Never under `min`: a title that cannot fit at
 * `min` is cut to the lines that do, the last one ending in an ellipsis.
 * `most` holds it to that many lines: it sets smaller before it takes another.
 */
export function fitCaps(text: string, width: number, height: number, max: number, min = 1, most = Infinity): Fit {
  let known = fits.get(text)
  if (known !== undefined) {
    for (let i = 0; i < known.length; i++) {
      const one = known[i]!
      if (one.width === width && one.height === height && one.max === max && one.min === min && one.most === most) return one.fit
    }
  }
  const fit = fitCapsOnce(text, width, height, max, min, most)
  if (known === undefined || known.length >= 8) {
    if (fits.size > 2000) fits.clear()
    known = []
    fits.set(text, known)
  }
  known.push({ width, height, max, min, most, fit })
  return fit
}

/** `fitCaps` without its memory, for a caller that keeps the answer itself (a game's card is fitted once and kept by the game). */
export function fitCapsOnce(text: string, width: number, height: number, max: number, min = 1, most = Infinity): Fit {
  // In caps from the start: the words are measured and set as they will be drawn.
  const words = wordsOf(caps(text))
  // The search works on numbers: each word's width once, a space's width between them
  // (the advance table has no kerning, so a line's width is the sum). The lines become
  // strings once, at the size found. A title is fitted for every card on its way in.
  const widths: number[] = []
  for (let i = 0; i < words.length; i++) widths.push(upperWidth(words[i]!))
  const space = upperWidth(" ")
  const holds = (size: number): boolean => {
    if (widths.length === 0) return true
    let lines = 1
    let line = widths[0]!
    let widest = 0
    for (let i = 1; i < widths.length; i++) {
      const tried = line + space + widths[i]!
      if (tried * size <= width) line = tried
      else {
        if (line > widest) widest = line
        lines++
        line = widths[i]!
      }
    }
    if (line > widest) widest = line
    return widest * size <= width && lines <= most && lines * size * DISPLAY_LINE <= height
  }
  let low = min
  let high = Math.max(min, max)
  let found: number | undefined
  if (holds(high)) found = high
  else {
    // Eight halvings: to a hundredth of the range, finer than a pixel at any size Velvet sets.
    for (let i = 0; i < 8; i++) {
      const size = (low + high) / 2
      if (holds(size)) {
        found = size
        low = size
      } else high = size
    }
  }
  let best: Fit | undefined
  if (found !== undefined) {
    // How many lines the words take at `size` in `measure`, by the same sums.
    const count = (size: number, measure: number): number => {
      let lines = 1
      let line = widths[0] ?? 0
      for (let i = 1; i < widths.length; i++) {
        const tried = line + space + widths[i]!
        if (tried * size <= measure) line = tried
        else {
          lines++
          line = widths[i]!
        }
      }
      return lines
    }
    // Balanced, as the prototype sets its display type (`text-wrap: balance`): the
    // narrowest measure that keeps the same number of lines, so no line is left short
    // under a long one ("MARIO KART WII: / RETRO REWIND", not "… RETRO / REWIND").
    let measure = width
    const many = count(found, width)
    if (many > 1) {
      let low = 0
      for (let i = 0; i < widths.length; i++) low = Math.max(low, widths[i]! * found)
      let high = width
      for (let i = 0; i < 12 && high - low > 0.5; i++) {
        const mid = (low + high) / 2
        if (count(found, mid) <= many) high = mid
        else low = mid
      }
      measure = high
    }
    // The lines at that size and measure.
    const lines: string[] = []
    let text = ""
    let line = 0
    for (let i = 0; i < words.length; i++) {
      const tried = line + space + widths[i]!
      if (text === "" ) {
        text = words[i]!
        line = widths[i]!
      } else if (tried * found <= measure) {
        text += " " + words[i]!
        line = tried
      } else {
        lines.push(text)
        text = words[i]!
        line = widths[i]!
      }
    }
    if (text !== "") lines.push(text)
    best = { size: found, lines }
  }
  if (best === undefined) {
    // Not even at the smallest size. A word wider than the box breaks where the box
    // ends; then the lines that fit are kept, and the last one says more was there.
    const pieces = words.flatMap(word => split(word, min, width))
    const lines = wrap(pieces, min, width)
    const room = Math.max(1, Math.min(most, Math.floor(height / (min * DISPLAY_LINE))))
    const kept = lines.slice(0, room)
    if (lines.length > room && kept.length > 0) kept[kept.length - 1] = cut(`${kept[kept.length - 1]}…`, min, width, true)
    best = { size: min, lines: kept.map(caps) }
  }
  return best
}

/** A word in pieces no wider than `width` at `size`: whole when it fits, else in pieces of about one length (no last letter alone). */
function split(word: string, size: number, width: number): string[] {
  const whole = capsWidth(word) * size
  if (whole <= width) return [word]
  const letters = [...word]
  // As few pieces as the widths allow, each about as long as the next.
  for (let count = Math.ceil(whole / width); count <= letters.length; count++) {
    const each = Math.ceil(letters.length / count)
    const pieces: string[] = []
    for (let at = 0; at < letters.length; at += each) pieces.push(letters.slice(at, at + each).join(""))
    if (pieces.every(piece => capsWidth(piece) * size <= width)) return pieces
  }
  return letters
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

// A width by the whole text, one memory a weight.
const textWidths = new Map<string, number>()
const strongWidths = new Map<string, number>()
onForget(() => {
  widths.clear()
  upperWidths.clear()
  fits.clear()
  textWidths.clear()
  strongWidths.clear()
})

/** The width of `text` in the text face (Archivo), in font sizes. */
export function textWidth(text: string, strong = false): number {
  const kept = strong ? strongWidths : textWidths
  const known = kept.get(text)
  if (known !== undefined) return known
  const table = strong ? STRONG_ADVANCES : TEXT_ADVANCES
  let total = 0
  for (const c of text) total += table[c.codePointAt(0)!] ?? 560
  const width = (total / 1000) * SAFETY
  if (kept.size > 4000) kept.clear()
  kept.set(text, width)
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
