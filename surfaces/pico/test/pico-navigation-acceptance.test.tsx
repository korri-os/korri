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
  const cart = document.querySelector("button.pico-cart-button")!
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

const settingsShown = () => document.querySelector(".pico-panel-screen") !== null
const findShown = () => document.querySelector(".pico-library-browser") !== null
const detailShown = () => document.querySelector(".pico-game-detail") !== null
const shelfShown = () => document.querySelector(".pico-cart-shelf") !== null

test("Back on Settings closes Settings before the location question on the game below it", () => {
  const host = createFixtureHost()
  render(<PicoSurface host={host} model={fixtureModel} />)
  fireEvent.click(screen.getByRole("button", { name: /Tetris/ }))
  fireEvent.click(screen.getByRole("button", { name: /PLAY/ }))
  act(() => host.press("system"))
  act(() => host.press("back"))
  expect(settingsShown()).toBe(false)
  expect(screen.getByText("PLAY WHERE?")).toBeTruthy()
  act(() => host.press("back"))
  expect(screen.queryByText("PLAY WHERE?")).toBeNull()
  expect(detailShown()).toBe(true)
})

test("Back on Settings closes Settings before the action question on the game below it", () => {
  const host = createFixtureHost()
  render(<PicoSurface host={host} model={fixtureModel} />)
  fireEvent.click(screen.getByRole("button", { name: /Hollow Knight/ }))
  fireEvent.click(screen.getByRole("button", { name: /Remove from device/ }))
  act(() => host.press("system"))
  act(() => host.press("back"))
  expect(settingsShown()).toBe(false)
  expect(screen.getByText("REMOVE FROM DEVICE?")).toBeTruthy()
  expect(host.calls).toEqual([])
})

test("Options on a game's own screen does nothing", () => {
  const host = createFixtureHost()
  render(<PicoSurface host={host} model={fixtureModel} />)
  fireEvent.click(screen.getByRole("button", { name: /Tetris/ }))
  act(() => host.press("options"))
  expect(detailShown()).toBe(true)
  act(() => host.press("back"))
  expect(findShown()).toBe(false)
  expect(shelfShown()).toBe(true)
})

const shelfCarts = () => [...document.querySelectorAll<HTMLButtonElement>(".pico-cart-shelf button.pico-cart-button")]

// The host's spatial focus moves the cursor with element.focus() and sends no
// key press to the surface. Moving along the shelf that way is still activity.
test("attract stays away while focus moves along the shelf without key presses", () => {
  jest.useFakeTimers()
  render(<PicoSurface host={createFixtureHost()} model={fixtureModel} />)
  const steps = 4
  for (let step = 0; step < steps; step += 1) {
    act(() => jest.advanceTimersByTime(PICO_ATTRACT_AFTER_MS / 2))
    expect(screen.queryByRole("img", { name: "Attract" }) === null).toBe(true)
    const carts = shelfCarts()
    act(() => carts[step % carts.length]!.focus())
  }
  act(() => jest.advanceTimersByTime(PICO_ATTRACT_AFTER_MS / 2))
  expect(screen.queryByRole("img", { name: "Attract" }) === null).toBe(true)
  act(() => jest.advanceTimersByTime(PICO_ATTRACT_AFTER_MS / 2 + 1))
  expect(screen.queryByRole("img", { name: "Attract" }) === null).toBe(false)
})

test("Back from a game's screen opened on the shelf puts the cursor on that cart", () => {
  const host = createFixtureHost()
  render(<PicoSurface host={host} model={fixtureModel} />)
  // The cursor enters the shelf on its chosen cart, then moves one along.
  act(() => shelfCarts()[0]!.focus())
  const cart = shelfCarts()[1]!
  const label = cart.getAttribute("aria-label")
  act(() => cart.focus())
  expect(document.activeElement === cart).toBe(true)
  fireEvent.click(cart)
  expect(detailShown()).toBe(true)
  act(() => host.press("back"))
  expect(shelfShown()).toBe(true)
  expect(document.activeElement === shelfCarts()[1]).toBe(true)
  expect(document.activeElement?.getAttribute("aria-label")).toBe(label)
})

test("Options on Settings does nothing", () => {
  const host = createFixtureHost()
  render(<PicoSurface host={host} model={fixtureModel} />)
  act(() => host.press("system"))
  act(() => host.press("options"))
  expect(settingsShown()).toBe(true)
  act(() => host.press("back"))
  expect(findShown()).toBe(false)
  expect(shelfShown()).toBe(true)
})

test("while Korri starts a game, System, Options and Menu change nothing hidden below", () => {
  const host = createFixtureHost()
  const view = render(<PicoSurface host={host} model={fixtureModel} />)
  view.rerender(<PicoSurface host={host} model={{ ...fixtureModel,
    status: { _tag: "Busy", kicker: "STARTING", gameId: "tetris" },
  }} />)
  act(() => host.press("system"))
  act(() => host.press("options"))
  act(() => host.press("menu"))
  view.rerender(<PicoSurface host={host} model={fixtureModel} />)
  expect(settingsShown()).toBe(false)
  expect(findShown()).toBe(false)
  expect(shelfShown()).toBe(true)
})

test("Menu changes home's layout only while home is shown", () => {
  const host = createFixtureHost()
  render(<PicoSurface host={host} model={fixtureModel} />)
  act(() => host.press("system"))
  act(() => host.press("menu"))
  act(() => host.press("back"))
  expect(shelfShown()).toBe(true)
  act(() => host.press("menu"))
  expect(shelfShown()).toBe(false)
  expect(document.querySelector(".pico-cart-grid")).not.toBeNull()
})
