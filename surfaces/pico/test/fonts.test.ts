/**
 * The vendored faces are the ones that were looked at.
 *
 * This pins bytes, not appearance: a woff2 from the wrong subset loads without
 * error and reports a face, so nothing short of rasterising glyphs can prove a
 * font is right. What this does catch is the file changing without anyone
 * deciding to change it — see src/fonts/README.md for how that bit once.
 */
import { describe, expect, test } from "bun:test"
import { createHash } from "node:crypto"
import { readFileSync } from "node:fs"
import { join } from "node:path"

const FONTS = join(import.meta.dir, "..", "src", "fonts")

const PINNED = {
  "press-start-2p.woff2":
    "afec86997fdaf54af1f59358fa2c1e2a0f1d04146edad18e5cd141d0384a7548",
  "vt323.woff2":
    "8ddbebcc1048154132e1d78eb9b1f7850bca1b7d857035ccf1cb4318ebc615b6",
} as const

describe("vendored faces", () => {
  test.each(Object.entries(PINNED))("%s matches its pinned bytes", (file, sha) => {
    const bytes = readFileSync(join(FONTS, file))
    expect(createHash("sha256").update(bytes).digest("hex")).toBe(sha)
  })

  test.each(Object.keys(PINNED))("%s is really woff2", (file) => {
    // A truncated or HTML-error-page download still has a plausible size.
    expect(readFileSync(join(FONTS, file)).subarray(0, 4).toString()).toBe("wOF2")
  })
})
