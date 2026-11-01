/**
 * The backup's QR code: the package's own encoder (src/qr.ts) draws exactly
 * the modules of the `qrcode` package Zone used, for the same text, level M
 * and mask 0, from the smallest version up past the version split points
 * (version 7 adds version bits, 10 the 16-bit count).
 */
import { describe, expect, test } from "bun:test"
// @ts-expect-error: qrcode ships no types for its core module.
import { create } from "qrcode/lib/core/qrcode"
import { encodeQr, qrDataUrl } from "../src/qr"

const bytes = (text: string) => [...new TextEncoder().encode(text)]
const texts = [
  "a",
  "korri",
  "ncryptsec1qgg9947rlpvqu76pj5ecreduf9jxhselq2nae2kghhvd5g7dgjtcxfqtd67p9m0w57lspw8gsq6yphnm8623nsl8xn9j4jdzz84zrgx9qjvkr4yfktapwqzwfr8xwpg3h2fywdn5nfmxqzs2vhmsmmmq9c3cyd8vgqfkq",
  "x".repeat(200),
  "bunker://" + "a".repeat(64) + "?relay=wss://relay.nsec.app",
]

describe("the backup's QR code", () => {
  for (const text of texts) {
    test(`${text.length} characters: the modules are qrcode's`, () => {
      const ours = encodeQr(bytes(text))
      const theirs = create([{ data: new Uint8Array(bytes(text)), mode: "byte" }], { errorCorrectionLevel: "M", maskPattern: 0 }) as { modules: { size: number; data: Uint8Array } }
      expect(ours.size).toBe(theirs.modules.size)
      expect(ours.dark.map(on => (on ? 1 : 0)).join("")).toBe([...theirs.modules.data].join(""))
    })
  }
  test("the image is a PNG data URL", () => {
    expect(qrDataUrl("korri").startsWith("data:image/png;base64,iVBORw0KGgo")).toBe(true)
  })
})
