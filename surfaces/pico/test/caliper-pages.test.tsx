import { afterEach, expect, test } from "bun:test"
import { act, cleanup, render, within } from "@testing-library/react"
import { createElement, type ComponentType } from "react"
import { createPicoAdapter } from "../caliper/adapter"
import FindPart from "../src/pages/PicoLibrary.page.part"
import SettingsPart from "../src/pages/PicoSettings.page.part"
import DetailPart from "../src/pages/PicoGameDetail.page.part"
import OverlayPart from "../src/pages/PicoOverlay.page.part"
import HomePart from "../src/pages/PicoHome.page.part"

afterEach(cleanup)
const story = (Part: ComponentType) => ({ layer: "page" as const, render: () => createElement(Part) })

function mountPage(Part: ComponentType, sourceId = "ready", scopeId = "part", adapter = createPicoAdapter()) {
  const spec = adapter.surfacePartMount(story(Part), { sourceId, inputValues: {} })!
  let host: { reseed(value: unknown, context: { scopeId?: string }): void } | undefined
  const rendered = render(createElement(adapter.partRegistryRoot, {
    initialValues: spec.initialValues, scopeId, onHost: next => { host = next },
  }, spec.node))
  return { adapter, ...rendered, reseed(source: string) {
    const next = adapter.surfacePartMount(story(Part), { sourceId: source, inputValues: {} })!
    act(() => host!.reseed(next.initialValues, { scopeId }))
  } }
}

test("placed Find types, opens a result and asks for a launch location", () => {
  const { container } = mountPage(FindPart)
  const ui = within(container)
  act(() => ui.getByRole("button", { name: "Type T" }).click())
  expect(container.querySelector(".pico-query-field-text")?.textContent).toBe("T")
  act(() => ui.getByRole("button", { name: /Tetris, GB/ }).click())
  expect(container.querySelector(".pico-game-detail")).not.toBeNull()
  act(() => ui.getByRole("button", { name: /PLAY/ }).click())
  expect(ui.getByRole("button", { name: "This device" })).toBeTruthy()
  act(() => ui.getByRole("button", { name: "zao" }).click())
  expect(container.textContent).toContain("PREVIEW: Starting game")
})

test("placed Settings updates choices and reseeds saving/failure without remounting", () => {
  const page = mountPage(SettingsPart)
  const ui = within(page.container)
  act(() => ui.getByRole("tab", { name: "PLUGINS" }).click())
  act(() => ui.getByRole("button", { name: /mGBA/ }).click())
  expect(page.container.querySelector('[data-lit="true"]')?.textContent).toBe("OFF")
  const root = page.container.querySelector(".pico-screen")
  page.reseed("settings-saving")
  expect(ui.getByText("SAVING")).toBeTruthy()
  expect(page.container.querySelector(".pico-screen")).toBe(root)
  page.reseed("settings-problem")
  expect(page.container.textContent).toContain("PREVIEW: Setting was not saved")
})

test("placed Detail inherits the authored game and runs confirmations", () => {
  const { container } = mountPage(DetailPart)
  const ui = within(container)
  expect(ui.getByRole("heading", { name: "Hollow Knight" })).toBeTruthy()
  act(() => ui.getByRole("button", { name: "Remove from device" }).click())
  expect(ui.getByRole("dialog")).toBeTruthy()
  expect(container.querySelector(".pico-launch-stage")).toBeNull()
})

test("placed Overlay starts in its own presentation and updates controls", () => {
  const { container } = mountPage(OverlayPart)
  const ui = within(container)
  act(() => ui.getByRole("button", { name: /Fast forward/ }).click())
  expect(ui.getByRole("button", { name: /Fast forward/ }).textContent).toContain("ON")
})

test("sources and scoped inputs reach placed pages, not their neighbours", () => {
  const one = mountPage(HomePart, "catalog-error", "one")
  expect(one.container.textContent).toContain("PREVIEW: Library could not be read")
  one.reseed("running")
  expect(one.container.textContent).toContain("PREVIEW: Game running")
  one.reseed("ready")
  const two = mountPage(HomePart, "ready", "two", one.adapter)
  act(() => one.adapter.surfacePartEvents(story(HomePart))[0]!.emit("options", { scopeId: "one" }))
  expect(one.container.querySelector(".pico-library-browser")).not.toBeNull()
  expect(two.container.querySelector(".pico-library-browser") === null).toBe(true)
})
