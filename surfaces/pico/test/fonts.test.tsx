/**
 * Pico's faces: ten free pixel fonts with real upper and lower case, Tiny5
 * unless the root names another.
 *
 * Each face is a glyph table in src/fonts/, read once out of its upstream file
 * by scripts/extract-font-tables.py, and built into a woff2 by
 * scripts/build-fonts.py. The builder is deterministic, so the checksums below
 * pin bytes, not appearance: a changed checksum means someone changed a table,
 * the symbols, or the builder, and the pin makes that a decision rather than an
 * accident. Look at the rendered text when changing any of them.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { cleanup, render } from "@testing-library/react"
import { createHash } from "node:crypto"
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs"
import { join, relative } from "node:path"
import { parseModule, renderedText, visitNodes } from "./source-ast"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { PICO_DEFAULT_FONT, PICO_FONTS } from "../src/pico-fonts"
import { PicoSurface } from "../src/PicoSurface"

afterEach(() => cleanup())

const SRC = join(import.meta.dir, "..", "src")
const FONTS = join(SRC, "fonts")
const read = (file: string) => readFileSync(file, "utf8")

const PINNED: Record<string, string> = {
  "tiny5.woff2": "0821105c4e9a78ae047d13af40b716bf534c48d9d926b9c6bc9ab2ba12966f52",
  "tom-thumb.woff2": "0368aeefea6928bed561413bdcb53e29724a9a4b53622e07d1f326b3eea9d09c",
  "x11-4x6.woff2": "b12aa5da65ed54c929741edfdd509ded79d6deac4968e9a467236fcd622e6459",
  "kenney-mini.woff2": "1b3f8c579d8003e859eaa712a45256c8745eb15822deae6ec04a50fc6a456da3",
  "m3x6.woff2": "9e2e34712bbc52fed03be04f00b033a7ff060486930b4d3967441b9ab16b668b",
  "x11-5x7.woff2": "4c63f29fa0e513386c6c501ecf58cc1045fc59bf1899046228afd103113a391b",
  "spleen-5x8.woff2": "46986778e41acdebcfa1db0133e8f281ab16086020d4f59acab648e2fe52c18e",
  "m5x7.woff2": "abd2c9cc5065ce7868a66add052be6aa2a3526fa432aa44e0cdb49ec8d423a0a",
  "monogram.woff2": "2932e1a034a6ec9aa5fb2eda3174bc7828dff113886fc0a40b06f3fe9b503bfb",
  "kenney-pixel.woff2": "c1b1f5d3e1ec84041ee3bdeae7d0e0109dd84f59b8badd0e3b4f0d641596b9eb",
}

interface Table {
  readonly ascent: number
  readonly descent: number
  readonly glyphs: ReadonlyMap<number, readonly string[]>
}

/** The table format both scripts share; see src/fonts/README.md. */
function parseTable(file: string): Table {
  const lines = read(file).split("\n")
  let ascent = 0
  let descent = 0
  const glyphs = new Map<number, readonly string[]>()
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i] ?? ""
    const metrics = line.match(/^metrics ascent (\d+) descent (\d+)$/)
    if (metrics) {
      ascent = Number(metrics[1])
      descent = Number(metrics[2])
      continue
    }
    const glyph = line.match(/^([0-9A-F]{4,5}) /)
    if (!glyph) continue
    const rows = lines.slice(i + 1, i + 1 + ascent + descent)
    glyphs.set(Number.parseInt(glyph[1] ?? "", 16), rows)
    i += ascent + descent
  }
  return { ascent, descent, glyphs }
}

describe("the faces Pico offers", () => {
  test("are the ten from the survey, Tiny5 first and by default", () => {
    expect(PICO_FONTS.map((font) => font.id)).toEqual([
      "tiny5",
      "tom-thumb",
      "x11-4x6",
      "kenney-mini",
      "m3x6",
      "x11-5x7",
      "spleen-5x8",
      "m5x7",
      "monogram",
      "kenney-pixel",
    ])
    expect(PICO_DEFAULT_FONT).toBe("tiny5")
  })

  test.each(PICO_FONTS.map((font) => [font.id, font] as const))(
    "%s has its table, its woff2 and, where its licence asks, the licence text",
    (id, font) => {
      expect(existsSync(join(FONTS, `${id}.glyphs.txt`))).toBe(true)
      expect(readFileSync(join(FONTS, `${id}.woff2`)).subarray(0, 4).toString()).toBe("wOF2")
      if (font.licenseFile !== undefined) {
        expect(existsSync(join(FONTS, "licenses", font.licenseFile))).toBe(true)
      }
    },
  )

  test("every woff2 matches its pinned bytes, and nothing unpinned ships", () => {
    const shipped = readdirSync(FONTS).filter((file) => file.endsWith(".woff2")).sort()
    expect(shipped).toEqual(Object.keys(PINNED).sort())
    for (const [file, sha] of Object.entries(PINNED)) {
      expect(createHash("sha256").update(readFileSync(join(FONTS, file))).digest("hex")).toBe(sha)
    }
  })

  test("PICO-8's face is gone", () => {
    expect(existsSync(join(FONTS, "pico8.woff2"))).toBe(false)
    expect(existsSync(join(FONTS, "pico8-glyphs.txt"))).toBe(false)
    const css = readdirSync(SRC, { recursive: true })
      .map(String)
      .filter((file) => file.endsWith(".css"))
      .filter((file) => /"Pico8"/.test(read(join(SRC, file))))
    expect(css).toEqual([])
  })
})

describe("every table", () => {
  const tables = PICO_FONTS.map((font) => [font.id, parseTable(join(FONTS, `${font.id}.glyphs.txt`))] as const)

  test.each(tables)("%s holds every printable ASCII character", (_, table) => {
    const missing: string[] = []
    for (let code = 0x20; code < 0x7f; code += 1) {
      if (!table.glyphs.has(code)) missing.push(String.fromCharCode(code))
    }
    expect(missing).toEqual([])
  })

  test.each(tables)("%s draws each capital apart from its small letter", (_, table) => {
    const same: string[] = []
    for (let code = 0x41; code <= 0x5a; code += 1) {
      const upper = table.glyphs.get(code)?.join("\n")
      const lower = table.glyphs.get(code + 32)?.join("\n")
      if (upper === lower) same.push(String.fromCharCode(code))
    }
    expect(same).toEqual([])
  })

  test.each(tables)("%s keeps every row the height its metrics state", (_, table) => {
    const rows = table.ascent + table.descent
    const bad = [...table.glyphs.entries()]
      .filter(([, glyph]) => glyph.length !== rows || glyph.some((row) => !/^[#.]+$/.test(row)))
      .map(([code]) => code.toString(16))
    expect(bad).toEqual([])
  })
})

describe("every character Pico writes itself has a glyph", () => {
  /**
   * Korri's copy can hold anything; Pico's own copy cannot be allowed to fall
   * back to another face mid-word. Every non-ASCII character in a string Pico
   * renders must be in Pico's symbols table, which the builder adds to every
   * face that lacks it. Comments do not render and are not scanned.
   */
  test("each non-ASCII character in Pico's strings is in pico-symbols.txt", () => {
    const symbols = parseTable(join(FONTS, "pico-symbols.txt"))
    const files = readdirSync(SRC, { recursive: true })
      .map(String)
      .filter((file) => /\.tsx?$/.test(file) && !file.endsWith(".part.tsx") && !file.startsWith("fixtures"))
    const missing = new Set<string>()
    for (const file of files) {
      visitNodes(parseModule(file, read(join(SRC, file))), node => {
        for (const character of renderedText(node) ?? "") {
          const code = character.codePointAt(0) ?? 0
          if (code > 0x7e && !symbols.glyphs.has(code)) missing.add(`${character} in ${file}`)
        }
      })
    }
    expect([...missing].sort()).toEqual([])
  })
})

describe("the root chooses the face", () => {
  const tokens = read(join(SRC, "pico-tokens.css"))
  const fontsCss = read(join(SRC, "pico-fonts.css"))
  const defaultFont = PICO_FONTS.find((font) => font.id === PICO_DEFAULT_FONT)!

  test("with no face named, the tokens use the default face and its rows", () => {
    const block = tokens.match(/:where\(\.pico-theme\)\s*\{[^}]*\}/s)?.[0] ?? ""
    expect(block).toContain(`--pico-font: "${defaultFont.family}", monospace;`)
    expect(block).toContain(`--pico-font-rows: ${defaultFont.rows};`)
  })

  test.each(PICO_FONTS.map((font) => [font.id, font] as const))(
    "a root naming %s gets its family, its rows and its file",
    (id, font) => {
      const rule = fontsCss.match(new RegExp(`\\[data-pico-font="${id}"\\]\\s*\\{[^}]*\\}`, "s"))?.[0] ?? ""
      expect(rule).toContain(`--pico-font: "${font.family}", monospace;`)
      expect(rule).toContain(`--pico-font-rows: ${font.rows};`)
      expect(fontsCss).toContain(`url("./fonts/${id}.woff2")`)
    },
  )

  test("type sizes are whole glyph pixels of the chosen face, not of a six-row em", () => {
    const block = tokens.match(/\.pico-theme > \*\s*\{[^}]*\}/s)?.[0] ?? ""
    for (const size of ["--pico-text-sm", "--pico-text", "--pico-text-lg", "--pico-text-xl"]) {
      const line = block.split("\n").find((candidate) => candidate.trim().startsWith(`${size}:`)) ?? ""
      expect(line).toContain("var(--pico-font-rows)")
    }
  })

  test("the surface names the default face, and the one it is given", () => {
    const host = createFixtureHost()
    const plain = render(<PicoSurface host={host} model={fixtureModel} />)
    expect(plain.container.querySelector(".pico-theme")?.getAttribute("data-pico-font")).toBe(PICO_DEFAULT_FONT)
    cleanup()
    const named = render(<PicoSurface font="m5x7" host={host} model={fixtureModel} />)
    expect(named.container.querySelector(".pico-theme")?.getAttribute("data-pico-font")).toBe("m5x7")
  })
})

describe("no stray source files", () => {
  test("font sources stay out of the surface: only tables, woff2, licences and the README", () => {
    const stray = readdirSync(FONTS, { recursive: true })
      .map(String)
      .filter((file) => statSync(join(FONTS, file)).isFile())
      .filter((file) => !/(\.glyphs\.txt|\.woff2|^pico-symbols\.txt|^README\.md|^licenses\/.+\.txt)$/.test(file))
      .map((file) => relative(FONTS, join(FONTS, file)))
    expect(stray).toEqual([])
  })
})
