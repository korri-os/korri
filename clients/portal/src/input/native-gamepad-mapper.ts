import type { NativeInputDeviceInfo, NativeInputInput } from "@contracts/generated/korrid"
import type { Direction, InputListener } from "./types"

// Extracted from legacy:product/platform/input/native/{button-codes,gamepad-mapper}.ts.
// Preserve metadata scaling, dominant axes and action codes, not the legacy
// 250ms stale timer (which expired before its 400ms first repeat).
const EV_KEY = 1
const EV_ABS = 3
const ABS_X = 0
const ABS_Y = 1
const ABS_HAT0X = 16
const ABS_HAT0Y = 17
const BUTTONS = new Map<number, "confirm" | "back" | "options" | "menu">([
  [0x130, "confirm"], [0x131, "back"], [0x13a, "back"], [0x134, "options"], [0x13b, "menu"],
])
// BTN_MODE/Home and system actions belong to the host; never duplicate them here.
const REPEAT_DELAY = 400
const REPEAT_INTERVAL = 100
// Match portal_input.rs: contracts/input MAX_PHYSICAL_SOURCES (255) plus the
// receiver's authenticated remote controllerNumber range 0..=15. Not seat count
// or a claim about hardware throughput; overflow sources still navigate.
const MAX_DEVICES = 255 + 16

type Axis = NonNullable<NativeInputDeviceInfo["axes"]>[number]
type Hold = { direction: Direction; gestureId: number; nextRepeat: number }
type Device = {
  initialized: boolean
  ready: boolean
  keys: Set<number>
  values: Map<number, number>
  axes: Map<number, Axis>
  pending?: { keys: Set<number>; values: Map<number, number> }
  hold?: Hold
}

export interface NativeGamepadMapperOptions {
  readonly isActive?: () => boolean
  readonly now?: () => number
}

/** Current state, not historical replay. Baselines never produce actions. */
export function createNativeGamepadMapper(emit: InputListener, options: NativeGamepadMapperOptions = {}) {
  const devices = new Map<string, Device>()
  const active = options.isActive ?? (() => true)
  const now = options.now ?? (() => performance.now())
  let nextGesture = 0
  let initialized = false

  const release = (state: Device) => {
    const hold = state.hold
    state.hold = undefined
    if (hold) emit({ type: "direction-end", direction: hold.direction, source: "native", gestureId: hold.gestureId })
  }
  const current = (id: string, state: Device) => devices.get(id) === state && initialized && state.initialized && active()
  const reconcile = (id: string, state: Device, time: number) => {
    if (!current(id, state)) return
    if (!state.ready) {
      if (neutral(state)) state.ready = true
      return // the neutral transition only arms; it never fires
    }
    const direction = directionFor(state)
    if (state.hold && state.hold.direction !== direction) release(state)
    if (!current(id, state)) return // release listener may blur/dispose synchronously
    if (!direction) return
    if (!state.hold) {
      state.hold = { direction, gestureId: ++nextGesture, nextRepeat: time + REPEAT_DELAY }
      emit({ type: "direction", direction, source: "native", releaseExpected: true, gestureId: state.hold.gestureId })
    } else if (time >= state.hold.nextRepeat) {
      state.hold.nextRepeat = time + REPEAT_INTERVAL // no catch-up burst after a busy frame
      emit({ type: "direction", direction, source: "native", releaseExpected: true, gestureId: state.hold.gestureId, repeat: true })
    }
  }
  const clearDevice = (id: string) => {
    const state = devices.get(id)
    devices.delete(id)
    if (state) release(state)
  }
  const commit = (state: Device) => {
    if (!state.pending) return
    state.keys = state.pending.keys
    state.values = state.pending.values
    state.pending = undefined
  }
  const reset = () => {
    initialized = false
    const retired = [...devices.values()]
    devices.clear()
    for (const state of retired) release(state)
  }
  return {
    configureDevice(info: NativeInputDeviceInfo) {
      clearDevice(info.deviceId)
      if (info.class !== "gamepad") return
      if (devices.size >= MAX_DEVICES) throw new Error("Native input device limit reached.")
      devices.set(info.deviceId, {
        initialized: false, ready: false, keys: new Set(), values: new Map(),
        axes: new Map(info.axes?.map(axis => [axis.code, axis])),
      })
    },
    initialize(deviceId?: string) {
      if (deviceId === undefined) initialized = true
      for (const [id, state] of devices) {
        if (deviceId !== undefined && deviceId !== id) continue
        commit(state) // initialization completion commits a silent whole baseline
        state.initialized = true
        // Arm a neutral baseline immediately, but a held baseline must reach
        // full neutral before ANY button or direction on that source can fire.
        state.ready = initialized && active() && neutral(state)
      }
    },
    handle(event: NativeInputInput) {
      const state = devices.get(event.deviceId)
      if (!state || event.class !== "gamepad") throw new Error("Unknown native input source.")
      if (!active()) { reset(); return }
      if (event.type === 0 && event.code === 0 && event.value === 0) {
        // EV_SYN/SYN_REPORT: one measured sample, never actionable partial deltas.
        const previousKeys = state.keys
        const ready = state.ready
        commit(state)
        reconcile(event.deviceId, state, now())
        for (const [code, type] of BUTTONS) {
          if (!ready || !current(event.deviceId, state)) break
          if (state.keys.has(code) && !previousKeys.has(code)) emit({ type, source: "native" })
        }
      } else if (event.type === EV_KEY || event.type === EV_ABS) {
        const pending = state.pending ??= { keys: new Set(state.keys), values: new Map(state.values) }
        if (event.type === EV_KEY) {
          if (event.value === 0) pending.keys.delete(event.code)
          else pending.keys.add(event.code)
        } else pending.values.set(event.code, event.value)
      }
    },
    tick(time: number) {
      if (!active()) { reset(); return }
      for (const [id, state] of devices) reconcile(id, state, time)
    },
    clearDevice,
    reset,
  }
}

function normalized(value: number, axis: Axis): number {
  const center = (axis.minimum + axis.maximum) / 2
  const offset = value - center
  if (axis.flat !== undefined && Math.abs(offset) <= axis.flat) return 0
  const extent = Math.max(axis.maximum - center, center - axis.minimum)
  return extent <= 0 ? 0 : Math.max(-1, Math.min(1, offset / extent))
}
function stick(state: Device): { x: number; y: number; threshold: number } {
  const x = state.axes.get(ABS_X)
  const y = state.axes.get(ABS_Y)
  // Legacy requires BOTH metadata ranges; no device-name scaling heuristics.
  if (x && y && x.maximum > x.minimum && y.maximum > y.minimum) return {
    x: normalized(state.values.get(ABS_X) ?? (x.minimum + x.maximum) / 2, x),
    y: normalized(state.values.get(ABS_Y) ?? (y.minimum + y.maximum) / 2, y), threshold: 0.6,
  }
  return { x: state.values.get(ABS_X) ?? 0, y: state.values.get(ABS_Y) ?? 0, threshold: 16_000 }
}
function directionFor(state: Device): Direction | undefined {
  const horizontal = Number(state.keys.has(0x223)) - Number(state.keys.has(0x222)) || state.values.get(ABS_HAT0X) || 0
  const vertical = Number(state.keys.has(0x221)) - Number(state.keys.has(0x220)) || state.values.get(ABS_HAT0Y) || 0
  // Match the existing semantic adapter: one gesture per source, D-pad priority.
  if (vertical !== 0) return vertical > 0 ? "down" : "up"
  if (horizontal !== 0) return horizontal > 0 ? "right" : "left"
  const { x, y, threshold } = stick(state)
  if (Math.abs(x) > Math.abs(y)) {
    if (x > threshold) return "right"
    if (x < -threshold) return "left"
  } else {
    if (y > threshold) return "down"
    if (y < -threshold) return "up"
  }
  return undefined
}
function neutral(state: Device): boolean {
  if (state.keys.size > 0 || directionFor(state) !== undefined) return false
  // Legacy W3C adapter also waits for triggers. Native evdev carries these as
  // ABS_Z/ABS_RZ rather than W3C buttons; their resting value is the minimum.
  for (const code of [2, 5]) {
    const axis = state.axes.get(code)
    const value = state.values.get(code)
    if (value !== undefined && value > (axis?.minimum ?? 0) + (axis?.flat ?? 0)) return false
  }
  return true
}
