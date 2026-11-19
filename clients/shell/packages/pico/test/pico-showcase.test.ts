/**
 * What the showcase draws. Ported from surfaces/pico/test/pico-displays.test.tsx;
 * the DOM cases there are main.slint's now.
 */
import { describe, expect, test } from "bun:test"
import type { SurfaceModel } from "../src/model"
import { fixtureModel } from "./fixtures/fixture-host"
import { picoShowcaseFrom } from "../src/pico-showcase-view"
import { initialNavigation } from "../src/state/navigation"
import { shownScreen } from "../src/state/shown"
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
