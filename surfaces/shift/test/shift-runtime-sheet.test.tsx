import { afterEach, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import type { SurfaceRuntimeChoice } from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { ShiftSurface } from "../src/ShiftSurface"

afterEach(cleanup)

const choice: SurfaceRuntimeChoice = {
  _tag: "Stale",
  gameTitle: "Wario Land 4",
  message: "A saved runtime is missing",
  saved: [{ label: "This game", runtimeId: "missing/runtime" }],
  warnings: [],
  routes: ["retroarch/mgba", "retroarch/mgba-nightly"].map((runtimeId, index) => ({
    runtimeId,
    launcherId: "retroarch/linux",
    launcherKind: "retroarch",
    systemId: "gba",
    runtimeBuild: `/nix/store/${index}-runtime`,
    launcherBuild: "/nix/store/exact-launcher",
    program: "/nix/store/exact-launcher/bin/retroarch",
    warnings: ["video_driver omitted in this exact build"],
    actions: [{ id: `runtime:launch:${index}`, label: "Launch once", enabled: true }],
  })),
  actions: [{ id: "runtime:cancel", label: "Cancel", enabled: true }],
}

test("the sheet renders full runtime/build identities and preserves host action IDs", () => {
  const host = createFixtureHost()
  render(<ShiftSurface host={host} model={{ ...fixtureModel, runtimeChoice: choice }} />)
  const panel = within(screen.getByRole("dialog", { name: "Runtimes for Wario Land 4" }))
  expect(panel.getByText("This game: missing/runtime")).toBeTruthy()
  expect(panel.getAllByText("/nix/store/exact-launcher")).toHaveLength(2)
  expect(panel.getAllByText("video_driver omitted in this exact build")).toHaveLength(2)
  const [first, second] = panel.getAllByRole("button", { name: "Launch once" })
  if (!first || !second) throw new Error("Expected two runtime choices")
  expect(document.activeElement).toBe(first)
  fireEvent.click(second)
  act(() => host.press("back"))
  expect(host.calls).toEqual(["action:runtime:launch:1", "action:runtime:cancel"])
})

test("runtime options are reachable from every Library game detail", () => {
  const host = createFixtureHost({
    "game:zao:neverball": [{ id: "runtimes", label: "Runtimes on this device", enabled: true }],
  })
  render(<ShiftSurface host={host} model={fixtureModel} />)
  fireEvent.focus(screen.getByRole("button", { name: "Library" }))
  fireEvent.click(screen.getByRole("button", { name: "Library" }))
  fireEvent.click(screen.getByRole("button", { name: "Neverball" }))
  fireEvent.click(screen.getByRole("button", { name: "Options" }))
  fireEvent.click(screen.getByRole("button", { name: "Runtimes on this device" }))
  expect(host.calls).toEqual(["game-action:game:zao:neverball:runtimes"])
})
