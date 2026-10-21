/**
 * LAUNCH SETTINGS, end to end: the whole surface on a fixture host with
 * Korri's editor open, and the surface's update driven with plain values.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { launchSettingsFixtures as launch, withLaunchSettings } from "../src/fixtures/fixture-launch-settings"
import { update, zoneShown, zoneSurfaceStart } from "../src/state/surface"
import { ZoneSurface } from "../src/ZoneSurface"
import { byWords } from "./words"

afterEach(cleanup)

const ready = withLaunchSettings(fixtureModel, launch.ready)

describe("launch settings: the surface's rules", () => {
  test("the editor takes the screen over the tabs", () => {
    const state = zoneSurfaceStart(ready, "blend")
    expect(zoneShown(state, ready)).toBe("launch")
  })

  test("B closes the keyboard first, then runs Korri's close action", () => {
    let state = zoneSurfaceStart(ready, "blend")
    state = update(state, { _tag: "Launch", message: { _tag: "PressedRow", rowId: "launch:profile:new" } }, ready).model
    expect(state.launch.keyboard?.entry).toMatchObject({ _tag: "Setting", settingId: "launch:profile:new" })
    const first = update(state, { _tag: "PressedBack" }, ready)
    expect(first.model.launch.keyboard).toBeUndefined()
    expect(first.requests).toEqual([])
    expect(update(first.model, { _tag: "PressedBack" }, ready).requests).toEqual([{ _tag: "RunAction", actionId: "launch:cancel" }])
  })

  test("a step sends the next real clock step; a typed name goes as typed", () => {
    let state = zoneSurfaceStart(ready, "blend")
    expect(update(state, { _tag: "Launch", message: { _tag: "SteppedRow", rowId: "launch:set:game:cpu.max", direction: 1 } }, ready).requests)
      .toEqual([{ _tag: "ChangeSetting", settingId: "launch:set:game:cpu.max", value: "23" }])
    state = update(state, { _tag: "Launch", message: { _tag: "PressedRow", rowId: "launch:profile:new" } }, ready).model
    for (const char of "Race") state = update(state, { _tag: "Launch", message: { _tag: "PressedKey", key: { kind: "char", char } } }, ready).model
    expect(update(state, { _tag: "Launch", message: { _tag: "PressedKey", key: { kind: "done" } } }, ready).requests)
      .toEqual([{ _tag: "ChangeSetting", settingId: "launch:profile:new", value: "Race" }])
  })

  test("DELETE PROFILE asks NO / YES before Korri's action runs", () => {
    let state = zoneSurfaceStart(ready, "blend")
    const asked = update(state, { _tag: "Launch", message: { _tag: "PressedRow", rowId: "launch:profile:remove:battery" } }, ready)
    expect(asked.requests).toEqual([])
    state = asked.model
    expect(state.launch.dialog?.confirm.headline).toBe("Delete Battery saver?")
    expect(update(state, { _tag: "Launch", message: { _tag: "PressedDialogButton", index: 1 } }, ready).requests)
      .toEqual([{ _tag: "RunAction", actionId: "launch:profile:remove:battery" }])
  })

  test("only actions Korri publishes run, and menu, options and system do nothing", () => {
    const state = zoneSurfaceStart(ready, "blend")
    expect(update(state, { _tag: "LaunchAction", actionId: "launch:reload" }, ready).requests)
      .toEqual([{ _tag: "RunAction", actionId: "launch:reload" }])
    expect(update(state, { _tag: "LaunchAction", actionId: "launch:retry" }, ready).requests).toEqual([])
    for (const tag of ["PressedMenu", "PressedOptions", "PressedSystem"] as const) {
      const step = update(state, { _tag: tag }, ready)
      expect(step.requests).toEqual([])
      expect(zoneShown(step.model, ready)).toBe("launch")
    }
  })

  test("closing forgets the cursor and the questions", () => {
    let state = zoneSurfaceStart(ready, "blend")
    state = update(state, { _tag: "Launch", message: { _tag: "PressedRow", rowId: "launch:profile:new" } }, ready).model
    const closed: SurfaceModel = { ...fixtureModel, launchSettings: { _tag: "Closed" } }
    state = update(state, { _tag: "KorriPublished" }, closed).model
    expect(state.launch.keyboard).toBeUndefined()
    expect(zoneShown(state, closed)).toBe("games")
  })
})

describe("launch settings: the page", () => {
  test("every value row says its source and DEVICE-ONLY; the focused row its note", () => {
    render(<ZoneSurface host={createFixtureHost()} model={ready} />)
    expect(screen.getByText(byWords("Launch settings"))).toBeTruthy()
    // CPU max on every layer but the profile names the profile that sets it.
    expect(screen.getAllByText("61 % (1.10 GHz) from Profile Battery saver")).toHaveLength(5)
    expect(screen.getByText("from Profile Battery saver")).toBeTruthy()
    // Korri's note stands under the focused row, the first one.
    expect(screen.getByText("Applies at next launch.")).toBeTruthy()
    expect(screen.getAllByText("Device-only").length).toBeGreaterThan(10)
  })

  test("Korri's actions are buttons, and B on the page asks Korri to close", () => {
    const host = createFixtureHost()
    render(<ZoneSurface host={host} model={ready} />)
    fireEvent.click(screen.getByRole("button", { name: /Reload/i }))
    act(() => host.press("back"))
    expect(host.calls).toEqual(["action:launch:reload", "action:launch:cancel"])
  })

  test("missing route layers keep the rest and say why, with CHOOSE A RUNNER", () => {
    const host = createFixtureHost()
    render(<ZoneSurface host={host} model={withLaunchSettings(fixtureModel, launch.needsRunner)} />)
    expect(screen.getByText(/appear once you choose a runner\. Values in force leave them out\./)).toBeTruthy()
    expect(screen.queryByRole("region", { name: "Runner mGBA" })).toBeNull()
    fireEvent.click(screen.getByRole("button", { name: /Choose a runner/i }))
    expect(host.calls).toEqual(["action:launch:runner"])
  })

  test("a conflict shows Korri's message and actions instead of the rows", () => {
    render(<ZoneSurface host={createFixtureHost()} model={withLaunchSettings(fixtureModel, launch.conflict)} />)
    expect(screen.getByText(/changed elsewhere/)).toBeTruthy()
    expect(screen.queryByRole("region", { name: "Device" })).toBeNull()
  })

  test("no devfreq GPU: one read-only GPU line", () => {
    render(<ZoneSurface host={createFixtureHost()} model={withLaunchSettings(fixtureModel, launch.noGpu)} />)
    expect(screen.getAllByText(byWords("GPU limits aren't available on this device")).length).toBeGreaterThan(0)
  })
})
