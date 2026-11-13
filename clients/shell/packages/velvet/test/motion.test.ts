/**
 * Motion and time, against the small Korri. Velvet has no clock and no
 * timers: idle time is the shell's Tick, "now" is the model's.
 *
 *  - The stage drifts after two seconds of rest and stops on any press.
 *  - `velvet.motion` (mark `device`) is "full" or "reduced". Reduced lands
 *    every change at once (`still`) and never drifts. Settings holds the
 *    row; choosing it asks the shell to remember the value.
 *  - `Resized` comes before the first view, and the plan follows it.
 *  - The same state and model give the same view object back.
 */
import { describe, expect, test } from "bun:test"
import "../logic"
import { DRIFT_AFTER_MS, MOTION_KEY } from "../src/update"
import { Play, scenario, targets } from "./korri"

const model = () => scenario("home").models[0]

describe("the idle drift", () => {
  test("starts after two seconds of rest and stops at once on a press", () => {
    const run = new Play(model())
    expect(run.view.stage.drift).toBe(false)
    run.deliver({ _tag: "Tick", idleMs: DRIFT_AFTER_MS - 1 })
    expect(run.view.stage.drift).toBe(false)
    run.deliver({ _tag: "Tick", idleMs: DRIFT_AFTER_MS })
    expect(run.view.stage.drift).toBe(true)
    run.press("down")
    expect(run.view.stage.drift).toBe(false)
  })
  test("a tap stops it too", () => {
    const run = new Play(model())
    run.deliver({ _tag: "Tick", idleMs: 5000 })
    expect(run.view.stage.drift).toBe(true)
    run.deliver({ _tag: "Activated", target: "row:1" })
    expect(run.view.stage.drift).toBe(false)
  })
  test("nothing drifts under a sheet or on a list page", () => {
    const run = new Play(model())
    run.press("menu")
    run.deliver({ _tag: "Tick", idleMs: 5000 })
    expect(run.view.stage.drift).toBe(false)
  })
})

describe("velvet.motion", () => {
  test("is declared, on the device, full at first", () => {
    expect(globalThis.surface.preferences).toEqual({ [MOTION_KEY]: { mark: "device", initial: "full" } })
  })
  test("reduced: every change lands at once and the stage never drifts", () => {
    const run = new Play(model(), 1280, 720, { [MOTION_KEY]: "reduced" })
    expect(run.view.still).toBe(true)
    run.deliver({ _tag: "Tick", idleMs: 60_000 })
    expect(run.view.stage.drift).toBe(false)
  })
  test("full, or anything else stored: motion", () => {
    expect(new Play(model(), 1280, 720, { [MOTION_KEY]: "full" }).view.still).toBe(false)
    expect(new Play(model(), 1280, 720, { [MOTION_KEY]: "banana" }).view.still).toBe(false)
    expect(new Play(model()).view.still).toBe(false)
  })
  test("Settings holds the row; choosing it asks the shell to remember", () => {
    const run = new Play(model())
    run.press("menu", "down", "confirm")
    expect(run.view.list.title).toBe("SETTINGS")
    const rows = run.view.listRows as any[]
    const row = rows.find(entry => entry.focusId === "v:motion")
    expect(row).toBeDefined()
    expect(row.label).toBe("Motion")
    let guard = 0
    while (targets(run.view).find(entry => entry.focused)?.focusId !== "v:motion" && guard++ < 60) run.press("down")
    const made = run.press("confirm", "confirm", "right", "confirm").concat(run.requests.slice(-3)) as any[]
    const remembered = run.requests.filter((request: any) => request._tag === "RememberPreference") as any[]
    expect(made.length).toBeGreaterThan(0)
    expect(remembered.length).toBeGreaterThan(0)
    expect(remembered[0].key).toBe(MOTION_KEY)
    expect(["full", "reduced"]).toContain(remembered[0].value)
    expect(run.view.still).toBe(remembered.at(-1).value === "reduced")
  })
})

describe("Resized", () => {
  test("the plan follows the window", () => {
    const run = new Play(model(), 320, 240)
    expect(run.view.screen.width).toBe(320)
    expect(run.view.screen.narrow).toBe(false)
    run.deliver({ _tag: "Resized", width: 480, height: 800 })
    expect(run.view.screen.width).toBe(480)
    expect(run.view.screen.narrow).toBe(true)
  })
  test("a resize keeps where the person is", () => {
    const scene = scenario("set-sorted")
    const run = new Play(scene.models[0], 1920, 1080)
    for (const step of scene.steps) run.step(step, scene.models)
    const before = targets(run.view).find(entry => entry.focused)?.focusId
    run.deliver({ _tag: "Resized", width: 480, height: 800 })
    expect(run.view.page).toBe("set")
    expect(targets(run.view).find(entry => entry.focused)?.focusId).toBe(before)
    run.deliver({ _tag: "Resized", width: 640, height: 480 })
    expect(targets(run.view).find(entry => entry.focused)?.focusId).toBe(before)
  })
})

describe("the view", () => {
  test("the same state and model give the same view back", () => {
    const run = new Play(model())
    expect(globalThis.surface.view(run.state, run.model)).toBe(globalThis.surface.view(run.state, run.model))
  })
  test("a view is plain data: it survives JSON unchanged", () => {
    for (const name of ["home", "lobby", "set", "detail", "settings", "overlay-stream", "busy-steps"]) {
      const scene = scenario(name)
      const run = new Play(scene.models[0])
      for (const step of scene.steps) run.step(step, scene.models)
      expect(JSON.parse(JSON.stringify(run.view)), name).toEqual(run.view)
    }
  })
})
