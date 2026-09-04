/**
 * The seam where Korri's catalog becomes the home screen's own state.
 *
 * Worth testing directly because every later branch trusts it: if a state is
 * mapped wrong here, the screen renders the wrong thing correctly, which is the
 * hardest kind of bug to see.
 */
import { describe, expect, test } from "bun:test"
import type { SurfaceCatalog } from "@contracts/surface/korri-surface"
import { picoHomeViewFromCatalog } from "../src/pico-home-view"

describe("picoHomeViewFromCatalog", () => {
  test("carries Korri's own failure copy through untouched", () => {
    const catalog: SurfaceCatalog = {
      _tag: "Error",
      message: "The library folder is not readable.",
    }

    expect(picoHomeViewFromCatalog(catalog)).toEqual({
      _tag: "Failed",
      message: "The library folder is not readable.",
    })
  })

  test("reads a ready catalog with no games as empty", () => {
    // Korri can spell "nothing to play" two ways; the screen must not have two
    // answers, or a Ready-but-empty catalog renders a shelf with no carts.
    expect(picoHomeViewFromCatalog({ _tag: "Ready", games: [] })).toEqual({
      _tag: "Empty",
    })
  })

  test("keeps absent facts absent rather than defaulting them", () => {
    const view = picoHomeViewFromCatalog({
      _tag: "Ready",
      games: [{ id: "celeste", title: "Celeste Classic" }],
    })

    expect(view).toEqual({
      _tag: "Shelf",
      games: [{ id: "celeste", title: "Celeste Classic" }],
    })
  })

  test("preserves Korri's launch-location order", () => {
    const view = picoHomeViewFromCatalog({
      _tag: "Ready",
      games: [
        {
          id: "tetris",
          title: "Tetris",
          launchLocations: [
            { id: "local", label: "This device" },
            { id: "zao", label: "zao" },
          ],
        },
      ],
    })

    const [game] = view._tag === "Shelf" ? view.games : []

    expect(game?.locations?.map((location) => location.id)).toEqual([
      "local",
      "zao",
    ])
  })

  test("maps loading and empty to their own states", () => {
    expect(picoHomeViewFromCatalog({ _tag: "Loading" })._tag).toBe("Loading")
    expect(picoHomeViewFromCatalog({ _tag: "Empty" })._tag).toBe("Empty")
  })
})
