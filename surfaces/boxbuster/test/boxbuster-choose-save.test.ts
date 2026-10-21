import { describe, expect, test } from "bun:test"
// Pinned: the labels are local time, so the zone decides the expected text.
process.env.TZ = "America/Denver"
import type { SurfaceModel, SurfaceStatus } from "@contracts/surface/korri-surface"
import { saveTimeLabel, statusActionsFrom, tvStatusFrom } from "../src/boxbuster-store-view"

/** 21 Oct 2026 09:18 and 20 Oct 2026 21:05, Denver time. */
const ended = Date.UTC(2026, 9, 21, 15, 18)
const before = Date.UTC(2026, 9, 21, 3, 5)
const split: SurfaceStatus = {
  _tag: "ChooseSave",
  gameId: "wario",
  kicker: "Saves split between two devices",
  detail: "No other device answered.",
  actions: [{ id: "saves:3:not-now", label: "Not now", enabled: true }],
  sides: [
    { actionId: "saves:3:pick:b", device: "This device", endedAt: ended, durationSeconds: 4320 },
    { actionId: "saves:3:pick:c", device: "zao", endedAt: before, durationSeconds: 1500 },
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
  test("states the end time in local time, noon and midnight as 12", () => {
    expect(saveTimeLabel(ended)).toBe("Oct 21, 9:18 AM")
    expect(saveTimeLabel(Date.UTC(2026, 0, 1, 7, 0))).toBe("Jan 1, 12:00 AM")
    expect(saveTimeLabel(Date.UTC(2026, 6, 4, 18, 30))).toBe("Jul 4, 12:30 PM")
    expect(saveTimeLabel(Number.NaN)).toBe("unknown time")
  })

  test("the decks answer it like Confirm: one action per side, by its own id, then Not now", () => {
    expect(statusActionsFrom(split)).toEqual([
      { id: "saves:3:pick:b", label: "This device, Oct 21, 9:18 AM", enabled: true },
      { id: "saves:3:pick:c", label: "zao, Oct 20, 9:05 PM", enabled: true },
      { id: "saves:3:not-now", label: "Not now", enabled: true },
    ])
  })

  test("the TV states the question and lists every answer", () => {
    expect(tvStatusFrom(model)).toEqual({
      _tag: "Working",
      kicker: "Saves split between two devices",
      detail: "No other device answered.",
      tapeId: "wario",
      lines: ["This device, Oct 21, 9:18 AM", "zao, Oct 20, 9:05 PM", "Not now"],
    })
  })
})
