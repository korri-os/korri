import { afterEach, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { runtimeChoice } from "../src/fixtures/runtime-choice"
import { PicoSurface } from "../src/PicoSurface"

afterEach(cleanup)

test("runtime identities and commands are separate from device locations", () => {
  const host = createFixtureHost()
  render(<PicoSurface host={host} model={{ ...fixtureModel, runtimeChoice }} />)
  const panel = within(screen.getByRole("dialog", { name: "Runtimes for Wario Land 4" }))
  expect(panel.getByText("This game: missing/runtime")).toBeTruthy()
  expect(panel.getByText("/nix/store/exact-retroarch-mgba-nightly")).toBeTruthy()
  expect(panel.getAllByText("retroarch/linux")).toHaveLength(2)
  expect(panel.getAllByRole("button", { name: "Launch once" })).toHaveLength(2)
  const second = panel.getAllByRole("button", { name: "Remember for this game" })[1]
  if (!second) throw new Error("Expected two runtime choices")
  fireEvent.click(second)
  expect(host.calls).toEqual(["action:runtime:game:1"])
  act(() => host.press("back"))
  expect(host.calls).toEqual(["action:runtime:game:1", "action:runtime:cancel"])
})

test("a busy runtime panel keeps cancellation reachable and hides catalog input", () => {
  const host = createFixtureHost()
  render(
    <PicoSurface
      host={host}
      model={{
        ...fixtureModel,
        runtimeChoice: {
          ...runtimeChoice,
          _tag: "Busy",
          message: "Saving…",
          routes: [],
          saved: [],
          actions: [{ id: "runtime:cancel", label: "Cancel", enabled: true }],
        },
      }}
    />,
  )
  expect(screen.queryByRole("button", { name: /Celeste Classic/ })).toBeNull()
  expect(document.activeElement).toBe(screen.getByRole("button", { name: "Cancel" }))
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }))
  expect(host.calls).toEqual(["action:runtime:cancel"])
})
