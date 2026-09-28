import { describe, expect, it } from "bun:test"
import type { SettingsSnapshot } from "@contracts/generated/korrid"
import { SecretSettingStatus } from "@contracts/generated/korrid"
import { createHttpKorridClient, createInMemoryKorridClient, type KorridClient } from "./client"

async function snapshot(client: KorridClient): Promise<SettingsSnapshot> {
  const result = await client.settingsSnapshot()
  if (result._tag !== "Ok") throw new Error("Settings snapshot failed.")
  return result.payload
}

async function seeded(editableSettingIds: string[]) {
  const settings = await snapshot(createInMemoryKorridClient())
  return {
    ...settings,
    steamGridDbCredential: SecretSettingStatus.Configured,
    editableSettingIds,
  }
}

describe("settings editability facts", () => {
  it("preserves working preview editors and includes metadata after a write", async () => {
    const client = createInMemoryKorridClient()
    const before = await snapshot(client)
    expect(new Set(before.editableSettingIds)).toEqual(new Set([
      "device-name", "steamgriddb-credential", "host.preferences.playerCount",
      ...before.plugins.map(plugin => plugin.id),
    ]))
    const changed = await client.updateSetting(before.revision, "device-name", "New name")
    expect(changed).toMatchObject({
      _tag: "Ok", payload: { deviceName: "New name", editableSettingIds: before.editableSettingIds },
    })
    const current = await snapshot(client)
    expect(await client.updateSetting(current.revision, "@korri:mgba", "false")).toMatchObject({ _tag: "Ok" })
    expect((await snapshot(client)).plugins.find(plugin => plugin.id === "@korri:mgba")?.enabled).toBe(false)
    expect(await client.setSteamGridDbCredential("example-value")).toMatchObject({
      _tag: "Ok", payload: { status: SecretSettingStatus.Configured },
    })
  })

  it("keeps read-only information without granting setting or credential writes", async () => {
    const client = createInMemoryKorridClient({ settings: await seeded([]) })
    const before = await snapshot(client)
    expect(before.plugins.length).toBeGreaterThan(0)
    expect(before.steamGridDbCredential).toBe(SecretSettingStatus.Configured)
    for (const [id, value] of [["device-name", "changed"], ["host.preferences.playerCount", "6"], ["@korri:mgba", "false"]] as const) {
      expect(await client.updateSetting(before.revision, id, value)).toMatchObject({
        _tag: "Err", payload: { code: "PermissionDenied" },
      })
    }
    expect(await client.setSteamGridDbCredential("example-value")).toMatchObject({
      _tag: "Err", payload: { code: "PermissionDenied" },
    })
    expect(await client.clearSteamGridDbCredential()).toMatchObject({
      _tag: "Err", payload: { code: "PermissionDenied" },
    })
    expect(await snapshot(client)).toEqual(before)
  })

  it("supports count-only callers without broadening other writes", async () => {
    const client = createInMemoryKorridClient({ settings: await seeded(["host.preferences.playerCount"]) })
    const before = await snapshot(client)
    expect(await client.updateSetting(before.revision, "host.preferences.playerCount", "6")).toMatchObject({
      _tag: "Ok", payload: { playerCount: 6, editableSettingIds: ["host.preferences.playerCount"] },
    })
    const current = await snapshot(client)
    expect(await client.updateSetting(current.revision, "device-name", "changed")).toMatchObject({
      _tag: "Err", payload: { code: "PermissionDenied" },
    })
    expect(await snapshot(client)).toEqual(current)
  })

  it("cannot grant writes by changing returned metadata or the seed object", async () => {
    const seed = await seeded([])
    const client = createInMemoryKorridClient({ settings: seed })
    const before = await snapshot(client)
    seed.editableSettingIds.push("device-name")
    before.editableSettingIds.push("device-name")
    expect(await client.updateSetting(before.revision, "device-name", "changed")).toMatchObject({
      _tag: "Err", payload: { code: "PermissionDenied" },
    })
    expect((await snapshot(client)).editableSettingIds).toEqual([])
  })

  it("does not advertise nonexistent writers or accept a credential through the generic setter", async () => {
    const readonly = createInMemoryKorridClient({ settings: await seeded(["unknown-setting"]) })
    expect((await snapshot(readonly)).editableSettingIds).toEqual([])
    const client = createInMemoryKorridClient()
    const before = await snapshot(client)
    expect(await client.updateSetting(before.revision, "steamgriddb-credential", "example-value")).toMatchObject({
      _tag: "Err", payload: { code: "OperationUnsupported" },
    })
    expect(await snapshot(client)).toEqual(before)
  })

  it("reports a real HTTP permission refusal without claiming the brain is unreachable", async () => {
    const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response(null, { status: 403 }) })
    try {
      const client = createHttpKorridClient(server.url.origin, "test-capability")
      for (const result of [
        await client.updateSetting("revision", "device-name", "changed"),
        await client.setSteamGridDbCredential("example-value"),
        await client.clearSteamGridDbCredential(),
      ]) {
        expect(result).toMatchObject({ _tag: "Err", payload: { code: "PermissionDenied" } })
      }
    } finally {
      await server.stop(true)
    }
  })
})
