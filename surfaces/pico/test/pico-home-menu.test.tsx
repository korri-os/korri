/**
 * Home's MENU key: Settings, Find and the layout, reached with the d-pad, A
 * and B alone.
 *
 * Many handhelds deliver no `system`, `options` or `menu` button to the
 * surface, so on those devices this key is the only way to Settings and Find.
 * The fixture host has no geometry, so these tests move focus the way the
 * host's focus mover does: by focusing the control it chose. A press is focus
 * then click, which is what the host's confirm does.
 */
import { afterEach, expect, jest, test } from "bun:test"
import { act, cleanup, render, screen, within } from "@testing-library/react"
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { PicoSurface } from "../src/PicoSurface"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { PICO_ATTRACT_AFTER_MS } from "../src/pico-attract"

afterEach(() => { cleanup(); jest.useRealTimers() })

const menuKey = () => screen.getByRole("button", { name: "MENU" })
const menuList = () => screen.queryByRole("dialog", { name: "MENU" })
const entry = (name: string | RegExp) => within(menuList()!).getByRole("button", { name })
const hints = () => [...document.querySelectorAll(".pico-hint")].map((hint) => hint.textContent)
const cart = (title: string) => screen.getByRole("button", { name: new RegExp(`^${title},`) })
const stage = () => document.querySelector(".pico-cart-shelf-stage")?.textContent ?? ""
const placement = (title: string) => cart(title).querySelector("[data-placement]")?.getAttribute("data-placement")

function aim(element: HTMLElement) {
  act(() => element.focus())
}

function press(element: HTMLElement) {
  act(() => {
    element.focus()
    element.click()
  })
}

/** Walk the shelf one cart at a time, as Left and Right do. */
function walkTo(title: string) {
  for (const name of ["Celeste Classic", "Hollow Knight", "Tetris", "Spelunky", "Lantern Keep"]) {
    aim(cart(name))
    if (name === title) return
  }
  throw new Error(`${title} is not on the walk`)
}

function home(model: SurfaceModel = fixtureModel) {
  const host = createFixtureHost()
  const view = render(<PicoSurface host={host} model={model} />)
  return { host, view }
}

test("MENU opens a list with the cursor on FIND", () => {
  home()
  expect(menuList()).toBeNull()
  press(menuKey())
  expect(menuList()).not.toBeNull()
  expect(menuKey().getAttribute("aria-expanded")).toBe("true")
  expect(document.activeElement).toBe(entry("FIND"))
})

test("Back closes the list and the cursor goes back to MENU", () => {
  const { host } = home()
  press(menuKey())
  act(() => host.press("back"))
  expect(menuList()).toBeNull()
  expect(document.activeElement).toBe(menuKey())
  expect(host.calls).toEqual([])
})

test.each([
  ["SETTINGS", ".pico-panel-screen"],
  ["FIND", ".pico-library-browser"],
])("%s opens its screen, and Back puts the cursor on MENU", (name, screenClass) => {
  const { host } = home()
  press(menuKey())
  press(entry(name))
  expect(document.querySelector(screenClass)).not.toBeNull()
  act(() => host.press("back"))
  expect(document.querySelector(".pico-cart-shelf")).not.toBeNull()
  expect(menuList()).toBeNull()
  expect(document.activeElement).toBe(menuKey())
})

test("VIEW changes the layout and leaves the list open on VIEW", () => {
  home()
  press(menuKey())
  expect(entry(/^VIEW/).textContent).toContain("SHELF")
  press(entry(/^VIEW/))
  expect(screen.getByRole("list", { name: "Library by collection" })).toBeTruthy()
  expect(menuList()).not.toBeNull()
  expect(document.activeElement).toBe(entry(/^VIEW/))
  expect(entry(/^VIEW/).textContent).toContain("GRID")
})

test("the hint names what A does on the focused control, and B closes the list", () => {
  home()
  expect(hints()).toEqual(["APLAY", "BBACK"])
  aim(menuKey())
  expect(hints()).toEqual(["AMENU", "BBACK"])
  press(menuKey())
  expect(hints()).toEqual(["AFIND", "BCLOSE"])
  aim(entry("SETTINGS"))
  expect(hints()).toEqual(["ASETTINGS", "BCLOSE"])
  aim(entry(/^VIEW/))
  expect(hints()).toEqual(["AVIEW", "BCLOSE"])
})

test("the shortcut buttons still work, and close the list on the way", () => {
  const { host } = home()
  press(menuKey())
  act(() => host.press("system"))
  expect(document.querySelector(".pico-panel-screen")).not.toBeNull()
  act(() => host.press("system"))
  expect(menuList()).toBeNull()
  press(menuKey())
  act(() => host.press("options"))
  expect(document.querySelector(".pico-library-browser")).not.toBeNull()
  act(() => host.press("back"))
  expect(menuList()).toBeNull()
  act(() => host.press("menu"))
  expect(screen.getByRole("list", { name: "Library by collection" })).toBeTruthy()
})

test("attract does not cover the open list", () => {
  jest.useFakeTimers()
  home()
  press(menuKey())
  act(() => jest.advanceTimersByTime(PICO_ATTRACT_AFTER_MS + 1))
  expect(screen.queryByRole("img", { name: "Attract" })).toBeNull()
  expect(menuList()).not.toBeNull()
})

test.each([
  ["reading", { ...fixtureModel, catalog: { _tag: "Loading" } }],
  ["empty", { ...fixtureModel, catalog: { _tag: "Empty" } }],
  ["unreadable", { ...fixtureModel, catalog: { _tag: "Error", message: "The library could not be read." } }],
] as const)("MENU is offered on a %s library, where Settings matters most", (_name, model) => {
  home(model)
  press(menuKey())
  expect(entry("SETTINGS")).toBeTruthy()
})

test("MENU is not offered while a game starts or runs", () => {
  home({ ...fixtureModel, status: { _tag: "Busy", kicker: "STARTING", detail: "Preparing the game.", gameId: "hollow" } })
  expect(screen.queryByRole("button", { name: "MENU" })).toBeNull()
  cleanup()
  home({ ...fixtureModel, status: { _tag: "Running", kicker: "PLAYING", gameId: "hollow" } })
  expect(screen.queryByRole("button", { name: "MENU" })).toBeNull()
})

test("the shelf keeps its chosen cart through a trip to Settings", () => {
  const { host } = home()
  walkTo("Spelunky")
  expect(stage()).toContain("Spelunky")
  act(() => host.press("system"))
  act(() => host.press("back"))
  expect(stage()).toContain("Spelunky")
  expect(placement("Spelunky")).toBe("hero")
  expect(placement("Celeste Classic")).toBe("side")
})

test("the shelf keeps its chosen cart through a game's own screen", () => {
  const { host } = home()
  walkTo("Tetris")
  press(cart("Tetris"))
  expect(document.querySelector(".pico-game-detail")).not.toBeNull()
  act(() => host.press("back"))
  expect(stage()).toContain("Tetris")
})

test("the cursor coming onto the shelf from outside lands on the chosen cart", () => {
  home()
  walkTo("Spelunky")
  aim(menuKey())
  // Up from MENU: the host's focus mover picks the cart nearest the key.
  aim(cart("Celeste Classic"))
  expect(document.activeElement).toBe(cart("Spelunky"))
  expect(stage()).toContain("Spelunky")
})

test("the first press on a fresh home lands on the chosen cart", () => {
  const { host } = home()
  walkTo("Spelunky")
  act(() => host.press("system"))
  act(() => host.press("back"))
  expect(document.activeElement).toBe(document.body)
  // With nothing focused, the host focuses the first control in the document.
  aim(cart("Celeste Classic"))
  expect(document.activeElement).toBe(cart("Spelunky"))
})

test("a pointer on another cart chooses that cart", () => {
  home()
  walkTo("Spelunky")
  aim(menuKey())
  const celeste = cart("Celeste Classic")
  act(() => {
    celeste.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }))
    celeste.focus()
  })
  expect(document.activeElement).toBe(celeste)
  expect(stage()).toContain("Celeste Classic")
})
