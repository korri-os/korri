import { describe, expect, it } from "bun:test"
import type { SurfaceLaunchSettings, SurfaceSettingItem } from "@contracts/surface/korri-surface"
import { createInMemoryKorridClient } from "../korrid/client"
import { RG353M_LAUNCH_FACTS } from "../korrid/in-memory-launch-settings"
import { createLaunchSettings } from "./launch-settings"
import { cpuSteps, gpuSteps, governors, LAUNCH_ID, parseLaunchId } from "./launch-settings-model"

const settle = () => new Promise(resolve => setTimeout(resolve, 0))
const games = { wario: { systemId: "gba", runner: { id: "retroarch/mgba", title: "mGBA" }, releaseId: "usa" } }

function rows(editor: SurfaceLaunchSettings, layerId: string): SurfaceSettingItem[] {
  if (editor._tag === "Closed") return []
  return editor.layers.find(layer => layer.id === layerId)?.groups.flatMap(group => [...group.items]) ?? []
}
const row = (editor: SurfaceLaunchSettings, layerId: string, label: string) =>
  rows(editor, layerId).find(item => item.label === label)

describe("clock steps", () => {
  it("are the RG353M's real steps, labelled with GHz", () => {
    const steps = cpuSteps(RG353M_LAUNCH_FACTS)
    expect(steps.guessed).toBe(false)
    expect(steps.choices.map(choice => choice.value)).toEqual(["23", "33", "45", "61", "79", "89", "100"])
    expect(steps.choices[2]?.label).toBe("45 % (0.82 GHz)")
    expect(steps.choices[3]?.label).toBe("61 % (1.10 GHz)")
    expect(steps.choices[6]?.label).toBe("100 % (1.80 GHz)")
    expect(gpuSteps(RG353M_LAUNCH_FACTS).choices.map(choice => choice.label)).toContain("50 % (0.40 GHz)")
  })

  it("fall back to 10 % steps without device facts", () => {
    const steps = cpuSteps(undefined)
    expect(steps.guessed).toBe(true)
    expect(steps.choices.map(choice => choice.label)).toEqual(["10 %", "20 %", "30 %", "40 %", "50 %", "60 %", "70 %", "80 %", "90 %", "100 %"])
    expect(governors(undefined)).toEqual([])
  })

  it("come from the policy with the highest top clock", () => {
    const facts = {
      ...RG353M_LAUNCH_FACTS,
      cpuPolicies: [
        { name: "policy0", cpus: [0], cpuinfoMaxFreq: 2_000_000, availableFrequencies: [1_000_000, 2_000_000], availableGovernors: ["schedutil", "performance"] },
        { name: "policy4", cpus: [4], cpuinfoMaxFreq: 3_000_000, availableFrequencies: [1_500_000, 3_000_000], availableGovernors: ["schedutil"] },
      ],
    }
    expect(cpuSteps(facts).choices.map(choice => choice.label)).toEqual(["50 % (1.50 GHz)", "100 % (3.00 GHz)"])
    expect(governors(facts)).toEqual(["schedutil"])
  })
})

describe("a device without a devfreq GPU", () => {
  it("shows one read-only GPU line instead of choices", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient({
      launchSettings: { deviceFacts: { ...RG353M_LAUNCH_FACTS, gpuDevfreq: [] } },
    }))
    await launch.refreshDevice()
    const items = launch.getSnapshot().deviceGroup?.items ?? []
    expect(items.map(item => item.label)).toEqual(["CPU min", "CPU max", "GPU limits aren't available on this device", "CPU governor"])
    expect(items[2]).toEqual({ id: "launch-device:gpu.min:unavailable", label: "GPU limits aren't available on this device" })
    expect(items[0]?.description).toBe("Applies at next launch.")
  })
})

describe("launch ids", () => {
  it("read back what the host published", () => {
    expect(parseLaunchId(LAUNCH_ID.set("runner:retroarch/mgba", "cpu.max"))).toEqual({ _tag: "Set", layerId: "runner:retroarch/mgba", setting: "cpu.max" })
    expect(parseLaunchId(LAUNCH_ID.device("cpu.governor"))).toEqual({ _tag: "Device", setting: "cpu.governor" })
    expect(parseLaunchId(LAUNCH_ID.rename("p1"))).toEqual({ _tag: "Rename", profileId: "p1" })
    expect(parseLaunchId("device-name")).toBeUndefined()
    expect(parseLaunchId("launch:set:game:cpu.turbo")).toBeUndefined()
  })
})

describe("device launch settings", () => {
  it("publish a Launch settings group with source, device-only mark and next-launch note", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient({ launchSettings: { deviceFacts: RG353M_LAUNCH_FACTS } }))
    await launch.refreshDevice()
    const group = launch.getSnapshot().deviceGroup
    expect(group?.title).toBe("Launch settings")
    expect(group?.items.map(item => item.label)).toEqual(["CPU min", "CPU max", "GPU min", "GPU max", "CPU governor"])
    const cpuMax = group?.items[1]
    expect(cpuMax).toMatchObject({ value: "Inherit", source: "100 % (1.80 GHz) from Device (default)", scope: "device", description: "Applies at next launch." })
    expect(group?.items[4]).toMatchObject({ value: "Inherit", source: "schedutil from Device (default)" })

    launch.change(LAUNCH_ID.device("cpu.max"), "45")
    expect(launch.getSnapshot().status).toEqual({ _tag: "Saving", settingId: LAUNCH_ID.device("cpu.max") })
    await settle()
    await settle()
    expect(launch.getSnapshot().status).toBeUndefined()
    expect(launch.getSnapshot().deviceGroup?.items[1]).toMatchObject({ value: "45 % (0.82 GHz)", source: "from Device" })
  })

  it("mark steps unknown when the device states no facts", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient())
    await launch.refreshDevice()
    expect(launch.getSnapshot().deviceGroup?.items[0]?.description).toBe("Applies at next launch. Clock steps unknown.")
    expect(launch.getSnapshot().deviceGroup?.items[4]?.description).toBe("Applies at next launch. Governors unknown.")
  })
})

describe("game launch settings editor", () => {
  it("opens Loading, then Ready with one layer per scope and the source of each value", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient({
      launchSettings: { games, deviceFacts: RG353M_LAUNCH_FACTS, layers: { "runner:retroarch/mgba": { cpu: { max: 61 }, gpu: {} } } },
    }))
    launch.open("wario", "Wario Land 4")
    expect(launch.getSnapshot().editor._tag).toBe("Loading")
    await settle()
    const editor = launch.getSnapshot().editor
    expect(editor._tag).toBe("Ready")
    if (editor._tag === "Closed") return
    expect(editor.layers.map(layer => layer.title)).toEqual(["Device", "System gba", "Runner mGBA", "Game", "Release usa"])
    expect(row(editor, "runner:retroarch/mgba", "CPU max")).toMatchObject({ value: "61 % (1.10 GHz)", source: "from Runner mGBA" })
    expect(row(editor, "game", "CPU max")).toMatchObject({ value: "Inherit", source: "61 % (1.10 GHz) from Runner mGBA" })
    expect(row(editor, "game", "CPU governor")).toBeUndefined()
    expect(row(editor, "device", "CPU governor")?.scope).toBe("device")
  })

  it("sets and clears a value on one layer", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient({ launchSettings: { games } }))
    launch.open("wario", "Wario Land 4")
    await settle()
    const id = LAUNCH_ID.set("game", "gpu.min")
    expect(launch.change(id, "30")).toBe(true)
    expect(launch.getSnapshot().status).toEqual({ _tag: "Saving", settingId: id })
    await settle(); await settle()
    expect(row(launch.getSnapshot().editor, "game", "GPU min")).toMatchObject({ value: "30 %", source: "from Game" })
    launch.change(id, "inherit")
    await settle(); await settle()
    expect(row(launch.getSnapshot().editor, "game", "GPU min")).toMatchObject({ value: "Inherit", source: "0 % from Device (default)" })
  })

  it("creates, picks, renames and deletes a profile", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient({ launchSettings: { games } }))
    launch.open("wario", "Wario Land 4")
    await settle()
    launch.change(LAUNCH_ID.create, "  ")
    expect(launch.getSnapshot().status).toEqual({ _tag: "Problem", settingId: LAUNCH_ID.create, message: "A profile needs a name." })
    launch.dismissProblem()
    launch.change(LAUNCH_ID.create, "Speedrun")
    await settle(); await settle()
    let editor = launch.getSnapshot().editor
    if (editor._tag === "Closed") throw new Error("closed")
    const profile = editor.profiles[0]
    expect(profile?.title).toBe("Speedrun")
    expect(editor.layers.at(-1)?.title).toBe("Profile Speedrun")

    launch.change(LAUNCH_ID.pick, profile?.id ?? "")
    await settle(); await settle()
    editor = launch.getSnapshot().editor
    expect(editor._tag !== "Closed" && editor.selectedProfileId).toBe(profile?.id)
    expect(row(editor, "game", "Profile at next launch")?.value).toBe("Speedrun")

    launch.change(LAUNCH_ID.set(`profile:${profile?.id}`, "cpu.max"), "50")
    await settle(); await settle()
    expect(row(launch.getSnapshot().editor, "game", "CPU max")?.source).toBe("50 % from Profile Speedrun")

    launch.change(LAUNCH_ID.rename(profile?.id ?? ""), "Race")
    await settle(); await settle()
    expect(row(launch.getSnapshot().editor, "game", "Profile at next launch")?.value).toBe("Race")

    expect(launch.act(LAUNCH_ID.remove(profile?.id ?? ""))).toBe(true)
    await settle(); await settle()
    editor = launch.getSnapshot().editor
    expect(editor._tag !== "Closed" && editor.profiles).toEqual([])
    expect(row(editor, "game", "Profile at next launch")?.value).toBe("No profile")
  })

  it("asks for a reload after a write korrid refuses as stale", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient({
      launchSettings: { games, failures: { set: { code: "SettingsConflict", message: "changed" } } },
    }))
    launch.open("wario", "Wario Land 4")
    await settle()
    launch.change(LAUNCH_ID.set("game", "cpu.min"), "20")
    await settle(); await settle()
    const editor = launch.getSnapshot().editor
    expect(editor._tag).toBe("Conflict")
    if (editor._tag !== "Conflict") return
    expect(editor.message).toContain("changed elsewhere")
    expect(editor.actions.map(action => action.id)).toEqual([LAUNCH_ID.reload, LAUNCH_ID.cancel])
    launch.act(LAUNCH_ID.reload)
    await settle()
    expect(launch.getSnapshot().editor._tag).toBe("Ready")
  })

  it("shows a failed read with Retry, and closes on the close action", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient({
      launchSettings: { failures: { snapshot: { code: "Unavailable", message: "Korri could not read launch settings." } } },
    }))
    launch.open("wario", "Wario Land 4")
    await settle()
    const editor = launch.getSnapshot().editor
    expect(editor).toMatchObject({ _tag: "Error", message: "Korri could not read launch settings.", layers: [] })
    if (editor._tag !== "Error") return
    expect(editor.actions.map(action => action.label)).toEqual(["Retry", "Close"])
    launch.act(LAUNCH_ID.cancel)
    expect(launch.getSnapshot().editor).toEqual({ _tag: "Closed" })
  })

  it("ignores a reply that arrives after the editor closed", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient({ launchSettings: { games, delayMs: 5 } }))
    launch.open("wario", "Wario Land 4")
    launch.cancel()
    await new Promise(resolve => setTimeout(resolve, 20))
    expect(launch.getSnapshot().editor).toEqual({ _tag: "Closed" })
  })

  it("without a runner choice keeps Device, Game and profiles editable, says what is missing, and offers the chooser", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient({
      launchSettings: { games: { wario: { ...games.wario, routeLayers: { _tag: "NeedsRunnerChoice" } } } },
    }))
    let opened = 0
    launch.open("wario", "Wario Land 4", () => { opened++ })
    await settle()
    const editor = launch.getSnapshot().editor
    if (editor._tag !== "Ready") throw new Error(editor._tag)
    expect(editor.layers.map(layer => layer.title)).toEqual(["Device", "Game"])
    expect(editor.note).toBe("System, runner and release settings appear once you choose a runner. Values in force leave them out.")
    expect(editor.actions.map(action => action.id)).toEqual([LAUNCH_ID.runner, LAUNCH_ID.reload, LAUNCH_ID.cancel])
    expect(launch.act(LAUNCH_ID.runner)).toBe(true)
    expect(opened).toBe(1)
  })

  it("does not offer the chooser when the game page has none", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient({
      launchSettings: { games: { wario: { routeLayers: { _tag: "NeedsRunnerChoice" } } } },
    }))
    launch.open("wario", "Wario Land 4")
    await settle()
    const editor = launch.getSnapshot().editor
    expect(editor._tag !== "Closed" && editor.actions.map(action => action.id)).toEqual([LAUNCH_ID.reload, LAUNCH_ID.cancel])
  })

  it("names why route layers are unavailable", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient({
      launchSettings: { games: { wario: { routeLayers: { _tag: "Unavailable", message: "The runner plugin did not load." } } } },
    }))
    launch.open("wario", "Wario Land 4", () => undefined)
    await settle()
    const editor = launch.getSnapshot().editor
    if (editor._tag !== "Ready") throw new Error(editor._tag)
    expect(editor.note).toBe("System, runner and release settings aren't available: The runner plugin did not load. Values in force leave them out.")
    expect(editor.actions.map(action => action.id)).toEqual([LAUNCH_ID.reload, LAUNCH_ID.cancel])
  })

  it("has no note when the route's layers are present", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient({ launchSettings: { games } }))
    launch.open("wario", "Wario Land 4")
    await settle()
    expect(launch.getSnapshot().editor).not.toHaveProperty("note")
  })
})

describe("review round 1", () => {
  const twoQuiet = {
    games,
    profiles: { wario: [
      { id: "p1", title: "Quiet", cpu: {}, gpu: {} },
      { id: "p2", title: "Quiet", cpu: { max: 50 }, gpu: {} },
    ] },
    selected: { wario: "p2" },
  }

  it("names a profile pick by id, marks it by value, and tells duplicate titles apart", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient({ launchSettings: twoQuiet }))
    launch.open("wario", "Wario Land 4")
    await settle()
    const editor = launch.getSnapshot().editor
    const pick = row(editor, "game", "Profile at next launch")
    expect(pick?.interaction).toEqual({
      kind: "choice",
      selected: "p2",
      choices: [{ value: "", label: "No profile" }, { value: "p1", label: "Quiet" }, { value: "p2", label: "Quiet (2)" }],
    })
    expect(pick?.value).toBe("Quiet (2)")
    expect(editor._tag !== "Closed" && editor.layers.slice(-2).map(layer => layer.title)).toEqual(["Profile Quiet", "Profile Quiet (2)"])
    expect(row(editor, "game", "CPU max")?.source).toBe("50 % from Profile Quiet (2)")
    launch.change(LAUNCH_ID.pick, "p1")
    await settle(); await settle()
    expect(row(launch.getSnapshot().editor, "game", "Profile at next launch")?.interaction).toMatchObject({ selected: "p1" })
    launch.change(LAUNCH_ID.pick, "")
    await settle(); await settle()
    expect(row(launch.getSnapshot().editor, "game", "Profile at next launch")?.interaction).toMatchObject({ selected: "" })
  })

  it("marks every launch choice row by value", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient({ launchSettings: { games, deviceFacts: RG353M_LAUNCH_FACTS, layers: { device: { cpu: { max: 61 }, gpu: {}, governor: "schedutil" } } } }))
    launch.open("wario", "Wario Land 4")
    await settle()
    const editor = launch.getSnapshot().editor
    expect(row(editor, "device", "CPU max")?.interaction).toMatchObject({ selected: "61" })
    expect(row(editor, "device", "CPU min")?.interaction).toMatchObject({ selected: "inherit" })
    expect(row(editor, "device", "CPU governor")?.interaction).toMatchObject({ selected: "schedutil" })
  })

  it("offers a 0 % floor on minimums only", async () => {
    for (const facts of [RG353M_LAUNCH_FACTS, undefined]) {
      const launch = createLaunchSettings(createInMemoryKorridClient({ launchSettings: { games, ...(facts === undefined ? {} : { deviceFacts: facts }) } }))
      launch.open("wario", "Wario Land 4")
      await settle()
      const editor = launch.getSnapshot().editor
      for (const label of ["CPU min", "GPU min"]) {
        const interaction = row(editor, "game", label)?.interaction
        expect(interaction?.kind === "choice" && interaction.choices[1]).toEqual({ value: "0", label: "0 % (no floor)" })
      }
      for (const label of ["CPU max", "GPU max"]) {
        const interaction = row(editor, "game", label)?.interaction
        expect(interaction?.kind === "choice" && interaction.choices.some(choice => choice.value === "0")).toBe(false)
      }
    }
  })

  it("keeps the Device group after a failed read, with Korri's message and Retry", async () => {
    let fail = true
    const base = createInMemoryKorridClient({ launchSettings: { deviceFacts: RG353M_LAUNCH_FACTS } })
    const korrid = {
      ...base,
      async launchSettingsSnapshot(gameId?: string) {
        return fail
          ? { _tag: "Err" as const, payload: { code: "Unavailable", message: "Korri could not read launch settings." } }
          : base.launchSettingsSnapshot(gameId)
      },
    }
    const launch = createLaunchSettings(korrid)
    await launch.refreshDevice()
    const group = launch.getSnapshot().deviceGroup
    expect(group?.title).toBe("Launch settings")
    expect(group?.items).toEqual([{
      id: LAUNCH_ID.deviceRetry,
      label: "Launch settings could not be read",
      description: "Korri could not read launch settings.",
      interaction: { kind: "action", actionId: LAUNCH_ID.deviceRetry },
    }])
    fail = false
    expect(launch.act(LAUNCH_ID.deviceRetry)).toBe(true)
    await settle(); await settle()
    expect(launch.getSnapshot().deviceGroup?.items.map(item => item.label)).toEqual(["CPU min", "CPU max", "GPU min", "GPU max", "CPU governor"])
  })
})
