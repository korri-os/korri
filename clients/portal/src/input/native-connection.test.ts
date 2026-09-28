import { afterEach, describe, expect, it } from "bun:test"
import type { ServerWebSocket } from "bun"
import type { NativeInputEvent } from "@contracts/generated/korrid"
import { connectNativeInput } from "./native-connection"

const CAPABILITY = "a".repeat(64)
const ADDED = {
  kind: "device-added",
  device: { deviceId: "seat-one", class: "gamepad", name: "Korri Seat P1", capabilities: [] },
} satisfies NativeInputEvent
const INPUT = {
  kind: "input", deviceId: "seat-one", class: "gamepad", type: 1, code: 304, value: 1, timestamp: 123,
} satisfies NativeInputEvent
const disposers: Array<() => void> = []
const servers: Array<{ stop(closeActiveConnections: boolean): unknown }> = []

afterEach(() => {
  for (const dispose of disposers.splice(0)) dispose()
  for (const server of servers.splice(0)) server.stop(true)
})

type Connection = { readonly index: number; readonly frames: string[] }
type Socket = ServerWebSocket<Connection>

function serve(options: { readonly subscribed?: (socket: Socket) => void; readonly acknowledged?: (socket: Socket, frame: string) => void; readonly capability?: string } = {}) {
  const requests: URL[] = []
  const connections: Connection[] = []
  const sockets: Socket[] = []
  const closed: number[] = []
  const server = Bun.serve<Connection>({
    hostname: "127.0.0.1",
    port: 0,
    fetch(request, server) {
      requests.push(new URL(request.url))
      const connection = { index: connections.length, frames: [] }
      connections.push(connection)
      if (server.upgrade(request, { data: connection })) return
      return new Response(null, { status: 400 })
    },
    websocket: {
      open(socket) { sockets.push(socket) },
      message(socket, message) {
        const frame = String(message)
        socket.data.frames.push(frame)
        if (socket.data.frames.length === 1) {
          if (frame !== `Bearer ${options.capability ?? CAPABILITY}`) socket.close(1008)
          return
        }
        if (socket.data.frames.length > 2 && options.acknowledged) {
          options.acknowledged(socket, frame)
          return
        }
        if (frame !== JSON.stringify({ classes: ["gamepad"] })) {
          socket.close(1008)
          return
        }
        options.subscribed?.(socket)
      },
      close(socket) { closed.push(socket.data.index) },
    },
  })
  servers.push(server)
  if (server.port === undefined) throw new Error("Native input test server has no TCP port.")
  return { port: server.port, requests, connections, sockets, closed }
}

function connect(port: number, options: Partial<Parameters<typeof connectNativeInput>[1]> = {}) {
  const events: NativeInputEvent[] = []
  let disconnects = 0
  const dispose = connectNativeInput({ korridPort: port, korridCapability: CAPABILITY }, {
    onEvent: event => events.push(event),
    onDisconnect: () => { disconnects += 1 },
    reconnect: { initialDelayMs: 20, maxDelayMs: 80 },
    ...options,
  })
  disposers.push(dispose)
  return { events, dispose, disconnects: () => disconnects }
}

async function waitFor(predicate: () => boolean) {
  const deadline = Date.now() + 2_000
  while (!predicate()) {
    if (Date.now() > deadline) throw new Error("Native input condition timed out.")
    await Bun.sleep(5)
  }
}

describe("native input connection", () => {
  it("authenticates before the unchanged legacy subscription without a credential in the URL", async () => {
    const server = serve({ subscribed(socket) {
      socket.send(JSON.stringify(ADDED))
      socket.send(JSON.stringify(INPUT))
    } })
    const client = connect(server.port)
    await waitFor(() => client.events.length === 2)
    expect(server.requests[0]?.pathname).toBe("/")
    expect(server.requests[0]?.search).toBe("")
    expect(server.requests[0]?.username).toBe("")
    expect(server.requests[0]?.password).toBe("")
    expect(server.connections[0]?.frames).toEqual([
      `Bearer ${CAPABILITY}`,
      JSON.stringify({ classes: ["gamepad"] }),
    ])
    expect(client.events).toEqual([ADDED, INPUT])
  })

  it("retires input and authenticates again on a fresh connection", async () => {
    const server = serve({ subscribed(socket) {
      if (socket.data.index === 0) socket.close()
      else socket.send(JSON.stringify(ADDED))
    } })
    const client = connect(server.port)
    await waitFor(() => client.events.length === 1)
    expect(client.disconnects()).toBe(1)
    expect(server.connections).toHaveLength(2)
    expect(server.connections[1]?.frames).toEqual([
      `Bearer ${CAPABILITY}`,
      JSON.stringify({ classes: ["gamepad"] }),
    ])
  })

  it("cancels pending reconnect when disposed", async () => {
    const server = serve({ subscribed(socket) { socket.close() } })
    const client = connect(server.port, { reconnect: { initialDelayMs: 100, maxDelayMs: 100 } })
    await waitFor(() => client.disconnects() === 1)
    client.dispose()
    client.dispose()
    await Bun.sleep(150)
    expect(server.connections).toHaveLength(1)
    expect(client.disconnects()).toBe(1)
  })

  it("does not deliver queued messages after disposal from an event listener", async () => {
    const server = serve({ subscribed(socket) {
      socket.send(JSON.stringify(ADDED))
      socket.send(JSON.stringify(INPUT))
    } })
    const events: NativeInputEvent[] = []
    const client = connect(server.port, { onEvent(event) {
      events.push(event)
      client.dispose()
    } })
    await waitFor(() => client.disconnects() === 1)
    await Bun.sleep(40)
    expect(events).toEqual([ADDED])
    expect(server.connections).toHaveLength(1)
  })

  it("does not reconnect after disposal from the disconnect listener", async () => {
    const server = serve({ subscribed(socket) { socket.close() } })
    let retired = 0
    const client = connect(server.port, { onDisconnect() {
      retired += 1
      client.dispose()
    } })
    await waitFor(() => retired === 1)
    await Bun.sleep(60)
    expect(server.connections).toHaveLength(1)
  })

  it.each([
    ["malformed JSON", "{"],
    ["binary data", new Uint8Array([1, 2, 3])],
    ["oversized data", JSON.stringify({ ...ADDED, device: { ...ADDED.device, name: "x".repeat(64 * 1024) } })],
    ["oversized UTF-8 data", JSON.stringify({ ...ADDED, device: { ...ADDED.device, name: "é".repeat(33_000) } })],
  ] as const)("retires input on %s without delivering it", async (_name, payload) => {
    const server = serve({ subscribed(socket) { socket.send(payload) } })
    const client = connect(server.port, { reconnect: { initialDelayMs: 100, maxDelayMs: 100 } })
    await waitFor(() => client.disconnects() === 1)
    client.dispose()
    expect(client.events).toEqual([])
  })

  it("backs off repeated authentication rejection instead of resetting on upgrade", async () => {
    const server = serve({ capability: "b".repeat(64) })
    const client = connect(server.port, { reconnect: { initialDelayMs: 50, maxDelayMs: 200 } })
    await waitFor(() => client.disconnects() === 2)
    const secondFailure = Date.now()
    await waitFor(() => client.disconnects() === 3)
    expect(Date.now() - secondFailure).toBeGreaterThanOrEqual(85)
    client.dispose()
    expect(client.events).toEqual([])
  })

  it("clears before ACK, ignores buffered old input across suspension, and authenticates a fresh baseline", async () => {
    const order: string[] = []
    let initialized = 0
    let ack: unknown
    const server = serve({
      subscribed(socket) {
        socket.send(JSON.stringify(ADDED))
        socket.send(JSON.stringify(INPUT)) // held baseline, not a press
        socket.send(JSON.stringify({ kind: "initialization-complete", generation: String(socket.data.index + 1) }))
      },
      acknowledged(socket, frame) {
        order.push("acknowledged")
        ack = JSON.parse(frame)
        // Deliberately deliver old payloads while the socket is lifecycle-only.
        // They stand for events already in TCP/Chromium's queue during freeze.
        socket.send(JSON.stringify(INPUT))
        socket.send(JSON.stringify(ADDED))
      },
    })
    const client = connect(server.port, {
      onInitialized: () => { initialized++ },
      onDisconnect: () => { order.push("cleared") },
    })
    await waitFor(() => initialized === 1)
    const old = server.sockets[0]!
    old.send(JSON.stringify({ kind: "suspend", generation: "1", requestId: "7" }))
    // This message is already queued when the suspend callback runs.
    old.send(JSON.stringify(INPUT))
    await waitFor(() => ack !== undefined)
    expect(ack).toEqual({ kind: "suspended", generation: "1", requestId: "7" })
    expect(order).toEqual(["cleared", "acknowledged"])
    await Bun.sleep(30)
    expect(client.events).toEqual([ADDED, INPUT])
    old.send(JSON.stringify({ kind: "resume", generation: "1", requestId: "7" }))
    old.send(JSON.stringify(INPUT)) // queued old callback must remain retired
    await waitFor(() => initialized === 2)
    expect(client.events).toEqual([ADDED, INPUT, ADDED, INPUT])
    expect(server.connections[1]?.frames).toEqual([`Bearer ${CAPABILITY}`, JSON.stringify({ classes: ["gamepad"] })])
    expect(order).toEqual(["cleared", "acknowledged"])
  })

  it("retires a mismatched lifecycle generation without acknowledging it", async () => {
    let initialized = false
    let acks = 0
    const server = serve({
      subscribed(socket) {
        socket.send(JSON.stringify({ kind: "initialization-complete", generation: "5" }))
      },
      acknowledged() { acks++ },
    })
    const client = connect(server.port, {
      onInitialized: () => { initialized = true },
      reconnect: { initialDelayMs: 100, maxDelayMs: 100 },
    })
    await waitFor(() => initialized)
    server.sockets[0]!.send(JSON.stringify({ kind: "suspend", generation: "4", requestId: "1" }))
    await waitFor(() => client.disconnects() === 1)
    client.dispose()
    expect(acks).toBe(0)
  })

  it("waits for generation before local release and closes only after matching retired reply", async () => {
    let retirement: unknown
    const server = serve({
      subscribed() {}, // baseline completion deliberately delayed across blur
      acknowledged(_socket, frame) { retirement = JSON.parse(frame) },
    })
    const client = connect(server.port)
    await waitFor(() => server.connections[0]?.frames.length === 2)
    client.dispose.setActive(false)
    expect(client.disconnects()).toBe(1)
    expect(retirement).toBeUndefined()
    const old = server.sockets[0]!
    old.send(JSON.stringify(ADDED))
    old.send(JSON.stringify(INPUT))
    old.send(JSON.stringify({ kind: "initialization-complete", generation: "1" }))
    await waitFor(() => retirement !== undefined)
    expect(retirement).toEqual({ kind: "retire", generation: "1" })
    expect(client.events).toEqual([])
    expect(server.closed).toEqual([])
    old.send(JSON.stringify({ kind: "retired", generation: "1" }))
    old.send(JSON.stringify(INPUT))
    await waitFor(() => server.closed.length === 1)
    expect(client.events).toEqual([])
    expect(client.disconnects()).toBe(1)
    client.dispose.setActive(true)
    await waitFor(() => server.connections[1]?.frames.length === 2)
    expect(server.connections[1]?.frames).toEqual([`Bearer ${CAPABILITY}`, JSON.stringify({ classes: ["gamepad"] })])
  })

  it("keeps pending local release alive when server suspend selection won", async () => {
    const commands: unknown[] = []
    let initialized = false
    const server = serve({
      subscribed(socket) { socket.send(JSON.stringify({ kind: "initialization-complete", generation: "1" })) },
      acknowledged(_socket, frame) { commands.push(JSON.parse(frame)) },
    })
    const client = connect(server.port, { onInitialized() { initialized = true } })
    await waitFor(() => initialized)
    client.dispose.setActive(false)
    await waitFor(() => commands.length === 1)
    expect(commands[0]).toEqual({ kind: "retire", generation: "1" })
    expect(server.closed).toEqual([])
    // Source selected this attachment before processing retire. It sends no
    // retired permission; client must still answer its exact suspension.
    server.sockets[0]!.send(JSON.stringify({ kind: "suspend", generation: "1", requestId: "3" }))
    await waitFor(() => commands.length === 2)
    expect(commands[1]).toEqual({ kind: "suspended", generation: "1", requestId: "3" })
    expect(server.closed).toEqual([])
    server.sockets[0]!.send(JSON.stringify({ kind: "resume", generation: "1", requestId: "3" }))
    await waitFor(() => server.closed.length === 1)
  })

  it("reports invalid configuration without including the credential", () => {
    const secret = "not-a-valid-capability"
    expect(() => connectNativeInput({ korridPort: 1234, korridCapability: secret }, {
      onEvent() {}, onDisconnect() {},
    })).toThrow("Invalid native input connection.")
    for (const initialDelayMs of [0, -1, NaN, Infinity]) {
      expect(() => connectNativeInput({ korridPort: 1234, korridCapability: CAPABILITY }, {
        onEvent() {}, onDisconnect() {}, reconnect: { initialDelayMs },
      })).toThrow("Invalid native input reconnect policy.")
    }
  })
})
