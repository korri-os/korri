import { describe, expect, it } from "bun:test"
import type { NativeInputDeviceInfo } from "@contracts/generated/korrid"
import { createNativeGamepadMapper } from "./native-gamepad-mapper"
import type { InputAction } from "./types"

const PAD: NativeInputDeviceInfo = { deviceId: "pad", class: "gamepad", name: "Pad", capabilities: [] }
function session() {
  const actions: InputAction[] = []
  let now = 0
  let active = true
  const mapper = createNativeGamepadMapper(action => actions.push(action), { now: () => now, isActive: () => active })
  mapper.configureDevice(PAD)
  return {
    mapper, actions,
    initialize: () => mapper.initialize(),
    tick(time: number) { now = time; mapper.tick(now) },
    inactive() { active = false; mapper.tick(now) },
    active() { active = true },
    key(code: number, value: number, deviceId = "pad", report = true) {
      mapper.handle({ kind: "input", class: "gamepad", deviceId, type: 1, code, value, timestamp: 0 })
      if (report) mapper.handle({ kind: "input", class: "gamepad", deviceId, type: 0, code: 0, value: 0, timestamp: 0 })
    },
    axis(code: number, value: number, deviceId = "pad", report = true) {
      mapper.handle({ kind: "input", class: "gamepad", deviceId, type: 3, code, value, timestamp: 0 })
      if (report) mapper.handle({ kind: "input", class: "gamepad", deviceId, type: 0, code: 0, value: 0, timestamp: 0 })
    },
    report(deviceId = "pad") { mapper.handle({ kind: "input", class: "gamepad", deviceId, type: 0, code: 0, value: 0, timestamp: 0 }) },
  }
}

describe("native mapper (deliberately extracted legacy mappings, current gesture contract)", () => {
  it("accepts all 255 physical plus 16 remote sources, with independent held baselines and a finite cap", () => {
    const actions: InputAction[] = []
    const mapper = createNativeGamepadMapper(action => actions.push(action))
    const count = 255 + 16 // shared MAX_PHYSICAL_SOURCES + receiver controllerNumber 0..=15
    const key = (deviceId: string, value: number) => {
      mapper.handle({ kind: "input", class: "gamepad", deviceId, type: 1, code: 0x130, value, timestamp: 0 })
      mapper.handle({ kind: "input", class: "gamepad", deviceId, type: 0, code: 0, value: 0, timestamp: 0 })
    }
    for (let index = 0; index < count; index++) {
      const deviceId = `source-${index}`
      mapper.configureDevice({ ...PAD, deviceId })
      key(deviceId, 1)
    }
    mapper.initialize()
    expect(actions).toEqual([])
    expect(() => mapper.configureDevice({ ...PAD, deviceId: "over-budget" })).toThrow("Native input device limit reached.")
    for (let index = 0; index < count; index++) {
      const deviceId = `source-${index}`
      key(deviceId, 0)
      key(deviceId, 1)
    }
    expect(actions).toHaveLength(count)
    expect(actions.every(action => action.type === "confirm")).toBe(true)
    mapper.clearDevice("source-0")
    mapper.configureDevice({ ...PAD, deviceId: "replacement" })
    mapper.initialize("replacement")
    key("replacement", 1)
    expect(actions).toHaveLength(count + 1)
  })

  it("never arms from the transient neutral inside a Start-held to A-held sample", () => {
    const s = session()
    s.key(0x13b, 1, "pad", false)
    s.initialize()
    s.key(0x13b, 0, "pad", false)
    s.tick(1000) // committed state remains Start-held during partial delivery
    s.key(0x130, 1, "pad", false)
    expect(s.actions).toEqual([])
    s.report()
    s.tick(2000)
    expect(s.actions).toEqual([])
    s.key(0x130, 0) // genuinely neutral complete sample
    s.key(0x130, 1)
    expect(s.actions).toEqual([{ type: "confirm", source: "native" }])
  })

  it("reconciles simultaneous axes once and preserves short press/release samples", () => {
    const s = session(); s.initialize()
    s.axis(0, 20_000, "pad", false)
    s.axis(1, -30_000, "pad", false)
    s.tick(0)
    expect(s.actions).toEqual([])
    s.report()
    expect(s.actions).toHaveLength(1)
    expect(s.actions[0]).toMatchObject({ type: "direction", direction: "up" })
    s.axis(0, 0, "pad", false); s.axis(1, 0, "pad", false); s.report()
    s.key(0x130, 1); s.key(0x130, 0)
    expect(s.actions.map(action => action.type)).toEqual(["direction", "direction-end", "confirm"])
  })

  it("never fires from a held baseline and requires full source neutral before rearm", () => {
    const s = session()
    s.key(0x130, 1)
    s.axis(16, 1)
    s.initialize()
    s.tick(1000)
    s.key(0x130, 0)
    s.key(0x131, 1)
    s.key(0x131, 0)
    expect(s.actions).toEqual([])
    s.axis(16, 0)
    s.key(0x130, 1)
    expect(s.actions).toEqual([{ type: "confirm", source: "native" }])
  })

  it("arms a neutral baseline without requiring a synthetic input event", () => {
    const s = session()
    s.initialize()
    expect(s.actions).toEqual([])
    s.key(0x130, 1)
    s.key(0x130, 2)
    s.key(0x130, 1)
    s.key(0x130, 0)
    s.key(0x130, 1)
    expect(s.actions).toEqual([{ type: "confirm", source: "native" }, { type: "confirm", source: "native" }])
  })

  it("keeps legacy buttons but leaves Home/system shortcuts to the host", () => {
    const s = session(); s.initialize()
    for (const code of [0x130, 0x131, 0x13a, 0x134, 0x13b, 0x13c]) { s.key(code, 1); s.key(code, 0) }
    expect(s.actions.map(action => action.type)).toEqual(["confirm", "back", "back", "options", "menu"])
  })

  it("repeats a real held direction after 400ms without refresh events, ends exactly once", () => {
    const s = session(); s.initialize()
    s.key(0x223, 1)
    s.tick(399)
    expect(s.actions).toHaveLength(1)
    s.tick(400); s.tick(500); s.tick(20_000)
    s.key(0x223, 0)
    s.tick(21_000)
    s.mapper.reset()
    expect(s.actions).toEqual([
      { type: "direction", direction: "right", source: "native", releaseExpected: true, gestureId: 1 },
      ...Array.from({ length: 3 }, (): InputAction => ({ type: "direction", direction: "right", source: "native", releaseExpected: true, gestureId: 1, repeat: true })),
      { type: "direction-end", direction: "right", source: "native", gestureId: 1 },
    ])
  })

  it("uses one gesture for agreeing stick and D-pad, releases before direction changes", () => {
    const s = session(); s.initialize()
    s.axis(0, 20_000); s.axis(16, 1)
    expect(s.actions).toHaveLength(1)
    s.axis(16, -1)
    expect(s.actions.map(action => action.type)).toEqual(["direction", "direction-end", "direction"])
    expect(s.actions.at(-1)).toMatchObject({ direction: "left", gestureId: 2 })
    s.mapper.clearDevice("pad")
    expect(s.actions.at(-1)).toMatchObject({ type: "direction-end", gestureId: 2 })
  })

  it("preserves legacy low-range metadata, dominant axes and flat dead zones", () => {
    const s = session()
    s.mapper.configureDevice({ ...PAD, axes: [
      { code: 0, minimum: -1408, maximum: 1408, flat: 200 },
      { code: 1, minimum: -1408, maximum: 1408, flat: 200 },
    ] })
    s.initialize()
    s.axis(0, 150); s.axis(1, -150)
    expect(s.actions).toEqual([])
    s.axis(0, 1000)
    expect(s.actions.at(-1)).toMatchObject({ direction: "right" })
    s.axis(1, -1200)
    expect(s.actions.at(-1)).toMatchObject({ direction: "up" })
  })

  it("does not infer scaling from device IDs or incomplete metadata", () => {
    for (const axes of [undefined, [{ code: 0, minimum: -1408, maximum: 1408 }], [{ code: 0, minimum: 0, maximum: 0 }, { code: 1, minimum: -1, maximum: 1 }]]) {
      const s = session()
      s.mapper.configureDevice({ ...PAD, name: "rsinput-gamepad/input0", axes })
      s.initialize(); s.axis(0, 1000)
      expect(s.actions).toEqual([])
    }
  })

  it("uses unsigned axis centers and trigger minima in held baselines", () => {
    const s = session()
    s.mapper.configureDevice({ ...PAD, axes: [
      { code: 0, minimum: 0, maximum: 255 }, { code: 1, minimum: 0, maximum: 255 },
      { code: 2, minimum: 0, maximum: 255 },
    ] })
    s.axis(0, 128); s.axis(1, 128); s.axis(2, 255); s.initialize()
    s.key(0x130, 1); s.key(0x130, 0)
    expect(s.actions).toEqual([])
    s.axis(2, 0); s.key(0x130, 1)
    expect(s.actions).toEqual([{ type: "confirm", source: "native" }])
  })

  it("separates held arrival and rearm for concurrent sources", () => {
    const s = session(); s.initialize()
    s.mapper.configureDevice({ ...PAD, deviceId: "second" })
    s.key(0x130, 1, "second")
    s.mapper.initialize("second")
    s.key(0x130, 1)
    s.key(0x131, 1, "second")
    expect(s.actions).toEqual([{ type: "confirm", source: "native" }])
    s.key(0x131, 0, "second"); s.key(0x130, 0, "second"); s.key(0x130, 1, "second")
    expect(s.actions).toHaveLength(2)
  })

  it("inactive/disconnected sources release gestures and returning held state never fires", () => {
    const s = session(); s.initialize(); s.axis(16, 1)
    s.inactive()
    expect(s.actions.at(-1)).toMatchObject({ type: "direction-end" })
    s.tick(1000)
    s.active(); s.mapper.configureDevice(PAD); s.axis(16, 1); s.initialize(); s.tick(2000)
    expect(s.actions).toHaveLength(2)
    s.axis(16, 0); s.axis(16, 1)
    expect(s.actions.at(-1)).toMatchObject({ type: "direction", gestureId: 2 })
  })

  it("does not emit the replacement gesture after a release listener resets input", () => {
    const actions: InputAction[] = []
    const mapper = createNativeGamepadMapper(action => {
      actions.push(action)
      if (action.type === "direction-end") mapper.reset()
    })
    mapper.configureDevice(PAD); mapper.initialize()
    const input = (value: number) => {
      mapper.handle({ kind: "input", class: "gamepad", deviceId: "pad", type: 3, code: 16, value, timestamp: 0 })
      mapper.handle({ kind: "input", class: "gamepad", deviceId: "pad", type: 0, code: 0, value: 0, timestamp: 0 })
    }
    input(1); input(-1)
    expect(actions.map(action => action.type)).toEqual(["direction", "direction-end"])
  })
})
