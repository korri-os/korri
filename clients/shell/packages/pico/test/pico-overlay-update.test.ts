/** The gameplay overlay's rules, driven through `updateOverlay` with plain values. */
import { expect, test } from "bun:test"
import type { PicoOverlayControlView, PicoOverlayView } from "../src/pico-overlay-view"
import {
  initialOverlay,
  type PicoOverlayMessage,
  RANGE_COMMIT_DELAY_MS,
  RANGE_RELEASE_FALLBACK_MS,
  rangeValue,
  updateOverlay,
} from "../src/state/overlay"
import type { PicoRequest } from "../src/state/requests"

const quit: PicoOverlayControlView = { id: "quit", label: "QUIT", enabled: true, destructive: true }
const toggle: PicoOverlayControlView = {
  id: "ff", label: "FAST FORWARD", enabled: true, destructive: false, sends: { kind: "toggle", value: true },
}
const volume: PicoOverlayControlView = {
  id: "vol", label: "VOLUME", enabled: true, destructive: false, range: { value: 80, min: 0, max: 100, step: 10 },
}
const view = (...controls: PicoOverlayControlView[]): PicoOverlayView =>
  ({ title: "Hollow Knight", controls, groups: [] })
const overlay = view(quit, toggle, volume)

function storyIn(korri: PicoOverlayView, ...messages: PicoOverlayMessage[]) {
  let state = initialOverlay
  const requests: PicoRequest[] = []
  for (const message of messages) {
    const step = updateOverlay(state, message, korri)
    state = step.model
    requests.push(...step.requests)
  }
  return { state, requests }
}
const story = (...messages: PicoOverlayMessage[]) => storyIn(overlay, ...messages)

const tap = (way: -1 | 1) =>
  ({ _tag: "SteppedRange", control: volume, request: { way, releaseExpected: false } }) as const
const hold = (way: -1 | 1, gestureId: number) =>
  ({ _tag: "SteppedRange", control: volume, request: { way, releaseExpected: true, gestureId } }) as const
const sent = (value: number): Extract<PicoRequest, { _tag: "InvokeGameplayControl" }> => ({ _tag: "InvokeGameplayControl", controlId: "vol", value: { kind: "range", value } })

test("Back, Menu and System each dismiss the overlay", () => {
  for (const _tag of ["PressedBack", "PressedMenu", "PressedSystem"] as const) {
    expect(story({ _tag }).requests).toEqual([{ _tag: "DismissGameplayOverlay" }])
  }
})

test("Back withdraws a destructive question before it dismisses anything", () => {
  const asked = story({ _tag: "AskedControl", control: quit }, { _tag: "PressedBack" })
  expect(asked.requests).toEqual([])
  expect(asked.state.asking).toBeUndefined()
})

test("a confirmed question sends its control once and closes", () => {
  const { state, requests } = story({ _tag: "AskedControl", control: quit }, { _tag: "ConfirmedControl" }, { _tag: "ConfirmedControl" })
  expect(requests).toEqual([{ _tag: "InvokeGameplayControl", controlId: "quit", value: undefined }])
  expect(state.asking).toBeUndefined()
})

test("a press sends what the control sends", () => {
  expect(story({ _tag: "InvokedControl", control: toggle }).requests).toEqual([
    { _tag: "InvokeGameplayControl", controlId: "ff", value: { kind: "toggle", value: true } },
  ])
})

test("a tap moves the range at once and asks Korri once, after a short wait", () => {
  const { state, requests } = story(tap(1), tap(1))
  expect(rangeValue(state, volume)).toBe(100)
  expect(requests).toEqual([
    { _tag: "Wait", ms: RANGE_COMMIT_DELAY_MS, token: 0 },
    { _tag: "Wait", ms: RANGE_COMMIT_DELAY_MS, token: 1 },
  ])
  // The first wait is stale; only the last one sends, and only once.
  const stale = storyIn(overlay, tap(1), tap(1), { _tag: "Waited", token: 0 })
  expect(stale.requests.filter(request => request._tag === "InvokeGameplayControl")).toEqual([])
  const done = storyIn(overlay, tap(1), tap(1), { _tag: "Waited", token: 1 }, { _tag: "Waited", token: 1 })
  expect(done.requests.filter(request => request._tag === "InvokeGameplayControl")).toEqual([sent(100)])
})

test("a held direction is sent on its own release, not another's", () => {
  const other = story(hold(-1, 7), { _tag: "ReleasedRange", control: volume, ended: { way: -1, gestureId: 8 } })
  expect(other.requests).toEqual([{ _tag: "Wait", ms: RANGE_RELEASE_FALLBACK_MS, token: 0 }])
  const own = story(hold(-1, 7), { _tag: "ReleasedRange", control: volume, ended: { way: -1, gestureId: 7 } })
  expect(own.requests.at(-1)).toEqual(sent(70))
})

test("the range stops at Korri's bounds and asks nothing past them", () => {
  const { state, requests } = story(tap(1), tap(1), tap(1))
  expect(rangeValue(state, volume)).toBe(100)
  expect(requests).toHaveLength(2)
})

test("leaving the overlay sends a held step first", () => {
  expect(story(hold(1, 1), { _tag: "PressedBack" }).requests.slice(1)).toEqual([sent(90), { _tag: "DismissGameplayOverlay" }])
})

test("leaving the range sends its held step", () => {
  expect(story(tap(-1), { _tag: "LeftRange", control: volume }).requests.at(-1)).toEqual(sent(70))
})

test("a value Korri republishes replaces the player's and drops the unsent step", () => {
  const moved = story(tap(1))
  const republished = view(quit, toggle, { ...volume, range: { ...volume.range!, value: 30 } })
  const step = updateOverlay(moved.state, { _tag: "KorriPublished" }, republished)
  expect(step.model.ranges).toEqual({})
  expect(updateOverlay(step.model, { _tag: "Waited", token: 0 }, republished).requests).toEqual([])
})

test("a disabled range does not move", () => {
  const off = { ...volume, enabled: false }
  expect(storyIn(view(off), { _tag: "SteppedRange", control: off, request: { way: 1, releaseExpected: false } }).requests).toEqual([])
})
