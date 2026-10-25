/** The in-memory brain answers as korrid does (ADR 0028, services/korrid/src/launch_settings). */
import { describe, expect, it } from "bun:test"
import type { LaunchPath, LaunchPlace } from "@contracts/generated/korrid"
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
      ["Game", ["runner", "cpu", "gpu", "modifiers", "services"]],
      ["Game for family retroarch", ["cpu", "gpu", "modifiers", "settings", "services"]],
      ["Game for mGBA", ["cpu", "gpu", "modifiers", "settings", "services"]],
      ["Game for gpSP", ["cpu", "gpu", "modifiers", "settings", "services"]],
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

  describe("nested settings (ADR 0037)", () => {
    const nested: InMemoryLaunchSettingsConfig["runners"] = {
      "retroarch/mgba": {
        title: "mGBA",
        family: "retroarch",
        schema: { properties: { video: { type: "object", properties: { fps: { type: "integer", minimum: 1, maximum: 60, default: 60 }, smooth: { type: "boolean" } } } } },
      },
    }
    const setting = (key: string): LaunchPath => ({ _tag: "Setting", payload: { key } })

    it("folds one leaf at a time; null clears less specific places and the plugin default holds", async () => {
      const result = await snapshot({
        runners: nested,
        bodies: {
          "game:wario/any": { runner: "retroarch/mgba" },
          "device/runner:retroarch/mgba": { settings: { video: { fps: 30, smooth: true } } },
          "game:wario/runner:retroarch/mgba": { settings: { video: { fps: null, smooth: null } } },
        },
      })
      expect(result.resolved.values["settings.video.fps"]).toEqual({ path: { _tag: "Editable", payload: setting("video.fps") }, value: 60, source: { _tag: "PluginDefault", payload: { pluginId: "retroarch/mgba" } }, adjusted: false })
      expect(result.resolved.values["settings.video.smooth"]).toBeUndefined()
    })

    it("writes a leaf by its dotted path, clears it to null, and takes it away with its empty sections", async () => {
      const brain = createInMemoryLaunchSettings({ games, runners: nested })
      const at = place("game:wario/runner:retroarch/mgba")
      const bodyOf = async () => {
        const result = await brain.methods.launchSettingsSnapshot("wario")
        return result._tag === "Ok" ? result.payload.places.find(entry => entry.title === "Game for mGBA")?.body : undefined
      }
      expect((await brain.methods.setLaunch({ place: at, path: setting("video.fps"), value: 30, expectedRevision: "games-1" }))._tag).toBe("Ok")
      expect(await bodyOf()).toEqual({ settings: { video: { fps: 30 } } })
      expect((await brain.methods.setLaunch({ place: at, path: setting("video.fps"), clear: true, expectedRevision: "games-2" }))._tag).toBe("Ok")
      expect(await bodyOf()).toEqual({ settings: { video: { fps: null } } })
      // clear with a value, and a clear of a whole section, are refused.
      expect((await brain.methods.setLaunch({ place: at, path: setting("video.fps"), value: 30, clear: true, expectedRevision: "games-3" }))._tag).toBe("Err")
      expect((await brain.methods.setLaunch({ place: at, path: setting("video"), clear: true, expectedRevision: "games-3" }))._tag).toBe("Err")
      expect((await brain.methods.setLaunch({ place: at, path: setting("video.fps"), expectedRevision: "games-3" }))._tag).toBe("Ok")
      expect(await bodyOf()).toEqual({ settings: {} })
    })

    it("keeps a value the schema does not take and names it in a SettingSkipped notice", async () => {
      const result = await snapshot({
        runners: nested,
        bodies: { "game:wario/any": { runner: "retroarch/mgba" }, "device/runner:retroarch/mgba": { settings: { video: { fps: 90, sharp: true } } } },
      })
      expect(result.resolved.values["settings.video.fps"]?.value).toBe(90)
      expect(result.notices).toEqual([
        { _tag: "SettingSkipped", payload: { path: setting("video.fps"), message: "90 is above the maximum of 60" } },
        { _tag: "SettingSkipped", payload: { path: setting("video.sharp"), message: "mGBA has no setting video.sharp" } },
      ])
    })
  })

  it("lists a stream runner on every snapshot, never chosen, and a launch service's settings for every place", async () => {
    const brain = createInMemoryLaunchSettings({
      games: { wario: { systemId: "gba", runners: ["retroarch/mgba"] } },
      runners: { ...runners, "@korri:moonlight/moonlight": { title: "Moonlight", choosable: false } },
      services: { "korri/sunshine": { title: "Sunshine", schema: { properties: { video: { type: "object", properties: { "max-bitrate-kbps": { type: "integer" } } } } } } },
      bodies: { "game:wario/any": { services: { "korri/sunshine": { settings: { video: { "max-bitrate-kbps": 3000 } } } } } },
    })
    const game = await brain.methods.launchSettingsSnapshot("wario")
    if (game._tag === "Err") throw new Error(game.payload.message)
    expect(game.payload.runners.map(runner => [runner.id, runner.selected, runner.choosable])).toEqual([["retroarch/mgba", true, true], ["@korri:moonlight/moonlight", false, false]])
    expect(game.payload.resolved.values["services.korri/sunshine.settings.video.max-bitrate-kbps"]).toMatchObject({ path: { _tag: "Editable", payload: { _tag: "ServiceSetting", payload: { id: "korri/sunshine", key: "video.max-bitrate-kbps" } } }, value: 3000 })
    const device = await brain.methods.launchSettingsSnapshot()
    if (device._tag === "Err") throw new Error(device.payload.message)
    expect(device.payload.places.map(entry => entry.title)).toEqual(["Device", "Device for Moonlight"])
    expect((await brain.methods.setLaunch({ place: place("game:wario/any"), path: { _tag: "Runner" }, value: "@korri:moonlight/moonlight", expectedRevision: "games-1" }))._tag).toBe("Err")
  })

  it("resolves a stream runner as the stream viewer does: defaults, Device, then the game's system, game and picked profile", async () => {
    const MOONLIGHT = "@korri:moonlight/moonlight"
    const brain = createInMemoryLaunchSettings({
      games: { wario: { systemId: "gba", releaseId: "usa", runners: ["retroarch/mgba"] } },
      runners: { ...runners, [MOONLIGHT]: { title: "Moonlight", choosable: false, schema: { properties: { video: { type: "object", properties: { codec: { type: "string", default: "h264" }, fps: { type: "integer" } } } } } } },
      bodies: {
        [`device/runner:${MOONLIGHT}`]: { settings: { video: { fps: 30, codec: "h265" } } },
        [`game:wario/runner:${MOONLIGHT}`]: { settings: { video: { codec: null } } },
        [`release:usa/runner:${MOONLIGHT}`]: { settings: { video: { fps: 60 } } },
      },
    })
    const stream = async (gameId?: string) => {
      const result = await brain.methods.launchSettingsSnapshot(gameId)
      if (result._tag === "Err") throw new Error(result.payload.message)
      const resolved = result.payload.runnerResolved
      expect(result.payload.resolved.values["settings.video.fps"]).toBeUndefined()
      return resolved.find(entry => entry.runnerId === MOONLIGHT)?.values ?? {}
    }
    const game = await stream("wario")
    expect(game["settings.video.fps"]).toMatchObject({ value: 30, source: { _tag: "Place", payload: place(`device/runner:${MOONLIGHT}`) } })
    expect(game["settings.video.codec"]).toMatchObject({ value: "h264", source: { _tag: "PluginDefault", payload: { pluginId: MOONLIGHT } } })
    const device = await stream()
    expect(device["settings.video.codec"]?.value).toBe("h265")
  })
})
