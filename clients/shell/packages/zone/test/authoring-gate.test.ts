/**
 * Authoring: every colour and every pixel length is a token; Zone never
 * blurs, blends or fades; one see-through exception.
 *
 *  - A raw colour (`#rrggbb`) or a pixel literal (`12px`) appears only in
 *    ui/tokens.slint. Components size everything in art px through
 *    `Tokens.a`, and convert with `Tokens.unit` (1px). A preview's window
 *    size is the one pixel literal a preview may hold.
 *  - No blur, no drop shadow, no radial gradient; a linear gradient is a
 *    hard-stop stripe: every colour stop shares its position with a
 *    neighbour.
 *  - No opacity under 1 and no colour with alpha, except Modern's veil:
 *    `Tokens.veil-see-through`, read by ZoneVeil alone.
 */
import { describe, expect, test } from "bun:test"
import { isPreview, read, slintFiles, stripComments } from "./slint-source"

const TOKENS = "ui/tokens.slint"
const files = slintFiles()
const others = files.filter(file => file !== TOKENS)

const hits = (pattern: RegExp, list: readonly string[], source = (file: string) => stripComments(read(file))) =>
  list.flatMap(file => [...source(file).matchAll(pattern)].map(match => `${file}: ${match[0]}`))

describe("authoring", () => {
  test("no raw colour outside the tokens", () => {
    expect(hits(/#[0-9a-fA-F]{3,8}\b/g, others)).toEqual([])
  })

  test("no pixel literal outside the tokens (a preview's window size aside)", () => {
    const source = (file: string) => {
      const text = stripComments(read(file))
      return isPreview(file) || file === "ui/preview-frame.slint" ? text.replace(/\b(width|height|preview-width|preview-height)\s*:\s*\d+px\s*;/g, "") : text
    }
    expect(hits(/\b\d+(\.\d+)?px\b/g, others, source)).toEqual([])
  })

  test("no blur, drop shadow or radial gradient anywhere", () => {
    expect(hits(/drop-shadow[\w-]*|blur|@radial-gradient|@conic-gradient/g, files)).toEqual([])
  })

  test("every linear gradient is a hard-stop stripe", () => {
    // Two neighbouring stops of different colours stand at one position: no blend between them.
    const wrong: string[] = []
    for (const file of files) {
      for (const match of stripComments(read(file)).matchAll(/@linear-gradient\(([^)]*)\)/g)) {
        const stops = match[1]!.split(",").slice(1).map(stop => {
          const found = /^(.*\S)\s+(-?[\d.]+)%$/.exec(stop.trim())
          return found === null ? undefined : { colour: found[1]!, at: found[2]! }
        })
        const blended = stops.some((stop, index) => {
          const next = stops[index + 1]
          return stop === undefined || (next !== undefined && next.colour !== stop.colour && next.at !== stop.at)
        })
        if (blended) wrong.push(`${file}: ${match[0]}`)
      }
    }
    expect(wrong).toEqual([])
  })

  test("no opacity under 1 anywhere", () => {
    expect(hits(/\bopacity\s*:\s*(0?\.\d+|0)\s*;/g, files)).toEqual([])
  })

  test("the one colour with alpha is the veil's see-through, and only ZoneVeil reads it", () => {
    const alpha = [...stripComments(read(TOKENS)).matchAll(/([\w-]+)\s*:\s*#[0-9a-fA-F]{8}\b/g)].map(match => match[1])
    expect(alpha).toEqual(["veil-see-through"])
    const readers = files.filter(file => /Tokens\.veil-see-through/.test(stripComments(read(file))))
    expect(readers).toEqual(["ui/atoms/ZoneVeil.slint"])
  })
})
