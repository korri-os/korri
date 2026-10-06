/** The gameplay overlay's rules, driven through `updateOverlay` with plain values. */
import { expect, test } from "bun:test"
import type { PicoOverlayControlView } from "../src/pico-overlay-view"
import { initialOverlay, type PicoOverlayMessage, updateOverlay } from "../src/state/overlay"
import type { PicoRequest } from "../src/state/requests"

const quit: PicoOverlayControlView = { id: "quit", label: "QUIT", enabled: true, destructive: true }
const toggle: PicoOverlayControlView = {
  id: "ff", label: "FAST FORWARD", enabled: true, destructive: false, sends: { kind: "toggle", value: true },
}

function story(...messages: PicoOverlayMessage[]) {
  let state = initialOverlay
  const requests: PicoRequest[] = []
  for (const message of messages) {
    const step = updateOverlay(state, message)
    state = step.model
    requests.push(...step.requests)
  }
  return { state, requests }
}

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

test("a press sends what the control sends; a range sends its settled value", () => {
  expect(story({ _tag: "InvokedControl", control: toggle }, { _tag: "AdjustedControl", control: toggle, value: 3 }).requests).toEqual([
    { _tag: "InvokeGameplayControl", controlId: "ff", value: { kind: "toggle", value: true } },
    { _tag: "InvokeGameplayControl", controlId: "ff", value: { kind: "range", value: 3 } },
  ])
})
