/**
 * The quantiser, exercised as bytes.
 *
 * It runs on a canvas in the browser, but it is a pure function of a pixel
 * buffer — so it is tested as one, with no DOM and no image decoding.
 */
import { describe, expect, test } from "bun:test"
import { PICO_PALETTE } from "../src/pico-palette"
import { quantizePico8 } from "../src/pico8-remap"

const pixel = (r: number, g: number, b: number, a = 255) =>
  new Uint8ClampedArray([r, g, b, a])

const asHex = (data: Uint8ClampedArray) =>
  `#${[...data.slice(0, 3)].map((c) => c.toString(16).padStart(2, "0")).join("")}`

describe("quantizePico8", () => {
  test("snaps a near-match to the exact palette entry", () => {
    const data = pixel(0x2b, 0xac, 0xfd) // a hair off PICO-8 blue
    quantizePico8(data, "flat")
    expect(asHex(data)).toBe("#29adff")
  })

  test("leaves an exact palette colour untouched", () => {
    const data = pixel(0xff, 0x00, 0x4d)
    quantizePico8(data, "flat")
    expect(asHex(data)).toBe("#ff004d")
  })

  test("maps every output to a colour that is actually in the palette", () => {
    const hexes = new Set(PICO_PALETTE.map((color) => color.hex))
    const data = new Uint8ClampedArray(256 * 4)
    for (let i = 0; i < 256; i += 1) {
      data[i * 4] = (i * 7) % 256
      data[i * 4 + 1] = (i * 13) % 256
      data[i * 4 + 2] = (i * 29) % 256
      data[i * 4 + 3] = 255
    }
    quantizePico8(data, "vivid")
    for (let i = 0; i < 256; i += 1) {
      expect(hexes.has(asHex(data.slice(i * 4, i * 4 + 4)))).toBe(true)
    }
  })

  test("leaves fully transparent pixels alone", () => {
    // Cut-out art must keep its holes rather than gaining a black background.
    const data = pixel(12, 34, 56, 0)
    quantizePico8(data, "vivid")
    expect([...data]).toEqual([12, 34, 56, 0])
  })

  test("vivid pushes a muddy colour off the greys, flat does not", () => {
    const muddy = () => pixel(120, 110, 100)
    const flat = muddy()
    const vivid = muddy()
    quantizePico8(flat, "flat")
    quantizePico8(vivid, "vivid")
    expect(asHex(flat)).toBe("#5f574f")
    expect(asHex(vivid)).not.toBe("#5f574f")
  })
})
