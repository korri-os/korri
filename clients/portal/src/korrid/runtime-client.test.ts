import { describe, expect, it } from "bun:test"
import type { RpcRequest } from "@contracts/generated/korrid"
import { createHttpKorridClient, createInMemoryKorridClient } from "./client"
import { runtimeRoutes } from "../surface/fixtures/runtime-routes"

describe("runtime RPC over localhost", () => {
  it("uses exact tagged requests and revision checked clears without launch-time writes", async () => {
    const requests: RpcRequest[] = []
    const implementation = createInMemoryKorridClient({ gameRoutes: [runtimeRoutes] })
    const server = Bun.serve({
      hostname: "127.0.0.1",
      port: 0,
      async fetch(request) {
        expect(new URL(request.url).pathname).toBe("/rpc")
        expect(request.headers.get("authorization")).toBe("Bearer test-capability")
        const rpc = (await request.json()) as RpcRequest
        requests.push(rpc)
        switch (rpc._tag) {
          case "app.local-games.routes":
            return Response.json({
              _tag: rpc._tag,
              outcome: await implementation.gameRoutes(rpc.payload.gameId),
            })
          case "app.local-games.runtime.set":
            return Response.json({
              _tag: rpc._tag,
              outcome: await implementation.setGameRuntime(rpc.payload),
            })
          case "app.local-games.launch.selected":
            return Response.json({
              _tag: rpc._tag,
              outcome: await implementation.launchSelectedGame(
                rpc.payload.gameId,
                rpc.payload.runtimeId,
              ),
            })
          default:
            return new Response("Unexpected RPC", { status: 400 })
        }
      },
    })
    try {
      const client = createHttpKorridClient(server.url.origin, "test-capability")
      expect((await client.gameRoutes("wl4"))._tag).toBe("Ok")
      expect(
        (
          await client.setGameRuntime({
            scope: { _tag: "Game", id: "wl4" },
            expectedRevision: "g1",
          })
        )._tag,
      ).toBe("Ok")
      expect((await client.launchSelectedGame("wl4", "retroarch/mgba"))._tag).toBe("Ok")
      expect(requests).toEqual([
        { _tag: "app.local-games.routes", payload: { gameId: "wl4" } },
        {
          _tag: "app.local-games.runtime.set",
          payload: { scope: { _tag: "Game", id: "wl4" }, expectedRevision: "g1" },
        },
        {
          _tag: "app.local-games.launch.selected",
          payload: { gameId: "wl4", runtimeId: "retroarch/mgba" },
        },
      ])
    } finally {
      await server.stop(true)
    }
  })

  it("reports the real HTTP permission rejection as a permission problem", async () => {
    const server = Bun.serve({
      hostname: "127.0.0.1",
      port: 0,
      fetch: () => new Response(null, { status: 403 }),
    })
    try {
      const client = createHttpKorridClient(server.url.origin, "read-only-capability")
      const result = await client.launchSelectedGame("wl4", "retroarch/mgba")
      expect(result._tag === "Err" && result.payload.code).toBe("PermissionDenied")
    } finally {
      await server.stop(true)
    }
  })
})
