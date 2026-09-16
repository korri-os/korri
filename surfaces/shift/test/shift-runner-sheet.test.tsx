import { afterEach, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import type { SurfaceRunnerChoice } from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { ShiftSurface } from "../src/ShiftSurface"

afterEach(cleanup)

const choice: SurfaceRunnerChoice = {
  _tag: "Stale",
  gameTitle: "Wario Land 4",
  message: "A saved runner is missing",
  saved: [{ label: "This game", runnerId: "missing/runner" }],
  warnings: [],
  routes: ["retroarch/mgba", "retroarch/mgba-nightly"].map((runnerId, index) => ({
    runnerId,
    familyId: "@korri:retroarch",
    systemId: "gba",
    runnerBuild: `/nix/store/${index}-runner`,
    // Each runner ships its own frontend build; there is no shared launcher.
    program: `/nix/store/${index}-runner/bin/retroarch`,
    warnings: ["video_driver omitted in this exact build"],
    actions: [{ id: `runner:launch:${index}`, label: "Launch once", enabled: true }],
  })),
  actions: [{ id: "runner:cancel", label: "Cancel", enabled: true }],
}

test("the sheet renders full runner/build identities and preserves host action IDs", () => {
  const host = createFixtureHost()
  render(<ShiftSurface host={host} model={{ ...fixtureModel, runnerChoice: choice }} />)
  const panel = within(screen.getByRole("dialog", { name: "Runners for Wario Land 4" }))
  expect(panel.getByText("This game: missing/runner")).toBeTruthy()
  for (const index of [0, 1]) {
    expect(panel.getByText(`/nix/store/${index}-runner`)).toBeTruthy()
    expect(panel.getByText(`/nix/store/${index}-runner/bin/retroarch`)).toBeTruthy()
  }
  expect(panel.getAllByText("video_driver omitted in this exact build")).toHaveLength(2)
  const [first, second] = panel.getAllByRole("button", { name: "Launch once" })
  if (!first || !second) throw new Error("Expected two runner choices")
  expect(document.activeElement).toBe(first)
  fireEvent.click(second)
  act(() => host.press("back"))
  expect(host.calls).toEqual(["action:runner:launch:1", "action:runner:cancel"])
})

test("runner options are reachable from every Library game detail", () => {
  const host = createFixtureHost({
    "game:zao:neverball": [{ id: "runners", label: "Runners on this device", enabled: true }],
  })
  render(<ShiftSurface host={host} model={fixtureModel} />)
  fireEvent.focus(screen.getByRole("button", { name: "Library" }))
  fireEvent.click(screen.getByRole("button", { name: "Library" }))
  fireEvent.click(screen.getByRole("button", { name: "Neverball" }))
  fireEvent.click(screen.getByRole("button", { name: "Options" }))
  fireEvent.click(screen.getByRole("button", { name: "Runners on this device" }))
  expect(host.calls).toEqual(["game-action:game:zao:neverball:runners"])
})
