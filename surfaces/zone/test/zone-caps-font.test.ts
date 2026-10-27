/**
 * Zone Caps is the grid, built: every character the brief lists exists, the
 * outlines in the committed font are exactly the pixels drawn in
 * src/fonts/zone-caps.grid, the advance widths follow the spec (so the
 * digits are tabular), and the build script rebuilds the same font from the
 * same grid.
 *
 * The font is read with a small TrueType reader here (head, hhea, maxp,
 * cmap format 4, loca, glyf, hmtx), not by the build tool, so a build that
 * drifted from the grid fails even if the tool agrees with itself.
 */
import { describe, expect, test } from "bun:test"
import { spawnSync } from "node:child_process"
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"

const ROOT = join(import.meta.dir, "..")
const FONTS = join(ROOT, "src", "fonts")
const PIXEL = 100
const CAP = 10
const ASCENT = 13

// ---- the grid ----

interface Drawn {
  readonly rows: string[]
  readonly top: number
  readonly advance?: number
}

interface Char {
  readonly ink: ReadonlySet<string>
  readonly advance: number
  readonly width: number
}

const codeOf = (token: string): number =>
  token.startsWith("U+") ? Number.parseInt(token.slice(2), 16) : (token.codePointAt(0) ?? 0)

const optionsOf = (tokens: readonly string[]): Record<string, number> =>
  Object.fromEntries(tokens.filter(token => token.includes("=")).map(token => {
    const [key = "", value = "0"] = token.split("=")
    return [key, Number(value)]
  }))

const pixelsOf = (rows: readonly string[], top: number, dx = 0): Set<string> => {
  const found = new Set<string>()
  rows.forEach((row, r) => [...row].forEach((cell, c) => {
    if (cell === "#") found.add(`${c + dx},${top - r}`)
  }))
  return found
}

function readGrid(): Map<number, Char> {
  const glyphs = new Map<number, Drawn>()
  const marks = new Map<string, Drawn>()
  const composes: { code: number; base: number; mark?: string; dx?: number }[] = []
  let current: Drawn | undefined
  for (const raw of readFileSync(join(FONTS, "zone-caps.grid"), "utf8").split("\n")) {
    const line = raw.trimEnd()
    if (line !== "" && /^[#.]+$/.test(line)) {
      current?.rows.push(line)
      continue
    }
    current = undefined
    if (line === "" || line.startsWith(";")) continue
    const words = line.split(/\s+/)
    const options = optionsOf(words.slice(2))
    if (words[0] === "glyph") {
      current = { rows: [], top: options.top ?? CAP, advance: options.advance }
      glyphs.set(codeOf(words[1] ?? ""), current)
    } else if (words[0] === "mark") {
      current = { rows: [], top: options.top ?? ASCENT }
      marks.set(words[1] ?? "", current)
    } else if (words[0] === "compose") {
      const rest = words.slice(3).filter(word => !word.includes("="))
      composes.push({ code: codeOf(words[1] ?? ""), base: codeOf(words[2] ?? ""), mark: rest[0], dx: optionsOf(words.slice(3)).dx })
    }
  }
  const chars = new Map<number, Char>()
  for (const [code, glyph] of glyphs) {
    const width = glyph.rows[0]?.length ?? 0
    chars.set(code, { ink: pixelsOf(glyph.rows, glyph.top), advance: glyph.advance ?? width + 1, width })
  }
  for (const { code, base, mark, dx } of composes) {
    const source = chars.get(base)
    if (source === undefined) throw new Error(`U+${code.toString(16)}: no base`)
    const ink = new Set(source.ink)
    if (mark !== undefined) {
      const drawn = marks.get(mark)
      if (drawn === undefined) throw new Error(`no mark ${mark}`)
      const offset = dx ?? Math.floor((source.width - (drawn.rows[0]?.length ?? 0)) / 2)
      for (const pixel of pixelsOf(drawn.rows, drawn.top, offset)) ink.add(pixel)
    }
    chars.set(code, { ink, advance: source.advance, width: source.width })
  }
  return chars
}

// ---- the font ----

interface Font {
  readonly unitsPerEm: number
  readonly ascent: number
  readonly descent: number
  readonly cmap: ReadonlyMap<number, number>
  /** Ink pixels per glyph id, read back from each rectangle contour. */
  outline(glyph: number): Set<string>
  advance(glyph: number): number
}

function readFont(bytes: Uint8Array): Font {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  const tables = new Map<string, number>()
  const count = view.getUint16(4)
  for (let i = 0; i < count; i += 1) {
    const at = 12 + i * 16
    const tag = String.fromCharCode(...bytes.slice(at, at + 4))
    tables.set(tag, view.getUint32(at + 8))
  }
  const table = (tag: string): number => {
    const offset = tables.get(tag)
    if (offset === undefined) throw new Error(`no ${tag} table`)
    return offset
  }
  const head = table("head")
  const unitsPerEm = view.getUint16(head + 18)
  const longLoca = view.getInt16(head + 50) === 1
  const hhea = table("hhea")
  const ascent = view.getInt16(hhea + 4)
  const descent = view.getInt16(hhea + 6)
  const metrics = view.getUint16(hhea + 34)
  const glyphs = view.getUint16(table("maxp") + 4)

  const cmap = new Map<number, number>()
  const cmapAt = table("cmap")
  for (let i = 0; i < view.getUint16(cmapAt + 2); i += 1) {
    const sub = cmapAt + view.getUint32(cmapAt + 4 + i * 8 + 4)
    if (view.getUint16(sub) !== 4) continue
    const segments = view.getUint16(sub + 6) / 2
    const ends = sub + 14
    const starts = ends + segments * 2 + 2
    const deltas = starts + segments * 2
    const ranges = deltas + segments * 2
    for (let s = 0; s < segments; s += 1) {
      const end = view.getUint16(ends + s * 2)
      const start = view.getUint16(starts + s * 2)
      const delta = view.getInt16(deltas + s * 2)
      const range = view.getUint16(ranges + s * 2)
      for (let c = start; c <= end && c !== 0xffff; c += 1) {
        const glyph = range === 0
          ? (c + delta) & 0xffff
          : view.getUint16(ranges + s * 2 + range + (c - start) * 2)
        if (glyph !== 0) cmap.set(c, glyph)
      }
    }
  }

  const loca = table("loca")
  const glyf = table("glyf")
  const locate = (glyph: number): [number, number] => longLoca
    ? [view.getUint32(loca + glyph * 4), view.getUint32(loca + glyph * 4 + 4)]
    : [view.getUint16(loca + glyph * 2) * 2, view.getUint16(loca + glyph * 2 + 2) * 2]

  const outline = (glyph: number): Set<string> => {
    const [start, end] = locate(glyph)
    const ink = new Set<string>()
    if (end === start) return ink
    const at = glyf + start
    const contours = view.getInt16(at)
    if (contours < 0) throw new Error(`glyph ${glyph} is a composite`)
    const endPoints = Array.from({ length: contours }, (_, i) => view.getUint16(at + 10 + i * 2))
    const points = (endPoints.at(-1) ?? -1) + 1
    let p = at + 10 + contours * 2
    p += 2 + view.getUint16(p)
    const flags: number[] = []
    while (flags.length < points) {
      const flag = view.getUint8(p++)
      flags.push(flag)
      if (flag & 8) for (let n = view.getUint8(p++); n > 0; n -= 1) flags.push(flag)
    }
    const coordinates = (short: number, same: number): number[] => {
      let value = 0
      return flags.map(flag => {
        if (flag & short) {
          const step = view.getUint8(p++)
          value += flag & same ? step : -step
        } else if (!(flag & same)) {
          value += view.getInt16(p)
          p += 2
        }
        return value
      })
    }
    const xs = coordinates(2, 16)
    const ys = coordinates(4, 32)
    let first = 0
    for (const last of endPoints) {
      const cx = xs.slice(first, last + 1)
      const cy = ys.slice(first, last + 1)
      if (cx.length !== 4) throw new Error(`glyph ${glyph}: a contour of ${cx.length} points is not a pixel run`)
      const [x0, x1, y0, y1] = [Math.min(...cx), Math.max(...cx), Math.min(...cy), Math.max(...cy)]
      if ([x0, x1, y0, y1].some(v => v % PIXEL !== 0) || y1 - y0 !== PIXEL) {
        throw new Error(`glyph ${glyph}: a contour off the pixel grid`)
      }
      for (let x = x0; x < x1; x += PIXEL) ink.add(`${x / PIXEL},${y1 / PIXEL}`)
      first = last + 1
    }
    return ink
  }

  const hmtx = table("hmtx")
  const advance = (glyph: number): number => view.getUint16(hmtx + Math.min(glyph, metrics - 1) * 4)
  if (glyphs <= 1) throw new Error("no glyphs")
  return { unitsPerEm, ascent, descent, cmap, outline, advance }
}

const grid = readGrid()
const ttf = readFileSync(join(FONTS, "ZoneCaps.ttf"))
const font = readFont(ttf)

// ---- the brief's coverage list, section 2a ----

const range = (from: number, to: number): number[] => Array.from({ length: to - from + 1 }, (_, i) => from + i)
const LETTERS = range(0x41, 0x5a)
const DIGITS = range(0x30, 0x39)
const PUNCTUATION = [..."'\":.,-/%?!&+()#@…×▲"].map(char => char.codePointAt(0) ?? 0)
/** À to Þ: every Latin-1 capital, × included. */
const LATIN_1 = range(0xc0, 0xde)
/** Every capital in Latin Extended-A: the even code points, and the odd ones the block's order makes capitals. */
const LATIN_EXT_A = [
  ...range(0x0100, 0x0137).filter(code => code % 2 === 0),
  ...range(0x0139, 0x0148).filter(code => code % 2 === 1),
  ...range(0x014a, 0x0177).filter(code => code % 2 === 0),
  0x0178, 0x0179, 0x017b, 0x017d,
].filter(code => code !== 0x0138)

describe("Zone Caps covers the brief's list", () => {
  test.each([
    ["A to Z", LETTERS],
    ["0 to 9", DIGITS],
    ["the punctuation", PUNCTUATION],
    ["Latin-1 capitals", LATIN_1],
    ["Latin Extended-A capitals", LATIN_EXT_A],
  ] as const)("%s", (_, codes) => {
    const missing = codes.filter(code => !font.cmap.has(code)).map(code => `U+${code.toString(16).toUpperCase().padStart(4, "0")}`)
    expect(missing).toEqual([])
  })

  test("space and no-break space are 4 art px", () => {
    for (const code of [0x20, 0xa0]) expect(font.advance(font.cmap.get(code) ?? 0)).toBe(4 * PIXEL)
  })
})

describe("the metrics follow the spec", () => {
  test("the em is the 10-px cap; 3 px of headroom over it, 3 px of descent under it", () => {
    expect(font.unitsPerEm).toBe(CAP * PIXEL)
    expect(font.ascent).toBe(ASCENT * PIXEL)
    expect(font.descent).toBe(-3 * PIXEL)
  })

  test("a letter is 7 art px wide, M and W 9, I 3; every digit 7, so the digits are tabular", () => {
    const wide = new Set([0x4d, 0x57])
    for (const code of LETTERS) {
      const expected = wide.has(code) ? 9 : code === 0x49 ? 3 : 7
      expect([String.fromCodePoint(code), font.advance(font.cmap.get(code) ?? 0)]).toEqual([String.fromCodePoint(code), expected * PIXEL])
    }
    expect(new Set(DIGITS.map(code => font.advance(font.cmap.get(code) ?? 0)))).toEqual(new Set([7 * PIXEL]))
  })

  test("every letter and digit stands on the baseline and reaches the cap, Q's tail alone below", () => {
    for (const code of [...LETTERS, ...DIGITS]) {
      const ys = [...(grid.get(code)?.ink ?? [])].map(pixel => Number(pixel.split(",")[1]))
      expect([String.fromCodePoint(code), Math.max(...ys)]).toEqual([String.fromCodePoint(code), CAP])
      expect([String.fromCodePoint(code), Math.min(...ys)]).toEqual([String.fromCodePoint(code), code === 0x51 ? 0 : 1])
    }
  })

  test("strokes are 2 px: every stemmed letter has a 2-px stem down its whole left edge", () => {
    const stems = [..."BDEFHKLMNPR"].map(char => char.codePointAt(0) ?? 0)
    for (const code of stems) {
      const ink = grid.get(code)?.ink ?? new Set()
      for (let y = 1; y <= CAP; y += 1) expect([String.fromCodePoint(code), y, ink.has(`0,${y}`) && ink.has(`1,${y}`)]).toEqual([String.fromCodePoint(code), y, true])
    }
  })

  test("round letters lose one pixel at each outer corner", () => {
    for (const char of "OQCGS0689") {
      const ink = grid.get(char.codePointAt(0) ?? 0)?.ink ?? new Set()
      expect([char, ink.has(`0,${CAP}`), ink.has("5,10"), ink.has("0,1")]).toEqual([char, false, false, false])
    }
  })
})

describe("the font is the grid, built", () => {
  test("every character in the grid is in the font with exactly the grid's pixels and advance", () => {
    const wrong: string[] = []
    for (const [code, char] of grid) {
      const glyph = font.cmap.get(code)
      if (glyph === undefined) {
        wrong.push(`U+${code.toString(16)} missing`)
        continue
      }
      const drawn = [...char.ink].sort()
      const built = [...font.outline(glyph)].sort()
      if (drawn.join(" ") !== built.join(" ")) wrong.push(`U+${code.toString(16)} outline`)
      if (font.advance(glyph) !== char.advance * PIXEL) wrong.push(`U+${code.toString(16)} advance`)
    }
    expect(wrong).toEqual([])
    expect(font.cmap.size).toBe(grid.size)
  })

  test("the woff2 the stylesheet embeds sits beside the ttf", () => {
    expect(existsSync(join(FONTS, "ZoneCaps.woff2"))).toBe(true)
    expect(readFileSync(join(ROOT, "src", "zone-fonts.css"), "utf8")).toContain("./fonts/ZoneCaps.woff2")
  })

  /**
   * The build is reproducible: the script, run into an empty folder, writes
   * the same bytes and the same outlines. It needs nix-shell (the script's
   * own shebang fetches fonttools); a machine without Nix skips it, and says
   * so in the test name.
   */
  const nixShell = spawnSync("sh", ["-c", "command -v nix-shell"]).status === 0
  test.skipIf(!nixShell)("a rebuild in a temporary folder writes the same font (needs nix-shell)", () => {
    const out = mkdtempSync(join(tmpdir(), "zone-caps-"))
    try {
      const run = spawnSync(join(ROOT, "scripts", "build-zone-caps.py"), ["--out", out], { cwd: ROOT, encoding: "utf8", timeout: 240_000 })
      expect(run.status, `font builder failed:\n${run.stderr}\n${run.stdout}\n${run.error ?? ""}`).toBe(0)
      const rebuilt = readFileSync(join(out, "ZoneCaps.ttf"))
      const again = readFont(rebuilt)
      for (const [code, glyph] of font.cmap) {
        expect([code, [...again.outline(again.cmap.get(code) ?? 0)].sort()]).toEqual([code, [...font.outline(glyph)].sort()])
      }
      expect(Buffer.compare(rebuilt, ttf)).toBe(0)
      expect(Buffer.compare(readFileSync(join(out, "ZoneCaps.woff2")), readFileSync(join(FONTS, "ZoneCaps.woff2")))).toBe(0)
    } finally {
      rmSync(out, { recursive: true, force: true })
    }
  }, 250_000)
})
