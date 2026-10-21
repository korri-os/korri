import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import type { SurfaceModel, SurfaceStatus } from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { ZONE_STATUS_START, updateStatus } from "../src/state/status"
import { ZoneSurface } from "../src/ZoneSurface"
import { zoneModelOf, zoneSaveTime, zoneStatusOf } from "../src/zone-save-split"

afterEach(() => cleanup())

const ended = new Date(2026, 9, 21, 9, 18).getTime()
const split: Extract<SurfaceStatus, { _tag: "ChooseSave" }> = {
  _tag: "ChooseSave",
  gameId: "celeste",
  kicker: "Saves split between two devices",
  detail: "No other device answered. Your pick syncs to them when they are next in reach.",
  actions: [{ id: "saves:3:not-now", label: "Not now", enabled: true }],
  sides: [
    { actionId: "saves:3:pick:sha256:b", device: "This device", endedAt: ended, durationSeconds: 4320 },
    { actionId: "saves:3:pick:sha256:c", device: "zao", endedAt: ended - 86_400_000, durationSeconds: 1500 },
  ],
}

describe("a save split on Zone (ChooseSave)", () => {
  test("is Korri's Confirm with one answer per side, then Not now", () => {
    expect(zoneSaveTime(ended)).toBe("21 Oct 2026 09:18")
    expect(zoneStatusOf(split)).toEqual({
      _tag: "Confirm",
      kicker: split.kicker,
      detail: split.detail,
      gameId: "celeste",
      actions: [
        { id: "saves:3:pick:sha256:b", label: "This device, 21 Oct 2026 09:18", enabled: true },
        { id: "saves:3:pick:sha256:c", label: "zao, 20 Oct 2026 09:18", enabled: true },
        { id: "saves:3:not-now", label: "Not now", enabled: true },
      ],
    })
    const model: SurfaceModel = { ...fixtureModel, status: split }
    const asked = zoneModelOf(model).status
    const opened = updateStatus(ZONE_STATUS_START, { _tag: "KorriPublished" }, asked).model
    expect(opened).toEqual({ shown: "confirm", focus: 0 })
    expect(updateStatus(opened, { _tag: "ChoseConfirmAction", actionId: "saves:3:pick:sha256:c" }, asked).requests)
      .toEqual([{ _tag: "RunAction", actionId: "saves:3:pick:sha256:c" }])
  })

  test("the dialog states the question and each side runs its own action id", () => {
    const host = createFixtureHost()
    const view = render(<ZoneSurface host={host} model={{ ...fixtureModel, status: split }} />)
    expect(screen.getByRole("dialog", { name: "Saves split between two devices" })).toBeTruthy()
    const answers = [...view.container.querySelectorAll<HTMLButtonElement>(".zone-status-layer-confirm button")]
    expect(answers.map(answer => answer.textContent)).toEqual([
      "This device, 21 Oct 2026 09:18", "zao, 20 Oct 2026 09:18", "Not now",
    ])
    act(() => { fireEvent.click(answers[1]!) })
    expect(host.calls).toContain("action:saves:3:pick:sha256:c")
    act(() => { fireEvent.click(answers[2]!) })
    expect(host.calls).toContain("action:saves:3:not-now")
  })
})
