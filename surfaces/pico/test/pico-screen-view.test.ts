/**
 * Which of Korri's two axes wins on screen.
 *
 * The catalog says what exists; the status says what is happening. They can
 * disagree — a full library while a launch is failing is the normal case — and
 * this is the one place that resolves it.
 */
import { describe, expect, test } from "bun:test"
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { fixtureModel } from "../src/fixtures/fixture-host"
import { picoScreenViewFromModel } from "../src/pico-screen-view"

const model = (overrides: Partial<SurfaceModel>): SurfaceModel => ({
  ...fixtureModel,
  ...overrides,
})

describe("picoScreenViewFromModel", () => {
  test("shows the shelf when nothing is happening", () => {
    expect(picoScreenViewFromModel(fixtureModel)._tag).toBe("Shelf")
  })

  test("a launch outranks a ready catalog", () => {
    // The shelf must not be drawn over a starting game: it would invite the
    // user to launch a second one on top of the first.
    const view = picoScreenViewFromModel(
      model({ status: { _tag: "Busy", kicker: "STARTING", detail: "Mounting" } }),
    )

    expect(view).toEqual({
      _tag: "Busy",
      kicker: "STARTING",
      detail: "Mounting",
    })
  })

  test("a failure outranks a ready catalog and keeps Korri's own wording", () => {
    const view = picoScreenViewFromModel(
      model({
        status: {
          _tag: "Problem",
          kicker: "COULD NOT START",
          reason: "zao did not answer.",
          canRetry: true,
          gameTitle: "Tetris",
        },
      }),
    )

    expect(view).toEqual({
      _tag: "Problem",
      kicker: "COULD NOT START",
      reason: "zao did not answer.",
      canRetry: true,
      gameTitle: "Tetris",
    })
  })

  test("names the running game by resolving Korri's id against the catalog", () => {
    const view = picoScreenViewFromModel(
      model({ status: { _tag: "Running", kicker: "PLAYING", gameId: "hollow" } }),
    )

    expect(view).toEqual({
      _tag: "Running",
      kicker: "PLAYING",
      gameTitle: "Hollow Knight",
    })
  })

  test("leaves the running game unnamed rather than guessing", () => {
    // An id Korri did not publish, or one the catalog does not hold, means the
    // surface does not know the title — not that it may substitute one.
    const unknown = picoScreenViewFromModel(
      model({ status: { _tag: "Running", kicker: "PLAYING", gameId: "ghost" } }),
    )
    const idless = picoScreenViewFromModel(
      model({ status: { _tag: "Running", kicker: "PLAYING" } }),
    )

    expect(unknown).toEqual({ _tag: "Running", kicker: "PLAYING" })
    expect(idless).toEqual({ _tag: "Running", kicker: "PLAYING" })
  })

  test("falls through to the catalog's own states while browsing", () => {
    expect(
      picoScreenViewFromModel(model({ catalog: { _tag: "Loading" } }))._tag,
    ).toBe("Loading")
    expect(
      picoScreenViewFromModel(model({ catalog: { _tag: "Empty" } }))._tag,
    ).toBe("Empty")
  })
})
