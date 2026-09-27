import { afterEach, expect, test } from "bun:test"
import { cleanup, render } from "@testing-library/react"
import { readdirSync } from "node:fs"
import { join, relative } from "node:path"
import type { ComponentType } from "react"
import { noArtworkGame } from "../src/fixtures/named-states"
import * as Home from "../src/pages/PicoHome.page.part"
import * as Shelf from "../src/ui/organisms/PicoCartShelf.organism.part"
import * as Cart from "../src/ui/molecules/PicoCart.molecule.part"
import * as Cover from "../src/ui/atoms/PicoCoverArt.atom.part"

const root = join(import.meta.dir, "..")
function partsIn(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const path = join(directory, entry.name)
    return entry.isDirectory() ? partsIn(path) : path.endsWith(".part.tsx") ? [path] : []
  })
}

const parts = new Map<string, Record<string, unknown>>()
for (const file of partsIn(join(root, "src"))) {
  parts.set(relative(root, file), await import(file))
}

afterEach(cleanup)

// This checks every product declaration, so moving a part or removing a state
// cannot leave Caliper with a link to a scenario Pico no longer supplies.
test("every declared composition names existing renderable parent and child states", () => {
  let links = 0
  for (const [path, exports] of parts) {
    if (exports.composition === undefined) continue
    const composition = exports.composition as Record<string, readonly { part: string; state: string }[]>
    for (const [state, children] of Object.entries(composition)) {
      expect(typeof exports[state], `${path}#${state}`).toBe("function")
      for (const child of children) {
        expect(typeof parts.get(child.part)?.[child.state], `${child.part}#${child.state}`).toBe("function")
        links += 1
      }
    }
  }
  expect(links).toBeGreaterThanOrEqual(3)
})

function subtree(State: ComponentType, selector: string): string {
  const view = render(<State />)
  const node = view.container.querySelector(selector)
  expect(node, selector).not.toBeNull()
  const markup = node!.outerHTML
  view.unmount()
  return markup
}

test("Home's missing-art scenario reaches the atom through real shelf and cart composition", () => {
  expect(Home.composition.NoArtwork).toEqual([
    { part: "src/ui/organisms/PicoCartShelf.organism.part.tsx", state: "NoArtwork" },
  ])
  expect(Shelf.composition.NoArtwork).toEqual([
    { part: "src/ui/molecules/PicoCart.molecule.part.tsx", state: "MissingArtHero" },
  ])
  expect(Cart.composition.MissingArtHero).toEqual([
    { part: "src/ui/atoms/PicoCoverArt.atom.part.tsx", state: "MissingArt" },
  ])
  expect(noArtworkGame.coverArtUrl).toBeUndefined()
  expect(subtree(Home.NoArtwork, ".pico-cart-shelf")).toBe(subtree(Shelf.NoArtwork, ".pico-cart-shelf"))
  expect(subtree(Shelf.NoArtwork, ".pico-cart-shelf-rack .pico-cart")).toBe(subtree(Cart.MissingArtHero, ".pico-cart"))
  expect(subtree(Cart.MissingArtHero, ".pico-cover-art-initials")).toBe(subtree(Cover.MissingArt, ".pico-cover-art-initials"))
})

test("the missing-art catalog drives Home's count and selected cart together", () => {
  const view = render(<Home.NoArtwork />)
  expect(view.getByText("1 CART")).toBeDefined()
  const rack = view.getByRole("list", { name: "Shelf" })
  const carts = rack.querySelectorAll(".pico-cart")
  expect(carts.length).toBe(1)
  expect(carts[0]?.getAttribute("data-placement")).toBe("hero")
  expect(carts[0]?.getAttribute("aria-label")).toContain(noArtworkGame.title)
  expect(view.container.querySelector(".pico-cover-art-canvas")).toBeNull()
})

test("Empty removes the shelf and declares no shelf or cart state", () => {
  const view = render(<Home.Empty />)
  expect(view.getByText("NO CARTS")).toBeDefined()
  expect(view.container.querySelector(".pico-cart-shelf")).toBeNull()
  expect(view.container.querySelector(".pico-cart")).toBeNull()
  expect(view.queryByText("1 CART")).toBeNull()
  expect(Object.hasOwn(Home.composition, "Empty")).toBe(false)
  expect(Object.hasOwn(Shelf.composition, "EmptyLibrary")).toBe(false)
})

test("the composed scenario returns to the same rendered state on a fresh mount", () => {
  expect(subtree(Home.NoArtwork, ".pico-screen-shell")).toBe(subtree(Home.NoArtwork, ".pico-screen-shell"))
})
