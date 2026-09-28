import type { LinuxRuntimeConfig } from "../runtime-config"
import { connectNativeInput, type NativeInputConnectionOptions } from "./native-connection"
import { createNativeGamepadMapper } from "./native-gamepad-mapper"
import type { InputAdapter } from "./types"

export interface NativeInputAdapterOptions {
  readonly isActive?: () => boolean
  readonly page?: EventTarget
  readonly visibility?: EventTarget
  readonly requestFrame?: (callback: FrameRequestCallback) => number
  readonly cancelFrame?: (id: number) => void
  readonly now?: () => number
  readonly reconnect?: NativeInputConnectionOptions["reconnect"]
}

/** Linux controller path. Never reads browser Gamepad or executes host shortcuts. */
export function createNativeInputAdapter(runtime: LinuxRuntimeConfig, options: NativeInputAdapterOptions = {}): InputAdapter {
  return {
    name: "native",
    start(emit) {
      const active = options.isActive ?? (() => document.visibilityState !== "hidden" && document.hasFocus())
      const page = options.page ?? window
      const visibility = options.visibility ?? document
      const request = options.requestFrame ?? requestAnimationFrame
      const cancel = options.cancelFrame ?? cancelAnimationFrame
      const mapper = createNativeGamepadMapper(emit, { isActive: active, now: options.now })
      let disposed = false
      let frame = 0
      const retire = () => {
        connection.setActive(false) // keep control-only channel for pending host ACK
        mapper.reset()
      }
      const activate = () => {
        if (disposed || !active()) { retire(); return }
        connection.setActive(true)
      }
      const connection = connectNativeInput(runtime, {
        initiallyActive: active(),
        reconnect: options.reconnect,
        onDisconnect: () => mapper.reset(),
        onInitialized: () => { if (active()) mapper.initialize(); else retire() },
        onDeviceInitialized: id => { if (active()) mapper.initialize(id); else retire() },
        onEvent: event => {
          if (disposed || !active()) { retire(); return }
          if (event.kind === "device-added") mapper.configureDevice(event.device)
          else if (event.kind === "device-removed") mapper.clearDevice(event.deviceId)
          else if (event.kind === "input" && event.class === "gamepad") mapper.handle(event)
          // Host owns action/system handling. Browser authentication grants
          // receipt only, never controller injection or host shortcut authority.
        },
      })
      const poll: FrameRequestCallback = time => {
        if (disposed) return
        if (!active()) retire()
        else { activate(); mapper.tick(time) }
        if (!disposed) frame = request(poll)
      }
      page.addEventListener("blur", retire)
      page.addEventListener("focus", activate)
      visibility.addEventListener("visibilitychange", activate)
      activate()
      frame = request(poll)
      return () => {
        if (disposed) return
        disposed = true
        cancel(frame)
        page.removeEventListener("blur", retire)
        page.removeEventListener("focus", activate)
        visibility.removeEventListener("visibilitychange", activate)
        connection()
        mapper.reset()
      }
    },
  }
}
