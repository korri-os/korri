import { describe, expect, it } from "bun:test"
import { createInputBus } from "./bus"
import { createGamepadAdapter, type GamepadSnapshot } from "./gamepad-adapter"
import { createSpatialFocusController } from "./spatial-focus"
import type { InputAction } from "./types"

function pad(pressed: readonly number[] = [], axes = [0, 0], id = "controller"): GamepadSnapshot {
  return {
    id, index: 0, mapping: "standard", connected: true, axes,
    buttons: Array.from({ length: 17 }, (_, index) => ({
      pressed: pressed.includes(index), touched: pressed.includes(index),
      value: pressed.includes(index) ? 1 : 0,
    })),
  }
}

function session() {
  let pads: readonly (GamepadSnapshot | null)[] = [pad()]
  let active = true
  let readCount = 0
  let readBlocked = false
  let nextId = 0
  const frames = new Map<number, FrameRequestCallback>()
  const page = new EventTarget()
  const visibility = new EventTarget()
  const actions: InputAction[] = []
  const adapter = createGamepadAdapter({
    readGamepads: () => {
      readCount++
      if (readBlocked) throw new DOMException("blocked", "SecurityError")
      return pads
    },
    isActive: () => active,
    page, visibility,
    requestFrame: callback => { frames.set(++nextId, callback); return nextId },
    cancelFrame: id => { frames.delete(id) },
  })
  const stop = adapter.start(action => actions.push(action))
  return {
    adapter, actions, stop,
    get reads() { return readCount },
    get pending() { return frames.size },
    setPads(value: readonly (GamepadSnapshot | null)[]) { pads = value },
    blockRead(blocked: boolean) { readBlocked = blocked },
    disconnectEvent(index = 0) {
      const event = new Event("gamepaddisconnected")
      Object.defineProperty(event, "gamepad", { value: { index } })
      page.dispatchEvent(event)
    },
    blur() { active = false; page.dispatchEvent(new Event("blur")) },
    hide() { active = false; visibility.dispatchEvent(new Event("visibilitychange")) },
    focus() { active = true },
    tick(now: number) {
      const scheduled = [...frames.values()]
      frames.clear()
      for (const callback of scheduled) callback(now)
    },
  }
}

describe("browser Gamepad adapter", () => {
  it("emits one direction gesture, bounded repeats, and a matching release", () => {
    const s = session()
    s.tick(0)
    s.setPads([pad([15])])
    s.tick(10)
    s.tick(409)
    expect(s.actions).toHaveLength(1)
    s.tick(410)
    s.tick(510)
    s.tick(20_000)
    s.setPads([pad()])
    s.tick(20_001)
    expect(s.actions).toEqual([
      { type: "direction", direction: "right", source: "gamepad", releaseExpected: true, gestureId: 1 },
      { type: "direction", direction: "right", source: "gamepad", releaseExpected: true, gestureId: 1, repeat: true },
      { type: "direction", direction: "right", source: "gamepad", releaseExpected: true, gestureId: 1, repeat: true },
      { type: "direction", direction: "right", source: "gamepad", releaseExpected: true, gestureId: 1, repeat: true },
      { type: "direction-end", direction: "right", source: "gamepad", gestureId: 1 },
    ])
    s.stop()
  })

  it("uses the legacy standard action mapping without repeating held buttons", () => {
    const s = session()
    s.tick(0)
    for (const [index, type] of [[0, "confirm"], [1, "back"], [3, "options"], [9, "menu"]] as const) {
      s.setPads([pad([index])]); s.tick(1); s.tick(1_000)
      expect(s.actions.at(-1)).toEqual({ type, source: "gamepad" })
      s.setPads([pad()]); s.tick(1_001)
    }
    expect(s.actions).toHaveLength(4)
    s.stop()
  })

  it("merges D-pad and stick movement and releases before changing direction", () => {
    const s = session()
    s.tick(0)
    s.setPads([pad([15], [1, 0])]); s.tick(1)
    expect(s.actions).toHaveLength(1)
    s.setPads([pad([], [-0.8, 0.3])]); s.tick(2)
    expect(s.actions.map(a => a.type)).toEqual(["direction", "direction-end", "direction"])
    expect(s.actions.at(-1)).toMatchObject({ direction: "left", gestureId: 2 })
    s.setPads([pad([], [0.4, -0.4])]); s.tick(3)
    expect(s.actions.at(-1)).toMatchObject({ type: "direction-end", direction: "left" })
    s.stop()
  })

  it("uses the dominant stick axis and gives diagonal D-pad presses vertical priority", () => {
    const s = session()
    s.tick(0)
    s.setPads([pad([], [0.3, 0.8])]); s.tick(1)
    expect(s.actions.at(-1)).toMatchObject({ type: "direction", direction: "down" })
    s.setPads([pad([], [0.3, -0.8])]); s.tick(2)
    expect(s.actions.at(-1)).toMatchObject({ type: "direction", direction: "up" })
    s.setPads([pad([13, 14])]); s.tick(3)
    expect(s.actions.at(-1)).toMatchObject({ type: "direction", direction: "down" })
    s.stop()
  })

  for (const loseFocus of ["blur", "hide"] as const) {
    it(`ends held input on ${loseFocus} and requires neutral before resuming`, () => {
      const s = session()
      s.tick(0)
      s.setPads([pad([15, 0])]); s.tick(1)
      s[loseFocus]()
      expect(s.actions.at(-1)).toMatchObject({ type: "direction-end", direction: "right" })
      const count = s.actions.length
      const reads = s.reads
      s.tick(500)
      expect(s.reads).toBe(reads)
      s.focus(); s.tick(501); s.tick(1_000)
      expect(s.actions).toHaveLength(count)
      s.setPads([pad()]); s.tick(1_001)
      s.setPads([pad([0])]); s.tick(1_002)
      expect(s.actions.at(-1)).toEqual({ type: "confirm", source: "gamepad" })
      expect(s.actions).toHaveLength(count + 1)
      s.stop()
    })
  }

  it("does not activate a held button at startup or after controller replacement", () => {
    const s = session()
    s.setPads([pad([0])]); s.tick(0)
    expect(s.actions).toEqual([])
    s.setPads([pad()]); s.tick(1)
    s.setPads([pad([15])]); s.tick(2)
    s.setPads([pad([0], [0, 0], "replacement")]); s.tick(3)
    expect(s.actions).toHaveLength(2)
    expect(s.actions.at(-1)?.type).toBe("direction-end")
    s.stop()
  })

  it("releases on disconnect and ignores nonstandard and disconnected devices", () => {
    const s = session()
    s.tick(0)
    s.setPads([pad([12])]); s.tick(1)
    s.setPads([null]); s.tick(2)
    expect(s.actions.at(-1)).toMatchObject({ type: "direction-end", direction: "up" })
    const count = s.actions.length
    s.setPads([{ ...pad([0]), mapping: "" }]); s.tick(3)
    s.setPads([{ ...pad([0]), connected: false }]); s.tick(4)
    expect(s.actions).toHaveLength(count)
    s.stop()
  })

  it("retires a same-slot reconnect even between animation frames", () => {
    const s = session()
    s.tick(0)
    s.setPads([pad([15])]); s.tick(1)
    s.disconnectEvent()
    expect(s.actions.at(-1)?.type).toBe("direction-end")
    s.tick(900)
    expect(s.actions).toHaveLength(2)
    s.setPads([pad()]); s.tick(901)
    s.setPads([pad([15])]); s.tick(902)
    expect(s.actions.at(-1)).toMatchObject({ type: "direction", gestureId: 2 })
    s.stop()
  })

  it("retires input after an API denial and requires neutral on recovery", () => {
    const s = session()
    s.tick(0)
    s.setPads([pad([15])]); s.tick(1)
    s.blockRead(true); s.tick(401)
    expect(s.actions.at(-1)?.type).toBe("direction-end")
    s.blockRead(false); s.tick(402)
    expect(s.actions).toHaveLength(2)
    s.setPads([pad()]); s.tick(403)
    s.setPads([pad([0])]); s.tick(404)
    expect(s.actions.at(-1)).toEqual({ type: "confirm", source: "gamepad" })
    s.stop()
  })

  it("does not emit another action after a listener removes page focus", () => {
    const s = session()
    s.stop()
    const stop = s.adapter.start(action => {
      s.actions.push(action)
      if (action.type === "confirm") s.blur()
    })
    s.tick(0)
    s.setPads([pad([0, 1])]); s.tick(1)
    expect(s.actions).toEqual([{ type: "confirm", source: "gamepad" }])
    stop()
  })

  for (const retire of ["blur", "dispose"] as const) {
    it(`does not start an orphan gesture when direction-end causes ${retire}`, () => {
      const s = session()
      s.stop()
      const stop = s.adapter.start(action => {
        s.actions.push(action)
        if (action.type === "direction-end") {
          if (retire === "blur") s.blur()
          else stop()
        }
      })
      s.tick(0)
      s.setPads([pad([15])]); s.tick(1)
      s.setPads([pad([14])]); s.tick(2)
      expect(s.actions.map(action => action.type)).toEqual(["direction", "direction-end"])
      stop()
    })
  }

  it("keeps independent gestures for two standard controllers", () => {
    const second = (buttons: readonly number[] = []) => ({ ...pad(buttons), index: 1 })
    const s = session()
    s.setPads([pad(), second()]); s.tick(0)
    s.setPads([pad([15]), second([12])]); s.tick(1)
    expect(s.actions).toMatchObject([
      { type: "direction", direction: "right", gestureId: 1 },
      { type: "direction", direction: "up", gestureId: 2 },
    ])
    s.disconnectEvent(0)
    expect(s.actions).toHaveLength(3)
    expect(s.actions.at(-1)).toMatchObject({ type: "direction-end", direction: "right", gestureId: 1 })
    s.setPads([null, second([12])]); s.tick(2)
    s.tick(401)
    expect(s.actions.at(-1)).toMatchObject({ type: "direction", direction: "up", gestureId: 2, repeat: true })
    s.stop()
  })

  it("stops frames and releases its gesture on disposal", () => {
    const s = session()
    s.tick(0); s.setPads([pad([14])]); s.tick(1)
    s.stop(); s.stop()
    expect(s.pending).toBe(0)
    expect(s.actions).toHaveLength(2)
    expect(s.actions.at(-1)).toMatchObject({ type: "direction-end", direction: "left" })
    s.tick(100)
    expect(s.actions).toHaveLength(2)
  })

  it("reaches actual native buttons through the existing bus and focus controller", () => {
    const s = session()
    s.stop()
    const group = document.createElement("section")
    group.setAttribute("data-block-exit", "")
    const first = document.createElement("button")
    first.getBoundingClientRect = () => new DOMRect(0, 0, 100, 30)
    const button = document.createElement("button")
    button.id = "gamepad-next"
    button.getBoundingClientRect = () => new DOMRect(150, 0, 100, 30)
    button.scrollIntoView = () => {}
    let clicks = 0
    button.addEventListener("click", () => clicks++)
    group.append(first, button)
    document.body.append(group)
    first.focus()
    const bus = createInputBus()
    const stopFocus = createSpatialFocusController(bus)
    bus.use(s.adapter)
    try {
      s.tick(0)
      s.setPads([pad([15])]); s.tick(1)
      expect(document.activeElement?.id).toBe("gamepad-next")
      s.setPads([pad()]); s.tick(2)
      s.setPads([pad([0])]); s.tick(3); s.tick(1_000)
      expect(clicks).toBe(1)
    } finally {
      bus.dispose(); stopFocus(); group.remove()
    }
  })
})
