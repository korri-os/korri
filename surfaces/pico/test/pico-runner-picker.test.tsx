import { afterEach, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { runnerChoice } from "../src/fixtures/runner-choice"
import { PicoSurface } from "../src/PicoSurface"

afterEach(cleanup)

test("runner identities and commands are separate from device locations", () => {
  const host = createFixtureHost()
  render(<PicoSurface host={host} model={{ ...fixtureModel, runnerChoice }} />)
  const panel = within(screen.getByRole("dialog", { name: "Runners for Wario Land 4" }))
  expect(panel.getByText("This game: missing/runner")).toBeTruthy()
  expect(panel.getByText("/nix/store/exact-retroarch-mgba-nightly")).toBeTruthy()
  expect(
    panel.getByText("/nix/store/exact-retroarch-mgba-nightly/bin/retroarch"),
  ).toBeTruthy()
  // The shared family is a settings scope; each runner still names its own build.
  expect(panel.getAllByText("@korri:retroarch")).toHaveLength(2)
  expect(panel.getAllByRole("button", { name: "Launch once" })).toHaveLength(2)
  const second = panel.getAllByRole("button", { name: "Remember for this game" })[1]
  if (!second) throw new Error("Expected two runner choices")
  fireEvent.click(second)
  expect(host.calls).toEqual(["action:runner:game:1"])
  act(() => host.press("back"))
  expect(host.calls).toEqual(["action:runner:game:1", "action:runner:cancel"])
})

test("a busy runner panel keeps cancellation reachable and hides catalog input", () => {
  const host = createFixtureHost()
  render(
    <PicoSurface
      host={host}
      model={{
        ...fixtureModel,
        runnerChoice: {
          ...runnerChoice,
          _tag: "Busy",
          message: "Saving…",
          routes: [],
          saved: [],
          actions: [{ id: "runner:cancel", label: "Cancel", enabled: true }],
        },
      }}
    />,
  )
  expect(screen.queryByRole("button", { name: /Celeste Classic/ })).toBeNull()
  expect(document.activeElement).toBe(screen.getByRole("button", { name: "Cancel" }))
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }))
  expect(host.calls).toEqual(["action:runner:cancel"])
})
