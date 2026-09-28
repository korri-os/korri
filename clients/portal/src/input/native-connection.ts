import type { NativeInputAcknowledgement, NativeInputEvent, NativeInputRetirement, NativeInputSubscription } from "@contracts/generated/korrid"
import type { LinuxRuntimeConfig } from "../runtime-config"
import { decodeNativeInputMessage } from "./native-decoder"

export interface NativeInputConnectionOptions {
  readonly onEvent: (event: NativeInputEvent) => void
  /** Synchronous: clear held state before the lifecycle acknowledgement. */
  readonly onDisconnect: () => void
  readonly onInitialized?: () => void
  readonly onDeviceInitialized?: (deviceId: string) => void
  readonly initiallyActive?: boolean
  readonly reconnect?: {
    readonly initialDelayMs?: number
    readonly maxDelayMs?: number
    readonly factor?: number
  }
}
export interface NativeInputConnection {
  (): void
  /** Retire data locally without destroying the host's pending ACK channel. */
  setActive(active: boolean): void
}

const RECONNECT_INITIAL_DELAY_MS = 250
const RECONNECT_MAX_DELAY_MS = 5_000
const RECONNECT_FACTOR = 2
const MAX_MESSAGE_BYTES = 64 * 1024
// Same finite attachment budget as korrid. Local retirement releases sockets
// only with server acknowledgement; selected host ACK channels stay alive.
const MAX_CONNECTIONS = 4

/** Private local korrid only. The browser subscribes; it cannot inject input. */
export function connectNativeInput(runtime: LinuxRuntimeConfig, options: NativeInputConnectionOptions): NativeInputConnection {
  const initialDelay = options.reconnect?.initialDelayMs ?? RECONNECT_INITIAL_DELAY_MS
  const maximumDelay = options.reconnect?.maxDelayMs ?? RECONNECT_MAX_DELAY_MS
  const factor = options.reconnect?.factor ?? RECONNECT_FACTOR
  if (!Number.isFinite(initialDelay) || initialDelay <= 0
    || !Number.isFinite(maximumDelay) || maximumDelay < initialDelay
    || !Number.isFinite(factor) || factor < 1) throw new Error("Invalid native input reconnect policy.")
  if (!Number.isInteger(runtime.korridPort) || runtime.korridPort < 1 || runtime.korridPort > 65_535
    || !/^[0-9a-f]{64}$/.test(runtime.korridCapability)) throw new Error("Invalid native input connection.")
  const subscription: NativeInputSubscription = { classes: ["gamepad"] }
  type Attachment = { socket: WebSocket; retired: boolean; releaseRequested?: boolean; generation?: string; requestId?: string }
  const attachments = new Set<Attachment>()
  let current: Attachment | undefined
  let disposed = false
  let active = options.initiallyActive ?? true
  let retry: ReturnType<typeof setTimeout> | undefined
  let nextDelay = initialDelay

  const clear = (attachment: Attachment) => {
    if (attachment.retired) return
    attachment.retired = true // permanently retire BEFORE emitting semantic releases
    options.onDisconnect()
  }
  const schedule = () => {
    if (disposed || !active || current || retry !== undefined) return
    retry = setTimeout(() => { retry = undefined; connect() }, nextDelay)
    nextDelay = Math.min(maximumDelay, nextDelay * factor)
  }
  const close = (attachment: Attachment) => {
    if (!attachments.delete(attachment)) return
    if (current === attachment) current = undefined
    attachment.socket.close()
    clear(attachment)
    schedule()
  }
  const release = (attachment: Attachment) => {
    if (!attachment.retired || attachment.generation === undefined
      || attachment.requestId !== undefined || attachment.releaseRequested
      || disposed || !attachments.has(attachment)) return
    // Clearing happened before this request. The server serializes release
    // with suspend selection; only its retired reply permits local close.
    attachment.releaseRequested = true
    const retirement: NativeInputRetirement = { kind: "retire", generation: attachment.generation }
    attachment.socket.send(JSON.stringify(retirement))
  }
  const connect = () => {
    if (disposed || !active || current) return
    if (attachments.size >= MAX_CONNECTIONS) { schedule(); return }
    const socket = new WebSocket(`ws://127.0.0.1:${runtime.korridPort}/`)
    const attachment: Attachment = { socket, retired: false }
    attachments.add(attachment)
    current = attachment
    socket.addEventListener("open", () => {
      if (disposed || !attachments.has(attachment)) return
      socket.send(`Bearer ${runtime.korridCapability}`)
      socket.send(JSON.stringify(subscription))
    })
    socket.addEventListener("message", message => {
      if (disposed || !attachments.has(attachment)) return
      if (typeof message.data !== "string" || message.data.length > MAX_MESSAGE_BYTES
        || new TextEncoder().encode(message.data).length > MAX_MESSAGE_BYTES) { close(attachment); return }
      try {
        const event = decodeNativeInputMessage(JSON.parse(message.data))
        // Lifecycle handling MUST precede the retired-data guard. Blur often
        // occurs after the host selects this subscriber but before suspend.
        if (event.kind === "suspend") {
          if (attachment.requestId !== undefined
            || (attachment.generation !== undefined && event.generation !== attachment.generation)) {
            close(attachment); return
          }
          attachment.generation = event.generation
          attachment.requestId = event.requestId
          clear(attachment)
          if (disposed || !attachments.has(attachment)) return
          const ack: NativeInputAcknowledgement = { kind: "suspended", generation: event.generation, requestId: event.requestId }
          socket.send(JSON.stringify(ack))
          return
        }
        if (event.kind === "retired") {
          if (!attachment.retired || !attachment.releaseRequested || event.generation !== attachment.generation) {
            close(attachment); return
          }
          close(attachment)
          return
        }
        if (event.kind === "resume") {
          if (!attachment.retired || event.generation !== attachment.generation || event.requestId !== attachment.requestId) {
            close(attachment); return
          }
          close(attachment) // never reactivate this closure; reconnect with baseline
          return
        }
        if (event.kind === "initialization-complete") {
          if (attachment.generation !== undefined) { close(attachment); return }
          attachment.generation = event.generation
          if (attachment.retired) { release(attachment); return }
          nextDelay = initialDelay
          options.onInitialized?.()
          return
        }
        if (attachment.retired) return
        nextDelay = initialDelay
        if (event.kind === "device-state-complete") options.onDeviceInitialized?.(event.deviceId)
        else options.onEvent(event)
      } catch {
        // Never include a frame or decoder/dependency error in diagnostics.
        close(attachment)
      }
    })
    socket.addEventListener("error", () => close(attachment))
    socket.addEventListener("close", () => close(attachment))
  }
  const dispose: NativeInputConnection = Object.assign(() => {
    if (disposed) return
    disposed = true
    if (retry !== undefined) clearTimeout(retry)
    for (const attachment of [...attachments]) close(attachment)
  }, {
    setActive(value: boolean) {
      if (disposed || active === value) return
      active = value
      if (!active) {
        if (retry !== undefined) { clearTimeout(retry); retry = undefined }
        const attachment = current
        current = undefined
        if (attachment) {
          clear(attachment)
          release(attachment) // keep lifecycle channel until server resolves race
        }
      } else connect()
    },
  })
  connect()
  return dispose
}
