/** The in-memory brain answers as korrid does (services/korrid/src/launch_settings). */
import { describe, expect, it } from "bun:test"
import { LaunchSetting, type LaunchSettingsScope } from "@contracts/generated/korrid"
import { createInMemoryLaunchSettings } from "./in-memory-launch-settings"

const games = { wario: { systemId: "gba", runner: { id: "retroarch/mgba", title: "mGBA" } } }
const game: LaunchSettingsScope = { _tag: "Game", id: "wario" }

async function resolved(brain: ReturnType<typeof createInMemoryLaunchSettings>) {
  const snapshot = await brain.launchSettingsSnapshot("wario")
  if (snapshot._tag === "Err") throw new Error(snapshot.payload.message)
  return snapshot.payload.resolved
}

describe("in-memory launch settings, korrid's rules", () => {
  it("across layers the more specific minimum wins and the maximum moves to meet it", async () => {
    const brain = createInMemoryLaunchSettings({ games, layers: { device: { cpu: { max: 40 }, gpu: {} }, "game:wario": { cpu: { min: 80 }, gpu: {} } } })
    expect((await resolved(brain)).cpu).toEqual({
      min: { value: 80, source: game, deviceDefault: false, adjusted: false },
      max: { value: 80, source: { _tag: "Device" }, deviceDefault: false, adjusted: true },
    })
  })

  it("across layers the more specific maximum wins and the minimum moves to meet it", async () => {
    const brain = createInMemoryLaunchSettings({ games, layers: { device: { cpu: {}, gpu: { min: 70 } }, "game:wario": { cpu: {}, gpu: { max: 30 } } } })
    expect((await resolved(brain)).gpu).toEqual({
      min: { value: 30, source: { _tag: "Device" }, deviceDefault: false, adjusted: true },
      max: { value: 30, source: game, deviceDefault: false, adjusted: false },
    })
  })

  it("refuses a minimum above the same layer's maximum, naming the layer and both values", async () => {
    const brain = createInMemoryLaunchSettings({ games, layers: { "game:wario": { cpu: { max: 10 }, gpu: {} } } })
    const snapshot = await brain.launchSettingsSnapshot("wario")
    if (snapshot._tag === "Err") throw new Error("read")
    const result = await brain.setLaunchSetting({ scope: game, setting: LaunchSetting.CpuMin, value: 80, expectedRevision: snapshot.payload.revisions.games })
    expect(result._tag).toBe("Err")
    if (result._tag === "Err") {
      expect(result.payload.code).toBe("InvalidSetting")
      expect(result.payload.message).toContain("Game")
      expect(result.payload.message).toContain("80")
      expect(result.payload.message).toContain("10")
    }
    expect((await resolved(brain)).cpu.min.value).toBe(0)
  })

  it("takes whole percents only: minimum 0 to 100, maximum 1 to 100", async () => {
    const brain = createInMemoryLaunchSettings({ games })
    const attempt = async (setting: LaunchSetting, value: number) => {
      const snapshot = await brain.launchSettingsSnapshot("wario")
      if (snapshot._tag === "Err") throw new Error("read")
      return (await brain.setLaunchSetting({ scope: game, setting, value, expectedRevision: snapshot.payload.revisions.games }))._tag
    }
    expect(await attempt(LaunchSetting.CpuMin, 45.5)).toBe("Err")
    expect(await attempt(LaunchSetting.CpuMax, 0)).toBe("Err")
    expect(await attempt(LaunchSetting.CpuMax, 101)).toBe("Err")
    expect(await attempt(LaunchSetting.GpuMin, -1)).toBe("Err")
    expect(await attempt(LaunchSetting.CpuMin, 0)).toBe("Ok")
    expect(await attempt(LaunchSetting.CpuMax, 1)).toBe("Ok")
    expect(await attempt(LaunchSetting.GpuMin, 100)).toBe("Ok")
  })

  it("refuses a profile minimum above its own maximum too", async () => {
    const brain = createInMemoryLaunchSettings({ games, profiles: { wario: [{ id: "p1", title: "Quiet", cpu: { min: 60 }, gpu: {} }] } })
    const snapshot = await brain.launchSettingsSnapshot("wario")
    if (snapshot._tag === "Err") throw new Error("read")
    const result = await brain.setLaunchSetting({
      scope: { _tag: "Profile", id: { gameId: "wario", profileId: "p1" } },
      setting: LaunchSetting.CpuMax, value: 20, expectedRevision: snapshot.payload.revisions.games,
    })
    expect(result._tag === "Err" && result.payload.message).toContain("Quiet")
  })

  it("marks a shipped default apart from an authored Device value", async () => {
    const brain = createInMemoryLaunchSettings({ games, layers: { device: { cpu: { max: 61 }, gpu: {} } } })
    const cpu = (await resolved(brain)).cpu
    expect(cpu.min).toEqual({ value: 0, source: { _tag: "Device" }, deviceDefault: true, adjusted: false })
    expect(cpu.max).toEqual({ value: 61, source: { _tag: "Device" }, deviceDefault: false, adjusted: false })
    const snapshot = await brain.launchSettingsSnapshot("wario")
    expect(snapshot._tag === "Ok" && snapshot.payload.layers[0]).toMatchObject({ cpu: { max: 61 }, gpu: {} })
  })
})
