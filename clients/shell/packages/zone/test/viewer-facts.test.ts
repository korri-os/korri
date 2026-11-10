import { expect, test } from "bun:test"
import type { SurfaceModel } from "../../../../../contracts/generated/surface"
import { overlayViewerFacts } from "../src/state/overlay"

test("strip facts remain read-only model inputs, separate from focus state", () => {
  const health = { rttMs: 8, deliveredBitrateKbps: 4000, framesDropped: 0 }
  const model: SurfaceModel = {
    presentation: { kind: "gameplay-overlay", title: "Game", controls: [], groups: [], batteryPercent: 57, streamHealth: health },
    clockLabel: "10:24 AM", catalog: { _tag: "Empty" }, status: { _tag: "Browsing" }, actions: [], settings: [], settingsStatus: { _tag: "Idle" },
  }
  expect(overlayViewerFacts(model)).toEqual({ title: "Game", clockLabel: "10:24 AM", batteryPercent: 57, streamHealth: health })
  expect(overlayViewerFacts({ ...model, presentation: { kind: "catalog" } })).toBeUndefined()
})
