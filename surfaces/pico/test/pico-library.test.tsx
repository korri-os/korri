/**
 * Searching and narrowing the library.
 *
 * Every fact here comes from the catalog Korri already published — nothing is
 * asked of Korri and nothing new is invented. That is what makes this screen
 * honest on a device that is offline or has never been paired.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { PicoSurface } from "../src/PicoSurface"

afterEach(() => cleanup())

const open = () => {
  const host = createFixtureHost()
  render(<PicoSurface host={host} model={fixtureModel} />)
  act(() => host.press("options"))
  return host
}

const type = (word: string) => {
  for (const letter of word) {
    fireEvent.click(screen.getByRole("button", { name: `Type ${letter}` }))
  }
}

describe("opening the browser", () => {
  test("the options button opens it over the shelf", () => {
    open()
    expect(screen.getByText("PICO ▸ FIND")).toBeTruthy()
  })

  test("back returns to the shelf without telling Korri", () => {
    const host = open()
    act(() => host.press("back"))
    expect(screen.getByText("PICO ▸ LIBRARY")).toBeTruthy()
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
    expect(screen.getByText("PICO ▸ GAME")).toBeTruthy()
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
    expect(rows[0]?.textContent).toContain("Celeste Classic")
    expect(rows.at(-1)?.textContent).toContain("Tetris")
  })

  test("sorts by how much a game has been played", () => {
    open()
    fireEvent.click(screen.getByRole("button", { name: "MOST PLAYED" }))
    const rows = screen.getAllByRole("button", { name: /·/ })
    expect(rows[0]?.textContent).toContain("Hollow Knight")
  })

  test("puts games Korri has never timed last rather than first", () => {
    open()
    fireEvent.click(screen.getByRole("button", { name: "RECENT" }))
    const rows = screen.getAllByRole("button", { name: /·/ })
    // Hollow Knight is the only fixture with a lastPlayedAt; everything else is
    // unknown, and unknown is not "a long time ago".
    expect(rows[0]?.textContent).toContain("Hollow Knight")
  })

  test("orders within the collection, not across it", () => {
    open()
    fireEvent.click(screen.getByRole("button", { name: "CONTINUE" }))
    fireEvent.click(screen.getByRole("button", { name: "A-Z" }))
    const rows = screen.getAllByRole("button", { name: /·/ })
    expect(rows).toHaveLength(2)
    expect(rows[0]?.textContent).toContain("Celeste Classic")
  })
})
