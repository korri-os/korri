/**
 * Attract mode.
 *
 * Legacy had no idle policy — attract was a screen you navigated to in a
 * gallery — so the timeout and the wake rule are decisions, not ports. Both are
 * asserted here because both are the kind of thing that is only wrong once
 * someone is holding the device.
 */
import { afterEach, describe, expect, jest, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { PICO_ATTRACT_AFTER_MS } from "../src/pico-attract"
import { PicoSurface } from "../src/PicoSurface"

afterEach(() => {
  cleanup()
  jest.useRealTimers()
})

const idle = (overrides: Partial<SurfaceModel> = {}) => {
  jest.useFakeTimers()
  const host = createFixtureHost()
  render(<PicoSurface host={host} model={{ ...fixtureModel, ...overrides }} />)
  act(() => {
    jest.advanceTimersByTime(PICO_ATTRACT_AFTER_MS + 1_000)
  })
  return host
}

describe("when it appears", () => {
  test("after the library has been left alone", () => {
    idle()
    expect(screen.getByRole("img", { name: "Attract" })).toBeTruthy()
  })

  test("not before", () => {
    jest.useFakeTimers()
    render(<PicoSurface host={createFixtureHost()} model={fixtureModel} />)
    act(() => {
      jest.advanceTimersByTime(PICO_ATTRACT_AFTER_MS - 1_000)
    })
    expect(screen.queryByRole("img", { name: "Attract" })).toBeNull()
  })

  test("never over a running game", () => {
    idle({ status: { _tag: "Running", kicker: "PLAYING", gameId: "hollow" } })
    expect(screen.queryByRole("img", { name: "Attract" })).toBeNull()
  })

  test("never while Korri is still reading the library", () => {
    idle({ catalog: { _tag: "Loading" } })
    expect(screen.queryByRole("img", { name: "Attract" })).toBeNull()
  })
})

describe("waking it", () => {
  test("a surface button dismisses it and does nothing else", () => {
    const host = idle()
    act(() => host.press("menu"))
    expect(screen.queryByRole("img", { name: "Attract" })).toBeNull()
    // menu cycles the home mode — but not the press that woke the screen.
    expect(screen.getByRole("list", { name: "Shelf" })).toBeTruthy()
    expect(host.calls).toEqual([])
  })

  test("a key dismisses it without activating what was focused", () => {
    const host = idle()
    act(() => {
      // From inside, the way a real press arrives: bubbling up from whatever
      // control had focus when the screen was left alone.
      fireEvent.keyDown(document.querySelector(".pico-cart")!, { key: "Enter" })
    })
    expect(screen.queryByRole("img", { name: "Attract" })).toBeNull()
    expect(host.calls).toEqual([])
  })

  test("comes back after the screen is left alone again", () => {
    const host = idle()
    act(() => host.press("menu"))
    act(() => {
      jest.advanceTimersByTime(PICO_ATTRACT_AFTER_MS + 1_000)
    })
    expect(screen.getByRole("img", { name: "Attract" })).toBeTruthy()
  })
})

describe("what it shows", () => {
  test("the library, not an invented advertisement", () => {
    idle()
    const attract = screen.getByRole("img", { name: "Attract" })
    expect(attract.textContent).toContain("PICO")
  })
})
