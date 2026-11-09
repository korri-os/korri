import { expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import type { SurfaceButton, SurfaceModel } from "../../../../../contracts/generated/surface"
import { init, update } from "../logic"

function model(): SurfaceModel {
  const scenario = JSON.parse(readFileSync(join(import.meta.dir, "..", "preview", "scenarios", "games.json"), "utf8"))
  const base = scenario.models[0]
  const games = base.catalog.games
  return {
    ...base,
    status: { _tag: "Browsing" },
    catalog: { _tag: "Ready", total: games.length, window: games, firstIndex: 0, query: { text: "", order: "korri" } },
    presentation: {
      kind: "gameplay-overlay", title: "Paused", groups: [],
      controls: [
        { id: "resume", label: "Resume", enabled: true, destructive: false, dismissOnSuccess: true, interaction: { kind: "command" } },
        { id: "quit", label: "Quit", enabled: true, destructive: true, dismissOnSuccess: true, interaction: { kind: "command" } },
      ],
    },
  }
}

test("the host Menu toggle dismisses PAUSED, including its destructive question", () => {
  for (const question of [false, true]) {
    const korri = model()
    let state = init(korri, {})
    const press = (button: SurfaceButton) => {
      const step = update(state, { _tag: "Pressed", button }, korri)
      state = step.state
      return step.requests.filter(request => request._tag !== "FocusGame")
    }
    if (question) {
      expect(press("down")).toEqual([])
      expect(press("confirm")).toEqual([])
    }
    expect(press("menu")).toEqual([{ _tag: "DismissGameplayOverlay" }])
  }
})
