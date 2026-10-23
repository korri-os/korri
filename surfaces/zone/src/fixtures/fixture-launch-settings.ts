/**
 * Launch settings as the portal publishes them (clients/portal/src/surface/
 * launch-settings-model.ts), for Wario Land 4 on an RG353M with two installed
 * runners of one family (mGBA and gpSP, RetroArch). A place is a layer
 * (Device, System gba, Game, Release usa, each Profile) for a runner scope
 * (any runner, Family retroarch, mGBA, gpSP). Ids, labels, sources and notes
 * are the portal's own copy; the values are made up.
 */
import type {
  SurfaceLaunchSettings,
  SurfaceLaunchSettingsPlace,
  SurfaceModel,
  SurfaceSettingGroup,
  SurfaceSettingItem,
  SurfaceSettingsStatus,
} from "@contracts/surface/korri-surface"
import { launchPlan, launchScopeStep, ZONE_LAUNCH_SCOPE_ROW } from "../plan/launch-settings"
import { OPTIONS_START, update as optionsUpdate, type ZoneOptions, type ZoneOptionsMessage } from "../state/options"
import type { ZoneRequest } from "../state/requests"

const CPU = ["23 % (0.41 GHz)", "33 % (0.60 GHz)", "45 % (0.82 GHz)", "61 % (1.10 GHz)", "79 % (1.42 GHz)", "89 % (1.61 GHz)", "100 % (1.80 GHz)"]
const GPU = ["25 % (0.20 GHz)", "38 % (0.30 GHz)", "50 % (0.40 GHz)", "75 % (0.60 GHz)", "88 % (0.70 GHz)", "100 % (0.80 GHz)"]
const GUESSED = ["10 %", "20 %", "30 %", "40 %", "50 %", "60 %", "70 %", "80 %", "90 %", "100 %"]
const GOVERNORS = ["conservative", "ondemand", "userspace", "powersave", "performance", "schedutil"]
const NEXT = "Applies at next launch."
const INHERIT = { value: "inherit", label: "Inherit" }

interface Steps { readonly cpu: readonly string[]; readonly gpu: readonly string[]; readonly guessed: boolean; readonly noGpu?: boolean }
const REAL: Steps = { cpu: CPU, gpu: GPU, guessed: false }
const UNKNOWN: Steps = { cpu: GUESSED, gpu: GUESSED, guessed: true }
const NO_GPU: Steps = { cpu: CPU, gpu: [], guessed: false, noGpu: true }

interface Layer { readonly id: string; readonly title: string; readonly file: "device.yaml" | "library.yaml" }
interface Scope { readonly key: string; readonly kind: "any" | "family" | "runner"; readonly label: string; readonly runner?: string }

const ANY: Scope = { key: "any", kind: "any", label: "Any runner" }
const FAMILY: Scope = { key: "family:retroarch", kind: "family", label: "Family retroarch" }
const MGBA: Scope = { key: "runner:retroarch/mgba", kind: "runner", label: "mGBA", runner: "mGBA" }
const GPSP: Scope = { key: "runner:retroarch/gpsp", kind: "runner", label: "gpSP", runner: "gpSP" }
const SCOPES = [ANY, FAMILY, MGBA, GPSP]

const DEVICE: Layer = { id: "device", title: "Device", file: "device.yaml" }
const SYSTEM: Layer = { id: "system:gba", title: "System gba", file: "device.yaml" }
const GAME: Layer = { id: "game:wario", title: "Game", file: "library.yaml" }
const RELEASE: Layer = { id: "release:usa", title: "Release usa", file: "library.yaml" }

export interface FixtureProfile { readonly id: string; readonly title: string }
const BATTERY: FixtureProfile = { id: "battery", title: "Battery saver" }

const placeId = (layer: Layer, scope: Scope) => `${layer.id}/${scope.key}`
const placeTitle = (layer: Layer, scope: Scope) => scope.kind === "any" ? layer.title : `${layer.title} for ${scope.kind === "family" ? "family retroarch" : scope.label}`
/** A row id, as the portal publishes it: opaque to every surface. */
export const launchRowId = (place: string, path: string) => `launch:set:${place}#${path}`

const percentChoices = (labels: readonly string[], floor: boolean) => [
  INHERIT,
  ...(floor ? [{ value: "0", label: "0 % (no floor)" }] : []),
  ...labels.map(label => ({ value: label.split(" ")[0] ?? label, label })),
]

function percentRow(id: string, label: string, own: string | undefined, source: string, steps: Steps, describe: string): SurfaceSettingItem {
  const labels = label.startsWith("CPU") ? steps.cpu : steps.gpu
  return {
    id,
    label,
    value: own ?? "Inherit",
    description: steps.guessed ? `${describe} Clock steps unknown.` : describe,
    source,
    scope: "device",
    interaction: { kind: "choice", choices: percentChoices(labels, label.endsWith("min")), selected: own === undefined ? "inherit" : own.split(" ")[0] ?? own },
  }
}

/** What the Ready fixture holds, place by place. */
interface World {
  readonly steps: Steps
  readonly profiles: readonly FixtureProfile[]
  readonly selected?: string
  /** The runner in use. Absent: none chosen. */
  readonly inUse?: "mGBA" | "gpSP"
  readonly route: boolean
  /** gpSP's dry run failed with this message. */
  readonly gpspFacts?: string
  /** The Game place names this runner, which is not installed. */
  readonly missingRunner?: string
}

const CPU_MAX_FROM_PROFILE = "Profile Battery saver"

function inForce(world: World, place: string, scope: Scope, value: string, from: string, fromPlace: string): string {
  // The cascade's runner-scoped places count only for the runner in use.
  if (scope.runner !== undefined && scope.runner !== world.inUse) {
    return world.inUse === undefined ? "Applies once a runner is chosen" : `Not in force: ${world.inUse} runs this game`
  }
  return fromPlace === place ? `from ${from}` : `${value} from ${from}`
}

function limitGroups(world: World, layer: Layer, scope: Scope): SurfaceSettingGroup[] {
  const place = placeId(layer, scope)
  const steps = world.steps
  const real = !steps.guessed
  const hostWide = layer.id === "device" && scope.kind === "any"
  const describe = hostWide ? `${NEXT} Sets the device's clock limits.` : `${NEXT} Limits the game's session.`
  const battery = world.selected === BATTERY.id && world.profiles.includes(BATTERY)
  const cpuMax = battery
    ? { value: real ? CPU[3] ?? "" : "60 %", from: CPU_MAX_FROM_PROFILE, place: "profile:battery/any" }
    : { value: real ? CPU[5] ?? "" : "90 %", from: "Device", place: "device/any" }
  const cpuMin = world.inUse === "mGBA"
    ? { value: real ? CPU[1] ?? "" : "30 %", from: "Device for mGBA", place: "device/runner:retroarch/mgba" }
    : { value: "0 %", from: "Device (default)", place: "" }
  const gpuMax = { value: real ? GPU[3] ?? "" : "70 %", from: "Game", place: "game:wario/any" }
  // What each place authors itself, whichever value is in force.
  const authored: Record<string, Partial<Record<"cpu.min" | "cpu.max" | "gpu.max", string>>> = {
    "device/any": { "cpu.max": real ? CPU[5] ?? "" : "90 %" },
    "device/runner:retroarch/mgba": { "cpu.min": real ? CPU[1] ?? "" : "30 %" },
    "game:wario/any": { "gpu.max": gpuMax.value },
    "profile:battery/any": { "cpu.max": real ? CPU[3] ?? "" : "60 %" },
  }
  const own = (key: "cpu.min" | "cpu.max" | "gpu.max") => authored[place]?.[key]
  const id = (path: string) => launchRowId(place, path)
  const cpu: SurfaceSettingGroup = {
    title: "CPU",
    items: [
      percentRow(id("cpu.min"), "CPU min", own("cpu.min"), inForce(world, place, scope, cpuMin.value, cpuMin.from, cpuMin.place), steps, describe),
      percentRow(id("cpu.max"), "CPU max", own("cpu.max"), inForce(world, place, scope, cpuMax.value, cpuMax.from, cpuMax.place), steps, describe),
      ...(hostWide ? [{
        id: id("cpu.governor"),
        label: "CPU governor",
        value: "Inherit",
        description: steps.guessed ? `${NEXT} Governors unknown.` : NEXT,
        source: real ? "schedutil from Device (default)" : "from Device (default)",
        scope: "device" as const,
        interaction: { kind: "choice" as const, choices: [INHERIT, ...(real ? GOVERNORS : []).map(name => ({ value: name, label: name }))], selected: "inherit" },
      }] : []),
    ],
  }
  const gpu: SurfaceSettingGroup = {
    title: "GPU",
    items: steps.noGpu === true
      ? [{ id: `${id("gpu.min")}:unavailable`, label: "GPU limits aren't available on this device" }]
      : [
          percentRow(id("gpu.min"), "GPU min", undefined, inForce(world, place, scope, "0 %", "Device (default)", ""), steps, describe),
          percentRow(id("gpu.max"), "GPU max", own("gpu.max"), inForce(world, place, scope, gpuMax.value, gpuMax.from, gpuMax.place), steps, describe),
        ],
  }
  return [cpu, gpu]
}

function runnerGroup(world: World, layer: Layer): SurfaceSettingGroup {
  const place = placeId(layer, ANY)
  const named = layer.id === "game:wario" ? world.missingRunner ?? (world.inUse === undefined ? undefined : world.inUse) : undefined
  const choices = [INHERIT, { value: "retroarch/mgba", label: "mGBA" }, { value: "retroarch/gpsp", label: "gpSP" },
    ...(world.missingRunner !== undefined && layer.id === "game:wario" ? [{ value: world.missingRunner, label: `${world.missingRunner} (not installed)` }] : [])]
  const selected = named === undefined ? "inherit" : named === "mGBA" ? "retroarch/mgba" : named === "gpSP" ? "retroarch/gpsp" : named
  const missing = world.missingRunner !== undefined && layer.id === "game:wario"
  return {
    title: "Runner",
    items: [{
      id: launchRowId(place, "runner"),
      label: "Runner",
      value: choices.find(choice => choice.value === selected)?.label ?? "Inherit",
      description: missing
        ? "This runner isn't installed. Korri skips it and its runner settings here."
        : "The most specific place that names an installed runner wins.",
      ...(world.inUse === undefined ? {} : { source: layer.id === "game:wario" && !missing ? "from Game" : `${world.inUse} from ${missing ? "System gba" : "Game"}` }),
      scope: "shared",
      interaction: { kind: "choice", choices, selected },
    }],
  }
}

/** mGBA's settings.describe, as the portal turns it into rows. */
const MGBA_SETTINGS = [
  { key: "frameskip", title: "Frameskip" },
  { key: "color", title: "Colour correction" },
  { key: "gb.solar", title: "Solar sensor" },
  { key: "lang", title: "Language" },
]

function settingsGroup(world: World, layer: Layer, scope: Scope): SurfaceSettingGroup | undefined {
  if (scope.kind === "any") return undefined
  const place = placeId(layer, scope)
  const id = (path: string) => launchRowId(place, path)
  const offered = scope.label === "gpSP" ? [{ key: "bios", title: "Use BIOS" }] : scope.kind === "family" ? [...MGBA_SETTINGS, { key: "bios", title: "Use BIOS" }] : MGBA_SETTINGS
  const items: SurfaceSettingItem[] = []
  const set = new Set<string>()
  if (layer.id === "game:wario" && scope === MGBA) {
    set.add("frameskip")
    set.add("gb.solar")
    items.push({
      id: id("setting:frameskip"),
      label: "Frameskip",
      value: "2",
      description: "Frames skipped between drawn ones.",
      source: inForce(world, place, scope, "2", "Game for mGBA", place),
      scope: "shared",
      interaction: { kind: "number", value: 2, min: 0, max: 10, step: 1, clearable: true },
    }, {
      id: id("setting:gb.solar"),
      label: "Solar sensor",
      value: "high",
      source: inForce(world, place, scope, "high", "Game for mGBA", place),
      scope: "shared",
      interaction: {
        kind: "choice",
        choices: [INHERIT, { value: "\"off\"", label: "off" }, { value: "\"low\"", label: "low" }, { value: "\"high\"", label: "high" }],
        selected: "\"high\"",
      },
    })
  }
  if (layer.id === "device" && scope === MGBA) {
    set.add("color")
    items.push({
      id: id("setting:color"),
      label: "Colour correction",
      value: "On",
      source: inForce(world, place, scope, "On", "Device for mGBA", place),
      scope: "shared",
      interaction: { kind: "choice", choices: [INHERIT, { value: "true", label: "On" }, { value: "false", label: "Off" }], selected: "true" },
    })
  }
  const unset = offered.filter(setting => !set.has(setting.key))
  if (unset.length > 0) {
    items.push({
      id: id("add-setting"),
      label: "Add setting",
      value: "Choose a setting",
      description: `${unset.length} more ${unset.length === 1 ? "setting" : "settings"} this runner offers.`,
      interaction: { kind: "choice", choices: unset.map(setting => ({ value: setting.key, label: setting.title })), selected: "" },
    })
  }
  return { title: "Runner settings", items }
}

/** Values only files set: shown, never changed here. */
function fileGroup(layer: Layer, scope: Scope): SurfaceSettingGroup | undefined {
  const place = placeId(layer, scope)
  const from = `from ${placeTitle(layer, scope)}`
  const describe = `Set in ${layer.file}. Change it there.`
  const row = (key: string, label: string, value: string): SurfaceSettingItem =>
    ({ id: launchRowId(place, `file:${key}`), label, value, source: from, scope: "device", description: describe })
  if (layer.id === "device" && scope === ANY) return { title: "From files", items: [row("hooks:use", "Routines", "fan-quiet")] }
  if (layer.id === "game:wario" && scope === ANY) {
    return {
      title: "From files",
      items: [
        row("env:SDL_AUDIODRIVER", "Environment SDL_AUDIODRIVER", "pipewire"),
        row("env:MESA_DEBUG", "Environment MESA_DEBUG", "Unset"),
        row("args:append", "Arguments after", "--verbose --save-dir=/srv/korri/storage/saves/person/wario-land-4-usa-rev-1"),
        row("cwd", "Working folder", "/srv/korri/storage/games/gba/Wario_Land_4_(USA,_Europe)_[!]"),
      ],
    }
  }
  if (layer.id === "game:wario" && scope === FAMILY) {
    return { title: "From files", items: [row("hooks:before:0", "Before launch", "notify-send 'Wario Land 4 is starting'")] }
  }
  return undefined
}

/** What the runner plugin itself sets, from korrid's dry run. */
function runnerFactsGroup(world: World, layer: Layer, scope: Scope): SurfaceSettingGroup | undefined {
  if (scope.kind !== "runner") return undefined
  const place = placeId(layer, scope)
  if (scope === GPSP && world.gpspFacts !== undefined) {
    return {
      title: "From the runner",
      items: [{
        id: launchRowId(place, "runner-facts:unavailable"),
        label: "Runner facts unavailable",
        value: world.gpspFacts,
        description: "Korri could not ask gpSP what it sets. Its environment, arguments and folder are not shown.",
      }],
    }
  }
  const from = `from ${scope.label} (default)`
  return {
    title: "From the runner",
    items: scope === MGBA
      ? [
          { id: launchRowId(place, "runner-facts:env:MGBA_AUDIO"), label: "Environment MGBA_AUDIO", value: "pulse", source: from, scope: "device" },
          { id: launchRowId(place, "runner-facts:args"), label: "Arguments", value: "-L mgba_libretro.so", source: from, scope: "device" },
        ]
      : [{ id: launchRowId(place, "runner-facts:args"), label: "Arguments", value: "-L gpsp_libretro.so", source: from, scope: "device" }],
  }
}

function profilePickGroup(world: World): SurfaceSettingGroup {
  const selected = world.profiles.find(profile => profile.id === world.selected)
  return {
    title: "Profile",
    items: [
      {
        id: "launch:profile:pick",
        label: "Profile at next launch",
        value: selected?.title ?? "No profile",
        description: "Remembered for this game on this device. Applies at next launch.",
        interaction: {
          kind: "choice",
          choices: [{ value: "", label: "No profile" }, ...world.profiles.map(profile => ({ value: profile.id, label: profile.title }))],
          selected: selected?.id ?? "",
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

function profileOwnGroup(world: World, profile: FixtureProfile): SurfaceSettingGroup {
  return {
    title: "Profile",
    items: [
      {
        id: `launch:profile:rename:${profile.id}`,
        label: "Name",
        value: profile.title,
        description: profile.id === world.selected ? "Used at next launch." : "Not used at next launch.",
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
  }
}

function placesOf(world: World): readonly SurfaceLaunchSettingsPlace[] {
  const layers: Layer[] = [
    DEVICE,
    ...(world.route ? [SYSTEM] : []),
    GAME,
    ...(world.route ? [RELEASE] : []),
    ...world.profiles.map(profile => ({ id: `profile:${profile.id}`, title: `Profile ${profile.title}`, file: "library.yaml" as const })),
  ]
  const scopes = world.route ? SCOPES : [ANY]
  return layers.flatMap(layer => scopes.map((scope): SurfaceLaunchSettingsPlace => {
    const profile = world.profiles.find(candidate => `profile:${candidate.id}` === layer.id)
    const groups = [
      scope === ANY ? runnerGroup(world, layer) : undefined,
      settingsGroup(world, layer, scope),
      ...limitGroups(world, layer, scope),
      layer === GAME && scope === ANY ? profilePickGroup(world) : undefined,
      profile !== undefined && scope === ANY ? profileOwnGroup(world, profile) : undefined,
      fileGroup(layer, scope),
      runnerFactsGroup(world, layer, scope),
    ].filter((group): group is SurfaceSettingGroup => group !== undefined)
    return {
      id: placeId(layer, scope),
      title: placeTitle(layer, scope),
      layer: { id: layer.id, title: layer.title },
      scope: { kind: scope.kind, label: scope.label },
      groups,
    }
  }))
}

export const LAUNCH_CLOSE = { id: "launch:cancel", label: "Close", enabled: true }
export const LAUNCH_RELOAD = { id: "launch:reload", label: "Reload", enabled: true }
export const LAUNCH_RETRY = { id: "launch:retry", label: "Retry", enabled: true }
export const LAUNCH_RUNNER = { id: "launch:runner", label: "Choose a runner", enabled: true }

const READY: World = { steps: REAL, profiles: [BATTERY], selected: BATTERY.id, inUse: "mGBA", route: true }

const content = (world: World, notes: readonly string[] = []) => ({
  gameTitle: "Wario Land 4",
  places: placesOf(world),
  profiles: world.profiles,
  ...(world.selected === undefined ? {} : { selectedProfileId: world.selected }),
  notes,
})

const ready = (world: World, notes?: readonly string[]) => ({ _tag: "Ready" as const, ...content(world, notes), actions: [LAUNCH_RELOAD, LAUNCH_CLOSE] })

export const launchSettingsFixtures = {
  loading: { _tag: "Loading", gameTitle: "Wario Land 4", places: [], profiles: [], notes: [], actions: [LAUNCH_CLOSE] },
  ready: ready(READY),
  noProfiles: ready({ ...READY, profiles: [], selected: undefined }),
  manyProfiles: ready({
    ...READY,
    profiles: [
      BATTERY,
      { id: "race", title: "Race" },
      { id: "long", title: "Speedrun any percent glitchless no-major-skips" },
      { id: "word", title: "Superextraordinarilyuncompromisingprofile" },
    ],
  }),
  needsRunner: {
    _tag: "Ready",
    ...content({ ...READY, inUse: undefined, route: false }, ["System and release settings appear once you choose a runner. Values in force leave them out."]),
    actions: [LAUNCH_RUNNER, LAUNCH_RELOAD, LAUNCH_CLOSE],
  },
  routeUnavailable: ready({ ...READY, inUse: undefined, route: false }, [
    "System and release settings aren't available: The runner plugin did not load. Values in force leave them out.",
  ]),
  runnerUnavailable: ready({ ...READY, missingRunner: "retroarch/vba-m" }, [
    "Game names runner retroarch/vba-m, which isn't installed. Korri skips it and its runner settings there and uses mGBA.",
  ]),
  factsUnavailable: ready({ ...READY, gpspFacts: "launch.prepare timed out after 2 s." }, [
    "gpSP facts are unavailable: launch.prepare timed out after 2 s.",
  ]),
  noGpu: ready({ ...READY, steps: NO_GPU }),
  factsMissing: ready({ ...READY, steps: UNKNOWN }),
  conflict: {
    _tag: "Conflict",
    message: "Launch settings changed elsewhere. Reload to see them before you change them.",
    ...content(READY),
    actions: [LAUNCH_RELOAD, LAUNCH_CLOSE],
  },
  error: {
    _tag: "Error",
    message: "Korri could not read launch settings.",
    gameTitle: "Wario Land 4",
    places: [],
    profiles: [],
    notes: [],
    actions: [LAUNCH_RETRY, LAUNCH_CLOSE],
  },
} satisfies Record<string, Exclude<SurfaceLaunchSettings, { _tag: "Closed" }>>

/** The index of a layer in the editor, for a state that opens on it. */
export function launchLayerIndex(launch: SurfaceLaunchSettings, layerId: string): number {
  if (launch._tag === "Closed") return 0
  const layers = launch.places.map(place => place.layer.id).filter((id, index, all) => all.indexOf(id) === index)
  return Math.max(0, layers.indexOf(layerId))
}

/** The index of a scope within a layer, for a state that opens on it. */
export function launchScopeIndex(launch: SurfaceLaunchSettings, layerId: string, label: string): number {
  if (launch._tag === "Closed") return 0
  return Math.max(0, launch.places.filter(place => place.layer.id === layerId).findIndex(place => place.scope.label === label))
}

/** Device settings' Launch settings group, as the portal publishes it with RG353M facts. */
export const deviceLaunchGroup: SurfaceSettingGroup = {
  title: "Launch settings",
  items: (() => {
    const [cpu, gpu] = limitGroups({ ...READY, profiles: [], selected: undefined, inUse: undefined }, DEVICE, ANY)
    const items = [...(cpu?.items ?? []), ...(gpu?.items ?? [])].map(item => ({ ...item, id: item.id.replace("launch:set:device/any#", "launch-device:") }))
    const governor = items.find(item => item.label === "CPU governor")
    return [...items.filter(item => item !== governor), ...(governor === undefined ? [] : [governor])]
  })(),
}

/** A model with the editor open, for a whole-surface test. */
export const withLaunchSettings = (model: SurfaceModel, launch: SurfaceLaunchSettings): SurfaceModel => ({ ...model, launchSettings: launch })

// ---- A live editor for a part: OPTIONS' update on the layers, as the surface runs it ----

/** A message a part's script sends the live editor, in OPTIONS' words. */
export type LaunchScript = ZoneOptionsMessage

export interface LaunchProgram {
  readonly subscribe: (listener: () => void) => () => void
  readonly get: () => {
    readonly launch: Exclude<SurfaceLaunchSettings, { _tag: "Closed" }>
    readonly state: ZoneOptions
    readonly action: string | undefined
    readonly scope: number
  }
  readonly dispatch: (message: ZoneOptionsMessage) => void
  readonly focusAction: (actionId: string) => void
}

export function createLaunchProgram(
  launch: Exclude<SurfaceLaunchSettings, { _tag: "Closed" }>,
  status: SurfaceSettingsStatus = { _tag: "Idle" },
  script: readonly ZoneOptionsMessage[] = [],
  perform?: (request: ZoneRequest) => void,
  scope = 0,
): LaunchProgram {
  const slicesFor = (shown: number) => launchPlan(launch, status, shown).slices
  let slices = slicesFor(scope)
  let current = { launch, state: optionsUpdate(OPTIONS_START, { _tag: "KorriPublished" }, slices).model, action: undefined as string | undefined, scope }
  for (const message of script) current = { ...current, state: optionsUpdate(current.state, message, slices).model }
  const listeners = new Set<() => void>()
  const notify = () => {
    for (const listener of [...listeners]) listener()
  }
  return {
    subscribe(listener) {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    get: () => current,
    dispatch(message) {
      // Zone's own RUNNER SCOPE row changes the scope shown, as the surface does.
      if ((message._tag === "SteppedRow" || message._tag === "PressedRow") && message.rowId === ZONE_LAUNCH_SCOPE_ROW) {
        const count = launchPlan(launch, status, current.scope).scopes.length
        const next = launchScopeStep(count, current.scope, message._tag === "SteppedRow" ? message.direction : 1)
        slices = slicesFor(next)
        current = { ...current, scope: next, state: { ...current.state, focused: ZONE_LAUNCH_SCOPE_ROW, at: 0, pick: undefined } }
        notify()
        return
      }
      const step = optionsUpdate(current.state, message, slices)
      for (const request of step.requests) perform?.(request)
      current = { ...current, state: step.model, ...(message._tag === "FocusedRow" ? { action: undefined } : {}) }
      notify()
    },
    focusAction(actionId) {
      current = { ...current, action: actionId }
      notify()
    },
  }
}
