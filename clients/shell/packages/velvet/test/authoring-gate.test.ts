/**
 * Authoring: what Velvet's sources may and may not hold.
 *
 * Views (.slint):
 *  - A raw colour (`#rrggbb`) appears only in ui/tokens.slint.
 *  - A pixel literal appears only in ui/tokens.slint. `0px` and `1px` are
 *    the two a component may write: nothing, and the unit that turns the
 *    plan's numbers into lengths. A preview's window size is the other.
 *  - No blur and no drop shadow, live or not: the shell's software renderer
 *    has neither, and a surface that asks for them looks different on every
 *    device. No rotation and no conic gradient, for the same reason.
 *  - No colour in the tokens is a saturated blue (the owner's taste: it
 *    reads as corporate).
 *  - An animation is of an element's own geometry, opacity or ground (the
 *    interpreter loses the value of an animated custom property; README,
 *    "What the interpreter taught us").
 *
 * Logic (.ts):
 *  - Pure: no clock, no randomness, no timers, no network, no storage.
 *    "Now" is `model.publishedAt`; idle time is the shell's Tick.
 *  - No local time: Korri's times are UTC epoch milliseconds and a surface
 *    has no zone, so a date is read with the UTC accessors only.
 *  - A package imports only types from contracts/generated/surface.ts, and
 *    its own files.
 */
import { describe, expect, test } from "bun:test"
import { readdirSync, readFileSync } from "node:fs"
import { join } from "node:path"
import { isPreview, PACKAGE, read, slintFiles, stripComments } from "./slint-source"

const TOKENS = "ui/tokens.slint"
const FRAME = "ui/preview-frame.slint"
const files = slintFiles()
const others = files.filter(file => file !== TOKENS)

const hits = (pattern: RegExp, list: readonly string[], source = (file: string) => stripComments(read(file))) =>
  list.flatMap(file => [...source(file).matchAll(pattern)].map(match => `${file}: ${match[0]}`))

const logic = ["logic.ts", ...readdirSync(join(PACKAGE, "src")).filter(name => name.endsWith(".ts")).map(name => `src/${name}`)]
const code = (file: string) => stripComments(readFileSync(join(PACKAGE, file), "utf8"))

describe("authoring: views", () => {
  test("no raw colour outside the tokens", () => {
    expect(hits(/#[0-9a-fA-F]{3,8}\b/g, others)).toEqual([])
  })

  test("no pixel literal outside the tokens (0px, 1px and a preview's window size aside)", () => {
    const source = (file: string) => {
      const text = stripComments(read(file))
      return isPreview(file) || file === FRAME ? text.replace(/\b(preferred-width|preferred-height|width|height)\s*:\s*\d+px\s*;/g, "") : text
    }
    const found = hits(/\b\d+(\.\d+)?px\b/g, others, source).filter(hit => !/: [01]px$/.test(hit))
    expect(found).toEqual([])
  })

  test("no blur, drop shadow, rotation or conic gradient anywhere", () => {
    expect(hits(/drop-shadow[\w-]*|\bblur\b|[\w-]*-blur\b|@conic-gradient|\brotation-[\w-]+/g, files)).toEqual([])
  })

  test("no token is a saturated blue", () => {
    const blues: string[] = []
    for (const match of stripComments(read(TOKENS)).matchAll(/([\w-]+)\s*:\s*#([0-9a-fA-F]{6})/g)) {
      const [r, g, b] = [0, 2, 4].map(at => parseInt(match[2]!.slice(at, at + 2), 16) / 255) as [number, number, number]
      const max = Math.max(r, g, b), min = Math.min(r, g, b)
      const light = (max + min) / 2
      const saturation = max === min ? 0 : (max - min) / (1 - Math.abs(2 * light - 1))
      const hue = max === min ? 0 : max === r ? ((g - b) / (max - min) + 6) % 6 * 60 : max === g ? ((b - r) / (max - min) + 2) * 60 : ((r - g) / (max - min) + 4) * 60
      if (hue >= 195 && hue <= 255 && saturation > 0.5 && light > 0.25) blues.push(`${match[1]}: #${match[2]}`)
    }
    expect(blues).toEqual([])
  })

  test("an animation is of an element's own geometry, opacity or ground", () => {
    const allowed = new Set(["x", "y", "width", "height", "opacity", "background", "content-x", "content-y", "color", "border-color"])
    const wrong: string[] = []
    for (const file of files) {
      for (const match of stripComments(read(file)).matchAll(/\banimate\s+([\w\-,\s]+?)\s*\{/g)) {
        for (const name of match[1]!.split(",").map(part => part.trim())) if (!allowed.has(name)) wrong.push(`${file}: animate ${name}`)
      }
    }
    expect(wrong).toEqual([])
  })

  test("no deprecated viewport property", () => {
    expect(hits(/\bviewport-(x|y|width|height)\b/g, files)).toEqual([])
  })
})

describe("authoring: logic", () => {
  test("pure: no clock, randomness, timers, network or storage", () => {
    const banned = /\bDate\.now\b|\bnew Date\(\s*\)|\bMath\.random\b|\bsetTimeout\b|\bsetInterval\b|\brequestAnimationFrame\b|\bperformance\.now\b|\bfetch\(|\bXMLHttpRequest\b|\blocalStorage\b|\bsessionStorage\b|\bcrypto\./g
    expect(logic.flatMap(file => [...code(file).matchAll(banned)].map(match => `${file}: ${match[0]}`))).toEqual([])
  })

  test("no local time: Korri's times are UTC and a surface has no zone", () => {
    const banned = /\.get(Date|Month|FullYear|Year|Hours|Minutes|Seconds|Milliseconds|Day|TimezoneOffset)\(|\.toLocale(Date|Time)String\(|\.to(Date|Time)String\(|\bIntl\.DateTimeFormat\b/g
    expect(logic.flatMap(file => [...code(file).matchAll(banned)].map(match => `${file}: ${match[0]}`))).toEqual([])
  })

  test("imports: only types from the treaty, and the package's own files", () => {
    const wrong: string[] = []
    for (const file of logic) {
      for (const match of code(file).matchAll(/^\s*(import|export)\s+(type\s+)?[^"';]*?from\s+"([^"]+)"/gm)) {
        const target = match[3]!
        if (target.startsWith("./")) continue
        if (target.endsWith("contracts/generated/surface") && match[2] !== undefined) continue
        wrong.push(`${file}: ${match[0].trim()}`)
      }
      for (const match of code(file).matchAll(/^\s*import\s+"([^"]+)"/gm)) wrong.push(`${file}: ${match[0].trim()}`)
    }
    expect(wrong).toEqual([])
  })

  test("the logic names no key, no pad and no device", () => {
    const banned = /\bkeyCode\b|\bKeyboardEvent\b|\bgamepad\b|\bGamepad\b|\b(Arrow(Up|Down|Left|Right))\b|\bBTN_[A-Z]+\b/g
    expect(logic.flatMap(file => [...code(file).matchAll(banned)].map(match => `${file}: ${match[0]}`))).toEqual([])
  })

  test("logic.js is the bundle of logic.ts: it sets the one global", () => {
    const bundle = readFileSync(join(PACKAGE, "logic.js"), "utf8")
    expect(bundle.includes("globalThis.surface=")).toBe(true)
    expect(/\brequire\(|\bimport\s*\(/.test(bundle)).toBe(false)
  })
})
