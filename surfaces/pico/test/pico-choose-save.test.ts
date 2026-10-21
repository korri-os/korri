import { describe, expect, test } from "bun:test"
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { fixtureModel } from "../src/fixtures/fixture-host"
import { picoSaveTime, picoScreenViewFromModel } from "../src/pico-screen-view"
import { initialNavigation } from "../src/state/navigation"
import { update } from "../src/state/update"

const ended = new Date(2026, 9, 21, 9, 18).getTime()
const model: SurfaceModel = {
  ...fixtureModel,
  status: {
    _tag: "ChooseSave",
    gameId: "nope",
    kicker: "Saves split between two devices",
    actions: [{ id: "saves:3:not-now", label: "Not now", enabled: true }],
    sides: [
      { actionId: "saves:3:pick:b", device: "This device", endedAt: ended, durationSeconds: 4320 },
      { actionId: "saves:3:pick:c", device: "zao", endedAt: ended - 86_400_000, durationSeconds: 1500 },
    ],
  },
}

describe("a save split on Pico (ChooseSave)", () => {
  test("is the Confirm notice with one answer per side, then Not now", () => {
    expect(picoSaveTime(ended)).toBe("OCT 21 09:18")
    expect(picoScreenViewFromModel(model)).toEqual({
      _tag: "Confirm",
      kicker: "Saves split between two devices",
      actions: [
        { id: "saves:3:pick:b", label: "This device, OCT 21 09:18", enabled: true },
        { id: "saves:3:pick:c", label: "zao, OCT 20 09:18", enabled: true },
        { id: "saves:3:not-now", label: "Not now", enabled: true },
      ],
    })
  })

  test("a side's press runs that side's action id, and nothing else is authority", () => {
    const press = (actionId: string) =>
      update(initialNavigation(), { _tag: "PressedStatusAction", actionId }, model).requests
    expect(press("saves:3:pick:c")).toEqual([{ _tag: "RunAction", actionId: "saves:3:pick:c" }])
    expect(press("saves:3:not-now")).toEqual([{ _tag: "RunAction", actionId: "saves:3:not-now" }])
    expect(press("saves:2:pick:c")).toEqual([])
  })
})
