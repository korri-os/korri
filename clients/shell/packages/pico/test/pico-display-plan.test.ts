/**
 * Which display shows and which chooses, from the rectangles alone
 * (src/pico-display-plan.ts), and what the showcase draws. Ported from
 * surfaces/pico/test/pico-displays.test.tsx; the DOM cases there are
 * main.slint's now, checked by the five-size renders.
 */
import { describe, expect, test } from "bun:test"
import type { SurfaceDisplay, SurfaceModel } from "../src/model"
import { fixtureModel } from "./fixtures/fixture-host"
import { rgdsDisplays, sideBySideDisplays } from "./fixtures/displays"
import { picoDisplayPlan } from "../src/pico-display-plan"
import { picoShowcaseFrom } from "../src/pico-showcase-view"
import { initialNavigation } from "../src/state/navigation"
import { shownScreen } from "../src/state/shown"
const display = (overrides: Partial<SurfaceDisplay> & { readonly id: string }): SurfaceDisplay => ({
  main: false,
  x: 0,
  y: 0,
  width: 640,
  height: 480,
  touch: true,
  ...overrides,
})

describe("the plan reads the rectangles", () => {
  test("no displays, none, or one is a single screen", () => {
    expect(picoDisplayPlan(undefined)).toBeUndefined()
    expect(picoDisplayPlan([])).toBeUndefined()
    expect(picoDisplayPlan([display({ id: "only", main: true })])).toBeUndefined()
  })

  test("on the RG DS the main display above shows and the one below chooses", () => {
    const plan = picoDisplayPlan(rgdsDisplays)
    expect(plan?.show.id).toBe("top")
    expect(plan?.choose.id).toBe("bottom")
  })

  test("side by side, the main display shows wherever it stands", () => {
    const plan = picoDisplayPlan(sideBySideDisplays)
    expect(plan?.show).toMatchObject({ id: "right", x: 640, y: 0 })
    expect(plan?.choose).toMatchObject({ id: "left", x: 0, y: 0 })
  })

  test("the host's order does not decide which display shows", () => {
    const plan = picoDisplayPlan([...rgdsDisplays].reverse())
    expect(plan?.show.id).toBe("top")
    expect(plan?.choose.id).toBe("bottom")
  })

  test("with no display marked main, the first shows", () => {
    const plan = picoDisplayPlan([display({ id: "a" }), display({ id: "b", y: 480 })])
    expect(plan?.show.id).toBe("a")
    expect(plan?.choose.id).toBe("b")
  })

  test("a touch display chooses before a larger one without touch", () => {
    const plan = picoDisplayPlan([
      display({ id: "main", main: true }),
      display({ id: "tv", x: 640, width: 1920, height: 1080, touch: false }),
      display({ id: "pad", y: 480, touch: true }),
    ])
    expect(plan?.choose.id).toBe("pad")
  })

  test("among touch displays the larger chooses; a third stays dark", () => {
    const plan = picoDisplayPlan([
      display({ id: "main", main: true }),
      display({ id: "small", y: 480, width: 320, height: 240 }),
      display({ id: "large", x: 640, width: 1280, height: 720 }),
    ])
    expect(plan?.show.id).toBe("main")
    expect(plan?.choose.id).toBe("large")
  })

  test("a display with no room is not a display to draw on", () => {
    expect(picoDisplayPlan([display({ id: "main", main: true }), display({ id: "none", width: 0 })])).toBeUndefined()
  })
})
describe("the showcase follows what the catalog shows", () => {
  const shown = (model: SurfaceModel, nav = initialNavigation()) => picoShowcaseFrom(shownScreen(nav, model))

  test("the shelf shows the chosen cart with its place on the shelf", () => {
    const nav = initialNavigation()
    const view = shown(fixtureModel, { ...nav, home: { ...nav.home, selectedGameId: "tetris" } })
    expect(view._tag).toBe("Game")
    if (view._tag !== "Game") return
    expect(view.game.title).toBe("Tetris")
    expect(view.game.position).toEqual({ index: 3, total: 9 })
  })

  test("with nothing chosen, the first cart, as the shelf's own stage does", () => {
    const view = shown(fixtureModel)
    expect(view._tag === "Game" && view.game.title).toBe("Celeste Classic")
    expect(view._tag === "Game" && view.game.kicker).toBe("CONTINUE")
  })

  test("the hero shows its pick and why it leads", () => {
    const nav = initialNavigation()
    const view = shown(fixtureModel, { ...nav, home: { ...nav.home, mode: "hero" } })
    expect(view._tag === "Game" && view.game.title).toBe("Hollow Knight")
    expect(view._tag === "Game" && view.game.kicker).toBe("LAST PLAYED")
  })

  test("a game's own screen shows that game", () => {
    const nav = initialNavigation()
    const view = shown(fixtureModel, { ...nav, detail: { gameId: "hollow", question: { _tag: "None" } } })
    expect(view._tag === "Game" && view.game.title).toBe("Hollow Knight")
    expect(view._tag === "Game" && view.game.resumable).toBe(true)
  })

  test("a launch shows its words, its bytes and its cart, never its actions", () => {
    const view = shown({
      ...fixtureModel,
      status: {
        _tag: "Busy",
        kicker: "Downloading FEX rootfs",
        progress: { done: 1, total: 4 },
        gameId: "hollow",
        actions: [{ id: "cancel", label: "Cancel", enabled: true }],
      },
    })
    expect(view).toMatchObject({ _tag: "Launch", phase: "starting", kicker: "Downloading FEX rootfs", progress: { done: 1, total: 4 } })
    expect(view._tag === "Launch" && view.cart?.title).toBe("Hollow Knight")
    expect(Object.keys(view)).not.toContain("actions")
  })

  test("running and paused are the launch stage at rest", () => {
    expect(shown({ ...fixtureModel, status: { _tag: "Running", kicker: "PLAYING", gameId: "hollow" } }))
      .toMatchObject({ _tag: "Launch", phase: "running" })
  })

  test("a library read, empty or failed, and every question, show the empty slot", () => {
    for (const catalog of [
      { _tag: "Loading" },
      { _tag: "Empty" },
      { _tag: "Error", message: "The library could not be read." },
    ] as const) {
      expect(shown({ ...fixtureModel, catalog })).toEqual({ _tag: "Idle" })
    }
    expect(shown({ ...fixtureModel, status: { _tag: "Problem", kicker: "COULD NOT START", reason: "No.", canRetry: true } }))
      .toEqual({ _tag: "Idle" })
    const nav = initialNavigation()
    expect(shown(fixtureModel, { ...nav, find: true })).toEqual({ _tag: "Idle" })
  })
})
