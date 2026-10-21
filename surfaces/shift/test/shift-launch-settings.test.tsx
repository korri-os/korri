import { afterEach, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { launchSettingsFixtures as launch, withLaunchSettings } from "../src/fixtures/launch-settings-fixtures"
import { ShiftSurface } from "../src/ShiftSurface"

afterEach(cleanup)

const ready = withLaunchSettings(fixtureModel, launch.ready)

test("the editor takes the screen: the game's title, a group per layer, each row's source and mark", () => {
  render(<ShiftSurface host={createFixtureHost()} model={ready} />)
  expect(screen.getByText("Launch settings")).toBeTruthy()
  expect(screen.getByText("Wario Land 4")).toBeTruthy()
  for (const layer of ["Device", "System gba", "Runner mGBA", "Game", "Release usa", "Profile Battery saver"]) {
    expect(screen.getByText(layer)).toBeTruthy()
  }
  const row = screen.getAllByRole("button", { name: /^CPU max/ })[0]
  if (row === undefined) throw new Error("no CPU max row")
  expect(within(row).getByText("61 % (1.10 GHz) from Profile Battery saver")).toBeTruthy()
  expect(within(row).getByText("Device-only")).toBeTruthy()
  expect(within(row).getByText("Applies at next launch.")).toBeTruthy()
  expect(document.activeElement?.textContent).toContain("CPU min")
})

test("a percent opens the chooser with the device's clock steps; a choice goes to Korri unchanged", () => {
  const host = createFixtureHost()
  render(<ShiftSurface host={host} model={ready} />)
  fireEvent.click(screen.getAllByRole("button", { name: /^CPU max/ })[0]!)
  fireEvent.click(screen.getByRole("button", { name: /^61 % \(1\.10 GHz\)/ }))
  expect(host.calls).toContain("setting:launch:set:device:cpu.max:61")
})

test("Back leaves through Korri's close action; Reload is Korri's", () => {
  const host = createFixtureHost()
  render(<ShiftSurface host={host} model={ready} />)
  fireEvent.click(screen.getByRole("button", { name: "Reload" }))
  act(() => host.press("back"))
  expect(host.calls.filter(call => call.startsWith("action:"))).toEqual(["action:launch:reload", "action:launch:cancel"])
})

test("missing route layers keep the rest, say why, and offer Choose a runner", () => {
  const host = createFixtureHost()
  render(<ShiftSurface host={host} model={withLaunchSettings(fixtureModel, launch.needsRunner)} />)
  expect(screen.getByRole("status").textContent).toContain("appear once you choose a runner. Values in force leave them out.")
  expect(screen.queryByText("Runner mGBA")).toBeNull()
  fireEvent.click(screen.getByRole("button", { name: "Choose a runner" }))
  expect(host.calls).toContain("action:launch:runner")
})

test("a conflict or a failure shows Korri's words and actions instead of the layers", () => {
  const host = createFixtureHost()
  const view = render(<ShiftSurface host={host} model={withLaunchSettings(fixtureModel, launch.conflict)} />)
  expect(screen.getByRole("status").textContent).toContain("changed elsewhere")
  expect(screen.queryByText("Device")).toBeNull()
  view.rerender(<ShiftSurface host={host} model={withLaunchSettings(fixtureModel, launch.error)} />)
  fireEvent.click(screen.getByRole("button", { name: "Retry" }))
  expect(host.calls).toContain("action:launch:retry")
})

test("no devfreq GPU: one read-only line; no facts: steps unknown", () => {
  const view = render(<ShiftSurface host={createFixtureHost()} model={withLaunchSettings(fixtureModel, launch.noGpu)} />)
  expect(screen.getAllByText("GPU limits aren't available on this device").length).toBeGreaterThan(0)
  view.rerender(<ShiftSurface host={createFixtureHost()} model={withLaunchSettings(fixtureModel, launch.factsMissing)} />)
  expect(screen.getAllByText("Applies at next launch. Clock steps unknown.").length).toBeGreaterThan(0)
})

test("Delete profile asks first, in Korri's words", () => {
  const host = createFixtureHost()
  render(<ShiftSurface host={host} model={ready} />)
  fireEvent.click(screen.getByRole("button", { name: "Delete profile" }))
  const question = within(screen.getByRole("dialog", { name: "Delete Battery saver?" }))
  expect(question.getByText("Its values go. The game's other launch settings stay.")).toBeTruthy()
  fireEvent.click(question.getByRole("button", { name: "Delete profile" }))
  expect(host.calls).toContain("action:launch:profile:remove:battery")
})
