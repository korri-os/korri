import { describe, expect, it } from "bun:test"
import type { SurfaceLaunchSettings, SurfaceSettingItem } from "@contracts/surface/korri-surface"
import { createInMemoryKorridClient } from "../korrid/client"
import { type InMemoryLaunchSettingsConfig, RG353M_LAUNCH_FACTS } from "../korrid/in-memory-launch-settings"
import { createLaunchSettings } from "./launch-settings"
import { cpuSteps, gpuSteps, governors, LAUNCH_ID } from "./launch-settings-model"

const settle = () => new Promise(resolve => setTimeout(resolve, 0))
const written = async () => { await settle(); await settle(); await settle() }

const section = (title: string, properties: Record<string, unknown>) => ({ type: "object", title, additionalProperties: false, properties })
const MGBA_SCHEMA = {
  type: "object",
  additionalProperties: false,
  properties: {
    video: section("Video", {
      frameskip: { type: "integer", title: "Frameskip", description: "Frames skipped between drawn ones.", minimum: 0, maximum: 10, default: 0 },
      "color-correction": { type: "boolean", title: "Colour correction" },
    }),
    // A leaf two objects down: labelled from both titles.
    input: section("Input", { solar: section("Solar sensor", { level: { type: "string", title: "Level", enum: ["off", "low", "high"] } }) }),
    locale: section("Language", { language: { type: "string", title: "Language", "x-korri-scope": "device" } }),
  },
}
const runners: InMemoryLaunchSettingsConfig["runners"] = {
  "retroarch/mgba": { title: "mGBA", family: "retroarch", schema: MGBA_SCHEMA, prepare: { _tag: "Ready", payload: { env: { MGBA_AUDIO: "pulse" }, args: ["-f"] } } },
  "retroarch/gpsp": { title: "gpSP", family: "retroarch", schema: { type: "object", properties: { bios: section("BIOS", { enable: { type: "boolean", title: "Use BIOS" } }) } } },
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
const groupOf = (editor: SurfaceLaunchSettings, placeId: string, title: string) =>
  editor._tag === "Closed" ? undefined : editor.places.find(place => place.id === placeId)?.groups.find(candidate => candidate.title === title)
const titles = (editor: SurfaceLaunchSettings, placeId: string) =>
  editor._tag === "Closed" ? [] : editor.places.find(place => place.id === placeId)?.groups.map(group => group.title) ?? []

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
    const group = launch.getSnapshot().deviceGroups?.[0]
    expect(group?.title).toBe("Launch settings")
    expect(group?.items.map(item => item.label)).toEqual(["CPU min", "CPU max", "GPU min", "GPU max", "CPU governor"])
    expect(group?.items[1]).toMatchObject({ value: "Inherit", source: "100 % (1.80 GHz) from Device (default)", scope: "device" })
    expect(group?.items[1]?.description).toBe("Applies at next launch. Sets the device's clock limits.")
    expect(group?.items[4]).toMatchObject({ value: "Inherit", source: "schedutil from Device (default)" })

    launch.change(LAUNCH_ID.device("cpu.max"), "45")
    expect(launch.getSnapshot().status).toEqual({ _tag: "Saving", settingId: LAUNCH_ID.device("cpu.max") })
    await written()
    expect(launch.getSnapshot().status).toBeUndefined()
    expect(launch.getSnapshot().deviceGroups?.[0]?.items[1]).toMatchObject({ value: "45 % (0.82 GHz)", source: "from Device" })
  })

  it("show one read-only GPU line on a device without a devfreq GPU", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient({
      launchSettings: { deviceFacts: { ...RG353M_LAUNCH_FACTS, gpuDevfreq: [] } },
    }))
    await launch.refreshDevice()
    const items = launch.getSnapshot().deviceGroups?.[0]?.items ?? []
    expect(items.map(item => item.label)).toEqual(["CPU min", "CPU max", "GPU limits aren't available on this device", "CPU governor"])
    expect(items[2]?.interaction).toBeUndefined()
  })

  it("mark steps unknown when the device states no facts", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient())
    await launch.refreshDevice()
    expect(launch.getSnapshot().deviceGroups?.[0]?.items[0]?.description).toBe("Applies at next launch. Sets the device's clock limits. Clock steps unknown.")
    expect(launch.getSnapshot().deviceGroups?.[0]?.items[4]?.description).toBe("Applies at next launch. Governors unknown.")
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
    expect(launch.getSnapshot().deviceGroups?.[0]?.items).toEqual([{
      id: LAUNCH_ID.deviceRetry,
      label: "Launch settings could not be read",
      description: "Korri could not read launch settings.",
      interaction: { kind: "action", actionId: LAUNCH_ID.deviceRetry },
    }])
    fail = false
    expect(launch.act(LAUNCH_ID.deviceRetry)).toBe(true)
    await written()
    expect(launch.getSnapshot().deviceGroups?.[0]?.items.map(item => item.label)).toEqual(["CPU min", "CPU max", "GPU min", "GPU max", "CPU governor"])
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

  it("groups a runner's settings by section, each with its own Add setting row", async () => {
    const launch = await opened(base)
    const editor = launch.getSnapshot().editor
    expect(titles(editor, MGBA).slice(0, 3)).toEqual(["mGBA · Video", "mGBA · Input", "mGBA · Language"])
    expect(groupOf(editor, MGBA, "mGBA · Video")?.items).toEqual([{
      id: LAUNCH_ID.set(MGBA, "add-setting:video"),
      label: "Add setting",
      value: "Choose a setting",
      description: "2 more settings mGBA offers here.",
      interaction: { kind: "choice", selected: "", choices: [{ value: "video.frameskip", label: "Frameskip" }, { value: "video.color-correction", label: "Colour correction" }] },
    }])
    // A deeper leaf reads from its titles; its value is its dotted path.
    expect(groupOf(editor, MGBA, "mGBA · Input")?.items[0]?.interaction).toMatchObject({ choices: [{ value: "input.solar.level", label: "Solar sensor level" }] })
    // The place for any runner holds no runner settings.
    expect(titles(editor, "game:wario/any").some(title => title.startsWith("mGBA"))).toBe(false)
  })

  it("adds a runner setting from its section, writes it nested by path once it has a value", async () => {
    const korrid = createInMemoryKorridClient({ launchSettings: base })
    const sent: unknown[] = []
    const launch = createLaunchSettings({ ...korrid, setLaunch: request => { sent.push(request); return korrid.setLaunch(request) } })
    launch.open("wario", "Wario Land 4")
    await settle()
    launch.change(LAUNCH_ID.set(MGBA, "add-setting:video"), "video.frameskip")
    expect(launch.getSnapshot().status).toBeUndefined()
    const pending = row(launch.getSnapshot().editor, MGBA, "Frameskip")
    expect(pending).toMatchObject({ value: "Not set here", source: "0 from mGBA (default)", scope: "shared", description: "Frames skipped between drawn ones." })
    expect(pending?.interaction).toEqual({ kind: "number", min: 0, max: 10, step: 1, clearable: true })
    expect(groupOf(launch.getSnapshot().editor, MGBA, "mGBA · Video")?.items.map(item => item.label)).toEqual(["Frameskip", "Default Frameskip", "Add setting"])

    // Out of the schema's range: refused on the row, nothing written.
    const frameskip = LAUNCH_ID.set(MGBA, "setting:video.frameskip")
    launch.change(frameskip, "12")
    expect(launch.getSnapshot().status).toEqual({ _tag: "Problem", settingId: frameskip, message: "That value isn't allowed here." })
    launch.dismissProblem()
    launch.change(frameskip, "2")
    await written()
    expect(sent.at(-1)).toMatchObject({ path: { _tag: "Setting", payload: { key: "video.frameskip" } }, value: 2 })
    expect(sent.at(-1)).not.toHaveProperty("clear")
    expect(row(launch.getSnapshot().editor, MGBA, "Frameskip")).toMatchObject({ value: "2", source: "from Game for mGBA" })
    const snapshot = await korrid.launchSettingsSnapshot("wario")
    expect(snapshot._tag === "Ok" && snapshot.payload.places.find(place => place.title === "Game for mGBA")?.body).toEqual({ settings: { video: { frameskip: 2 } } })

    launch.change(LAUNCH_ID.set(MGBA, "add-setting:input"), "input.solar.level")
    launch.change(LAUNCH_ID.set(MGBA, "setting:input.solar.level"), JSON.stringify("high"))
    await written()
    expect(row(launch.getSnapshot().editor, MGBA, "Solar sensor level")).toMatchObject({ value: "high", source: "from Game for mGBA" })

    launch.change(LAUNCH_ID.set(MGBA, "add-setting:locale"), "locale.language")
    expect(row(launch.getSnapshot().editor, MGBA, "Language")?.scope).toBe("device")
    launch.change(LAUNCH_ID.set(MGBA, "add-setting:video"), "video.color-correction")
    // Inherit on a setting never written takes it away again.
    launch.change(LAUNCH_ID.set(MGBA, "setting:video.color-correction"), "inherit")
    expect(launch.getSnapshot().status).toBeUndefined()
    expect(row(launch.getSnapshot().editor, MGBA, "Colour correction")).toBeUndefined()

    // Clearing a written number takes it away; its empty sections go too.
    launch.change(frameskip, "")
    await written()
    expect(row(launch.getSnapshot().editor, MGBA, "Frameskip")).toBeUndefined()
    const after = await korrid.launchSettingsSnapshot("wario")
    expect(after._tag === "Ok" && after.payload.places.find(place => place.title === "Game for mGBA")?.body).toEqual({ settings: { input: { solar: { level: "high" } } } })
  })

  it("offers a family place every installed runner's settings in that family, by section", async () => {
    const launch = await opened(base)
    const editor = launch.getSnapshot().editor
    const FAMILY = "game:wario/family:retroarch"
    expect(titles(editor, FAMILY).slice(0, 4)).toEqual(["Family retroarch · Video", "Family retroarch · Input", "Family retroarch · Language", "Family retroarch · BIOS"])
    expect(groupOf(editor, FAMILY, "Family retroarch · BIOS")?.items[0]?.interaction).toMatchObject({ choices: [{ value: "bios.enable", label: "Use BIOS" }] })
    expect(row(editor, "game:wario/any", "Add setting")).toBeUndefined()
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
          video: section("Video", {
            output: section("Output", { width: { type: "integer", title: "Width", minimum: 320, maximum: 7680 } }),
            scaling: section("Scaling", { filter: { type: "string", title: "Filter", enum: ["linear", "fsr"] } }),
          }),
          window: section("Window", { borderless: { type: "boolean", title: "Borderless" } }),
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
      "device/any": { modifiers: { "korri/gamescope": { enabled: true, settings: { video: { output: { width: 1280 } } } } } },
      "profile:quiet/any": { modifiers: { "korri/gamescope": { enabled: false } } },
      "game:wario/any": { runner: "retroarch/mgba", modifiers: { "korri/vkbasalt": { enabled: true } } },
    },
    profiles: { wario: [{ id: "quiet", title: "Quiet" }] },
  }
  const group = groupOf

  it("lists every installed modifier on every place, inherited with its source, its settings by section", async () => {
    const launch = await opened(withModifiers)
    const editor = launch.getSnapshot().editor
    const gamescope = group(editor, MGBA, "Modifier Gamescope")
    expect(gamescope?.items).toHaveLength(1)
    expect(gamescope?.items[0]).toMatchObject({ label: "Gamescope", value: "Inherit", source: "On from Device", scope: "device" })
    expect(gamescope?.items[0]?.interaction).toEqual({
      kind: "choice",
      selected: "inherit",
      choices: [{ value: "inherit", label: "Inherit" }, { value: "true", label: "On" }, { value: "false", label: "Off" }],
    })
    expect(titles(editor, MGBA).filter(title => title.includes("Gamescope"))).toEqual(["Modifier Gamescope", "Gamescope · Video", "Gamescope · Window"])
    expect(group(editor, MGBA, "Gamescope · Video")?.items.map(item => item.label)).toEqual(["Add setting"])
    expect(group(editor, "device/any", "Gamescope · Video")?.items.map(item => [item.label, item.value])).toEqual([
      ["Output width", "1280"], ["Default Output width", "Use"], ["Add setting", "Choose a setting"],
    ])
  })

  it("shows a profile turning a modifier off, and the off in force once the profile is picked", async () => {
    const launch = await opened({ ...withModifiers, selected: { wario: "quiet" } })
    const editor = launch.getSnapshot().editor
    expect(group(editor, "profile:quiet/any", "Modifier Gamescope")?.items[0]).toMatchObject({ value: "Off", source: "from Profile Quiet" })
    expect(group(editor, "game:wario/any", "Modifier Gamescope")?.items[0]).toMatchObject({ value: "Inherit", source: "Off from Profile Quiet" })
  })

  it("gives a modifier with no schema its on/off row and nothing else", async () => {
    const launch = await opened(withModifiers)
    const editor = launch.getSnapshot().editor
    expect(group(editor, "game:wario/any", "Modifier MangoHud")?.items.map(item => item.label)).toEqual(["MangoHud"])
    expect(titles(editor, "game:wario/any").filter(title => title.startsWith("MangoHud"))).toEqual([])
  })

  it("names a modifier a place sets that isn't installed", async () => {
    const launch = await opened(withModifiers)
    expect(group(launch.getSnapshot().editor, "game:wario/any", "Modifier korri/vkbasalt")?.items[0]).toMatchObject({
      value: "On",
      description: "This modifier isn't installed or is turned off on this device.",
    })
  })

  it("turns a modifier on, adds a setting from its section, writes it once it has a value", async () => {
    const launch = await opened(withModifiers)
    const place = "game:wario/any"
    launch.change(LAUNCH_ID.set(place, "modifier:korri/gamescope:enabled"), "false")
    await written()
    expect(group(launch.getSnapshot().editor, place, "Modifier Gamescope")?.items[0]).toMatchObject({ value: "Off", source: "from Game" })
    launch.change(LAUNCH_ID.set(place, "modifier:korri/gamescope:add-setting:video"), "video.output.width")
    const pending = LAUNCH_ID.set(place, "modifier:korri/gamescope:setting:video.output.width")
    expect(row(launch.getSnapshot().editor, place, "Output width")).toMatchObject({ value: "Not set here", source: "1280 from Device", scope: "device" })
    expect(row(launch.getSnapshot().editor, place, "Output width")?.interaction).toEqual({ kind: "number", min: 320, max: 7680, step: 1, clearable: true })
    launch.change(pending, "100")
    expect(launch.getSnapshot().status).toMatchObject({ _tag: "Problem", settingId: pending })
    launch.dismissProblem()
    launch.change(pending, "1920")
    await written()
    expect(row(launch.getSnapshot().editor, place, "Output width")).toMatchObject({ value: "1920", source: "from Game" })
    // A runner setting and a modifier's are pending apart.
    launch.change(LAUNCH_ID.set(place, "modifier:korri/gamescope:add-setting:video"), "video.scaling.filter")
    expect(row(launch.getSnapshot().editor, place, "Scaling filter")).toMatchObject({ value: "Not set here" })
    launch.change(LAUNCH_ID.set(place, "modifier:korri/gamescope:setting:video.scaling.filter"), "inherit")
    expect(row(launch.getSnapshot().editor, place, "Scaling filter")).toBeUndefined()
  })
})

describe("string settings", () => {
  const LANG = "setting:locale.language"
  const ADD = LAUNCH_ID.set(MGBA, "add-setting:locale")

  it("hold their raw text, take an empty string as a value, and go back to Inherit through their own row", async () => {
    const launch = await opened(base)
    launch.change(ADD, "locale.language")
    // Pending: no value, the placeholder says so, and the editor starts empty.
    expect(row(launch.getSnapshot().editor, MGBA, "Language")).toMatchObject({ scope: "device" })
    expect(row(launch.getSnapshot().editor, MGBA, "Language")?.value).toBeUndefined()
    expect(row(launch.getSnapshot().editor, MGBA, "Language")?.interaction).toEqual({ kind: "text", placeholder: "Not set here", allowEmpty: true })
    expect(row(launch.getSnapshot().editor, MGBA, "Inherit Language")?.interaction).toEqual({ kind: "action", actionId: LAUNCH_ID.set(MGBA, `clear:${LANG}`) })
    expect(row(launch.getSnapshot().editor, MGBA, "Default Language")?.interaction).toEqual({ kind: "action", actionId: LAUNCH_ID.set(MGBA, `default:${LANG}`) })

    launch.change(LAUNCH_ID.set(MGBA, LANG), "de")
    await written()
    expect(row(launch.getSnapshot().editor, MGBA, "Language")).toMatchObject({ value: "de", source: "from Game for mGBA" })

    // An empty string is a value of its own, not a clear.
    launch.change(LAUNCH_ID.set(MGBA, LANG), "")
    await written()
    const empty = row(launch.getSnapshot().editor, MGBA, "Language")
    expect(empty?.value).toBeUndefined()
    expect(empty?.interaction).toMatchObject({ placeholder: "Empty" })
    expect(empty?.source).toBe("from Game for mGBA")

    // Inherit takes it away: the row goes, Add setting offers it again.
    expect(launch.act(LAUNCH_ID.set(MGBA, `clear:${LANG}`))).toBe(true)
    await written()
    expect(row(launch.getSnapshot().editor, MGBA, "Language")).toBeUndefined()
    const add = groupOf(launch.getSnapshot().editor, MGBA, "mGBA · Language")?.items[0]?.interaction
    expect(add?.kind === "choice" && add.choices.some(choice => choice.value === "locale.language")).toBe(true)
  })

  it("withdraws a pending string through Inherit without writing", async () => {
    const korrid = createInMemoryKorridClient({ launchSettings: base })
    let writes = 0
    const counted = { ...korrid, setLaunch: (request: Parameters<typeof korrid.setLaunch>[0]) => { writes++; return korrid.setLaunch(request) } }
    const launch = createLaunchSettings(counted)
    launch.open("wario", "Wario Land 4")
    await settle()
    launch.change(ADD, "locale.language")
    launch.act(LAUNCH_ID.set(MGBA, `clear:${LANG}`))
    expect(row(launch.getSnapshot().editor, MGBA, "Language")).toBeUndefined()
    expect(writes).toBe(0)
  })

  it("does the same for a modifier's string setting", async () => {
    const launch = await opened({
      ...base,
      modifiers: { "korri/gamescope": { title: "Gamescope", schema: { properties: { backend: section("Backend", { name: { type: "string", title: "Backend" } }) } } } },
      bodies: { ...chosen, "device/any": { modifiers: { "korri/gamescope": { settings: { backend: { name: "wayland" } } } } } },
    })
    expect(row(launch.getSnapshot().editor, "device/any", "Backend")).toMatchObject({ value: "wayland" })
    launch.act(LAUNCH_ID.set("device/any", "clear:modifier:korri/gamescope:setting:backend.name"))
    await written()
    expect(row(launch.getSnapshot().editor, "device/any", "Backend")).toBeUndefined()
  })
})

describe("Default: a cleared setting", () => {
  it("offers Default on every choice row, writes clear: true, and shows Default with the plugin's default as its source", async () => {
    const korrid = createInMemoryKorridClient({ launchSettings: { ...base, bodies: { ...chosen, "device/runner:retroarch/mgba": { settings: { video: { frameskip: 4, "color-correction": true } } } } } })
    const sent: unknown[] = []
    const launch = createLaunchSettings({ ...korrid, setLaunch: request => { sent.push(request); return korrid.setLaunch(request) } })
    launch.open("wario", "Wario Land 4")
    await settle()
    const DEVICE_MGBA = "device/runner:retroarch/mgba"
    const color = row(launch.getSnapshot().editor, DEVICE_MGBA, "Colour correction")
    expect(color?.interaction).toEqual({
      kind: "choice",
      selected: "true",
      choices: [{ value: "inherit", label: "Inherit" }, { value: "default", label: "Default" }, { value: "true", label: "On" }, { value: "false", label: "Off" }],
    })
    launch.change(LAUNCH_ID.set(DEVICE_MGBA, "setting:video.color-correction"), "default")
    await written()
    expect(sent.at(-1)).toEqual({ place: { layer: { _tag: "Device" }, scope: { _tag: "Runner", id: "retroarch/mgba" } }, path: { _tag: "Setting", payload: { key: "video.color-correction" } }, clear: true, expectedRevision: "device-1" })
    // No default in mGBA's schema: mGBA decides.
    expect(row(launch.getSnapshot().editor, DEVICE_MGBA, "Colour correction")).toMatchObject({ value: "Default", source: "mGBA decides" })
    expect(row(launch.getSnapshot().editor, DEVICE_MGBA, "Colour correction")?.interaction).toMatchObject({ selected: "default" })

    // A number's Default is a row of its own; the Game inherits the default.
    expect(launch.act(LAUNCH_ID.set(DEVICE_MGBA, "default:setting:video.frameskip"))).toBe(true)
    await written()
    expect(sent.at(-1)).toMatchObject({ path: { _tag: "Setting", payload: { key: "video.frameskip" } }, clear: true })
    expect(sent.at(-1)).not.toHaveProperty("value")
    const frameskip = row(launch.getSnapshot().editor, DEVICE_MGBA, "Frameskip")
    expect(frameskip).toMatchObject({ value: "Default", source: "0 from mGBA (default)" })
    expect(frameskip?.interaction).toEqual({ kind: "number", min: 0, max: 10, step: 1, clearable: true })
    expect(row(launch.getSnapshot().editor, DEVICE_MGBA, "Default Frameskip")).toBeUndefined()
    const snapshot = await korrid.launchSettingsSnapshot("wario")
    expect(snapshot._tag === "Ok" && snapshot.payload.places.find(place => place.title === "Device for mGBA")?.body).toEqual({ settings: { video: { frameskip: null, "color-correction": null } } })

    // Inherit from Default takes the place's value away: no value, no clear.
    launch.change(LAUNCH_ID.set(DEVICE_MGBA, "setting:video.frameskip"), "")
    await written()
    expect(sent.at(-1)).toEqual({ place: { layer: { _tag: "Device" }, scope: { _tag: "Runner", id: "retroarch/mgba" } }, path: { _tag: "Setting", payload: { key: "video.frameskip" } }, expectedRevision: "device-3" })
  })

  it("clears what less specific places set, so a more specific row names the plugin's default", async () => {
    const launch = await opened({
      ...base,
      bodies: { ...chosen, "device/runner:retroarch/mgba": { settings: { video: { frameskip: 4 } } }, "game:wario/runner:retroarch/mgba": { settings: { video: { frameskip: null } } } },
    })
    const editor = launch.getSnapshot().editor
    expect(row(editor, "device/runner:retroarch/mgba", "Frameskip")).toMatchObject({ value: "4", source: "0 from mGBA (default)" })
    expect(row(editor, MGBA, "Frameskip")).toMatchObject({ value: "Default", source: "0 from mGBA (default)" })
  })
})

describe("settings korrid skips", () => {
  it("say on their rows and in a note that the value is not applied, and why", async () => {
    const launch = await opened({ ...base, bodies: { ...chosen, "device/runner:retroarch/mgba": { settings: { video: { frameskip: 30 } } } } })
    const editor = launch.getSnapshot().editor
    if (editor._tag !== "Ready") throw new Error(editor._tag)
    expect(row(editor, "device/runner:retroarch/mgba", "Frameskip")).toMatchObject({ value: "30", source: "Not applied: 30 is above the maximum of 10" })
    expect(row(editor, "game:wario/any", "Frameskip")).toBeUndefined()
    launch.change(LAUNCH_ID.set(MGBA, "add-setting:video"), "video.frameskip")
    expect(row(launch.getSnapshot().editor, MGBA, "Frameskip")?.source).toBe("30 from Device for mGBA isn't applied: 30 is above the maximum of 10")
    expect(editor.notes).toEqual(["Frameskip (mGBA) isn't applied: 30 is above the maximum of 10."])
  })
})

const MOONLIGHT = "@korri:moonlight/moonlight"
const MOONLIGHT_SCHEMA = {
  type: "object",
  properties: {
    video: section("Video", {
      fps: { type: "integer", title: "FPS", minimum: 10, maximum: 240 },
      codec: { type: "string", title: "Codec", enum: ["auto", "h264", "h265", "av1"], default: "h264" },
      hdr: section("HDR", { enable: { type: "boolean", title: "Enable" } }),
    }),
    audio: section("Audio", { channels: { type: "string", title: "Channels", enum: ["stereo", "5.1", "7.1"] } }),
  },
}
const withMoonlight: InMemoryLaunchSettingsConfig = {
  ...base,
  runners: { ...runners, [MOONLIGHT]: { title: "Moonlight", schema: MOONLIGHT_SCHEMA, choosable: false } },
  bodies: {
    ...chosen,
    [`device/runner:${MOONLIGHT}`]: { settings: { video: { fps: 30, codec: null } } },
    [`game:wario/runner:${MOONLIGHT}`]: { settings: { audio: { channels: "stereo" } } },
  },
}

describe("Moonlight, a runner that only views streams", () => {
  it("is its own scope with groups by section on a game page, its values from korrid's stream fold, and never a Runner choice", async () => {
    const launch = await opened(withMoonlight)
    const editor = launch.getSnapshot().editor
    if (editor._tag !== "Ready") throw new Error(editor._tag)
    const GAME = `game:wario/runner:${MOONLIGHT}`
    expect(editor.places.filter(place => place.layer.id === "game:wario").map(place => place.scope.label)).toEqual(["Any runner", "Family retroarch", "mGBA", "gpSP", "Moonlight"])
    expect(titles(editor, GAME).slice(0, 2)).toEqual(["Moonlight · Video", "Moonlight · Audio"])
    const runner = row(editor, "game:wario/any", "Runner")?.interaction
    expect(runner?.kind === "choice" && runner.choices.map(choice => choice.label)).toEqual(["Inherit", "mGBA", "gpSP"])
    expect(row(editor, GAME, "Channels")).toMatchObject({ value: "stereo", source: "from Game for Moonlight", scope: "device", description: "Applies when this device streams the game." })
    // Its other rows say the same, never "Not in force: mGBA runs this game".
    expect(row(editor, GAME, "CPU max")?.source).toBe("Applies when this device streams the game")
    // The Device's cleared codec names Moonlight's default; HDR reads from its section.
    const DEVICE = `device/runner:${MOONLIGHT}`
    expect(row(editor, DEVICE, "Codec")).toMatchObject({ value: "Default", source: "h264 from Moonlight (default)" })
    expect(row(editor, DEVICE, "FPS")).toMatchObject({ value: "30", source: "from Device for Moonlight" })
    // The Game inherits the Device's 30 through runnerResolved, never through resolved.values.
    launch.change(LAUNCH_ID.set(GAME, "add-setting:video"), "video.fps")
    expect(row(launch.getSnapshot().editor, GAME, "FPS")).toMatchObject({ value: "Not set here", source: "30 from Device for Moonlight" })
    expect(row(launch.getSnapshot().editor, GAME, "Codec")).toBeUndefined()
    launch.change(LAUNCH_ID.set(GAME, "add-setting:video"), "video.codec")
    expect(row(launch.getSnapshot().editor, GAME, "Codec")?.source).toBe("h264 from Moonlight (default)")
    // A leaf no place sets and with no default says when it applies.
    launch.change(LAUNCH_ID.set(GAME, "add-setting:video"), "video.hdr.enable")
    expect(row(launch.getSnapshot().editor, GAME, "HDR")?.source).toBe("Applies when this device streams the game")
    expect(groupOf(editor, DEVICE, "Moonlight · Video")?.items.at(-1)?.interaction).toMatchObject({ choices: [{ value: "video.hdr.enable", label: "HDR" }] })
    // Moonlight never runs the game, so it never asks for a choice either.
    expect(editor.notes).toEqual([])
  })

  it("is offered on the Device settings page, with Add setting and Default", async () => {
    const korrid = createInMemoryKorridClient({ launchSettings: withMoonlight })
    const sent: unknown[] = []
    const launch = createLaunchSettings({ ...korrid, setLaunch: request => { sent.push(request); return korrid.setLaunch(request) } })
    await launch.refreshDevice()
    const groups = () => launch.getSnapshot().deviceGroups ?? []
    expect(groups().map(group => group.title)).toEqual(["Launch settings", "Moonlight · Video", "Moonlight · Audio"])
    const video = groups()[1]
    const DEVICE = `device/runner:${MOONLIGHT}`
    expect(video?.items.map(item => [item.label, item.value, item.source])).toEqual([
      ["FPS", "30", "from Device for Moonlight"],
      ["Default FPS", "Use", undefined],
      ["Codec", "Default", "h264 from Moonlight (default)"],
      ["Add setting", "Choose a setting", undefined],
    ])
    expect(video?.items[0]?.id).toBe(LAUNCH_ID.deviceSet(DEVICE, "setting:video.fps"))

    // Add setting shows the row with nothing written; a choice writes it.
    expect(launch.change(LAUNCH_ID.deviceSet(DEVICE, "add-setting:audio"), "audio.channels")).toBe(true)
    expect(sent).toEqual([])
    expect(groups()[2]?.items.map(item => item.label)).toEqual(["Channels"])
    launch.change(LAUNCH_ID.deviceSet(DEVICE, "setting:audio.channels"), JSON.stringify("5.1"))
    await written()
    expect(sent.at(-1)).toMatchObject({ path: { _tag: "Setting", payload: { key: "audio.channels" } }, value: "5.1" })
    expect(groups()[2]?.items[0]).toMatchObject({ label: "Channels", value: "5.1" })

    // Default on a number writes a Cleared setting.
    expect(launch.act(LAUNCH_ID.deviceSet(DEVICE, "default:setting:video.fps"))).toBe(true)
    await written()
    expect(sent.at(-1)).toMatchObject({ path: { _tag: "Setting", payload: { key: "video.fps" } }, clear: true })
    expect(groups()[1]?.items[0]).toMatchObject({ label: "FPS", value: "Default", source: "Moonlight decides" })
  })

  it("is refused as a game's runner", async () => {
    const launch = await opened(withMoonlight)
    launch.change(LAUNCH_ID.set("game:wario/any", "runner"), MOONLIGHT)
    await written()
    expect(launch.getSnapshot().editor).toMatchObject({ _tag: "Error", message: "@korri:moonlight/moonlight runs no game; it views streams." })
  })
})

describe("Sunshine, a launch service", () => {
  const SUNSHINE = "@korri:sunshine/sunshine"
  const withSunshine: InMemoryLaunchSettingsConfig = {
    ...base,
    services: {
      [SUNSHINE]: {
        title: "Sunshine",
        schema: {
          type: "object",
          properties: {
            video: section("Video", { "max-bitrate-kbps": { type: "integer", title: "Max bitrate (kbps)", minimum: 0 } }),
            input: section("Input", { gamepad: { type: "string", title: "Gamepad", enum: ["auto", "ds5"], default: "auto" } }),
          },
        },
      },
    },
    bodies: { ...chosen, "device/any": { services: { [SUNSHINE]: { settings: { video: { "max-bitrate-kbps": 3000 } } } } } },
  }

  it("has groups by section on every place, device-only, and writes a ServiceSetting", async () => {
    const korrid = createInMemoryKorridClient({ launchSettings: withSunshine })
    const sent: unknown[] = []
    const launch = createLaunchSettings({ ...korrid, setLaunch: request => { sent.push(request); return korrid.setLaunch(request) } })
    launch.open("wario", "Wario Land 4")
    await settle()
    const editor = launch.getSnapshot().editor
    for (const place of ["device/any", "game:wario/any", MGBA]) expect(titles(editor, place).filter(title => title.startsWith("Sunshine"))).toEqual(["Sunshine · Video", "Sunshine · Input"])
    expect(row(editor, "device/any", "Max bitrate (kbps)")).toMatchObject({ value: "3000", source: "from Device", scope: "device" })
    launch.change(LAUNCH_ID.set(MGBA, `service:${SUNSHINE}:add-setting:video`), "video.max-bitrate-kbps")
    expect(row(launch.getSnapshot().editor, MGBA, "Max bitrate (kbps)")).toMatchObject({ value: "Not set here", source: "3000 from Device", scope: "device" })
    launch.change(LAUNCH_ID.set(MGBA, `service:${SUNSHINE}:setting:video.max-bitrate-kbps`), "8000")
    await written()
    expect(sent.at(-1)).toMatchObject({ path: { _tag: "ServiceSetting", payload: { id: SUNSHINE, key: "video.max-bitrate-kbps" } }, value: 8000 })
    expect(row(launch.getSnapshot().editor, "game:wario/any", "Max bitrate (kbps)")).toBeUndefined()
    expect(groupOf(launch.getSnapshot().editor, "game:wario/any", "Sunshine · Video")?.items[0]?.label).toBe("Add setting")

    launch.change(LAUNCH_ID.set("game:wario/any", `service:${SUNSHINE}:add-setting:input`), "input.gamepad")
    launch.change(LAUNCH_ID.set("game:wario/any", `service:${SUNSHINE}:setting:input.gamepad`), "default")
    await written()
    expect(sent.at(-1)).toMatchObject({ path: { _tag: "ServiceSetting", payload: { id: SUNSHINE, key: "input.gamepad" } }, clear: true })
    expect(row(launch.getSnapshot().editor, "game:wario/any", "Gamepad")).toMatchObject({ value: "Default", source: "auto from Sunshine (default)" })
  })

  it("shows on the Device settings page", async () => {
    const launch = createLaunchSettings(createInMemoryKorridClient({ launchSettings: withSunshine }))
    await launch.refreshDevice()
    expect(launch.getSnapshot().deviceGroups?.map(group => group.title)).toEqual(["Launch settings", "Sunshine · Video", "Sunshine · Input"])
  })
})
