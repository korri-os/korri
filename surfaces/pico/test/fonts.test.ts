/**
 * The face is the one that was built from the glyph table.
 *
 * This pins bytes, not appearance. scripts/build-font.py is deterministic, so
 * a changed checksum means someone changed the table or the builder — and the
 * pin makes that a decision rather than an accident. Re-check the rendered
 * text by eye when changing either; see src/fonts/README.md.
 */
import { describe, expect, test } from "bun:test"
import { createHash } from "node:crypto"
import { readFileSync } from "node:fs"
import { join } from "node:path"

const FONTS = join(import.meta.dir, "..", "src", "fonts")

const PINNED = {
  "pico8.woff2": "f1cc9fa26350ad56630c949266508239e379fadb07c74a7f156da1286bbec7ac",
} as const

describe("the vendored face", () => {
  test.each(Object.entries(PINNED))("%s matches its pinned bytes", (file, sha) => {
    const bytes = readFileSync(join(FONTS, file))
    expect(createHash("sha256").update(bytes).digest("hex")).toBe(sha)
  })

  test.each(Object.keys(PINNED))("%s is really woff2", (file) => {
    expect(readFileSync(join(FONTS, file)).subarray(0, 4).toString()).toBe("wOF2")
  })

  test("the glyph table holds every printable ASCII character but the capitals", () => {
    const table = readFileSync(join(FONTS, "pico8-glyphs.txt"), "utf8")
    const codes = new Set(
      [...table.matchAll(/^([0-9A-F]{4}) /gm)].map((match) => Number.parseInt(match[1] ?? "", 16)),
    )
    const missing: string[] = []
    for (let code = 0x20; code < 0x7f; code += 1) {
      const isCapital = code >= 0x41 && code <= 0x5a
      if (!isCapital && !codes.has(code)) missing.push(String.fromCharCode(code))
    }
    expect(missing).toEqual([])
  })
})
