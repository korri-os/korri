import { afterEach, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { cleanup, render } from "@testing-library/react"
import { PICO_PALETTE } from "../src/pico-palette"
import { PicoPaletteBar } from "../src/ui/atoms/PicoPaletteBar"

afterEach(cleanup)

test("the decorative palette bar skips black and retains the other fifteen in order", () => {
  const view = render(<PicoPaletteBar />)
  const bar = view.container.querySelector(".pico-palette-bar")!
  expect(bar.getAttribute("aria-hidden")).toBe("true")
  expect([...bar.children].map(cell => cell.getAttribute("data-cell"))).toEqual(
    Array.from({ length: 15 }, (_, index) => String(index + 1)),
  )
})

test("fifteen equal grid tracks retain their original palette tokens without a black track", () => {
  const css = readFileSync(join(import.meta.dir, "../src/ui/atoms/PicoPaletteBar.css"), "utf8")
  expect(css).toMatch(/grid-template-columns:\s*repeat\(15,\s*minmax\(0,\s*1fr\)\)/)
  const cells = [...css.matchAll(/\[data-cell="(\d+)"\]\s*\{\s*background:\s*var\(--p8-(\w+)\)/g)]
  expect(cells.map(([, index, name]) => [Number(index), name])).toEqual(
    PICO_PALETTE.slice(1).map((color, index) => [index + 1, color.name]),
  )
})
