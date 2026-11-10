import { expect, test } from "bun:test"
import type { SurfaceModel } from "../../../../../contracts/generated/surface"
import { overlayViewerFacts } from "../src/state/overlay"
import { zoneStatusStrip } from "../src/plan/status-strip"
import { init, overlay } from "../logic"

test("strip facts remain read-only model inputs, separate from focus state", () => {
  const health = { rttMs: 8, deliveredBitrateKbps: 4000, framesDropped: 0 }
  const model: SurfaceModel = {
    presentation: { kind: "gameplay-overlay", title: "Game", controls: [], groups: [], batteryPercent: 57, streamHealth: health },
    clockLabel: "10:24 AM", catalog: { _tag: "Empty" }, status: { _tag: "Browsing" }, actions: [], settings: [], settingsStatus: { _tag: "Idle" },
  }
  expect(overlayViewerFacts(model)).toEqual({ title: "Game", clockLabel: "10:24 AM", batteryPercent: 57, streamHealth: health })
  expect(overlayViewerFacts({ ...model, presentation: { kind: "catalog" } })).toBeUndefined()
})

test("PAUSED's strip: every fact Korri gives, Zone's dashes for each it does not, a measured 0 as 0", () => {
  const full = zoneStatusStrip({ clockLabel: "3:42 PM", batteryPercent: 82, streamHealth: { rttMs: 3, deliveredBitrateKbps: 20140, framesDropped: 0 } })
  expect(full.device).toEqual([{ label: "Time", value: "3:42 PM", known: true }, { label: "Battery", value: "82%", known: true }])
  expect(full.stream).toEqual([{ label: "RTT", value: "3 ms", known: true }, { label: "Video", value: "20.1 Mb/s", known: true }, { label: "Dropped", value: "0", known: true }])
  const none = zoneStatusStrip({})
  expect([...none.device, ...none.stream].map(fact => [fact.label, fact.value, fact.known])).toEqual([
    ["Time", "--", false], ["Battery", "--", false], ["RTT", "--", false], ["Video", "--", false], ["Dropped", "--", false],
  ])
  expect(zoneStatusStrip({ streamHealth: { deliveredBitrateKbps: 820 } }).stream[1]?.value).toBe("820 kb/s")
  expect(zoneStatusStrip({ clockLabel: "" }).device[0]?.known).toBe(false)
})

test("the overlay view carries the strip, and no strip entry is a focus target", () => {
  const model: SurfaceModel = {
    presentation: { kind: "gameplay-overlay", title: "Game", controls: [], groups: [], batteryPercent: 57 },
    clockLabel: "10:24 AM", catalog: { _tag: "Empty" }, status: { _tag: "Browsing" }, actions: [], settings: [], settingsStatus: { _tag: "Idle" },
  }
  const state = init(model, {})
  const shown = overlay(state, model) as { strip: { shown: boolean; device: object[]; stream: object[] } }
  expect(shown.strip.shown).toBe(true)
  expect(JSON.stringify(shown.strip)).not.toContain("focus")
  expect(shown.strip.stream.every(fact => (fact as { known: boolean }).known === false)).toBe(true)
})
