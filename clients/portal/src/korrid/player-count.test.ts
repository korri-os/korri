import { describe, expect, it } from "bun:test"
import type { RpcRequest, SettingsSnapshot } from "@contracts/generated/korrid"
import { createHttpKorridClient, createInMemoryKorridClient, type KorridClient } from "./client"

const SETTING = "host.preferences.playerCount"

async function snapshot(client: KorridClient): Promise<SettingsSnapshot> {
  const outcome = await client.settingsSnapshot()
  if (outcome._tag !== "Ok") throw new Error("Settings snapshot failed.")
  return outcome.payload
}

describe("device player count", () => {
  it("defaults to four and changes only the approved setting under the current revision", async () => {
    const client = createInMemoryKorridClient()
    const before = await snapshot(client)
    expect(before.playerCount).toBe(4)
    for (const playerCount of [1, 6, 255]) {
      const current = await snapshot(client)
      const result = await client.updateSetting(current.revision, SETTING, String(playerCount))
      expect(result._tag).toBe("Ok")
      const next = await snapshot(client)
      expect(next.playerCount).toBe(playerCount)
      expect(next.revision).not.toBe(current.revision)
      expect(next.deviceName).toBe(before.deviceName)
      expect(next.plugins).toEqual(before.plugins)
      expect(next.steamGridDbCredential).toBe(before.steamGridDbCredential)
    }
    const current = await snapshot(client)
    expect(await client.updateSetting(before.revision, SETTING, "2")).toMatchObject({
      _tag: "Err", payload: { code: "SettingsConflict" },
    })
    expect(await snapshot(client)).toEqual(current)
  })

  it.each(["0", "-1", "256", "3.5", "1e1", "0x10", "NaN", "", " 6 ", "6\n", "true"])(
    "rejects invalid count %j without changing the revision or settings",
    async value => {
      const client = createInMemoryKorridClient()
      const before = await snapshot(client)
      expect(await client.updateSetting(before.revision, SETTING, value)).toMatchObject({
        _tag: "Err", payload: { code: "SettingsInvalid" },
      })
      expect(await snapshot(client)).toEqual(before)
    },
  )

  it.each(["running", "frozen", "focus-failed", "stopping"])(
    "refuses a count change while a session is %s",
    async phase => {
      const client = createInMemoryKorridClient({
        activeSession: { launchId: "current", gameId: "neverball", phase },
      })
      const before = await snapshot(client)
      expect(await client.updateSetting(before.revision, SETTING, "6")).toMatchObject({
        _tag: "Err", payload: { code: "ActiveSessionConflict" },
      })
      expect(await snapshot(client)).toEqual(before)
    },
  )

  it("allows the change after the exact paused session ends", async () => {
    const client = createInMemoryKorridClient({
      activeSession: { launchId: "current", gameId: "neverball", phase: "frozen" },
    })
    const before = await snapshot(client)
    expect((await client.sessionStop("current"))._tag).toBe("Ok")
    expect(await client.updateSetting(before.revision, SETTING, "6")).toMatchObject({
      _tag: "Ok", payload: { playerCount: 6 },
    })
  })

  it("refuses a change when inactivity cannot be established", async () => {
    const client = createInMemoryKorridClient({ behavior: "status-fail" })
    const before = await snapshot(client)
    expect(await client.updateSetting(before.revision, SETTING, "6")).toMatchObject({
      _tag: "Err", payload: { code: "HostRecoveryBlocked" },
    })
    expect(await snapshot(client)).toEqual(before)
  })

  it("uses the existing authenticated settings RPC without a new request shape", async () => {
    const implementation = createInMemoryKorridClient()
    const requests: RpcRequest[] = []
    const server = Bun.serve({
      hostname: "127.0.0.1",
      port: 0,
      async fetch(request) {
        expect(new URL(request.url).pathname).toBe("/rpc")
        expect(request.headers.get("authorization")).toBe("Bearer test-capability")
        const rpc: RpcRequest = await request.json()
        requests.push(rpc)
        switch (rpc._tag) {
          case "system.settings.snapshot":
            return Response.json({ _tag: rpc._tag, outcome: await implementation.settingsSnapshot() })
          case "system.settings.update":
            return Response.json({
              _tag: rpc._tag,
              outcome: await implementation.updateSetting(
                rpc.payload.expectedRevision, rpc.payload.settingId, rpc.payload.value,
              ),
            })
          default:
            return new Response(null, { status: 400 })
        }
      },
    })
    try {
      const client = createHttpKorridClient(server.url.origin, "test-capability")
      const before = await snapshot(client)
      expect(await client.updateSetting(before.revision, SETTING, "6")).toMatchObject({
        _tag: "Ok", payload: { playerCount: 6 },
      })
      expect(requests).toEqual([
        { _tag: "system.settings.snapshot", payload: {} },
        {
          _tag: "system.settings.update",
          payload: { expectedRevision: before.revision, settingId: SETTING, value: "6" },
        },
      ])
    } finally {
      await server.stop(true)
    }
  })
})
