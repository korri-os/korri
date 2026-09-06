import { afterEach, expect, jest, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import { PicoSurface } from "../src/PicoSurface"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { PICO_ATTRACT_AFTER_MS } from "../src/pico-attract"

afterEach(() => { cleanup(); jest.useRealTimers() })

test("Back from a Find result restores Find and its query before leaving it", () => {
  const host = createFixtureHost()
  render(<PicoSurface host={host} model={fixtureModel} />)
  act(() => host.press("options"))
  fireEvent.click(screen.getByRole("button", { name: "Type T" }))
  fireEvent.click(screen.getByRole("button", { name: /Tetris, GB/ }))
  act(() => host.press("back"))
  expect(document.querySelector(".pico-library-browser")).not.toBeNull()
  expect(document.querySelector(".pico-query-field-text")?.textContent).toBe("T")
  act(() => host.press("back"))
  expect(document.querySelector(".pico-cart-shelf")).not.toBeNull()
})

test.each(["settings", "find", "detail", "location"])("attract does not cover %s", view => {
  jest.useFakeTimers()
  const host = createFixtureHost()
  render(<PicoSurface host={host} model={fixtureModel} />)
  if (view === "settings") act(() => host.press("system"))
  if (view === "find") act(() => host.press("options"))
  if (view === "detail" || view === "location") {
    fireEvent.click(screen.getByRole("button", { name: /Tetris/ }))
    if (view === "location") fireEvent.click(screen.getByRole("button", { name: /PLAY/ }))
  }
  act(() => jest.advanceTimersByTime(PICO_ATTRACT_AFTER_MS + 1))
  expect(screen.queryByRole("img", { name: "Attract" }) === null).toBe(true)
})

test("a launch failure outranks local settings and Find", () => {
  const host = createFixtureHost()
  const view = render(<PicoSurface host={host} model={fixtureModel} />)
  act(() => host.press("system"))
  view.rerender(<PicoSurface host={host} model={{ ...fixtureModel,
    status: { _tag: "Problem", kicker: "LAUNCH FAILED", reason: "Please retry", canRetry: true },
  }} />)
  expect(screen.getByText("LAUNCH FAILED")).toBeTruthy()
  expect(document.querySelector(".pico-panel-screen") === null).toBe(true)
})

test("the pointer sequence that wakes attract cannot activate the underlying cart", () => {
  jest.useFakeTimers()
  render(<PicoSurface host={createFixtureHost()} model={fixtureModel} />)
  act(() => jest.advanceTimersByTime(PICO_ATTRACT_AFTER_MS + 1))
  const cart = document.querySelector(".pico-cart")!
  fireEvent.pointerDown(cart)
  fireEvent.pointerUp(cart)
  fireEvent.click(cart)
  expect(screen.queryByRole("img", { name: "Attract" }) === null).toBe(true)
  expect(document.querySelector(".pico-game-detail") === null).toBe(true)
  fireEvent.pointerDown(cart)
  fireEvent.pointerUp(cart)
  fireEvent.click(cart)
  expect(document.querySelector(".pico-game-detail")).not.toBeNull()
})

test("Back dismisses the visible failure before touching navigation underneath", () => {
  const host = createFixtureHost()
  const view = render(<PicoSurface host={host} model={fixtureModel} />)
  act(() => host.press("system"))
  view.rerender(<PicoSurface host={host} model={{ ...fixtureModel,
    status: { _tag: "Problem", kicker: "LAUNCH FAILED", reason: "Please retry", canRetry: true },
  }} />)
  act(() => host.press("back"))
  expect(host.calls).toEqual(["dismiss"])
  view.rerender(<PicoSurface host={host} model={fixtureModel} />)
  expect(document.querySelector(".pico-panel-screen")).not.toBeNull()
})
