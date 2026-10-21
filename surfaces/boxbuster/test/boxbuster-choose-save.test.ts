import { describe, expect, test } from "bun:test"
import type { SurfaceModel, SurfaceStatus } from "@contracts/surface/korri-surface"
import { saveTimeLabel, statusActionsFrom, tvStatusFrom } from "../src/boxbuster-store-view"

const ended = new Date(2026, 9, 21, 9, 18).getTime()
const split: SurfaceStatus = {
  _tag: "ChooseSave",
  gameId: "wario",
  kicker: "Saves split between two devices",
  detail: "No other device answered.",
  actions: [{ id: "saves:3:not-now", label: "Not now", enabled: true }],
  sides: [
    { actionId: "saves:3:pick:b", device: "This device", endedAt: ended, durationSeconds: 4320 },
    { actionId: "saves:3:pick:c", device: "zao", endedAt: ended - 86_400_000, durationSeconds: 1500 },
  ],
}
const model: SurfaceModel = {
  presentation: { kind: "catalog" },
  catalog: { _tag: "Ready", games: [] },
  status: split,
  actions: [],
  settings: [],
  settingsStatus: { _tag: "Idle" },
}

describe("a save split on Boxbuster (ChooseSave)", () => {
  test("the decks answer it like Confirm: one action per side, by its own id, then Not now", () => {
    expect(statusActionsFrom(split)).toEqual([
      { id: "saves:3:pick:b", label: `This device, ${saveTimeLabel(ended)}`, enabled: true },
      { id: "saves:3:pick:c", label: `zao, ${saveTimeLabel(ended - 86_400_000)}`, enabled: true },
      { id: "saves:3:not-now", label: "Not now", enabled: true },
    ])
  })

  test("the TV states the question and lists every answer", () => {
    expect(tvStatusFrom(model)).toEqual({
      _tag: "Working",
      kicker: "Saves split between two devices",
      detail: "No other device answered.",
      tapeId: "wario",
      lines: [`This device, ${saveTimeLabel(ended)}`, `zao, ${saveTimeLabel(ended - 86_400_000)}`, "Not now"],
    })
  })
})
