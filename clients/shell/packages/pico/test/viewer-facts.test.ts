import { expect, test } from "bun:test"
import { picoOverlayViewFrom, picoStatusStrip } from "../src/pico-overlay-view"

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

test("the status strip shows every fact Korri gives and `--` for every one it does not", () => {
  const full = picoStatusStrip({ clockLabel: "3:42 PM", batteryPercent: 82, streamHealth: { rttMs: 3, deliveredBitrateKbps: 20140, framesDropped: 0 } })
  expect(full.device.map(f => [f.value, f.known, f.battery])).toEqual([["3:42 PM", true, -2], ["82%", true, 4]])
  expect(full.stream.map(f => `${f.before} ${f.value} ${f.after}`.trim())).toEqual(["RTT 3 ms", "20.1 Mb/s", "0 DROPPED"])
  expect(full.stream.every(f => f.known)).toBe(true)
  const none = picoStatusStrip({})
  expect(none.device.map(f => [f.value, f.known, f.battery])).toEqual([["--:--", false, -2], ["--", false, -1]])
  expect(none.stream.map(f => `${f.before} ${f.value} ${f.after}`.trim())).toEqual(["RTT -- ms", "-- Mb/s", "-- DROPPED"])
  expect(none.stream.some(f => f.known)).toBe(false)
  // An empty clock is korrid's failed conversion: unknown, not a time.
  expect(picoStatusStrip({ clockLabel: "" }).device[0]?.known).toBe(false)
})

test("the strip states a slow stream in kb/s, a measured zero as zero, and never a negative", () => {
  const slow = picoStatusStrip({ streamHealth: { deliveredBitrateKbps: 820 } })
  expect([slow.stream[1]?.value, slow.stream[1]?.after]).toEqual(["820", "kb/s"])
  expect(picoStatusStrip({ streamHealth: { deliveredBitrateKbps: 0 } }).stream[1]?.value).toBe("0")
  expect(picoStatusStrip({ streamHealth: { rttMs: -1 } }).stream[0]?.known).toBe(false)
  expect(picoStatusStrip({ batteryPercent: 4 }).device[1]?.battery).toBe(0)
  expect(picoStatusStrip({ batteryPercent: 100 }).device[1]?.battery).toBe(5)
})
