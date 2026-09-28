import { afterEach, describe, expect, it } from "bun:test"
import type { ServerWebSocket } from "bun"
import { createNativeInputAdapter } from "./native-adapter"
import type { InputAction } from "./types"

const CAPABILITY = "a".repeat(64)
const PAD = { kind: "device-added", device: { deviceId: "pad", class: "gamepad", name: "Pad", capabilities: [] } }
const input = (code: number, value: number) => ({ kind: "input", deviceId: "pad", class: "gamepad", type: 1, code, value, timestamp: 0 })
const disposers: Array<() => void> = []
afterEach(() => { for (const dispose of disposers.splice(0)) dispose() })

async function waitFor(predicate: () => boolean) {
  const deadline = Date.now() + 2000
  while (!predicate()) {
    if (Date.now() >= deadline) throw new Error("Native adapter condition timed out.")
    await Bun.sleep(5)
  }
}
function session(held = false) {
  type Data = { frames: number; generation: string }
  const sockets: Array<ServerWebSocket<Data>> = []
  const acknowledgements: unknown[] = []
  const releases: string[] = []
  const live = new Set<ServerWebSocket<Data>>()
  let baselineHeld = held
  let suspended = false
  let refusedAttachments = 0
  let generation = 0
  const server = Bun.serve<Data>({
    hostname: "127.0.0.1", port: 0,
    fetch(request, server) {
      if (suspended) { refusedAttachments++; return new Response(null, { status: 503 }) }
      if (live.size >= 4) return new Response(null, { status: 429 })
      if (server.upgrade(request, { data: { frames: 0, generation: String(++generation) } })) return
      return new Response(null, { status: 400 })
    },
    websocket: {
      open(socket) { sockets.push(socket); live.add(socket) },
      close(socket) { live.delete(socket) },
      message(socket, message) {
        socket.data.frames++
        const frame = String(message)
        if (socket.data.frames === 1) {
          if (frame !== `Bearer ${CAPABILITY}`) socket.close(1008)
        } else if (socket.data.frames === 2) {
          if (frame !== JSON.stringify({ classes: ["gamepad"] })) { socket.close(1008); return }
          socket.send(JSON.stringify(PAD))
          socket.send(JSON.stringify(input(0x130, Number(baselineHeld))))
          socket.send(JSON.stringify({ kind: "initialization-complete", generation: socket.data.generation }))
        } else {
          const control = JSON.parse(frame)
          if (control.kind === "retire") {
            if (control.generation !== socket.data.generation) { socket.close(1008); return }
            if (suspended) return // selection won: still needs exact suspended ACK
            releases.push(control.generation)
            socket.send(JSON.stringify({ kind: "retired", generation: control.generation }))
            socket.close()
          } else acknowledgements.push(control)
        }
      },
    },
  })
  const actions: InputAction[] = []
  const page = new EventTarget()
  const visibility = new EventTarget()
  let active = true
  let now = 0
  let nextFrame = 0
  const frames = new Map<number, FrameRequestCallback>()
  const adapter = createNativeInputAdapter({ korridPort: server.port!, korridCapability: CAPABILITY }, {
    page, visibility, isActive: () => active, now: () => now,
    requestFrame: callback => { frames.set(++nextFrame, callback); return nextFrame },
    cancelFrame: id => { frames.delete(id) },
    reconnect: { initialDelayMs: 10, maxDelayMs: 20 },
  })
  const stop = adapter.start(action => actions.push(action))
  disposers.push(() => { stop(); server.stop(true) })
  return {
    actions, acknowledgements, releases, sockets, stop,
    get liveAttachments() { return live.size },
    async ready(count = 1) { await waitFor(() => sockets.length >= count && sockets[count - 1]!.data.frames >= 2); await Bun.sleep(20) },
    send(message: unknown, index = sockets.length - 1, report = true) {
      sockets[index]!.send(JSON.stringify(message))
      if (report && typeof message === "object" && message !== null && "kind" in message && message.kind === "input" && "deviceId" in message) {
        sockets[index]!.send(JSON.stringify({ kind: "input", class: "gamepad", deviceId: message.deviceId, type: 0, code: 0, value: 0, timestamp: 0 }))
      }
    },
    suspend(value: boolean) { suspended = value },
    get refusedAttachments() { return refusedAttachments },
    baseline(held: boolean) { baselineHeld = held },
    tick(time: number) { now = time; const callbacks = [...frames.values()]; frames.clear(); for (const callback of callbacks) callback(time) },
    blur() { active = false; page.dispatchEvent(new Event("blur")) },
    hide() { active = false; visibility.dispatchEvent(new Event("visibilitychange")) },
    focus() { active = true; page.dispatchEvent(new Event("focus")) },
  }
}

describe("native adapter with actual local WebSockets", () => {
  it("attaches a silent baseline then delivers live semantic button actions", async () => {
    const s = session(true); await s.ready()
    expect(s.actions).toEqual([])
    s.send(input(0x131, 1)); s.send(input(0x131, 0))
    await Bun.sleep(20)
    expect(s.actions).toEqual([])
    s.send(input(0x130, 0)); s.send(input(0x130, 1))
    await waitFor(() => s.actions.length === 1)
    expect(s.actions).toEqual([{ type: "confirm", source: "native" }])
  })

  it("keeps live held repeats, releases on loss and does not replay a reconnect baseline", async () => {
    const s = session(); await s.ready()
    s.send(input(0x223, 1))
    await waitFor(() => s.actions.length === 1)
    s.tick(399); expect(s.actions).toHaveLength(1)
    s.tick(400); expect(s.actions.at(-1)).toMatchObject({ type: "direction", repeat: true })
    s.baseline(true)
    s.sockets[0]!.close()
    await waitFor(() => s.actions.some(action => action.type === "direction-end"))
    await s.ready(2)
    s.tick(1000)
    expect(s.actions.map(action => action.type)).toEqual(["direction", "direction", "direction-end"])
  })

  for (const loss of ["blur", "hide"] as const) {
    it(`clears on ${loss}, ignores inactive input and requires a fresh held baseline to reach neutral`, async () => {
      const s = session(); await s.ready()
      s.send(input(0x223, 1)); await waitFor(() => s.actions.length === 1)
      // Deliver a callback after blur rather than relying on TCP close to remove it.
      s.send(input(0x130, 1)); s[loss]()
      await Bun.sleep(20); s.tick(1000)
      expect(s.actions.map(action => action.type)).toEqual(["direction", "direction-end"])
      s.baseline(true); s.focus(); await s.ready(2)
      s.send(input(0x131, 1)); s.send(input(0x131, 0)); await Bun.sleep(20)
      expect(s.actions).toHaveLength(2)
      s.send(input(0x130, 0)); s.send(input(0x130, 1))
      await waitFor(() => s.actions.length === 3)
      expect(s.actions.at(-1)).toEqual({ type: "confirm", source: "native" })
    })
  }

  it("releases ordinary blur attachments over eight focus cycles and still ACKs later host suspend", async () => {
    const s = session(); await s.ready()
    for (let cycle = 0; cycle < 8; cycle++) {
      s.blur()
      await waitFor(() => s.releases.length === cycle + 1)
      await waitFor(() => s.liveAttachments === 0)
      s.focus()
      await s.ready(cycle + 2)
      s.send(input(0x130, 1)); s.send(input(0x130, 0))
      await waitFor(() => s.actions.length === cycle + 1)
      expect(s.actions.at(-1)).toEqual({ type: "confirm", source: "native" })
    }
    s.suspend(true)
    s.blur() // local retire now loses to host selection, so must not close
    s.send({ kind: "suspend", generation: "9", requestId: "10" })
    await waitFor(() => s.acknowledgements.length === 1)
    expect(s.acknowledgements[0]).toEqual({ kind: "suspended", generation: "9", requestId: "10" })
    expect(s.releases).toHaveLength(8)
    expect(s.liveAttachments).toBe(1)
  })

  it("ACKs the selected attachment when blur runs before suspend delivery, retaining control across refused focus retries", async () => {
    const s = session(); await s.ready()
    s.send(input(0x223, 1)); await waitFor(() => s.actions.length === 1)
    // Server has selected socket 0 and disallows new attachment. Blur runs
    // before its queued suspend callback: deterministic host-handoff ordering.
    s.suspend(true)
    s.blur()
    expect(s.actions.map(action => action.type)).toEqual(["direction", "direction-end"])
    s.send(input(0x130, 1), 0) // inactive callback must not destroy ACK channel
    s.send({ kind: "suspend", generation: "1", requestId: "9" }, 0)
    await waitFor(() => s.acknowledgements.length === 1)
    expect(s.acknowledgements[0]).toEqual({ kind: "suspended", generation: "1", requestId: "9" })
    s.focus()
    await waitFor(() => s.refusedAttachments > 0)
    expect(s.sockets).toHaveLength(1)
    expect(s.actions).toHaveLength(2)
    s.baseline(true)
    s.suspend(false)
    s.send({ kind: "resume", generation: "1", requestId: "9" }, 0)
    s.send(input(0x131, 1), 0) // retired data never becomes active again
    await s.ready(2)
    expect(s.actions).toHaveLength(2)
    s.send(input(0x130, 0)); s.send(input(0x130, 1))
    await waitFor(() => s.actions.length === 3)
    expect(s.actions.at(-1)).toEqual({ type: "confirm", source: "native" })
  })

  it("commits split measured samples only at SYN_REPORT, never transient neutral", async () => {
    const s = session(); await s.ready()
    // A new live source with held Start, silently initialized.
    s.send({ ...PAD, device: { ...PAD.device, deviceId: "second" } })
    s.send({ ...input(0x13b, 1), deviceId: "second" }, 0, false)
    s.send({ kind: "device-state-complete", deviceId: "second" })
    s.send({ ...input(0x13b, 0), deviceId: "second" }, 0, false)
    await Bun.sleep(20)
    s.tick(1000)
    s.send({ ...input(0x130, 1), deviceId: "second" }) // closes same measured sample
    await Bun.sleep(20)
    expect(s.actions).toEqual([])
    s.send({ ...input(0x130, 0), deviceId: "second" })
    s.send({ ...input(0x130, 1), deviceId: "second" })
    s.send({ ...input(0x130, 0), deviceId: "second" })
    await waitFor(() => s.actions.length === 1)
    expect(s.actions).toEqual([{ type: "confirm", source: "native" }])
  })

  it("retires before freeze ACK and ignores deliberately buffered input through thaw", async () => {
    const s = session(); await s.ready()
    s.send(input(0x223, 1)); await waitFor(() => s.actions.length === 1)
    s.send({ kind: "suspend", generation: "1", requestId: "8" })
    s.send(input(0x130, 1)) // already buffered behind suspend
    await waitFor(() => s.acknowledgements.length === 1)
    expect(s.actions.map(action => action.type)).toEqual(["direction", "direction-end"])
    expect(s.acknowledgements).toEqual([{ kind: "suspended", generation: "1", requestId: "8" }])
    s.send(input(0x130, 1)) // lifecycle-only callback while nominally frozen
    s.tick(10_000)
    s.baseline(true)
    s.send({ kind: "resume", generation: "1", requestId: "8" })
    s.send(input(0x131, 1)) // old callback survives until after resume
    await s.ready(2)
    s.tick(20_000)
    expect(s.actions).toHaveLength(2)
    s.send(input(0x130, 0)); s.send(input(0x130, 1))
    await waitFor(() => s.actions.length === 3)
    expect(s.actions.at(-1)).toEqual({ type: "confirm", source: "native" })
  })

  it("live arrival completion is silent and source removal ends only that source gesture", async () => {
    const s = session(); await s.ready()
    s.send(input(0x223, 1)); await waitFor(() => s.actions.length === 1)
    s.send({ ...PAD, device: { ...PAD.device, deviceId: "second" } })
    s.send({ ...input(0x130, 1), deviceId: "second" })
    s.send({ kind: "device-state-complete", deviceId: "second" })
    await Bun.sleep(20)
    expect(s.actions).toHaveLength(1)
    s.send({ kind: "device-removed", deviceId: "pad" })
    await waitFor(() => s.actions.length === 2)
    expect(s.actions.at(-1)).toMatchObject({ type: "direction-end", gestureId: 1 })
    s.send({ ...input(0x130, 0), deviceId: "second" }); s.send({ ...input(0x130, 1), deviceId: "second" })
    await waitFor(() => s.actions.length === 3)
  })
})
