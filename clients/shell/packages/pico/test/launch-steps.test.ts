/**
 * Amendment A4: a launch's named steps reach Korri's launch stage as one
 * cell per step, the one running now, and "Step i of n · its label".
 */
import { describe, expect, test } from "bun:test"
import type { KorriModel, SurfaceModel } from "../src/model"
import { init, update, view } from "../src/native/program"
import { NO_STEPS, stepsView } from "../src/native/kit"
import { picoScreenViewFromModel } from "../src/pico-screen-view"
import { fixtureModel } from "./fixtures/fixture-host"

const steps = { labels: ["Checking saves", "Getting the game ready", "Starting the game"], current: 1 }

describe("a launch's named steps", () => {
  test("a Busy carries Korri's steps to the screen", () => {
    const model: SurfaceModel = { ...fixtureModel, status: { _tag: "Busy", kicker: "Starting Hollow…", steps } }
    expect(picoScreenViewFromModel(model)).toEqual({ _tag: "Busy", kicker: "Starting Hollow…", steps })
  })

  test("the stage counts the cells and words the step running now", () => {
    expect(stepsView(steps)).toEqual({ shown: true, count: 3, current: 1, words: "Step 2 of 3 · Getting the game ready" })
  })

  test("no steps draw nothing; a current step past the end is the last", () => {
    expect(stepsView(undefined)).toEqual(NO_STEPS)
    expect(stepsView({ labels: [], current: 0 })).toEqual(NO_STEPS)
    expect(stepsView({ labels: ["Starting the game"], current: 4 }).words).toBe("Step 1 of 1 · Starting the game")
  })

  test("the full clock meter gives way to the steps; measured bytes keep their bar", () => {
    const launchOf = (status: SurfaceModel["status"]) => {
      const games = fixtureModel.catalog._tag === "Ready" ? fixtureModel.catalog.games : []
      const catalog = { _tag: "Ready", total: games.length, window: games, firstIndex: 0, query: { text: "", order: "korri" } }
      const model = { ...fixtureModel, catalog, status } as unknown as KorriModel
      const state = update(init(model, {}), { _tag: "KorriPublished" }, model).state
      return ((view(state, model) as Record<string, unknown>)["home"] as { launch: { bar: { shown: boolean }; steps: { shown: boolean } } }).launch
    }
    const stepped = launchOf({ _tag: "Busy", kicker: "Starting Hollow…", steps, gameId: "hollow" })
    expect(stepped.steps.shown).toBe(true)
    expect(stepped.bar.shown).toBe(false)
    const plain = launchOf({ _tag: "Busy", kicker: "Starting Hollow…", gameId: "hollow" })
    expect(plain.steps.shown).toBe(false)
    expect(plain.bar.shown).toBe(true)
    const measured = launchOf({ _tag: "Busy", kicker: "Downloading", steps, progress: { done: 1, total: 4 }, gameId: "hollow" })
    expect(measured.steps.shown).toBe(true)
    expect(measured.bar.shown).toBe(true)
  })
})
