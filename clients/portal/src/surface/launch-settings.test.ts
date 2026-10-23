import { describe, expect, it } from "bun:test"
import type { SurfaceLaunchSettings, SurfaceSettingItem } from "@contracts/surface/korri-surface"
import { createInMemoryKorridClient } from "../korrid/client"
import { type InMemoryLaunchSettingsConfig, RG353M_LAUNCH_FACTS } from "../korrid/in-memory-launch-settings"
import { createLaunchSettings } from "./launch-settings"
import { cpuSteps, gpuSteps, governors, LAUNCH_ID } from "./launch-settings-model"

const settle = () => new Promise(resolve => setTimeout(resolve, 0))
const written = async () => { await settle(); await settle(); await settle() }

const MGBA_SCHEMA = {
  type: "object",
  properties: {
    frameskip: { type: "integer", title: "Frameskip", description: "Frames skipped between drawn ones.", minimum: 0, maximum: 10, default: 0 },
    color: { type: "boolean", title: "Colour correction" },
    // A key with a dot: never split.
    "gb.solar": { type: "string", title: "Solar sensor", enum: ["off", "low", "high"] },
    lang: { type: "string", title: "Language", "x-korri-scope": "device" },
  },
}
const runners: InMemoryLaunchSettingsConfig["runners"] = {
  "retroarch/mgba": { title: "mGBA", family: "retroarch", schema: MGBA_SCHEMA, prepare: { _tag: "Ready", payload: { env: { MGBA_AUDIO: "pulse" }, args: ["-f"] } } },
  "retroarch/gpsp": { title: "gpSP", family: "retroarch", schema: { type: "object", properties: { bios: { type: "boolean", title: "Use BIOS" } } } },
}
const games: InMemoryLaunchSettingsConfig["games"] = { wario: { systemId: "gba", releaseId: "usa", runners: ["retroarch/mgba", "retroarch/gpsp"] } }
const chosen = { "game:wario/any": { runner: "retroarch/mgba" } }
const base: InMemoryLaunchSettingsConfig = { games, runners, bodies: chosen }

const MGBA = "game:wario/runner:retroarch/mgba"

function rows(editor: SurfaceLaunchSettings, placeId: string): SurfaceSettingItem[] {
  if (editor._tag === "Closed") return []
  return editor.places.find(place => place.id === placeId)?.groups.flatMap(group => [...group.items]) ?? []
}
const row = (editor: SurfaceLaunchSettings, placeId: string, label: string) =>
  rows(editor, placeId).find(item => item.label === label)

async function opened(config: InMemoryLaunchSettingsConfig, chooseRunners?: () => void) {
  const launch = createLaunchSettings(createInMemoryKorridClient({ launchSettings: config }))
  launch.open("wario", "Wario Land 4", chooseRunners)
  await settle()
  return launch
}

describe("clock steps", () => {
  it("are the RG353M's real steps, labelled with GHz", () => {
    const steps = cpuSteps(RG353M_LAUNCH_FACTS)
    expect(steps.guessed).toBe(false)
    expect(steps.choices.map(choice => choice.value)).toEqual(["23", "33", "45", "61", "79", "89", "100"])
    expect(steps.choices[2]?.label).toBe("45 % (0.82 GHz)")
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

describe("device launch settings", () => {
  it("publish a Launch settings group with source, device-only mark and next-launch note", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient({ launchSettings: { deviceFacts: RG353M_LAUNCH_FACTS } }))
    await launch.refreshDevice()
    const group = launch.getSnapshot().deviceGroup
    expect(group?.title).toBe("Launch settings")
    expect(group?.items.map(item => item.label)).toEqual(["CPU min", "CPU max", "GPU min", "GPU max", "CPU governor"])
    expect(group?.items[1]).toMatchObject({ value: "Inherit", source: "100 % (1.80 GHz) from Device (default)", scope: "device" })
    expect(group?.items[1]?.description).toBe("Applies at next launch. Sets the device's clock limits.")
    expect(group?.items[4]).toMatchObject({ value: "Inherit", source: "schedutil from Device (default)" })

    launch.change(LAUNCH_ID.device("cpu.max"), "45")
    expect(launch.getSnapshot().status).toEqual({ _tag: "Saving", settingId: LAUNCH_ID.device("cpu.max") })
    await written()
    expect(launch.getSnapshot().status).toBeUndefined()
    expect(launch.getSnapshot().deviceGroup?.items[1]).toMatchObject({ value: "45 % (0.82 GHz)", source: "from Device" })
  })

  it("show one read-only GPU line on a device without a devfreq GPU", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient({
      launchSettings: { deviceFacts: { ...RG353M_LAUNCH_FACTS, gpuDevfreq: [] } },
    }))
    await launch.refreshDevice()
    const items = launch.getSnapshot().deviceGroup?.items ?? []
    expect(items.map(item => item.label)).toEqual(["CPU min", "CPU max", "GPU limits aren't available on this device", "CPU governor"])
    expect(items[2]?.interaction).toBeUndefined()
  })

  it("mark steps unknown when the device states no facts", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient())
    await launch.refreshDevice()
    expect(launch.getSnapshot().deviceGroup?.items[0]?.description).toBe("Applies at next launch. Sets the device's clock limits. Clock steps unknown.")
    expect(launch.getSnapshot().deviceGroup?.items[4]?.description).toBe("Applies at next launch. Governors unknown.")
  })

  it("keep the group after a failed read, with Korri's message and Retry", async () => {
    let fail = true
    const brain = createInMemoryKorridClient({ launchSettings: { deviceFacts: RG353M_LAUNCH_FACTS } })
    const korrid = {
      ...brain,
      async launchSettingsSnapshot(gameId?: string) {
        return fail
          ? { _tag: "Err" as const, payload: { code: "Unavailable", message: "Korri could not read launch settings." } }
          : brain.launchSettingsSnapshot(gameId)
      },
    }
    const launch = createLaunchSettings(korrid)
    await launch.refreshDevice()
    expect(launch.getSnapshot().deviceGroup?.items).toEqual([{
      id: LAUNCH_ID.deviceRetry,
      label: "Launch settings could not be read",
      description: "Korri could not read launch settings.",
      interaction: { kind: "action", actionId: LAUNCH_ID.deviceRetry },
    }])
    fail = false
    expect(launch.act(LAUNCH_ID.deviceRetry)).toBe(true)
    await written()
    expect(launch.getSnapshot().deviceGroup?.items.map(item => item.label)).toEqual(["CPU min", "CPU max", "GPU min", "GPU max", "CPU governor"])
  })
})

describe("game launch settings editor", () => {
  it("opens Loading, then Ready with a place per layer and runner scope, most general first", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient({ launchSettings: base }))
    launch.open("wario", "Wario Land 4")
    expect(launch.getSnapshot().editor._tag).toBe("Loading")
    await settle()
    const editor = launch.getSnapshot().editor
    if (editor._tag !== "Ready") throw new Error(editor._tag)
    expect(editor.places.map(place => `${place.layer.title} | ${place.scope.kind} ${place.scope.label}`)).toEqual([
      "Device | any Any runner", "Device | family Family retroarch", "Device | runner mGBA", "Device | runner gpSP",
      "System gba | any Any runner", "System gba | family Family retroarch", "System gba | runner mGBA", "System gba | runner gpSP",
      "Game | any Any runner", "Game | family Family retroarch", "Game | runner mGBA", "Game | runner gpSP",
      "Release usa | any Any runner", "Release usa | family Family retroarch", "Release usa | runner mGBA", "Release usa | runner gpSP",
    ])
    expect(new Set(editor.places.map(place => place.id)).size).toBe(editor.places.length)
    expect(editor.notes).toEqual([])
  })

  it("says where each value in force comes from, and when a runner place is not in force", async () => {
    const launch = await opened({ ...base, deviceFacts: RG353M_LAUNCH_FACTS, bodies: { ...chosen, "device/runner:retroarch/mgba": { cpu: { max: 61 } } } })
    const editor = launch.getSnapshot().editor
    expect(row(editor, "device/runner:retroarch/mgba", "CPU max")).toMatchObject({ value: "61 % (1.10 GHz)", source: "from Device for mGBA" })
    expect(row(editor, "game:wario/any", "CPU max")).toMatchObject({ value: "Inherit", source: "61 % (1.10 GHz) from Device for mGBA" })
    expect(row(editor, "game:wario/any", "CPU max")?.description).toBe("Applies at next launch. Limits the game's session.")
    expect(row(editor, "game:wario/runner:retroarch/gpsp", "CPU max")?.source).toBe("Not in force: mGBA runs this game")
    expect(row(editor, "game:wario/any", "CPU governor")).toBeUndefined()
    expect(row(editor, "device/any", "CPU governor")?.scope).toBe("device")
  })

  it("sets and clears a value on one place", async () => {
    const launch = await opened(base)
    const id = LAUNCH_ID.set("game:wario/any", "gpu.min")
    expect(launch.change(id, "30")).toBe(true)
    expect(launch.getSnapshot().status).toEqual({ _tag: "Saving", settingId: id })
    await written()
    expect(row(launch.getSnapshot().editor, "game:wario/any", "GPU min")).toMatchObject({ value: "30 %", source: "from Game" })
    launch.change(id, "inherit")
    await written()
    expect(row(launch.getSnapshot().editor, "game:wario/any", "GPU min")).toMatchObject({ value: "Inherit", source: "0 % from Device (default)" })
  })

  it("chooses the runner with a typed set, shared across devices", async () => {
    const launch = await opened(base)
    const runner = row(launch.getSnapshot().editor, "game:wario/any", "Runner")
    expect(runner).toMatchObject({ value: "mGBA", scope: "shared", source: "from Game" })
    expect(runner?.interaction).toEqual({
      kind: "choice",
      selected: "retroarch/mgba",
      choices: [{ value: "inherit", label: "Inherit" }, { value: "retroarch/mgba", label: "mGBA" }, { value: "retroarch/gpsp", label: "gpSP" }],
    })
    expect(row(launch.getSnapshot().editor, MGBA, "Runner")).toBeUndefined()
    launch.change(LAUNCH_ID.set("game:wario/any", "runner"), "retroarch/gpsp")
    await written()
    expect(row(launch.getSnapshot().editor, "device/any", "Runner")?.source).toBe("gpSP from Game")
    expect(row(launch.getSnapshot().editor, MGBA, "CPU min")?.source).toBe("Not in force: gpSP runs this game")
  })

  it("adds a runner setting from the schema, writes only once it has a value, and never splits a dotted key", async () => {
    const launch = await opened(base)
    const add = row(launch.getSnapshot().editor, MGBA, "Add setting")
    expect(add?.interaction).toEqual({
      kind: "choice",
      selected: "",
      choices: [
        { value: "frameskip", label: "Frameskip" },
        { value: "color", label: "Colour correction" },
        { value: "gb.solar", label: "Solar sensor" },
        { value: "lang", label: "Language" },
      ],
    })
    launch.change(LAUNCH_ID.set(MGBA, "add-setting"), "frameskip")
    expect(launch.getSnapshot().status).toBeUndefined()
    const pending = row(launch.getSnapshot().editor, MGBA, "Frameskip")
    expect(pending).toMatchObject({ value: "Not set here", source: "0 from mGBA (default)", scope: "shared", description: "Frames skipped between drawn ones." })
    expect(pending?.interaction).toEqual({ kind: "number", min: 0, max: 10, step: 1, clearable: true })
    const left = row(launch.getSnapshot().editor, MGBA, "Add setting")?.interaction
    expect(left?.kind === "choice" && left.choices).toHaveLength(3)

    // Out of the schema's range: refused on the row, nothing written.
    launch.change(LAUNCH_ID.set(MGBA, "setting:frameskip"), "12")
    expect(launch.getSnapshot().status).toEqual({ _tag: "Problem", settingId: LAUNCH_ID.set(MGBA, "setting:frameskip"), message: "That value isn't allowed here." })
    launch.dismissProblem()
    launch.change(LAUNCH_ID.set(MGBA, "setting:frameskip"), "2")
    await written()
    expect(row(launch.getSnapshot().editor, MGBA, "Frameskip")).toMatchObject({ value: "2", source: "from Game for mGBA" })
    expect(row(launch.getSnapshot().editor, MGBA, "Frameskip")?.interaction).toMatchObject({ kind: "number", value: 2 })

    launch.change(LAUNCH_ID.set(MGBA, "add-setting"), "gb.solar")
    launch.change(LAUNCH_ID.set(MGBA, "setting:gb.solar"), JSON.stringify("high"))
    await written()
    expect(row(launch.getSnapshot().editor, MGBA, "Solar sensor")).toMatchObject({ value: "high", source: "from Game for mGBA" })

    launch.change(LAUNCH_ID.set(MGBA, "add-setting"), "lang")
    expect(row(launch.getSnapshot().editor, MGBA, "Language")?.scope).toBe("device")
    launch.change(LAUNCH_ID.set(MGBA, "add-setting"), "color")
    // Inherit on a setting never written takes it away again.
    launch.change(LAUNCH_ID.set(MGBA, "setting:color"), "inherit")
    expect(launch.getSnapshot().status).toBeUndefined()
    expect(row(launch.getSnapshot().editor, MGBA, "Colour correction")).toBeUndefined()
    expect(row(launch.getSnapshot().editor, MGBA, "Language")?.value).toBe("Not set here")

    // Clearing a written number removes it.
    launch.change(LAUNCH_ID.set(MGBA, "setting:frameskip"), "")
    await written()
    expect(row(launch.getSnapshot().editor, MGBA, "Frameskip")).toBeUndefined()
  })

  it("offers a family place every installed runner's settings in that family", async () => {
    const launch = await opened(base)
    const add = row(launch.getSnapshot().editor, "game:wario/family:retroarch", "Add setting")?.interaction
    expect(add?.kind === "choice" && add.choices.map(choice => choice.value)).toEqual(["frameskip", "color", "gb.solar", "lang", "bios"])
    expect(row(launch.getSnapshot().editor, "game:wario/any", "Add setting")).toBeUndefined()
  })

  it("shows file-only values read-only, with source and device-only scope", async () => {
    const launch = await opened({
      ...base,
      bodies: {
        ...chosen,
        "game:wario/any": {
          runner: "retroarch/mgba",
          env: { SDL_AUDIODRIVER: "pipewire", MESA_DEBUG: null },
          args: { prepend: ["gamemoderun"], append: ["--verbose"] },
          cwd: "/srv/wario",
          hooks: { before: [{ run: "fan quiet" }], after: [{ run: "fan auto" }] },
        },
      },
    })
    const editor = launch.getSnapshot().editor
    if (editor._tag !== "Ready") throw new Error(editor._tag)
    const files = editor.places.find(place => place.id === "game:wario/any")?.groups.find(group => group.title === "From files")?.items
    expect(files?.map(item => [item.label, item.value])).toEqual([
      ["Environment SDL_AUDIODRIVER", "pipewire"],
      ["Environment MESA_DEBUG", "Unset"],
      ["Working folder", "/srv/wario"],
      ["Arguments before", "gamemoderun"],
      ["Arguments after", "--verbose"],
      ["Before launch", "fan quiet"],
      ["After the session", "fan auto"],
    ])
    for (const item of files ?? []) {
      expect(item.interaction).toBeUndefined()
      expect(item).toMatchObject({ scope: "device", description: "Set in library.yaml. Change it there." })
    }
    // Plain file values name their place; a list's parts also say they are in force.
    expect(files?.map(item => item.source)).toEqual([
      "from Game", "from Game", "from Game", "In force: from Game", "In force: from Game", "In force: from Game", "In force: from Game",
    ])
    expect(row(launch.getSnapshot().editor, MGBA, "Environment MGBA_AUDIO")).toMatchObject({ value: "pulse", source: "from mGBA (default)" })
    expect(row(launch.getSnapshot().editor, MGBA, "Arguments")).toMatchObject({ value: "-f" })
  })

  it("shows each place only its own parts of a list kind, its routines first, marked in force or not", async () => {
    const launch = await opened({
      ...base,
      routines: { "night-mode": { before: [{ run: "brightnessctl set 30%" }, { run: "fan quiet" }], after: [{ run: "brightnessctl set 100%" }] } },
      bodies: {
        ...chosen,
        "device/any": { hooks: { use: ["night-mode"], before: [{ run: "sync" }] } },
        "game:wario/any": { runner: "retroarch/mgba", hooks: { before: [{ run: "notify-send 'Wario Land 4'" }] } },
        "game:wario/runner:retroarch/gpsp": { hooks: { use: ["night-mode"], before: [{ run: "gpsp-bios-check" }] } },
        "profile:quiet/any": { args: { append: ["--quiet"] } },
      },
      profiles: { wario: [{ id: "quiet", title: "Quiet" }] },
    })
    const editor = launch.getSnapshot().editor
    const files = (placeId: string) => rows(editor, placeId)
      .filter(item => item.id.includes("#file:"))
      .map(({ label, value, source, description }) => ({ label, value, source, description }))
    const DEVICE = "Set in device.yaml. Change it there."
    const LIBRARY = "Set in library.yaml. Change it there."
    expect(files("device/any")).toEqual([
      { label: "Before launch", value: "brightnessctl set 30%; fan quiet", source: "In force: routine night-mode, used by Device", description: DEVICE },
      { label: "Before launch", value: "sync", source: "In force: from Device", description: DEVICE },
      { label: "After the session", value: "brightnessctl set 100%", source: "In force: routine night-mode, used by Device", description: DEVICE },
    ])
    // The Game shows its own hook only, not the Device's.
    expect(files("game:wario/any")).toEqual([
      { label: "Before launch", value: "notify-send 'Wario Land 4'", source: "In force: from Game", description: LIBRARY },
    ])
    // gpSP is not the runner in use: its own parts and the routines it names.
    expect(files("game:wario/runner:retroarch/gpsp")).toEqual([
      { label: "Before launch", value: "gpsp-bios-check", source: "Not in force: mGBA runs this game", description: LIBRARY },
      { label: "Routine", value: "night-mode", source: "Not in force: mGBA runs this game", description: DEVICE },
    ])
    // A profile not picked: its parts are not in force.
    expect(files("profile:quiet/any")).toEqual([
      { label: "Arguments after", value: "--quiet", source: "Not in force: this profile isn't picked", description: LIBRARY },
    ])
    // Places that add nothing show nothing.
    expect(files("game:wario/runner:retroarch/mgba")).toEqual([])
  })

  it("says when a runner's facts are unavailable, in a note and in its places", async () => {
    const launch = await opened({
      ...base,
      runners: { ...runners, "retroarch/gpsp": { title: "gpSP", family: "retroarch", prepare: { _tag: "Unavailable", payload: "launch.prepare timed out." } } },
    })
    const editor = launch.getSnapshot().editor
    if (editor._tag !== "Ready") throw new Error(editor._tag)
    expect(editor.notes).toEqual(["gpSP facts are unavailable: launch.prepare timed out."])
    expect(row(editor, "game:wario/runner:retroarch/gpsp", "Runner facts unavailable")).toMatchObject({ value: "launch.prepare timed out." })
    expect(row(editor, "game:wario/runner:retroarch/gpsp", "Runner facts unavailable")?.interaction).toBeUndefined()
  })

  it("says when a place names a runner that isn't installed", async () => {
    const launch = await opened({ ...base, bodies: { "system:gba/any": { runner: "retroarch/mgba" }, "game:wario/any": { runner: "retroarch/vba" } } })
    const editor = launch.getSnapshot().editor
    if (editor._tag !== "Ready") throw new Error(editor._tag)
    expect(editor.notes).toEqual(["Game names runner retroarch/vba, which isn't installed. Korri skips it and its runner settings there and uses mGBA."])
    const runner = row(editor, "game:wario/any", "Runner")
    expect(runner?.value).toBe("retroarch/vba (not installed)")
    expect(runner?.description).toBe("This runner isn't installed. Korri skips it and its runner settings here.")
  })

  it("creates, picks, renames and deletes a profile, and remembers the pick for Play", async () => {
    const launch = await opened(base)
    launch.change(LAUNCH_ID.create, "  ")
    expect(launch.getSnapshot().status).toEqual({ _tag: "Problem", settingId: LAUNCH_ID.create, message: "A profile needs a name." })
    launch.dismissProblem()
    launch.change(LAUNCH_ID.create, "Speedrun")
    await written()
    let editor = launch.getSnapshot().editor
    if (editor._tag === "Closed") throw new Error("closed")
    const profile = editor.profiles[0]
    expect(profile?.title).toBe("Speedrun")
    expect(editor.places.at(-1)?.layer.title).toBe("Profile Speedrun")
    expect(launch.getSnapshot().activeProfiles.get("wario")).toBeUndefined()

    launch.change(LAUNCH_ID.pick, profile?.id ?? "")
    await written()
    editor = launch.getSnapshot().editor
    expect(editor._tag !== "Closed" && editor.selectedProfileId).toBe(profile?.id)
    expect(row(editor, "game:wario/any", "Profile at next launch")?.value).toBe("Speedrun")
    expect(launch.getSnapshot().activeProfiles.get("wario")).toBe("Speedrun")

    launch.change(LAUNCH_ID.set(`profile:${profile?.id}/any`, "cpu.max"), "50")
    await written()
    expect(row(launch.getSnapshot().editor, "game:wario/any", "CPU max")?.source).toBe("50 % from Profile Speedrun")

    launch.change(LAUNCH_ID.rename(profile?.id ?? ""), "Race")
    await written()
    expect(row(launch.getSnapshot().editor, "game:wario/any", "Profile at next launch")?.value).toBe("Race")

    expect(launch.act(LAUNCH_ID.remove(profile?.id ?? ""))).toBe(true)
    await written()
    editor = launch.getSnapshot().editor
    expect(editor._tag !== "Closed" && editor.profiles).toEqual([])
    expect(row(editor, "game:wario/any", "Profile at next launch")?.value).toBe("No profile")
    expect(launch.getSnapshot().activeProfiles.get("wario")).toBeUndefined()
  })

  it("reads a game's profile pick once for its page", async () => {
    const korrid = createInMemoryKorridClient({ launchSettings: { ...base, profiles: { wario: [{ id: "p1", title: "Quiet" }] }, selected: { wario: "p1" } } })
    let reads = 0
    const counted = { ...korrid, launchSettingsSnapshot: (gameId?: string) => { reads++; return korrid.launchSettingsSnapshot(gameId) } }
    const launch = createLaunchSettings(counted)
    launch.learn("wario")
    launch.learn("wario")
    await settle()
    expect(launch.getSnapshot().activeProfiles.get("wario")).toBe("Quiet")
    launch.learn("wario")
    expect(reads).toBe(1)
  })

  it("asks for a reload after a write korrid refuses as stale", async () => {
    const launch = await opened({ ...base, failures: { set: { code: "SettingsConflict", message: "changed" } } })
    launch.change(LAUNCH_ID.set("game:wario/any", "cpu.min"), "20")
    await written()
    const editor = launch.getSnapshot().editor
    if (editor._tag !== "Conflict") throw new Error(editor._tag)
    expect(editor.message).toContain("changed elsewhere")
    expect(editor.actions.map(action => action.id)).toEqual([LAUNCH_ID.reload, LAUNCH_ID.cancel])
    launch.act(LAUNCH_ID.reload)
    await settle()
    expect(launch.getSnapshot().editor._tag).toBe("Ready")
  })

  it("shows a failed read with Retry, and closes on the close action", async () => {
    const launch = await opened({ failures: { snapshot: { code: "Unavailable", message: "Korri could not read launch settings." } } })
    const editor = launch.getSnapshot().editor
    expect(editor).toMatchObject({ _tag: "Error", message: "Korri could not read launch settings.", places: [], notes: [] })
    if (editor._tag !== "Error") return
    expect(editor.actions.map(action => action.label)).toEqual(["Retry", "Close"])
    launch.act(LAUNCH_ID.cancel)
    expect(launch.getSnapshot().editor).toEqual({ _tag: "Closed" })
  })

  it("ignores a reply that arrives after the editor closed", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient({ launchSettings: { ...base, delayMs: 5 } }))
    launch.open("wario", "Wario Land 4")
    launch.cancel()
    await new Promise(resolve => setTimeout(resolve, 20))
    expect(launch.getSnapshot().editor).toEqual({ _tag: "Closed" })
  })

  it("without a runner choice says what is missing and offers the chooser", async () => {
    let opened_ = 0
    const launch = await opened({ games: { wario: { runners: ["retroarch/mgba"], routeLayers: { _tag: "NeedsRunnerChoice" } } }, runners }, () => { opened_++ })
    const editor = launch.getSnapshot().editor
    if (editor._tag !== "Ready") throw new Error(editor._tag)
    expect([...new Set(editor.places.map(place => place.layer.title))]).toEqual(["Device", "Game"])
    expect(editor.notes).toEqual(["System and release settings appear once you choose a runner. Values in force leave them out."])
    expect(editor.actions.map(action => action.id)).toEqual([LAUNCH_ID.runner, LAUNCH_ID.reload, LAUNCH_ID.cancel])
    expect(launch.act(LAUNCH_ID.runner)).toBe(true)
    expect(opened_).toBe(1)
  })

  it("does not offer the chooser when the game page has none", async () => {
    const launch = await opened({ games: { wario: { routeLayers: { _tag: "NeedsRunnerChoice" } } } })
    const editor = launch.getSnapshot().editor
    expect(editor._tag !== "Closed" && editor.actions.map(action => action.id)).toEqual([LAUNCH_ID.reload, LAUNCH_ID.cancel])
  })

  it("names why route places are unavailable", async () => {
    const launch = await opened({ games: { wario: { routeLayers: { _tag: "Unavailable", message: "The runner plugin did not load." } } } }, () => undefined)
    const editor = launch.getSnapshot().editor
    if (editor._tag !== "Ready") throw new Error(editor._tag)
    expect(editor.notes).toEqual(["System and release settings aren't available: The runner plugin did not load. Values in force leave them out."])
  })
})

describe("profiles and choices", () => {
  const twoQuiet: InMemoryLaunchSettingsConfig = {
    ...base,
    bodies: { ...chosen, "profile:p2/any": { cpu: { max: 50 } } },
    profiles: { wario: [{ id: "p1", title: "Quiet" }, { id: "p2", title: "Quiet" }] },
    selected: { wario: "p2" },
  }

  it("names a profile pick by id, marks it by value, and tells duplicate titles apart", async () => {
    const launch = await opened(twoQuiet)
    const editor = launch.getSnapshot().editor
    const pick = row(editor, "game:wario/any", "Profile at next launch")
    expect(pick?.interaction).toEqual({
      kind: "choice",
      selected: "p2",
      choices: [{ value: "", label: "No profile" }, { value: "p1", label: "Quiet" }, { value: "p2", label: "Quiet (2)" }],
    })
    expect(pick?.value).toBe("Quiet (2)")
    expect(editor._tag !== "Closed" && [...new Set(editor.places.map(place => place.layer.title))].slice(-2)).toEqual(["Profile Quiet", "Profile Quiet (2)"])
    expect(row(editor, "game:wario/any", "CPU max")?.source).toBe("50 % from Profile Quiet (2)")
    expect(row(editor, "profile:p2/any", "Delete profile")?.interaction).toMatchObject({ confirmation: { title: "Delete Quiet (2)?" } })
    launch.change(LAUNCH_ID.pick, "p1")
    await written()
    expect(row(launch.getSnapshot().editor, "game:wario/any", "Profile at next launch")?.interaction).toMatchObject({ selected: "p1" })
  })

  it("marks every launch choice row by value", async () => {
    const launch = await opened({ ...base, deviceFacts: RG353M_LAUNCH_FACTS, bodies: { ...chosen, "device/any": { cpu: { max: 61, governor: "schedutil" } } } })
    const editor = launch.getSnapshot().editor
    expect(row(editor, "device/any", "CPU max")?.interaction).toMatchObject({ selected: "61" })
    expect(row(editor, "device/any", "CPU min")?.interaction).toMatchObject({ selected: "inherit" })
    expect(row(editor, "device/any", "CPU governor")?.interaction).toMatchObject({ selected: "schedutil" })
  })

  it("offers a 0 % floor on minimums only", async () => {
    for (const facts of [RG353M_LAUNCH_FACTS, undefined]) {
      const launch = await opened({ ...base, ...(facts === undefined ? {} : { deviceFacts: facts }) })
      const editor = launch.getSnapshot().editor
      for (const label of ["CPU min", "GPU min"]) {
        const interaction = row(editor, "game:wario/any", label)?.interaction
        expect(interaction?.kind === "choice" && interaction.choices[1]).toEqual({ value: "0", label: "0 % (no floor)" })
      }
      for (const label of ["CPU max", "GPU max"]) {
        const interaction = row(editor, "game:wario/any", label)?.interaction
        expect(interaction?.kind === "choice" && interaction.choices.some(choice => choice.value === "0")).toBe(false)
      }
    }
  })
})

describe("launch modifiers", () => {
  const modifiers: InMemoryLaunchSettingsConfig["modifiers"] = {
    "korri/gamescope": {
      title: "Gamescope",
      schema: {
        type: "object",
        properties: {
          "output.width": { type: "integer", title: "Output width", minimum: 320, maximum: 7680 },
          filter: { type: "string", title: "Upscale filter", enum: ["linear", "fsr"] },
        },
      },
    },
    "korri/mangohud": { title: "MangoHud" },
  }
  const withModifiers: InMemoryLaunchSettingsConfig = {
    ...base,
    modifiers,
    bodies: {
      ...chosen,
      "device/any": { modifiers: { "korri/gamescope": { enabled: true, settings: { "output.width": 1280 } } } },
      "profile:quiet/any": { modifiers: { "korri/gamescope": { enabled: false } } },
      "game:wario/any": { runner: "retroarch/mgba", modifiers: { "korri/vkbasalt": { enabled: true } } },
    },
    profiles: { wario: [{ id: "quiet", title: "Quiet" }] },
  }
  const group = (editor: SurfaceLaunchSettings, placeId: string, title: string) =>
    editor._tag === "Closed" ? undefined : editor.places.find(place => place.id === placeId)?.groups.find(candidate => candidate.title === title)

  it("lists every installed modifier on every place, inherited with its source", async () => {
    const launch = await opened(withModifiers)
    const editor = launch.getSnapshot().editor
    const gamescope = group(editor, "game:wario/runner:retroarch/mgba", "Modifier Gamescope")
    expect(gamescope?.items[0]).toMatchObject({ label: "Gamescope", value: "Inherit", source: "On from Device", scope: "device" })
    expect(gamescope?.items[0]?.interaction).toEqual({
      kind: "choice",
      selected: "inherit",
      choices: [{ value: "inherit", label: "Inherit" }, { value: "true", label: "On" }, { value: "false", label: "Off" }],
    })
    expect(gamescope?.items.map(item => item.label)).toEqual(["Gamescope", "Add Gamescope setting"])
    const device = group(editor, "device/any", "Modifier Gamescope")
    expect(device?.items.map(item => [item.label, item.value])).toEqual([["Gamescope", "On"], ["Output width", "1280"], ["Add Gamescope setting", "Choose a setting"]])
  })

  it("shows a profile turning a modifier off, and the off in force once the profile is picked", async () => {
    const launch = await opened({ ...withModifiers, selected: { wario: "quiet" } })
    const editor = launch.getSnapshot().editor
    expect(group(editor, "profile:quiet/any", "Modifier Gamescope")?.items[0]).toMatchObject({ value: "Off", source: "from Profile Quiet" })
    expect(group(editor, "game:wario/any", "Modifier Gamescope")?.items[0]).toMatchObject({ value: "Inherit", source: "Off from Profile Quiet" })
  })

  it("gives a modifier with no schema its on/off row and nothing else", async () => {
    const launch = await opened(withModifiers)
    expect(group(launch.getSnapshot().editor, "game:wario/any", "Modifier MangoHud")?.items.map(item => item.label)).toEqual(["MangoHud"])
  })

  it("names a modifier a place sets that isn't installed", async () => {
    const launch = await opened(withModifiers)
    expect(group(launch.getSnapshot().editor, "game:wario/any", "Modifier korri/vkbasalt")?.items[0]).toMatchObject({
      value: "On",
      description: "This modifier isn't installed or is turned off on this device.",
    })
  })

  it("turns a modifier on, adds a setting from its schema, writes it once it has a value, and never splits a dotted key", async () => {
    const launch = await opened(withModifiers)
    const place = "game:wario/any"
    launch.change(LAUNCH_ID.set(place, "modifier:korri/gamescope:enabled"), "false")
    await written()
    expect(group(launch.getSnapshot().editor, place, "Modifier Gamescope")?.items[0]).toMatchObject({ value: "Off", source: "from Game" })
    launch.change(LAUNCH_ID.set(place, "modifier:korri/gamescope:add-setting"), "output.width")
    const pending = LAUNCH_ID.set(place, "modifier:korri/gamescope:setting:output.width")
    expect(row(launch.getSnapshot().editor, place, "Output width")).toMatchObject({ value: "Not set here", source: "1280 from Device", scope: "device" })
    expect(row(launch.getSnapshot().editor, place, "Output width")?.interaction).toEqual({ kind: "number", min: 320, max: 7680, step: 1, clearable: true })
    launch.change(pending, "100")
    expect(launch.getSnapshot().status).toMatchObject({ _tag: "Problem", settingId: pending })
    launch.dismissProblem()
    launch.change(pending, "1920")
    await written()
    expect(row(launch.getSnapshot().editor, place, "Output width")).toMatchObject({ value: "1920", source: "from Game" })
    // A runner setting and a modifier's are pending apart.
    launch.change(LAUNCH_ID.set(place, "modifier:korri/gamescope:add-setting"), "filter")
    launch.change(LAUNCH_ID.set(place, "modifier:korri/gamescope:setting:filter"), "inherit")
    expect(row(launch.getSnapshot().editor, place, "Upscale filter")).toBeUndefined()
  })
})
