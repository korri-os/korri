/**
 * Touch (ADR 0055), against the small Korri. The shell sends `Touched` when
 * a finger goes down, `Activated` with a target's id for a tap, and
 * `Dragged` for a region dragged by whole tiles or rows.
 *
 *  - A tap on a game tile moves the focus there; a second tap opens it.
 *  - A tap on a button does what Confirm does there.
 *  - While touch is in use the focus highlight hides (`touching`); the next
 *    button press brings it back, and brings a dragged row back to its focus.
 *  - A drag moves what is shown, never the focus.
 *  - A touch screen has no Back button: `wayBack` shows the chip wherever
 *    there is a level to go back to, and its tap is Back. A tap beside a
 *    sheet, or over the keyboard's field, is Back too.
 *  - Under a curtain, only the curtain's buttons answer.
 */
import { describe, expect, test } from "bun:test"
import { Play, scenario, targets } from "./korri"

const home = () => new Play(scenario("home").models[0])
const tap = (run: Play, target: string) => run.deliver({ _tag: "Activated", target })
const focus = (run: Play) => targets(run.view).find(entry => entry.focused === true)

describe("a tap", () => {
  test("on a row tile: the focus goes there; again: the detail opens", () => {
    const run = home()
    run.deliver({ _tag: "Touched" })
    tap(run, "row:2")
    expect(run.view.page).toBe("home")
    expect(run.view.home.depth).toBe(1)
    expect(focus(run)?.focusId).toBe("row:2")
    expect(run.requests.filter((request: any) => request._tag === "FocusGame").at(-1)).toEqual({ _tag: "FocusGame", gameId: "moss-and-stone" } as any)
    tap(run, "row:2")
    expect(run.view.page).toBe("detail")
    expect(run.view.detail.mark.key).toBe("moss-and-stone")
  })
  test("on Play: what Confirm does", () => {
    const run = home()
    tap(run, "play")
    expect(run.view.page).toBe("detail")
    expect(tap(run, "d:play")).toEqual([{ _tag: "LaunchGame", gameId: "ember-road" }] as any)
  })
  test("on the Menu pill: the house menu; on a row of it: that row's page", () => {
    const run = home()
    tap(run, "menu")
    expect(run.view.layer).toBe("sheet")
    tap(run, "m:settings")
    expect(run.view.page).toBe("list")
    expect(run.view.list.title).toBe("SETTINGS")
  })
  test("on a target that is not there: nothing", () => {
    const run = home()
    const before = run.state
    expect(tap(run, "row:99")).toEqual([])
    expect(tap(run, "nonsense")).toEqual([])
    expect(run.view.page).toBe("home")
    expect({ ...run.state, touched: before.touched }).toEqual(before)
  })
})

describe("the highlight", () => {
  test("hides while touch is in use, and the next button press brings it back", () => {
    const run = home()
    expect(run.view.touching).toBe(false)
    run.deliver({ _tag: "Touched" })
    expect(run.view.touching).toBe(true)
    run.press("down")
    expect(run.view.touching).toBe(false)
    tap(run, "row:1")
    expect(run.view.touching).toBe(true)
  })
})

describe("a drag", () => {
  test("moves a shelf by whole tiles and leaves the focus where it was", () => {
    const run = home()
    run.press("down", "down")
    const before = focus(run)?.focusId
    run.deliver({ _tag: "Dragged", region: "shelf:systems", by: 3 })
    expect(focus(run)?.focusId).toBe(before)
    expect((run.view.shelves as any[]).find(shelf => shelf.key === "systems").at).toBe(3)
    // A button brings every shelf back to its focus.
    run.press("down")
    expect((run.view.shelves as any[]).find(shelf => shelf.key === "systems").at).toBe(0)
  })
  test("moves a set by whole rows, inside its ends", () => {
    const scene = scenario("set")
    const run = new Play(scene.models[0], 640, 480)
    for (const step of scene.steps) run.step(step, scene.models)
    const first = run.view.set.firstRow
    run.deliver({ _tag: "Dragged", region: "grid", by: 2 })
    expect(run.view.set.firstRow).toBe(first + 2)
    run.deliver({ _tag: "Dragged", region: "grid", by: 500 })
    expect(run.view.set.firstRow).toBeLessThan(run.view.set.rows)
    run.deliver({ _tag: "Dragged", region: "grid", by: -900 })
    expect(run.view.set.firstRow).toBe(0)
  })
  test("of nothing, or of a region that is not shown, changes nothing", () => {
    const run = home()
    const before = JSON.stringify(run.view)
    run.deliver({ _tag: "Dragged", region: "grid", by: 2 })
    run.deliver({ _tag: "Dragged", region: "row", by: 0 })
    expect(JSON.stringify({ ...run.view, touching: false, wayBack: false })).toBe(before)
  })
})

describe("the way back", () => {
  test("no chip while buttons are in use, and none where there is no level to go back to", () => {
    const run = home()
    expect(run.view.wayBack).toBe(false)
    run.deliver({ _tag: "Touched" })
    expect(run.view.wayBack).toBe(false)
  })
  test("the chip stands on the row, the lobby, a set, the detail and a list page; its tap is Back", () => {
    const run = home()
    tap(run, "row:1")
    expect(run.view.wayBack).toBe(true)
    tap(run, "row:1")
    expect(run.view.page).toBe("detail")
    expect(run.view.wayBack).toBe(true)
    tap(run, "back")
    expect(run.view.page).toBe("home")
    expect(run.view.home.depth).toBe(1)
    tap(run, "back")
    expect(run.view.home.depth).toBe(0)
    expect(run.view.wayBack).toBe(false)
    tap(run, "menu")
    tap(run, "m:settings")
    expect(run.view.wayBack).toBe(true)
    tap(run, "back")
    expect(run.view.page).toBe("home")
  })
  test("a tap beside a sheet closes it; over the keyboard's field, it puts the keyboard away", () => {
    const run = home()
    tap(run, "menu")
    expect(run.view.layer).toBe("sheet")
    tap(run, "back")
    expect(run.view.layer).toBe("screen")
    const scene = scenario("settings-text")
    const typing = new Play(scene.models[0])
    for (const step of scene.steps) typing.step(step, scene.models)
    expect(typing.view.layer).toBe("keyboard")
    tap(typing, "back")
    expect(typing.view.layer).toBe("screen")
  })
  test("under a curtain only the curtain answers", () => {
    const scene = scenario("problem")
    const run = new Play(scene.models[0])
    expect(run.view.layer).toBe("curtain")
    expect(tap(run, "back")).toEqual([])
    expect(tap(run, "play")).toEqual([])
    expect(run.view.layer).toBe("curtain")
    expect(tap(run, "x:0").length).toBeGreaterThan(0)
  })
})

describe("a range", () => {
  test("a tap along the track sets the value there", () => {
    const scene = scenario("overlay-groups")
    const run = new Play(scene.models[0])
    const range = (run.view.rows as any[]).find(entry => entry.kind === "range")
    expect(range).toBeDefined()
    const made = run.deliver({ _tag: "Activated", target: range.focusId, at: 0.25, released: true }) as any[]
    expect(made.length).toBe(1)
    expect(JSON.stringify(made[0])).toMatch(/25/)
  })
})
