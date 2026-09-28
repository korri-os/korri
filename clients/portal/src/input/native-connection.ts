import type { NativeInputEvent, NativeInputSubscription } from "@contracts/generated/korrid"
import type { LinuxRuntimeConfig } from "../runtime-config"

export interface NativeInputConnectionOptions {
  readonly onEvent: (event: NativeInputEvent) => void
  /** Retire held input whenever the connection is lost or disposed. */
  readonly onDisconnect: () => void
  readonly reconnect?: {
    readonly initialDelayMs?: number
    readonly maxDelayMs?: number
    readonly factor?: number
  }
}

// Preserve legacy native-adapter.ts's reconnect policy. Reset the delay only
// after server data, not an upgrade that can still fail authentication.
const RECONNECT_INITIAL_DELAY_MS = 250
const RECONNECT_MAX_DELAY_MS = 5_000
const RECONNECT_FACTOR = 2
const MAX_MESSAGE_BYTES = 64 * 1024

/** The socket only subscribes to this device's korrid. It cannot inject input. */
export function connectNativeInput(
  runtime: LinuxRuntimeConfig,
  options: NativeInputConnectionOptions,
): () => void {
  const initialDelay = options.reconnect?.initialDelayMs ?? RECONNECT_INITIAL_DELAY_MS
  const maximumDelay = options.reconnect?.maxDelayMs ?? RECONNECT_MAX_DELAY_MS
  const factor = options.reconnect?.factor ?? RECONNECT_FACTOR
  if (!Number.isFinite(initialDelay) || initialDelay <= 0
    || !Number.isFinite(maximumDelay) || maximumDelay < initialDelay
    || !Number.isFinite(factor) || factor < 1) {
    throw new Error("Invalid native input reconnect policy.")
  }
  // The same private bootstrap values already used by the HTTP RPC client.
  if (!Number.isInteger(runtime.korridPort) || runtime.korridPort < 1 || runtime.korridPort > 65_535
    || !/^[0-9a-f]{64}$/.test(runtime.korridCapability)) {
    throw new Error("Invalid native input connection.")
  }
  const subscription: NativeInputSubscription = { classes: ["gamepad"] }
  let disposed = false
  let socket: WebSocket | undefined
  let retry: ReturnType<typeof setTimeout> | undefined
  let nextDelay = initialDelay

  const retire = (current: WebSocket) => {
    if (socket !== current) return
    socket = undefined
    current.close()
    options.onDisconnect()
    if (disposed) return
    retry = setTimeout(() => {
      retry = undefined
      connect()
    }, nextDelay)
    nextDelay = Math.min(maximumDelay, nextDelay * factor)
  }

  const connect = () => {
    if (disposed) return
    const current = new WebSocket(`ws://127.0.0.1:${runtime.korridPort}/`)
    socket = current
    current.addEventListener("open", () => {
      if (disposed || socket !== current) return
      // Ordered frames: existing bearer representation, then legacy subscription.
      // There is no credential in the URL and no new authentication object.
      current.send(`Bearer ${runtime.korridCapability}`)
      current.send(JSON.stringify(subscription))
    })
    current.addEventListener("message", message => {
      if (disposed || socket !== current) return
      if (typeof message.data !== "string"
        || message.data.length > MAX_MESSAGE_BYTES
        || new TextEncoder().encode(message.data).length > MAX_MESSAGE_BYTES) {
        retire(current)
        return
      }
      try {
        // Korrid serializes the Rust treaty after validation and authentication.
        // Keep the generated type as the sole event schema, like the RPC client.
        const event: NativeInputEvent = JSON.parse(message.data)
        nextDelay = initialDelay
        options.onEvent(event)
      } catch {
        // Never include a received frame or connection credential in diagnostics.
        retire(current)
      }
    })
    current.addEventListener("error", () => retire(current))
    current.addEventListener("close", () => retire(current))
  }

  connect()
  return () => {
    if (disposed) return
    disposed = true
    if (retry !== undefined) clearTimeout(retry)
    if (socket) retire(socket)
  }
}
