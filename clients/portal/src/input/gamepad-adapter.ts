import type { Direction, InputAdapter, InputListener } from "./types"

/** Browser-owned W3C Gamepad fields. Hardware data stops at this adapter. */
export type GamepadSnapshot = Pick<Gamepad, "id" | "index" | "mapping" | "connected" | "buttons" | "axes">

export interface GamepadAdapterOptions {
  readonly readGamepads?: () => readonly (GamepadSnapshot | null)[]
  readonly isActive?: () => boolean
  readonly requestFrame?: (callback: FrameRequestCallback) => number
  readonly cancelFrame?: (id: number) => void
  readonly page?: EventTarget
  readonly visibility?: EventTarget
}

// Preserve legacy:product/platform/input/gamepad-adapter.ts's standard layout
// and repeat timing. Nonstandard pads must be normalized, never guessed here.
const BUTTON_ACTIONS = [[0, "confirm"], [1, "back"], [3, "options"], [9, "menu"]] as const
const REPEAT_DELAY = 400
const REPEAT_INTERVAL = 100
const AXIS_THRESHOLD = 0.5

type NavigationDirection = { readonly kind: "neutral" } | { readonly kind: "direction"; readonly direction: Direction }

type HeldDirection =
  | { readonly kind: "released" }
  | { readonly kind: "held"; readonly direction: Direction; readonly gestureId: number; nextRepeat: number }
type PadState =
  | { readonly kind: "waiting-neutral"; readonly id: string }
  | { readonly kind: "ready"; readonly id: string; held: HeldDirection; buttons: readonly boolean[] }

export function createGamepadAdapter(options: GamepadAdapterOptions = {}): InputAdapter {
  return {
    name: "gamepad",
    start(emit) {
      if (!options.readGamepads && (typeof navigator === "undefined" || !navigator.getGamepads)) return () => {}
      const read = options.readGamepads ?? (() => navigator.getGamepads())
      const active = options.isActive ?? (() => document.visibilityState !== "hidden" && document.hasFocus())
      const request = options.requestFrame ?? requestAnimationFrame
      const cancel = options.cancelFrame ?? cancelAnimationFrame
      const page = options.page ?? window
      const visibility = options.visibility ?? document
      const states = new Map<number, PadState>()
      let nextGestureId = 0
      let disposed = false
      let frame = 0

      const retire = () => {
        for (const state of states.values()) release(state, emit)
        states.clear()
      }
      const onVisibility = () => { if (!active()) retire() }
      const onDisconnect = (event: Event) => {
        if (!("gamepad" in event)) return
        const pad = event.gamepad
        if (typeof pad !== "object" || pad === null || !("index" in pad) || typeof pad.index !== "number") return
        const state = states.get(pad.index)
        if (!state) return
        states.delete(pad.index)
        release(state, emit)
      }

      const poll: FrameRequestCallback = now => {
        if (disposed) return
        if (!active()) {
          retire()
        } else {
          const seen = new Set<number>()
          // SecurityError is possible under Permissions-Policy. Leave the
          // existing keyboard/native input paths usable and retire held input.
          let pads: readonly (GamepadSnapshot | null)[]
          try { pads = read() } catch { pads = [] }
          for (const pad of pads) {
            if (disposed || !active()) break
            if (!pad?.connected || pad.mapping !== "standard") continue
            seen.add(pad.index)
            let state = states.get(pad.index)
            if (!state || state.id !== pad.id) {
              if (state) {
                release(state, emit)
                if (disposed || !active() || states.get(pad.index) !== state) continue
              }
              state = { kind: "waiting-neutral", id: pad.id }
              states.set(pad.index, state)
            }
            const navigation = directionFor(pad)
            if (state.kind === "waiting-neutral") {
              // A press that exposed the pad, survived a focus change, or was
              // held on connection must not launch a game on the next frame.
              if (navigation.kind !== "neutral" || pad.buttons.some(button => button.pressed)) continue
              state = { kind: "ready", id: pad.id, held: { kind: "released" }, buttons: [] }
              states.set(pad.index, state)
            }
            const held = state.held
            if (held.kind === "held" && (navigation.kind === "neutral" || held.direction !== navigation.direction)) release(state, emit)
            // A release listener can synchronously blur or dispose the page.
            if (disposed || !active() || states.get(pad.index) !== state) continue
            if (navigation.kind === "direction") {
              const { direction } = navigation
              if (state.held.kind === "released") {
                state.held = { kind: "held", direction, gestureId: ++nextGestureId, nextRepeat: now + REPEAT_DELAY }
                emit({ type: "direction", direction, source: "gamepad", releaseExpected: true, gestureId: state.held.gestureId })
              } else if (now >= state.held.nextRepeat) {
                state.held.nextRepeat = now + REPEAT_INTERVAL
                emit({ type: "direction", direction, source: "gamepad", releaseExpected: true, gestureId: state.held.gestureId, repeat: true })
              }
            }
            for (const [index, type] of BUTTON_ACTIONS) {
              if (disposed || !active() || states.get(pad.index) !== state) break
              if (pad.buttons[index]?.pressed && !state.buttons[index]) emit({ type, source: "gamepad" })
            }
            state.buttons = pad.buttons.map(button => button.pressed)
          }
          for (const [index, state] of states) {
            if (seen.has(index)) continue
            release(state, emit)
            states.delete(index)
          }
        }
        if (!disposed) frame = request(poll)
      }
      page.addEventListener("blur", retire)
      page.addEventListener("gamepaddisconnected", onDisconnect)
      visibility.addEventListener("visibilitychange", onVisibility)
      frame = request(poll)
      return () => {
        if (disposed) return
        disposed = true
        cancel(frame)
        page.removeEventListener("blur", retire)
        page.removeEventListener("gamepaddisconnected", onDisconnect)
        visibility.removeEventListener("visibilitychange", onVisibility)
        retire()
      }
    },
  }
}

function release(state: PadState, emit: InputListener): void {
  if (state.kind !== "ready" || state.held.kind !== "held") return
  const held = state.held
  state.held = { kind: "released" }
  emit({ type: "direction-end", direction: held.direction, source: "gamepad", gestureId: held.gestureId })
}

function directionFor(pad: GamepadSnapshot): NavigationDirection {
  const horizontal = Number(pad.buttons[15]?.pressed ?? false) - Number(pad.buttons[14]?.pressed ?? false)
  const vertical = Number(pad.buttons[13]?.pressed ?? false) - Number(pad.buttons[12]?.pressed ?? false)
  // One gesture per pad avoids double movement when stick and D-pad agree.
  if (vertical !== 0) return { kind: "direction", direction: vertical > 0 ? "down" : "up" }
  if (horizontal !== 0) return { kind: "direction", direction: horizontal > 0 ? "right" : "left" }
  const x = pad.axes[0] ?? 0
  const y = pad.axes[1] ?? 0
  if (Math.abs(x) > Math.abs(y)) {
    if (x > AXIS_THRESHOLD) return { kind: "direction", direction: "right" }
    if (x < -AXIS_THRESHOLD) return { kind: "direction", direction: "left" }
  } else {
    if (y > AXIS_THRESHOLD) return { kind: "direction", direction: "down" }
    if (y < -AXIS_THRESHOLD) return { kind: "direction", direction: "up" }
  }
  return { kind: "neutral" }
}
