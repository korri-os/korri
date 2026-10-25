/**
 * Launch settings as the portal publishes them (clients/portal/src/surface/
 * launch-settings-model.ts), for Wario Land 4 on an RG353M with two installed
 * runners of one family (mGBA and gpSP, RetroArch), Moonlight (a runner
 * that only views streams, never the game's runner) and Sunshine (a launch
 * service). A place is a layer (Device, System gba, Game, Release usa, each
 * Profile) for a runner scope (any runner, Family retroarch, mGBA, gpSP,
 * Moonlight). A plugin's settings come one group per section ("Moonlight ·
 * Video"), each with its own Add setting row. Ids, labels, sources and notes
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
interface Scope { readonly key: string; readonly kind: "any" | "family" | "runner"; readonly label: string; readonly runner?: string; readonly stream?: true }

const ANY: Scope = { key: "any", kind: "any", label: "Any runner" }
const FAMILY: Scope = { key: "family:retroarch", kind: "family", label: "Family retroarch" }
const MGBA: Scope = { key: "runner:retroarch/mgba", kind: "runner", label: "mGBA", runner: "mGBA" }
const GPSP: Scope = { key: "runner:retroarch/gpsp", kind: "runner", label: "gpSP", runner: "gpSP" }
/** Moonlight: its settings apply when this device streams the game. */
const MOONLIGHT: Scope = { key: "runner:@korri:moonlight/moonlight", kind: "runner", label: "Moonlight", runner: "Moonlight", stream: true }
const SCOPES = [ANY, FAMILY, MGBA, GPSP, MOONLIGHT]

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
  /** The Device for mGBA sets an FPS limit above mGBA's maximum: korrid skips it. */
  readonly skipped?: boolean
}

const CPU_MAX_FROM_PROFILE = "Profile Battery saver"

function inForce(world: World, place: string, scope: Scope, value: string, from: string, fromPlace: string): string {
  if (scope.stream === true) return STREAMS_GAME
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

/** Gamescope, on for the device with a width set there and off in the
 * profile Battery saver, its settings by section; MangoHud, which describes
 * no settings. */
function modifierGroups(world: World, layer: Layer, scope: Scope): SurfaceSettingGroup[] {
  const place = placeId(layer, scope)
  const id = (path: string) => launchRowId(place, path)
  const battery = world.selected === BATTERY.id && world.profiles.includes(BATTERY)
  const gamescope = battery
    ? { value: "Off", from: "Profile Battery saver", place: "profile:battery/any" }
    : { value: "On", from: "Device", place: "device/any" }
  const own = place === "device/any" ? "On" : place === "profile:battery/any" ? "Off" : undefined
  const onOff = (selected: string) => ({
    kind: "choice" as const,
    choices: [INHERIT, { value: "true", label: "On" }, { value: "false", label: "Off" }],
    selected,
  })
  return [
    {
      title: "Modifier Gamescope",
      items: [
        {
          id: id("modifier:korri/gamescope:enabled"),
          label: "Gamescope",
          value: own ?? "Inherit",
          description: NEXT,
          source: inForce(world, place, scope, gamescope.value, gamescope.from, gamescope.place),
          scope: "device",
          interaction: onOff(own === "On" ? "true" : own === "Off" ? "false" : "inherit"),
        },
      ],
    },
    ...sectionGroups(id, GAMESCOPE, place === "device/any" ? { "video.output.width": { value: 1280, source: "from Device" } } : {}),
    {
      title: "Modifier MangoHud",
      items: [{
        id: id("modifier:korri/mangohud:enabled"),
        label: "MangoHud",
        value: "Inherit",
        description: NEXT,
        ...(scope.runner !== undefined && scope.runner !== world.inUse ? { source: inForce(world, place, scope, "", "", "") } : {}),
        scope: "device",
        interaction: onOff("inherit"),
      }],
    },
  ]
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

// ---- Plugin settings: one group per section, as the portal publishes them ----

const DEFAULT = { value: "default", label: "Default" }
const KEEP = [INHERIT, DEFAULT]
const STREAMS_GAME = "Applies when this device streams the game"
const STREAMS_A_GAME = "Applies when this device streams a game"

/** One leaf of a plugin's settings, by its dotted setting path. */
interface Leaf {
  readonly path: string
  readonly label: string
  readonly kind: "number" | "choice" | "toggle" | "text"
  readonly choices?: readonly string[]
  readonly min?: number
  readonly max?: number
  readonly description?: string
  readonly device?: boolean
}
interface Section { readonly key: string; readonly title: string; readonly leaves: readonly Leaf[] }

/** Whose settings a run of groups holds, and how its rows are named. */
interface Owner {
  readonly title: string
  readonly sections: readonly Section[]
  readonly rowPath: (path: string) => string
  readonly addPath: (section: string) => string
  /** Every row stays on this device (a modifier's, a service's, Moonlight's). */
  readonly device: boolean
  /** Said before each row's description: when a stream runner's settings apply. */
  readonly note?: string
}

/** A value one place sets: its JSON value, or `null` for Default (a Cleared setting). */
interface Own { readonly value: unknown; readonly source: string }

const valueWords = (value: unknown) => value === true ? "On" : value === false ? "Off" : String(value)

function leafRows(rowId: (path: string) => string, owner: Owner, leaf: Leaf, own: Own): SurfaceSettingItem[] {
  const id = rowId(owner.rowPath(leaf.path))
  const cleared = own.value === null
  const scope = owner.device || leaf.device === true ? "device" as const : "shared" as const
  const description = [owner.note, leaf.description].filter(part => part !== undefined).join(" ")
  const base = {
    id,
    label: leaf.label,
    ...(description === "" ? {} : { description }),
    source: own.source,
    scope,
  }
  const defaultRow = (): SurfaceSettingItem => ({
    id: rowId(`default:${owner.rowPath(leaf.path)}`),
    label: `Default ${leaf.label}`,
    value: "Use",
    description: `${owner.title}'s own default holds here, whatever less specific places set.`,
    interaction: { kind: "action", actionId: rowId(`default:${owner.rowPath(leaf.path)}`) },
  })
  switch (leaf.kind) {
    case "choice":
    case "toggle": {
      const choices = leaf.kind === "toggle"
        ? [{ value: "true", label: "On" }, { value: "false", label: "Off" }]
        : (leaf.choices ?? []).map(choice => ({ value: JSON.stringify(choice), label: choice }))
      const selected = cleared ? "default" : leaf.kind === "toggle" ? String(own.value) : JSON.stringify(own.value)
      return [{ ...base, value: cleared ? "Default" : valueWords(own.value), interaction: { kind: "choice", choices: [...KEEP, ...choices], selected } }]
    }
    case "number":
      return [
        {
          ...base,
          value: cleared ? "Default" : String(own.value),
          interaction: {
            kind: "number",
            ...(typeof own.value === "number" ? { value: own.value } : {}),
            ...(leaf.min === undefined ? {} : { min: leaf.min }),
            ...(leaf.max === undefined ? {} : { max: leaf.max }),
            step: 1,
            clearable: true,
          },
        },
        ...(cleared ? [] : [defaultRow()]),
      ]
    case "text": {
      // A string: its raw text is its value, and Inherit and Default are rows of their own.
      const clear = rowId(`clear:${owner.rowPath(leaf.path)}`)
      return [
        {
          ...base,
          ...(typeof own.value === "string" && own.value !== "" ? { value: own.value } : {}),
          interaction: { kind: "text", placeholder: cleared ? "Default" : "Empty", allowEmpty: true },
        },
        {
          id: clear,
          label: `Inherit ${leaf.label}`,
          value: "Clear",
          description: "Takes the value set here away, so the value in force comes from elsewhere.",
          interaction: { kind: "action", actionId: clear },
        },
        ...(cleared ? [] : [defaultRow()]),
      ]
    }
  }
}

/** One plugin's settings on one place: a group per section, its own values, then its Add setting row. */
function sectionGroups(rowId: (path: string) => string, owner: Owner, own: Readonly<Record<string, Own>>): SurfaceSettingGroup[] {
  return owner.sections.map(section => {
    const set = section.leaves.filter(leaf => own[leaf.path] !== undefined)
    const unset = section.leaves.filter(leaf => own[leaf.path] === undefined)
    const items = set.flatMap(leaf => leafRows(rowId, owner, leaf, own[leaf.path] as Own))
    if (unset.length > 0) {
      items.push({
        id: rowId(owner.addPath(section.key)),
        label: "Add setting",
        value: "Choose a setting",
        description: `${unset.length} more ${unset.length === 1 ? "setting" : "settings"} ${owner.title} offers here.`,
        interaction: { kind: "choice", choices: unset.map(leaf => ({ value: leaf.path, label: leaf.label })), selected: "" },
      })
    }
    return { title: `${owner.title} · ${section.title}`, items }
  })
}

/** mGBA's and gpSP's settings.describe, by section. */
const MGBA_SECTIONS: readonly Section[] = [
  {
    key: "video",
    title: "Video",
    leaves: [
      { path: "video.frameskip", label: "Frameskip", kind: "number", min: 0, max: 10, description: "Frames skipped between drawn ones." },
      { path: "video.color-correction", label: "Colour correction", kind: "toggle" },
      { path: "video.fps-limit", label: "FPS limit", kind: "number", min: 0, max: 240 },
    ],
  },
  { key: "input", title: "Input", leaves: [{ path: "input.solar.level", label: "Solar sensor level", kind: "choice", choices: ["off", "low", "high"] }] },
  { key: "locale", title: "Language", leaves: [{ path: "locale.language", label: "Language", kind: "text", device: true }] },
]
const GPSP_SECTIONS: readonly Section[] = [{ key: "bios", title: "BIOS", leaves: [{ path: "bios.enable", label: "Use BIOS", kind: "toggle" }] }]

/** Moonlight's settings, after docs/briefs/2026-10-25-settings-vocabulary.md. */
const MOONLIGHT_SECTIONS: readonly Section[] = [
  {
    key: "video",
    title: "Video",
    leaves: [
      { path: "video.resolution.width", label: "Resolution width", kind: "number", min: 320, max: 7680 },
      { path: "video.resolution.height", label: "Resolution height", kind: "number", min: 240, max: 4320 },
      { path: "video.fps", label: "FPS", kind: "number", min: 10, max: 240 },
      { path: "video.bitrate-kbps", label: "Bitrate (kbps)", kind: "number", min: 500, max: 150000 },
      { path: "video.codec", label: "Codec", kind: "choice", choices: ["auto", "h264", "h265", "av1"] },
      { path: "video.hdr.enable", label: "HDR", kind: "toggle" },
    ],
  },
  {
    key: "audio",
    title: "Audio",
    leaves: [
      { path: "audio.channels", label: "Channels", kind: "choice", choices: ["stereo", "5.1", "7.1"] },
      { path: "audio.play-on-host", label: "Play on host", kind: "toggle" },
    ],
  },
  { key: "input", title: "Input", leaves: [{ path: "input.mouse-emulation", label: "Mouse emulation", kind: "toggle" }] },
  {
    key: "network",
    title: "Network",
    leaves: [
      { path: "network.remote-optimizations", label: "Remote optimizations", kind: "choice", choices: ["auto", "on", "off"] },
      { path: "network.packet-size-bytes", label: "Packet size (bytes)", kind: "number", min: 256, max: 65536 },
    ],
  },
  { key: "logging", title: "Logging", leaves: [{ path: "logging.level", label: "Level", kind: "choice", choices: ["info", "verbose", "debug"] }] },
]

/** Some of Sunshine's settings, after the same brief. */
const SUNSHINE_SECTIONS: readonly Section[] = [
  {
    key: "video",
    title: "Video",
    leaves: [
      { path: "video.max-bitrate-kbps", label: "Max bitrate (kbps)", kind: "number", min: 0, max: 150000 },
      { path: "video.minimum-fps", label: "Minimum FPS", kind: "number", min: 0, max: 240 },
      { path: "video.encoder.name", label: "Encoder", kind: "choice", choices: ["v4l2m2m", "rkmpp", "software"] },
    ],
  },
  {
    key: "input",
    title: "Input",
    leaves: [
      { path: "input.gamepad", label: "Gamepad", kind: "choice", choices: ["auto", "ds5", "switch", "xone"] },
      { path: "input.mouse", label: "Mouse", kind: "toggle" },
    ],
  },
  { key: "network", title: "Network", leaves: [{ path: "network.lan-encryption", label: "LAN encryption", kind: "choice", choices: ["off", "supported", "required"] }] },
]

const GAMESCOPE_SECTIONS: readonly Section[] = [
  {
    key: "video",
    title: "Video",
    leaves: [
      { path: "video.output.width", label: "Output width", kind: "number", min: 320, max: 7680 },
      { path: "video.output.height", label: "Output height", kind: "number", min: 240, max: 4320 },
      { path: "video.scaling.filter", label: "Scaling filter", kind: "choice", choices: ["linear", "nearest", "fsr", "nis"] },
    ],
  },
  { key: "window", title: "Window", leaves: [{ path: "window.borderless", label: "Borderless", kind: "toggle" }] },
]

const runnerOwner = (title: string, sections: readonly Section[], device = false, note?: string): Owner => ({
  title,
  sections,
  rowPath: path => `setting:${path}`,
  addPath: section => `add-setting:${section}`,
  device,
  ...(note === undefined ? {} : { note }),
})
const SUNSHINE_ID = "@korri:sunshine/sunshine"
const SUNSHINE: Owner = {
  title: "Sunshine",
  sections: SUNSHINE_SECTIONS,
  rowPath: path => `service:${SUNSHINE_ID}:setting:${path}`,
  addPath: section => `service:${SUNSHINE_ID}:add-setting:${section}`,
  device: true,
}
const GAMESCOPE: Owner = {
  title: "Gamescope",
  sections: GAMESCOPE_SECTIONS,
  rowPath: path => `modifier:korri/gamescope:setting:${path}`,
  addPath: section => `modifier:korri/gamescope:add-setting:${section}`,
  device: true,
}

/**
 * A runner place's settings, by section: mGBA's on the Game (a number, an
 * enum and a cleared Colour correction) and on the Device (a string, a
 * switch the Game clears, and, in the `skipped` world, an FPS limit korrid
 * does not apply); Moonlight's on the Device (30 FPS, its codec cleared to
 * Moonlight's default) and the Game (a resolution), sourced from korrid's
 * fold for the stream viewer, each row saying when it applies.
 */
function runnerSettingGroups(world: World, layer: Layer, scope: Scope): SurfaceSettingGroup[] {
  if (scope.kind === "any") return []
  const place = placeId(layer, scope)
  const id = (path: string) => launchRowId(place, path)
  if (scope === MOONLIGHT) {
    const own: Record<string, Own> = place === "device/runner:@korri:moonlight/moonlight"
      ? { "video.fps": { value: 30, source: "from Device for Moonlight" }, "video.codec": { value: null, source: "h264 from Moonlight (default)" } }
      : place === "game:wario/runner:@korri:moonlight/moonlight"
        ? { "video.resolution.width": { value: 854, source: "from Game for Moonlight" }, "video.resolution.height": { value: 480, source: "from Game for Moonlight" } }
        : {}
    return sectionGroups(id, runnerOwner("Moonlight", MOONLIGHT_SECTIONS, true, `${STREAMS_GAME}.`), own)
  }
  const sections = scope === GPSP ? GPSP_SECTIONS : scope.kind === "family" ? [...MGBA_SECTIONS, ...GPSP_SECTIONS] : MGBA_SECTIONS
  const title = scope.kind === "family" ? "Family retroarch" : scope.label
  const here = (value: string) => inForce(world, place, scope, value, placeTitle(layer, scope), place)
  const colour = inForce(world, place, scope, "Off", "mGBA (default)", "")
  const own: Record<string, Own> = place === "game:wario/runner:retroarch/mgba"
    ? {
        "video.frameskip": { value: 2, source: here("2") },
        "video.color-correction": { value: null, source: colour },
        "input.solar.level": { value: "high", source: here("high") },
      }
    : place === "device/runner:retroarch/mgba"
      ? {
          "video.color-correction": { value: true, source: colour },
          ...(world.skipped === true ? { "video.fps-limit": { value: 300, source: "Not applied: 300 is above the maximum of 240" } } : {}),
          "locale.language": { value: "de", source: here("de") },
        }
      : {}
  return sectionGroups(id, runnerOwner(title, sections), own)
}

/** Sunshine on every place: the Device sets its bitrate cap and encoder. */
function sunshineGroups(layer: Layer, scope: Scope, rowId?: (path: string) => string): SurfaceSettingGroup[] {
  const place = placeId(layer, scope)
  const own: Record<string, Own> = place === "device/any"
    ? { "video.max-bitrate-kbps": { value: 3000, source: "from Device" }, "video.encoder.name": { value: "v4l2m2m", source: "from Device" } }
    : {}
  return sectionGroups(rowId ?? (path => launchRowId(place, path)), SUNSHINE, own)
}

/**
 * Values only files set: shown, never changed here. A list kind (arguments,
 * hooks) shows only this place's parts, its routines first, marked In force
 * or Not in force: the Device's routine night-mode and its own hook, the
 * Game's arguments, the Game's hook for the RetroArch family, and gpSP's
 * hook, not in force while mGBA runs.
 */
function fileGroup(world: World, layer: Layer, scope: Scope): SurfaceSettingGroup | undefined {
  const place = placeId(layer, scope)
  const here = placeTitle(layer, scope)
  const describe = `Set in ${layer.file}. Change it there.`
  const DEVICE_FILE = "Set in device.yaml. Change it there."
  const row = (key: string, label: string, value: string, source = `from ${here}`, description = describe): SurfaceSettingItem =>
    ({ id: launchRowId(place, `file:${key}`), label, value, source, scope: "device", description })
  const inForce = scope.runner === undefined ? scope.kind === "any" || world.inUse !== undefined : scope.runner === world.inUse
  const notInForce = world.inUse === undefined ? "Applies once a runner is chosen" : `Not in force: ${world.inUse} runs this game`
  const items: SurfaceSettingItem[] = []
  if (place === "device/any") {
    items.push(
      row("hooks:before:0", "Before launch", "brightnessctl set 30%; systemctl --user start fan-quiet", "In force: routine night-mode, used by Device", DEVICE_FILE),
      row("hooks:before:1", "Before launch", "sync", "In force: from Device"),
      row("hooks:after:0", "After the session", "brightnessctl set 100%", "In force: routine night-mode, used by Device", DEVICE_FILE),
    )
  }
  if (place === "game:wario/any") {
    items.push(
      row("env:SDL_AUDIODRIVER", "Environment SDL_AUDIODRIVER", "pipewire"),
      row("env:MESA_DEBUG", "Environment MESA_DEBUG", "Unset"),
      row("cwd", "Working folder", "/srv/korri/storage/games/gba/Wario_Land_4_(USA,_Europe)_[!]"),
      row("args:append:0", "Arguments after", "--verbose --save-dir=/srv/korri/storage/saves/person/wario-land-4-usa-rev-1", "In force: from Game"),
    )
  }
  if (place === "game:wario/family:retroarch") {
    items.push(inForce
      ? row("hooks:before:0", "Before launch", "notify-send 'Wario Land 4 is starting'", `In force: from ${here}`)
      : row("hooks:before:own", "Before launch", "notify-send 'Wario Land 4 is starting'", notInForce))
  }
  if (place === "game:wario/runner:retroarch/gpsp") {
    items.push(inForce
      ? row("hooks:before:0", "Before launch", "gpsp-bios-check --bios /srv/korri/storage/bios/gba_bios.bin", `In force: from ${here}`)
      : row("hooks:before:own", "Before launch", "gpsp-bios-check --bios /srv/korri/storage/bios/gba_bios.bin", notInForce))
  }
  return items.length === 0 ? undefined : { title: "From files", items }
}

/** What the runner plugin itself sets, from korrid's dry run. */
function runnerFactsGroup(world: World, layer: Layer, scope: Scope): SurfaceSettingGroup | undefined {
  if (scope.kind !== "runner" || scope.stream === true) return undefined
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
      ...runnerSettingGroups(world, layer, scope),
      ...limitGroups(world, layer, scope),
      ...modifierGroups(world, layer, scope),
      ...sunshineGroups(layer, scope),
      layer === GAME && scope === ANY ? profilePickGroup(world) : undefined,
      profile !== undefined && scope === ANY ? profileOwnGroup(world, profile) : undefined,
      fileGroup(world, layer, scope),
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
  /** Battery saver is not picked: Gamescope is on from the Device everywhere else. */
  noProfilePicked: ready({ ...READY, selected: undefined }),
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
  /** The Device for mGBA sets an FPS limit korrid skips: the row and a note say why. */
  skipped: ready({ ...READY, skipped: true }, ["FPS limit (mGBA) isn't applied: 300 is above the maximum of 240."]),
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

/**
 * Device settings' launch groups, as the portal publishes them with RG353M
 * facts: Launch settings (the clock limits), then Moonlight's settings from
 * its Device place, then Sunshine's from the Device.
 */
export const deviceLaunchGroups: readonly SurfaceSettingGroup[] = (() => {
  const [cpu, gpu] = limitGroups({ ...READY, profiles: [], selected: undefined, inUse: undefined }, DEVICE, ANY)
  const items = [...(cpu?.items ?? []), ...(gpu?.items ?? [])].map(item => ({ ...item, id: item.id.replace("launch:set:device/any#", "launch-device:") }))
  const governor = items.find(item => item.label === "CPU governor")
  const deviceRow = (place: string) => (path: string) => `launch-device:set:${place}#${path}`
  const moonlight = sectionGroups(deviceRow("device/runner:@korri:moonlight/moonlight"), runnerOwner("Moonlight", MOONLIGHT_SECTIONS, true, `${STREAMS_A_GAME}.`), {
    "video.fps": { value: 30, source: "from Device for Moonlight" },
    "video.codec": { value: null, source: "h264 from Moonlight (default)" },
  })
  return [
    { title: "Launch settings", items: [...items.filter(item => item !== governor), ...(governor === undefined ? [] : [governor])] },
    ...moonlight,
    ...sunshineGroups(DEVICE, ANY, deviceRow("device/any")),
  ]
})()

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
