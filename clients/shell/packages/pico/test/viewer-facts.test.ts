import { expect, test } from "bun:test"
import { picoOverlayViewFrom } from "../src/pico-overlay-view"

test("the overlay adapter retains read-only viewer facts and the declared stream group", () => {
  const health = { rttMs: 8, deliveredBitrateKbps: 4000, framesDropped: 0 }
  const view = picoOverlayViewFrom({ kind: "gameplay-overlay", title: "Game", controls: [],
    groups: [{ id: "opaque-group", label: "Stream controls", controls: [] }],
    batteryPercent: 57, streamHealth: health }, { _tag: "Browsing" }, "10:24 AM")
  expect(view.clockLabel).toBe("10:24 AM")
  expect(view.batteryPercent).toBe(57)
  expect(view.streamHealth).toBe(health)
  expect(view.groups[0].id).toBe("opaque-group")
})
