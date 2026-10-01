/**
 * Searching and narrowing the library.
 *
 * Every fact here comes from the catalog Korri already published — nothing is
 * asked of Korri and nothing new is invented. That is what makes this screen
 * honest on a device that is offline or has never been paired.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import type { SurfaceGame, SurfaceModel } from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { PicoSurface } from "../src/PicoSurface"

afterEach(() => cleanup())

const open = (model: SurfaceModel = fixtureModel) => {
  const host = createFixtureHost()
  render(<PicoSurface host={host} model={model} />)
  act(() => host.press("options"))
  return host
}

/* Every shape of play facts the treaty's types allow, in this catalog order. */
const mixedPlayFacts: SurfaceModel = (() => {
  const game = (id: string, title: string, facts: Partial<SurfaceGame>): SurfaceGame => ({
    id,
    title,
    subtitle: "PC · This device",
    ...facts,
  })
  return {
    ...fixtureModel,
    catalog: {
      _tag: "Ready",
      games: [
        game("plays", "Plays Only", { playCount: 40 }),
        game("both", "Both Facts", { playCount: 1, totalPlaytimeSeconds: 600, lastPlayedAt: 2 }),
        game("none", "No Record", {}),
        game("time", "Time Only", { totalPlaytimeSeconds: 3_600 }),
        game("zero", "Zero Time", { playCount: 1, totalPlaytimeSeconds: 0, lastPlayedAt: 1 }),
      ],
    },
  }
})()

const type = (word: string) => {
  for (const letter of word) {
    fireEvent.click(screen.getByRole("button", { name: `Type ${letter}` }))
  }
}

describe("opening the browser", () => {
  test("the options button opens it over the shelf", () => {
    open()
    expect(screen.getByText("FIND")).toBeTruthy()
  })

  test("back returns to the shelf without telling Korri", () => {
    const host = open()
    act(() => host.press("back"))
    expect(screen.getByText("LIBRARY")).toBeTruthy()
    expect(host.calls).toEqual([])
  })

  test("starts by listing the whole library", () => {
    open()
    expect(screen.getByRole("button", { name: /Celeste Classic/ })).toBeTruthy()
    expect(screen.getByRole("button", { name: /Spelunky/ })).toBeTruthy()
  })
})

describe("typing a query", () => {
  test("narrows to matching titles as letters are added", () => {
    open()
    type("SP")
    expect(screen.getByRole("button", { name: /Spelunky/ })).toBeTruthy()
    expect(screen.queryByRole("button", { name: /Celeste Classic/ })).toBeNull()
  })

  test("matches regardless of case", () => {
    open()
    type("TET")
    expect(screen.getByRole("button", { name: /Tetris/ })).toBeTruthy()
  })

  test("matches the provenance line too, not just the title", () => {
    open()
    type("SWITCH")
    expect(screen.getByRole("button", { name: /Hollow Knight/ })).toBeTruthy()
  })

  test("erases the last letter on backspace", () => {
    open()
    type("SPX")
    expect(screen.getByText("NOTHING MATCHES")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "Backspace" }))
    expect(screen.getByRole("button", { name: /Spelunky/ })).toBeTruthy()
  })

  test("says so when nothing matches, without blaming Korri", () => {
    open()
    type("ZZZ")
    expect(screen.getByText("NOTHING MATCHES")).toBeTruthy()
    expect(screen.queryByText(/error/i)).toBeNull()
  })

  test("shows what has been typed", () => {
    open()
    type("SP")
    expect(screen.getByText("SP")).toBeTruthy()
  })
})

describe("narrowing to a collection", () => {
  test("offers exactly the sections Korri grouped by", () => {
    open()
    for (const section of ["ALL", "CONTINUE", "ZAO", "THIS DEVICE"]) {
      expect(screen.getByRole("button", { name: section })).toBeTruthy()
    }
  })

  test("keeps only that section's games", () => {
    open()
    fireEvent.click(screen.getByRole("button", { name: "ZAO" }))
    expect(screen.getByRole("button", { name: /Tetris/ })).toBeTruthy()
    expect(screen.queryByRole("button", { name: /Celeste Classic/ })).toBeNull()
  })

  test("combines with the query rather than replacing it", () => {
    open()
    fireEvent.click(screen.getByRole("button", { name: "CONTINUE" }))
    type("HOL")
    expect(screen.getByRole("button", { name: /Hollow Knight/ })).toBeTruthy()
    expect(screen.queryByRole("button", { name: /Celeste Classic/ })).toBeNull()
  })

  test("ALL puts the whole library back", () => {
    open()
    fireEvent.click(screen.getByRole("button", { name: "ZAO" }))
    fireEvent.click(screen.getByRole("button", { name: "ALL" }))
    expect(screen.getByRole("button", { name: /Celeste Classic/ })).toBeTruthy()
  })
})

describe("choosing a result", () => {
  test("opens the game rather than launching it", () => {
    const host = open()
    type("SP")
    fireEvent.click(screen.getByRole("button", { name: /Spelunky/ }))
    expect(host.calls).toEqual([])
    expect(screen.getByText("LIBRARY ›")).toBeTruthy()
  })
})

describe("ordering the results", () => {
  test("keeps Korri's order until asked otherwise", () => {
    open()
    const rows = screen.getAllByRole("button", { name: /·/ })
    expect(rows[0]?.textContent).toContain("Celeste Classic")
  })

  test("offers only orders it can derive from published facts", () => {
    open()
    for (const order of ["KORRI", "A-Z", "MOST PLAYED", "RECENT"]) {
      expect(screen.getByRole("button", { name: order })).toBeTruthy()
    }
  })

  test("sorts by title", () => {
    open()
    fireEvent.click(screen.getByRole("button", { name: "A-Z" }))
    const rows = screen.getAllByRole("button", { name: /·/ })
    expect(rows[0]?.textContent).toContain("Bramble Run")
    expect(rows.at(-1)?.textContent).toContain("Tide Pool")
  })

  test("sorts by how much a game has been played", () => {
    open()
    fireEvent.click(screen.getByRole("button", { name: "MOST PLAYED" }))
    const rows = screen.getAllByRole("button", { name: /·/ })
    // Lantern Keep has the most playtime Korri published: 11h 30m.
    expect(rows[0]?.textContent).toContain("Lantern Keep")
    expect(rows[1]?.textContent).toContain("Hollow Knight")
  })

  test("ranks most played by playtime alone and never by a play count", () => {
    open(mixedPlayFacts)
    fireEvent.click(screen.getByRole("button", { name: "MOST PLAYED" }))
    const rows = screen.getAllByRole("button", { name: /·/ }).map((row) => row.textContent)
    // Ranked by seconds; a recorded zero is a time and ranks. Games with no
    // playtime follow in Korri's order, however many plays they have: 40 plays
    // is not 40 seconds, and no record is not zero.
    expect(rows).toEqual([
      expect.stringContaining("Time Only"),
      expect.stringContaining("Both Facts"),
      expect.stringContaining("Zero Time"),
      expect.stringContaining("Plays Only"),
      expect.stringContaining("No Record"),
    ])
  })

  test("puts games Korri has never timed last rather than first", () => {
    open()
    fireEvent.click(screen.getByRole("button", { name: "RECENT" }))
    const rows = screen.getAllByRole("button", { name: /·/ })
    // Three fixtures carry a lastPlayedAt, Hollow Knight the latest; everything
    // else is unknown, and unknown is not "a long time ago".
    expect(rows[0]?.textContent).toContain("Hollow Knight")
    expect(rows[1]?.textContent).toContain("Comet Courier")
    expect(rows[2]?.textContent).toContain("Lantern Keep")
  })

  test("orders within the collection, not across it", () => {
    open()
    fireEvent.click(screen.getByRole("button", { name: "CONTINUE" }))
    fireEvent.click(screen.getByRole("button", { name: "A-Z" }))
    const rows = screen.getAllByRole("button", { name: /·/ })
    expect(rows).toHaveLength(3)
    expect(rows[0]?.textContent).toContain("Celeste Classic")
    expect(rows[1]?.textContent).toContain("Comet Courier")
  })
})
