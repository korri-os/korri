/**
 * Launch settings as the portal publishes them (clients/portal/src/surface/
 * launch-settings-model.ts), for Wario Land 4 on an RG353M: the device's real
 * clock steps, a runner value, a game value and one profile. Ids, labels,
 * sources and notes are the portal's own copy; the values are made up.
 */
import type {
  SurfaceLaunchSettings,
  SurfaceLaunchSettingsLayer,
  SurfaceModel,
  SurfaceSettingGroup,
  SurfaceSettingItem,
} from "@contracts/surface/korri-surface"

const CPU = ["23 % (0.41 GHz)", "33 % (0.60 GHz)", "45 % (0.82 GHz)", "61 % (1.10 GHz)", "79 % (1.42 GHz)", "89 % (1.61 GHz)", "100 % (1.80 GHz)"]
const GPU = ["25 % (0.20 GHz)", "38 % (0.30 GHz)", "50 % (0.40 GHz)", "75 % (0.60 GHz)", "88 % (0.70 GHz)", "100 % (0.80 GHz)"]
const GUESSED = ["10 %", "20 %", "30 %", "40 %", "50 %", "60 %", "70 %", "80 %", "90 %", "100 %"]
const GOVERNORS = ["conservative", "ondemand", "userspace", "powersave", "performance", "schedutil"]
const NEXT = "Applies at next launch."

const choices = (labels: readonly string[], floor: boolean) => [
  { value: "inherit", label: "Inherit" },
  ...(floor ? [{ value: "0", label: "0 % (no floor)" }] : []),
  ...labels.map(label => ({ value: label.split(" ")[0] ?? label, label })),
]

interface Steps { readonly cpu: readonly string[]; readonly gpu: readonly string[]; readonly guessed: boolean; readonly noGpu?: boolean }
const REAL: Steps = { cpu: CPU, gpu: GPU, guessed: false }
const UNKNOWN: Steps = { cpu: GUESSED, gpu: GUESSED, guessed: true }
/** Facts present, but no devfreq GPU: one read-only GPU line. */
const NO_GPU: Steps = { cpu: CPU, gpu: [], guessed: false, noGpu: true }

function row(id: string, label: string, value: string | undefined, source: string, labels: readonly string[], guessed: boolean): SurfaceSettingItem {
  // Minimums may be no floor at all; Korri marks the choice in force by value.
  const offered = choices(labels, label.endsWith("min"))
  return {
    id,
    label,
    value: value ?? "Inherit",
    description: guessed ? `${NEXT} Clock steps unknown.` : NEXT,
    source,
    scope: "device",
    interaction: {
      kind: "choice",
      choices: offered,
      selected: offered.find(choice => choice.label === (value ?? "Inherit"))?.value ?? "inherit",
    },
  }
}

interface Values { readonly cpuMin?: string; readonly cpuMax?: string; readonly gpuMin?: string; readonly gpuMax?: string }
interface Sources { readonly cpuMin: string; readonly cpuMax: string; readonly gpuMin: string; readonly gpuMax: string }

function valueGroups(layerId: string, values: Values, sources: Sources, steps: Steps, governor?: { value?: string; source: string }): SurfaceSettingGroup[] {
  const id = (setting: string) => `launch:set:${layerId}:${setting}`
  return [
    {
      title: "CPU",
      items: [
        row(id("cpu.min"), "CPU min", values.cpuMin, sources.cpuMin, steps.cpu, steps.guessed),
        row(id("cpu.max"), "CPU max", values.cpuMax, sources.cpuMax, steps.cpu, steps.guessed),
        ...(governor === undefined ? [] : [{
          id: id("cpu.governor"),
          label: "CPU governor",
          value: governor.value ?? "Inherit",
          description: steps.guessed ? `${NEXT} Governors unknown.` : NEXT,
          source: governor.source,
          scope: "device" as const,
          interaction: {
            kind: "choice" as const,
            choices: [{ value: "inherit", label: "Inherit" }, ...(steps.guessed ? [] : GOVERNORS).map(name => ({ value: name, label: name }))],
            selected: governor.value ?? "inherit",
          },
        }]),
      ],
    },
    {
      title: "GPU",
      items: steps.noGpu === true
        ? [{ id: `${id("gpu.min")}:unavailable`, label: "GPU limits aren't available on this device" }]
        : [
            row(id("gpu.min"), "GPU min", values.gpuMin, sources.gpuMin, steps.gpu, steps.guessed),
            row(id("gpu.max"), "GPU max", values.gpuMax, sources.gpuMax, steps.gpu, steps.guessed),
          ],
    },
  ]
}

export interface FixtureProfile { readonly id: string; readonly title: string }

function profileGroup(profiles: readonly FixtureProfile[], selected?: string): SurfaceSettingGroup {
  return {
    title: "Profile",
    items: [
      {
        id: "launch:profile:pick",
        label: "Profile at next launch",
        value: profiles.find(profile => profile.id === selected)?.title ?? "No profile",
        description: "Remembered for this game. Applies at next launch.",
        interaction: {
          kind: "choice",
          choices: [{ value: "", label: "No profile" }, ...profiles.map(profile => ({ value: profile.id, label: profile.title }))],
          selected: profiles.some(profile => profile.id === selected) ? selected ?? "" : "",
        },
      },
      {
        id: "launch:profile:new",
        label: "New profile",
        description: "Starts with every value inherited.",
        interaction: { kind: "text", placeholder: "Profile name", maxLength: 40 },
      },
    ],
  }
}

function profileLayer(profile: FixtureProfile, selected: boolean, values: Values, sources: Sources, steps: Steps): SurfaceLaunchSettingsLayer {
  const layerId = `profile:${profile.id}`
  return {
    id: layerId,
    title: `Profile ${profile.title}`,
    groups: [
      ...valueGroups(layerId, values, sources, steps),
      {
        title: "Profile",
        items: [
          {
            id: `launch:profile:rename:${profile.id}`,
            label: "Name",
            value: profile.title,
            description: selected ? "Used at next launch." : "Not used at next launch.",
            interaction: { kind: "text", placeholder: "Profile name", maxLength: 40 },
          },
          {
            id: `launch:profile:remove:${profile.id}`,
            label: "Delete profile",
            interaction: {
              kind: "action",
              actionId: `launch:profile:remove:${profile.id}`,
              destructive: true,
              confirmation: {
                title: `Delete ${profile.title}?`,
                message: "Its values go. The game's other launch settings stay.",
                confirmLabel: "Delete profile",
              },
            },
          },
        ],
      },
    ],
  }
}

export const LAUNCH_CLOSE = { id: "launch:cancel", label: "Close", enabled: true }
export const LAUNCH_RELOAD = { id: "launch:reload", label: "Reload", enabled: true }
export const LAUNCH_RETRY = { id: "launch:retry", label: "Retry", enabled: true }
export const LAUNCH_RUNNER = { id: "launch:runner", label: "Choose a runner", enabled: true }

const BATTERY: FixtureProfile = { id: "battery", title: "Battery saver" }

/**
 * Device caps the CPU at 89 %; Runner mGBA sets a 33 % floor; the game caps
 * the GPU at 75 %; the selected profile Battery saver caps the CPU at 61 %.
 * Every other value is inherited from the device's defaults.
 */
function layers(steps: Steps, profiles: readonly FixtureProfile[], selected: string | undefined, route = true): readonly SurfaceLaunchSettingsLayer[] {
  const real = !steps.guessed
  const label = (index: number, list: readonly string[], fallback = "?") => list[index] ?? fallback
  const cpuMax = selected === BATTERY.id && profiles.includes(BATTERY)
    ? { value: real ? label(3, CPU) : "60 %", from: "Profile Battery saver", layer: `profile:${BATTERY.id}` }
    : { value: real ? label(5, CPU) : "90 %", from: "Device", layer: "device" }
  const cpuMin = route
    ? { value: real ? label(1, CPU) : "30 %", from: "Runner mGBA", layer: "runner:retroarch/mgba" }
    : { value: "0 %", from: "Device (default)", layer: "" }
  const gpuMax = { value: real ? label(3, GPU, "75 %") : "70 %", from: "Game", layer: "game" }
  const say = (layerId: string, resolved: { value: string; from: string; layer: string }) =>
    resolved.layer === layerId ? `from ${resolved.from}` : `${resolved.value} from ${resolved.from}`
  const sources = (layerId: string): Sources => ({
    cpuMin: say(layerId, cpuMin),
    cpuMax: say(layerId, cpuMax),
    gpuMin: "0 % from Device (default)",
    gpuMax: say(layerId, gpuMax),
  })
  const all = [
    { id: "device", title: "Device", groups: valueGroups("device", { cpuMax: real ? label(5, CPU) : "90 %" }, sources("device"), steps, { source: real ? "schedutil from Device (default)" : "from Device (default)" }) },
    { id: "system:gba", title: "System gba", groups: valueGroups("system:gba", {}, sources("system:gba"), steps) },
    { id: "runner:retroarch/mgba", title: "Runner mGBA", groups: valueGroups("runner:retroarch/mgba", { cpuMin: cpuMin.value }, sources("runner:retroarch/mgba"), steps) },
    { id: "game", title: "Game", groups: [...valueGroups("game", { gpuMax: gpuMax.value }, sources("game"), steps), profileGroup(profiles, selected)] },
    { id: "release:usa", title: "Release usa", groups: valueGroups("release:usa", {}, sources("release:usa"), steps) },
    ...profiles.map(profile => profileLayer(
      profile,
      profile.id === selected,
      profile.id === BATTERY.id ? { cpuMax: real ? label(3, CPU) : "60 %" } : {},
      sources(`profile:${profile.id}`),
      steps,
    )),
  ]
  // Without the route korrid gives Device, Game and the profiles only.
  return route ? all : all.filter(layer => layer.id === "device" || layer.id === "game" || layer.id.startsWith("profile:"))
}

const content = (steps: Steps, profiles: readonly FixtureProfile[], selected?: string, route = true) => ({
  gameTitle: "Wario Land 4",
  layers: layers(steps, profiles, selected, route),
  profiles,
  ...(selected === undefined ? {} : { selectedProfileId: selected }),
})

export const launchSettingsFixtures = {
  loading: { _tag: "Loading", gameTitle: "Wario Land 4", layers: [], profiles: [], actions: [LAUNCH_CLOSE] },
  ready: { _tag: "Ready", ...content(REAL, [BATTERY], BATTERY.id), actions: [LAUNCH_RELOAD, LAUNCH_CLOSE] },
  noProfiles: { _tag: "Ready", ...content(REAL, []), actions: [LAUNCH_RELOAD, LAUNCH_CLOSE] },
  manyProfiles: {
    _tag: "Ready",
    ...content(REAL, [
      BATTERY,
      { id: "race", title: "Race" },
      { id: "long", title: "Speedrun any percent glitchless no-major-skips" },
      { id: "word", title: "Superextraordinarilyuncompromisingprofile" },
    ], BATTERY.id),
    actions: [LAUNCH_RELOAD, LAUNCH_CLOSE],
  },
  needsRunner: {
    _tag: "Ready",
    ...content(REAL, [BATTERY], BATTERY.id, false),
    note: "System, runner and release settings appear once you choose a runner. Values in force leave them out.",
    actions: [LAUNCH_RUNNER, LAUNCH_RELOAD, LAUNCH_CLOSE],
  },
  routeUnavailable: {
    _tag: "Ready",
    ...content(REAL, [BATTERY], BATTERY.id, false),
    note: "System, runner and release settings aren't available: The runner plugin did not load. Values in force leave them out.",
    actions: [LAUNCH_RELOAD, LAUNCH_CLOSE],
  },
  noGpu: { _tag: "Ready", ...content(NO_GPU, [BATTERY], BATTERY.id), actions: [LAUNCH_RELOAD, LAUNCH_CLOSE] },
  factsMissing: { _tag: "Ready", ...content(UNKNOWN, [BATTERY], BATTERY.id), actions: [LAUNCH_RELOAD, LAUNCH_CLOSE] },
  conflict: {
    _tag: "Conflict",
    message: "Launch settings changed elsewhere. Reload to see them before you change them.",
    ...content(REAL, [BATTERY], BATTERY.id),
    actions: [LAUNCH_RELOAD, LAUNCH_CLOSE],
  },
  error: {
    _tag: "Error",
    message: "Korri could not read launch settings.",
    gameTitle: "Wario Land 4",
    layers: [],
    profiles: [],
    actions: [LAUNCH_RETRY, LAUNCH_CLOSE],
  },
} satisfies Record<string, Exclude<SurfaceLaunchSettings, { _tag: "Closed" }>>

/** The index of a layer in the editor, for a state that opens on it. */
export const launchLayerIndex = (launch: SurfaceLaunchSettings, id: string): number =>
  launch._tag === "Closed" ? 0 : Math.max(0, launch.layers.findIndex(layer => layer.id === id))

/** Device settings' Launch settings group, as the portal publishes it with RG353M facts. */
export const deviceLaunchGroup: SurfaceSettingGroup = {
  title: "Launch settings",
  items: (() => {
    const [cpu, gpu] = valueGroups("device", { cpuMax: CPU[5] }, {
      cpuMin: "0 % from Device (default)",
      cpuMax: "from Device",
      gpuMin: "0 % from Device (default)",
      gpuMax: "100 % (0.80 GHz) from Device (default)",
    }, REAL, { source: "schedutil from Device (default)" })
    const items = [...(cpu?.items ?? []), ...(gpu?.items ?? [])].map(item => ({ ...item, id: item.id.replace("launch:set:device:", "launch-device:") }))
    const governor = items.find(item => item.label === "CPU governor")
    return [...items.filter(item => item !== governor), ...(governor === undefined ? [] : [governor])]
  })(),
}

/** A model with the editor open over the catalog. */
export const withLaunchSettings = (model: SurfaceModel, launch: SurfaceLaunchSettings, savingId?: string): SurfaceModel => ({
  ...model,
  launchSettings: launch,
  ...(savingId === undefined ? {} : { settingsStatus: { _tag: "Saving", settingId: savingId } }),
})
