/**
 * LAUNCH SETTINGS, end to end: the whole surface on a fixture host with
 * Korri's editor open, and the surface's update driven with plain values.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { launchRowId, launchSettingsFixtures as launch, withLaunchSettings } from "../src/fixtures/fixture-launch-settings"
import { ZONE_LAUNCH_SCOPE_ROW } from "../src/plan/launch-settings"
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
    const cpuMax = launchRowId("game:wario/any", "cpu.max")
    expect(update(state, { _tag: "Launch", message: { _tag: "SteppedRow", rowId: cpuMax, direction: 1 } }, ready).requests)
      .toEqual([{ _tag: "ChangeSetting", settingId: cpuMax, value: "23" }])
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
    expect(screen.getAllByText("61 % (1.10 GHz) from Profile Battery saver")).toHaveLength(4)
    expect(screen.getAllByText("from Profile Battery saver").length).toBeGreaterThan(0)
    // Zone's note stands under the focused row, the first one: RUNNER SCOPE.
    expect(screen.getByText("Which runners the values below apply to.")).toBeTruthy()
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
    // One scope only: no RUNNER SCOPE row.
    expect(screen.queryByText(byWords("Runner scope"))).toBeNull()
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

describe("launch settings: inert actions and choices by value", () => {
  test("a disabled editor action is drawn, marked disabled, and does nothing", () => {
    const host = createFixtureHost()
    const editor = { ...launch.ready, actions: launch.ready.actions.map(action => action.id === "launch:reload" ? { ...action, enabled: false } : action) }
    render(<ZoneSurface host={host} model={withLaunchSettings(fixtureModel, editor)} />)
    const reload = screen.getByRole("button", { name: /Reload/i })
    expect(reload.getAttribute("aria-disabled") === "true" || (reload as HTMLButtonElement).disabled).toBe(true)
    fireEvent.click(reload)
    expect(host.calls).toEqual([])
  })

  test("a step starts from the selected value, not from the first matching label", () => {
    const editor = {
      ...launch.ready,
      places: launch.ready.places.map(place => place.id !== "game:wario/any" ? place : {
        ...place,
        groups: [{
          title: "Profile",
          items: [{
            id: "launch:profile:pick",
            label: "Profile at next launch",
            value: "Quiet",
            interaction: {
              kind: "choice" as const,
              selected: "p2",
              choices: [{ value: "", label: "No profile" }, { value: "p1", label: "Quiet" }, { value: "p2", label: "Quiet" }],
            },
          }],
        }],
      }),
    }
    const model = withLaunchSettings(fixtureModel, editor)
    const state = zoneSurfaceStart(model, "blend")
    expect(update(state, { _tag: "Launch", message: { _tag: "SteppedRow", rowId: "launch:profile:pick", direction: 1 } }, model).requests)
      .toEqual([{ _tag: "ChangeSetting", settingId: "launch:profile:pick", value: "" }])
  })
})

describe("launch settings: places by runner scope", () => {
  const MGBA = "game:wario/runner:retroarch/mgba"
  const toMgba = (state: ReturnType<typeof zoneSurfaceStart>) => {
    let next = state
    for (let index = 0; index < 2; index++) {
      next = update(next, { _tag: "Launch", message: { _tag: "SteppedRow", rowId: ZONE_LAUNCH_SCOPE_ROW, direction: 1 } }, ready).model
    }
    return next
  }

  test("RUNNER SCOPE steps the scope on Zone's side and keeps the cursor on it; Korri hears nothing", () => {
    const state = zoneSurfaceStart(ready, "blend")
    const stepped = update(state, { _tag: "Launch", message: { _tag: "SteppedRow", rowId: ZONE_LAUNCH_SCOPE_ROW, direction: -1 } }, ready)
    expect(stepped.requests).toEqual([])
    expect(stepped.model.launchScope).toBe(3)
    expect(stepped.model.launch.focused).toBe(ZONE_LAUNCH_SCOPE_ROW)
    expect(update(stepped.model, { _tag: "Launch", message: { _tag: "PressedRow", rowId: ZONE_LAUNCH_SCOPE_ROW } }, ready).model.launchScope).toBe(0)
  })

  test("a number steps within its range on left and right, and DONE sends only a number that fits", () => {
    let state = toMgba(zoneSurfaceStart(ready, "blend"))
    const frameskip = launchRowId(MGBA, "setting:frameskip")
    expect(update(state, { _tag: "Launch", message: { _tag: "SteppedRow", rowId: frameskip, direction: 1 } }, ready).requests)
      .toEqual([{ _tag: "ChangeSetting", settingId: frameskip, value: "3" }])
    state = update(state, { _tag: "Launch", message: { _tag: "PressedRow", rowId: frameskip } }, ready).model
    expect(state.launch.keyboard?.draft).toBe("2")
    for (const char of "0") state = update(state, { _tag: "Launch", message: { _tag: "PressedKey", key: { kind: "char", char } } }, ready).model
    const tooMany = update(state, { _tag: "Launch", message: { _tag: "PressedKey", key: { kind: "done" } } }, ready)
    expect(tooMany.requests).toEqual([])
    expect(tooMany.model.launch.keyboard).toBeDefined()
    // INHERIT, the clear key, sends Korri's clear.
    expect(update(state, { _tag: "Launch", message: { _tag: "PressedClear" } }, ready).requests)
      .toEqual([{ _tag: "ChangeSetting", settingId: frameskip, value: "" }])
  })

  test("ADD SETTING moves a candidate on Zone's side; A sends the one shown", () => {
    let state = toMgba(zoneSurfaceStart(ready, "blend"))
    const add = launchRowId(MGBA, "add-setting")
    const moved = update(state, { _tag: "Launch", message: { _tag: "SteppedRow", rowId: add, direction: 1 } }, ready)
    expect(moved.requests).toEqual([])
    state = moved.model
    expect(update(state, { _tag: "Launch", message: { _tag: "PressedRow", rowId: add } }, ready).requests)
      .toEqual([{ _tag: "ChangeSetting", settingId: add, value: "lang" }])
  })

  test("rows only files set are read-only, with their source, and say where to change them when focused", () => {
    const state = zoneSurfaceStart(ready, "blend")
    const env = launchRowId("game:wario/any", "file:env:SDL_AUDIODRIVER")
    // A press on a file-only row does nothing.
    expect(update(state, { _tag: "Launch", message: { _tag: "PressedRow", rowId: env } }, ready).requests).toEqual([])
    render(<ZoneSurface host={createFixtureHost()} model={ready} />)
    expect(screen.getAllByText(byWords("Environment SDL_AUDIODRIVER")).length).toBeGreaterThan(0)
    expect(screen.getAllByText("pipewire").length).toBeGreaterThan(0)
    expect(screen.getAllByText("from Game").length).toBeGreaterThan(0)
  })

  test("Korri's lines about a missing runner and unavailable runner facts stand above the rows", () => {
    const view = render(<ZoneSurface host={createFixtureHost()} model={withLaunchSettings(fixtureModel, launch.runnerUnavailable)} />)
    expect(screen.getByText(/names runner retroarch\/vba-m, which isn't installed/)).toBeTruthy()
    view.rerender(<ZoneSurface host={createFixtureHost()} model={withLaunchSettings(fixtureModel, launch.factsUnavailable)} />)
    expect(screen.getByText("gpSP facts are unavailable: launch.prepare timed out after 2 s.")).toBeTruthy()
  })
})

describe("launch settings: modifiers", () => {
  const picked = withLaunchSettings(fixtureModel, launch.noProfilePicked)

  test("each modifier steps On, Off or Inherit on every place; a step goes to Korri", () => {
    const state = zoneSurfaceStart(picked, "blend")
    const gamescope = launchRowId("game:wario/any", "modifier:korri/gamescope:enabled")
    expect(update(state, { _tag: "Launch", message: { _tag: "SteppedRow", rowId: gamescope, direction: 1 } }, picked).requests)
      .toEqual([{ _tag: "ChangeSetting", settingId: gamescope, value: "true" }])
    render(<ZoneSurface host={createFixtureHost()} model={picked} />)
    expect(screen.getAllByText("On from Device").length).toBeGreaterThan(0)
  })

  test("a profile turning Gamescope off says so, and the Game inherits it", () => {
    render(<ZoneSurface host={createFixtureHost()} model={ready} />)
    expect(screen.getAllByText("from Profile Battery saver").length).toBeGreaterThan(0)
    expect(screen.getAllByText("Off from Profile Battery saver").length).toBeGreaterThan(0)
  })

  test("a modifier with no schema has its row and no ADD SETTING", () => {
    render(<ZoneSurface host={createFixtureHost()} model={ready} />)
    expect(screen.getAllByText(byWords("MangoHud")).length).toBeGreaterThan(0)
    expect(screen.queryByText(byWords("Add MangoHud setting"))).toBeNull()
    expect(screen.getAllByText(byWords("Add Gamescope setting")).length).toBeGreaterThan(0)
  })
})
