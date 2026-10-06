import { expect, test } from "bun:test"
import { PendingLaunchPhase, type RpcRequest } from "@contracts/generated/korrid"
import { createHttpKorridClient, createInMemoryKorridClient } from "./client"
import { runnerRoutes } from "../surface/fixtures/runner-routes"

for (const selected of [false, true]) {
  test(`owned ${selected ? "selected" : "command"} startup uses the exact reserve/start/cancel HTTP wire`, async () => {
    const implementation = createInMemoryKorridClient({
      games: [{ id: "wl4", title: "Wario Land 4", supportsRunnerSelection: selected, source: { label: "This device", isLocal: true } }],
      gameRoutes: [runnerRoutes], sessionFocusDelayMs: 60_000,
    })
    const requests: RpcRequest[] = []
    const server = Bun.serve({ hostname: "127.0.0.1", port: 0, async fetch(request) {
      expect(request.headers.get("authorization")).toBe("Bearer private-capability")
      const rpc = await request.json() as RpcRequest
      requests.push(rpc)
      switch (rpc._tag) {
        case "app.session.reserve": return Response.json({ _tag: rpc._tag, outcome: await implementation.sessionReserve(rpc.payload) })
        case "app.session.start": return Response.json({ _tag: rpc._tag, outcome: await implementation.sessionStart(rpc.payload) })
        case "app.session.cancel": return Response.json({ _tag: rpc._tag, outcome: await implementation.sessionCancel(rpc.payload) })
        case "app.session.status": return Response.json({ _tag: rpc._tag, outcome: await implementation.sessionStatus() })
        default: return new Response("Unexpected request", { status: 400 })
      }
    } })
    try {
      const client = createHttpKorridClient(server.url.origin, "private-capability")
      const reserved = await client.sessionReserve({ gameId: "wl4" })
      if (reserved._tag !== "Ok") throw new Error("reserve failed")
      const expectedLaunchId = reserved.payload.launchId
      const start = { gameId: "wl4", expectedLaunchId, ...(selected ? { runnerId: "retroarch/mgba" } : {}) }
      expect(await client.sessionStart(start)).toMatchObject({ _tag: "Ok", payload: { session: reserved.payload, warnings: [] } })
      const status = await client.sessionStatus()
      expect(status).toMatchObject({ _tag: "Ok", payload: { active: { launchId: expectedLaunchId, phase: "running" } } })
      expect(status._tag === "Ok" && status.payload.active?.focusOwnership).toBeUndefined()
      expect(await client.sessionCancel({ expectedLaunchId: "not-this-launch" })).toMatchObject({ _tag: "Err", payload: { code: "StaleLaunchIdentity" } })
      expect(await client.sessionCancel({ expectedLaunchId })).toMatchObject({ _tag: "Ok" })
      expect(await client.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
      expect(requests.slice(0, 2)).toEqual([
        { _tag: "app.session.reserve", payload: { gameId: "wl4" } },
        { _tag: "app.session.start", payload: start },
      ])
      expect(requests.filter(rpc => rpc._tag === "app.session.cancel")).toEqual([
        { _tag: "app.session.cancel", payload: { expectedLaunchId: "not-this-launch" } },
        { _tag: "app.session.cancel", payload: { expectedLaunchId } },
      ])
    } finally { await server.stop(true) }
  })
}

test("lifecycle methods report real HTTP permission rejection without another operation", async () => {
  const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response(null, { status: 403 }) })
  try {
    const client = createHttpKorridClient(server.url.origin, "read-only")
    expect(await client.sessionReserve({ gameId: "wl4" })).toMatchObject({ _tag: "Err", payload: { code: "PermissionDenied" } })
    expect(await client.sessionStart({ gameId: "wl4", expectedLaunchId: "exact" })).toMatchObject({ _tag: "Err", payload: { code: "PermissionDenied" } })
    expect(await client.sessionCancel({ expectedLaunchId: "exact" })).toMatchObject({ _tag: "Err", payload: { code: "PermissionDenied" } })
  } finally { await server.stop(true) }
})

test("manual recovery HTTP Cancel targets one of two owned reservations and preserves the other", async () => {
  const implementation = createInMemoryKorridClient({ games: [{ id: "wl4", title: "Wario Land 4", supportsRunnerSelection: false, source: { label: "This device", isLocal: true } }] })
  const requests: RpcRequest[] = []
  const server = Bun.serve({ hostname: "127.0.0.1", port: 0, async fetch(request) {
    expect(request.headers.get("authorization")).toBe("Bearer private-capability")
    const rpc = await request.json() as RpcRequest
    requests.push(rpc)
    switch (rpc._tag) {
      case "app.session.reserve": return Response.json({ _tag: rpc._tag, outcome: await implementation.sessionReserve(rpc.payload) })
      case "app.session.cancel": return Response.json({ _tag: rpc._tag, outcome: await implementation.sessionCancel(rpc.payload) })
      case "app.session.status": return Response.json({ _tag: rpc._tag, outcome: await implementation.sessionStatus() })
      default: return new Response("Unexpected request", { status: 400 })
    }
  } })
  try {
    const client = createHttpKorridClient(server.url.origin, "private-capability")
    const first = await client.sessionReserve({ gameId: "wl4" })
    const second = await client.sessionReserve({ gameId: "wl4" })
    if (first._tag !== "Ok" || second._tag !== "Ok") throw new Error("reserve failed")
    expect(await client.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { pendingLaunches: [
      { session: first.payload, phase: PendingLaunchPhase.Reserved }, { session: second.payload, phase: PendingLaunchPhase.Reserved },
    ] } })
    expect(await client.sessionCancel({ expectedLaunchId: first.payload.launchId })).toMatchObject({ _tag: "Ok", payload: { phase: "stopped" } })
    expect(await client.sessionStatus()).toEqual({ _tag: "Ok", payload: { pendingLaunches: [{ session: second.payload, phase: PendingLaunchPhase.Reserved }] } })
    expect(requests.filter(rpc => rpc._tag === "app.session.cancel")).toEqual([
      { _tag: "app.session.cancel", payload: { expectedLaunchId: first.payload.launchId } },
    ])
    expect(requests.some(rpc => rpc._tag === "app.session.start")).toBe(false)
  } finally { await server.stop(true) }
})
