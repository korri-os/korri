/** The in-memory brain answers as korrid does (ADR 0028, services/korrid/src/launch_settings). */
import { describe, expect, it } from "bun:test"
import type { LaunchPlace } from "@contracts/generated/korrid"
import { createInMemoryLaunchSettings, type InMemoryLaunchSettingsConfig } from "./in-memory-launch-settings"

const runners: InMemoryLaunchSettingsConfig["runners"] = {
  "retroarch/mgba": { title: "mGBA", family: "retroarch", schema: { properties: { frameskip: { type: "integer", minimum: 0, maximum: 10, default: 0 } } } },
  "retroarch/gpsp": { title: "gpSP", family: "retroarch", schema: { properties: { bios: { type: "boolean" } } } },
}
const games: InMemoryLaunchSettingsConfig["games"] = { wario: { systemId: "gba", runners: ["retroarch/mgba", "retroarch/gpsp"] } }
const place = (key: string): LaunchPlace => {
  const at = key.indexOf("/")
  const layer = key.slice(0, at)
  const scope = key.slice(at + 1)
  return {
    layer: layer === "device" ? { _tag: "Device" } : layer.startsWith("system:") ? { _tag: "System", id: layer.slice(7) } : { _tag: "Game", id: layer.slice(5) },
    scope: scope === "any" ? { _tag: "Any" } : scope.startsWith("family:") ? { _tag: "Family", id: scope.slice(7) } : { _tag: "Runner", id: scope.slice(7) },
  }
}

async function snapshot(config: InMemoryLaunchSettingsConfig) {
  const brain = createInMemoryLaunchSettings({ games, runners, ...config })
  const result = await brain.methods.launchSettingsSnapshot("wario")
  if (result._tag === "Err") throw new Error(result.payload.message)
  return result.payload
}

describe("in-memory launch settings, korrid's rules", () => {
  it("merges in ADR 0028's order: device runner beats system any, game any beats device runner", async () => {
    const result = await snapshot({
      bodies: {
        "game:wario/any": { runner: "retroarch/mgba", gpu: { max: 40 } },
        "system:gba/any": { cpu: { max: 90 }, gpu: { max: 90 } },
        "device/runner:retroarch/mgba": { cpu: { max: 70 }, gpu: { max: 70 } },
      },
    })
    expect(result.resolved.values["cpu.max"]).toEqual({ path: { _tag: "Editable", payload: { _tag: "CpuMax" } }, value: 70, source: { _tag: "Place", payload: place("device/runner:retroarch/mgba") }, adjusted: false })
    expect(result.resolved.values["gpu.max"]).toEqual({ path: { _tag: "Editable", payload: { _tag: "GpuMax" } }, value: 40, source: { _tag: "Place", payload: place("game:wario/any") }, adjusted: false })
  })

  it("chooses the runner first; a more specific place naming a missing runner adds a notice", async () => {
    const result = await snapshot({ bodies: { "system:gba/any": { runner: "retroarch/gpsp" }, "game:wario/any": { runner: "retroarch/vba" } } })
    expect(result.runners.find(runner => runner.selected)?.id).toBe("retroarch/gpsp")
    expect(result.notices).toEqual([{ _tag: "RunnerUnavailable", payload: { runnerId: "retroarch/vba", place: place("game:wario/any") } }])
  })

  it("asks for a choice when no place names a runner and two can play", async () => {
    const result = await snapshot({})
    expect(result.runners.some(runner => runner.selected)).toBe(false)
    expect(result.routeLayers).toEqual({ _tag: "NeedsRunnerChoice" })
  })

  it("resolves runner settings for the selected runner only, over the plugin's default", async () => {
    const result = await snapshot({ bodies: { "game:wario/any": { runner: "retroarch/mgba" }, "device/runner:retroarch/gpsp": { settings: { bios: true } } } })
    expect(result.resolved.values["settings.frameskip"]).toEqual({ path: { _tag: "Editable", payload: { _tag: "Setting", payload: { key: "frameskip" } } }, value: 0, source: { _tag: "PluginDefault", payload: { pluginId: "retroarch/mgba" } }, adjusted: false })
    expect(result.resolved.values["settings.bios"]).toBeUndefined()
  })

  it("across places the more specific minimum wins and the maximum moves to meet it", async () => {
    const result = await snapshot({ bodies: { "device/any": { cpu: { max: 40 } }, "game:wario/any": { runner: "retroarch/mgba", cpu: { min: 80 } } } })
    expect(result.resolved.values["cpu.min"]?.value).toBe(80)
    expect(result.resolved.values["cpu.max"]).toMatchObject({ value: 80, adjusted: true })
  })

  it("refuses a floor above the ceiling on one place, a governor off the device, and a setting outside the schema", async () => {
    const brain = createInMemoryLaunchSettings({ games, runners, bodies: { "game:wario/any": { cpu: { max: 40 } } } })
    const games_ = "games-1"
    expect((await brain.methods.setLaunch({ place: place("game:wario/any"), path: { _tag: "CpuMin" }, value: 60, expectedRevision: games_ }))._tag).toBe("Err")
    expect((await brain.methods.setLaunch({ place: place("game:wario/any"), path: { _tag: "CpuGovernor" }, value: "performance", expectedRevision: games_ }))._tag).toBe("Err")
    expect((await brain.methods.setLaunch({ place: place("game:wario/runner:retroarch/mgba"), path: { _tag: "Setting", payload: { key: "frameskip" } }, value: 11, expectedRevision: games_ }))._tag).toBe("Err")
    expect((await brain.methods.setLaunch({ place: place("game:wario/any"), path: { _tag: "Setting", payload: { key: "frameskip" } }, value: 1, expectedRevision: games_ }))._tag).toBe("Err")
    expect((await brain.methods.setLaunch({ place: place("game:wario/family:retroarch"), path: { _tag: "Setting", payload: { key: "bios" } }, value: true, expectedRevision: games_ }))._tag).toBe("Ok")
  })

  it("refuses a write made from a stale revision", async () => {
    const brain = createInMemoryLaunchSettings({ games, runners })
    const result = await brain.methods.setLaunch({ place: place("device/any"), path: { _tag: "CpuMax" }, value: 50, expectedRevision: "device-0" })
    expect(result._tag === "Err" && result.payload.code).toBe("SettingsConflict")
  })

  it("lists a place for each layer and every installed runner, with editable kinds", async () => {
    const result = await snapshot({ bodies: { "game:wario/any": { runner: "retroarch/mgba" } } })
    expect(result.places.filter(entry => entry.place.layer._tag === "Game").map(entry => [entry.title, entry.editableKinds])).toEqual([
      ["Game", ["runner", "cpu", "gpu", "modifiers"]],
      ["Game for family retroarch", ["cpu", "gpu", "modifiers", "settings"]],
      ["Game for mGBA", ["cpu", "gpu", "modifiers", "settings"]],
      ["Game for gpSP", ["cpu", "gpu", "modifiers", "settings"]],
    ])
  })

  it("lists installed modifiers and refuses a setting a modifier's schema does not name", async () => {
    const modifiers = { "korri/gamescope": { title: "Gamescope", schema: { properties: { fsr: { type: "boolean" } } } }, "korri/mangohud": { title: "MangoHud" } }
    const brain = createInMemoryLaunchSettings({ games, runners, modifiers })
    const result = await brain.methods.launchSettingsSnapshot("wario")
    expect(result._tag === "Ok" && result.payload.modifiers.map(modifier => [modifier.id, modifier.schema !== undefined])).toEqual([["korri/gamescope", true], ["korri/mangohud", false]])
    const at = (key: string) => ({ place: place("game:wario/any"), path: { _tag: "ModifierSetting" as const, payload: { id: key.split("#")[0] ?? "", key: key.split("#")[1] ?? "" } }, expectedRevision: "games-1" })
    expect((await brain.methods.setLaunch({ ...at("korri/mangohud#fps"), value: 1 }))._tag).toBe("Err")
    expect((await brain.methods.setLaunch({ ...at("korri/gamescope#fsr"), value: "yes" }))._tag).toBe("Err")
    expect((await brain.methods.setLaunch({ ...at("korri/gamescope#fsr"), value: true }))._tag).toBe("Ok")
  })

  it("concatenates list kinds in merge order with each place's part, routines before the place's own steps", async () => {
    const result = await snapshot({
      routines: { lights: { before: [{ run: "lights off" }] } },
      bodies: {
        "device/any": { args: { append: ["--a"] }, hooks: { use: ["lights"], before: [{ run: "sync" }] } },
        "game:wario/any": { runner: "retroarch/mgba", args: { append: ["--b"] } },
      },
    })
    expect(result.resolved.values["args.append"]).toEqual({
      path: { _tag: "ArgsAppend" },
      value: ["--a", "--b"],
      contributions: [
        { source: { _tag: "Place", payload: place("device/any") }, value: ["--a"] },
        { source: { _tag: "Place", payload: place("game:wario/any") }, value: ["--b"] },
      ],
      adjusted: false,
    })
    expect(result.resolved.values["hooks.before"]?.source).toBeUndefined()
    expect(result.resolved.values["hooks.before"]?.contributions).toEqual([
      { source: { _tag: "Place", payload: place("device/any") }, value: [{ run: "lights off" }], routine: "lights" },
      { source: { _tag: "Place", payload: place("device/any") }, value: [{ run: "sync" }] },
    ])
  })
})
