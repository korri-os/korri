/**
 * The RUN WITH rules, one block per message, driven with plain values.
 */
import { describe, expect, test } from "bun:test"
import type { SurfaceRunnerChoice } from "@contracts/surface/korri-surface"
import { fixtureRunnerReady, fixtureRunnerStale } from "../src/fixtures/fixture-options"
import { runWithPlan, zoneRouteRowId, zoneSavedRowId } from "../src/plan/options-run-with"
import { RUN_WITH_START, update, type ZoneRunWith, type ZoneRunWithMessage } from "../src/state/run-with"

const after = (state: ZoneRunWith, message: ZoneRunWithMessage, choice: SurfaceRunnerChoice = fixtureRunnerReady) =>
  update(state, message, { runnerChoice: choice })

describe("the plan", () => {
  test("route rows and actions, top to bottom, then the choice's actions", () => {
    if (fixtureRunnerReady._tag === "Closed") throw new Error("fixture")
    expect(runWithPlan(fixtureRunnerReady).ids).toEqual([
      zoneRouteRowId("retroarch-mgba"), "runner:3:launch:0", "runner:3:game:0", "runner:3:system:0",
      zoneRouteRowId("mgba-standalone"), "runner:3:launch:1", "runner:3:game:1", "runner:3:system:1",
      "runner:3:reload", "runner:cancel",
    ])
  })
  test("a saved choice no route offers comes first, by its name", () => {
    if (fixtureRunnerStale._tag === "Closed") throw new Error("fixture")
    const plan = runWithPlan(fixtureRunnerStale)
    expect(plan.orphans).toEqual([{ label: "System gba", runnerId: "vba-m" }])
    expect(plan.ids[0]).toBe(zoneSavedRowId("System gba"))
  })
})

describe("FocusedRow", () => {
  test("follows the cursor onto a row or an action", () => {
    expect(after(RUN_WITH_START, { _tag: "FocusedRow", rowId: "runner:cancel" }).model.focused).toBe("runner:cancel")
  })
  test("ignores an id the page does not have", () => {
    expect(after(RUN_WITH_START, { _tag: "FocusedRow", rowId: "nope" }).model).toBe(RUN_WITH_START)
  })
})

describe("PressedAction", () => {
  test("hands the id back unchanged", () => {
    expect(after(RUN_WITH_START, { _tag: "PressedAction", actionId: "runner:3:game:1" }).requests)
      .toEqual([{ _tag: "RunAction", actionId: "runner:3:game:1" }])
  })
  test("an inert action or an unknown id does nothing", () => {
    const inert: SurfaceRunnerChoice = { ...fixtureRunnerReady, actions: [{ id: "runner:3:reload", label: "Reload", enabled: false }] } as SurfaceRunnerChoice
    expect(after(RUN_WITH_START, { _tag: "PressedAction", actionId: "runner:3:reload" }, inert).requests).toEqual([])
    expect(after(RUN_WITH_START, { _tag: "PressedAction", actionId: "nope" }).requests).toEqual([])
  })
})

describe("PressedBack", () => {
  test("does nothing: the way out is the host's own action", () => {
    const state = { focused: "runner:cancel" }
    const step = after(state, { _tag: "PressedBack" })
    expect(step.model).toBe(state)
    expect(step.requests).toEqual([])
  })
})

describe("KorriPublished", () => {
  test("keeps the cursor on a row that is still there", () => {
    const state = { focused: "runner:cancel" }
    expect(after(state, { _tag: "KorriPublished" }).model).toBe(state)
  })
  test("lets go of a row Korri took away", () => {
    expect(after({ focused: "runner:3:launch:7" }, { _tag: "KorriPublished" }).model).toEqual(RUN_WITH_START)
  })
  test("Closed forgets the cursor", () => {
    expect(after({ focused: "runner:cancel" }, { _tag: "KorriPublished" }, { _tag: "Closed" }).model).toEqual(RUN_WITH_START)
  })
})
