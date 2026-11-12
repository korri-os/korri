/**
 * Launch settings as the one list (ADR 0066), written by hand to the surface
 * treaty: korrid does not publish this model yet. Each fixture is one
 * `SurfaceLaunchSettings`; preview/scenarios.ts lays them over the fixture
 * host's model, and the tests read them from here.
 *
 * The rows, their values and their words are the accepted prototype's
 * (branch proto/one-list-launch-settings), the same models Zone's package
 * holds: a package imports nothing from another, so each keeps its copy.
 * Row ids are opaque: a surface sends them back unchanged and reads nothing
 * from them.
 */
import type {
  SurfaceAction,
  SurfaceLaunchSettings,
  SurfaceLaunchSettingsContent,
  SurfaceModel,
  SurfaceSettingGroup,
  SurfaceSettingItem,
} from "../../src/model"

/** The one list's content. `places` is not part of it: no fixture carries one. */
type Content = Omit<SurfaceLaunchSettingsContent, "places">
type Open = Exclude<SurfaceLaunchSettings, { _tag: "Closed" }>

const open = (tag: "Loading" | "Ready", content: Content): SurfaceLaunchSettings => ({ _tag: tag, ...content }) as Open
const failed = (tag: "Conflict" | "Error", message: string, content: Content): SurfaceLaunchSettings => ({ _tag: tag, message, ...content }) as Open

/** Korri's actions, with the ids it publishes today (the close action ends with ":cancel"). */
const RELOAD: SurfaceAction = { id: "launch:reload", label: "Reload", enabled: true }
const RETRY: SurfaceAction = { id: "launch:retry", label: "Retry", enabled: true }
const CLOSE: SurfaceAction = { id: "launch:cancel", label: "Close", enabled: true }

/** Where a value stays, as Korri says it beside the row. */
const ON_AKA = "Stays on aka"
const HERE = "Stays on this device"

interface RowOptions {
  /** The value in force; a choice row's choices, the value among them. */
  readonly choices?: readonly string[]
  readonly source?: string
  /** Why the row cannot change: it then carries no interaction. */
  readonly fixed?: string
  readonly description?: string
}

const slug = (words: string) => words.toLowerCase().replace(/[^a-z0-9]+/g, "-")

function row(section: string, label: string, value: string, options: RowOptions = {}): SurfaceSettingItem {
  const id = `launch:${slug(section)}.${slug(label)}`
  const description = options.fixed ?? options.description
  return {
    id,
    label,
    value,
    ...(description === undefined ? {} : { description }),
    ...(options.source === undefined ? {} : { source: options.source }),
    ...(options.fixed !== undefined || options.choices === undefined
      ? {}
      : { interaction: { kind: "choice" as const, choices: options.choices.map(choice => ({ value: slug(choice), label: choice })), selected: slug(value) } }),
  }
}

/** The id `row` gives a setting, for the changes list. */
export const launchRowId = (section: string, label: string) => `launch:${slug(section)}.${slug(label)}`

const SCALES = ["1x", "2x", "3x", "4x", "5x", "6x"]
const ASPECTS = ["Original", "Fill view", "Stretch", "4:3", "16:9"]
const WINDOWS = ["Windowed", "Borderless", "Exclusive"]
const VOLUMES = ["0%", "25%", "50%", "75%", "100%"]
const CPU_LIMITS = ["25%", "50%", "75%", "100%"]
const GOVERNORS = ["Powersave", "Schedutil", "Performance"]
const ON_OFF = ["Off", "On"]

interface World {
  readonly scale?: string
  readonly aspect?: string
  /** Where the runner's values stay; absent for a game that runs here. */
  readonly holder?: string
  /** The playing device's own stream section. */
  readonly stream?: { readonly frameRate?: string; readonly bitrate?: string }
  /** The long labels and values of the stress set, and more rows than a screen holds. */
  readonly long?: boolean
  /** Every row of the device that runs the game cannot change, for this reason. */
  readonly unreachable?: string
}

function groups(world: World): SurfaceSettingGroup[] {
  const held = world.holder === undefined ? {} : { source: world.holder }
  const locked = world.unreachable === undefined ? {} : { fixed: world.unreachable }
  const mine = (choices: readonly string[]) => ({ choices, ...held, ...locked })
  const video: SurfaceSettingItem[] = [
    row("Video", "Render scale", world.scale ?? "4x", mine(SCALES)),
    row("Video", "Aspect ratio", world.aspect ?? "Fill view", mine(ASPECTS)),
    row("Video", "Window mode", "Borderless", mine(WINDOWS)),
    row("Video", "FPS limit", "Off", { ...held, fixed: world.unreachable ?? "Not supported by WiiCompiled" }),
    ...(world.long === true
      ? [
          row("Video", "Anisotropic filtering", "16x", mine(["Off", "2x", "4x", "8x", "16x"])),
          row("Video", "Texture filter", "Sharp bilinear, integer prescale", mine(["Nearest", "Linear", "Sharp bilinear, integer prescale"])),
          row("Video", "Multisample anti-aliasing", "Off", mine(["Off", "2x", "4x", "8x"])),
        ]
      : []),
  ]
  const stream: SurfaceSettingGroup[] = world.stream === undefined ? [] : [{
    title: "Stream",
    items: [
      row("Stream", "Stream size", "This screen", { choices: ["This screen", "1920×1080", "1280×720"], source: HERE }),
      row("Stream", "Stream frame rate", world.stream.frameRate ?? "60", { choices: ["30", "60", "90", "120"], source: HERE }),
      row("Stream", "Bitrate", world.stream.bitrate ?? "40 Mbps", { choices: ["10 Mbps", "20 Mbps", "40 Mbps", "80 Mbps"], source: HERE }),
      row("Stream", "Codec", "Automatic", { choices: ["Automatic", "H.264", "HEVC", "AV1"], source: HERE }),
    ],
  }]
  return [
    { title: "Video", items: video },
    { title: "Audio", items: [row("Audio", "Volume", "100%", mine(VOLUMES))] },
    ...stream,
    ...korriGroups(world),
    ...(world.long === true
      ? [
          { title: "Language", items: [row("Language", "Language", "Português (Brasil)", mine(["English", "Português (Brasil)", "Deutsch"]))] },
          { title: "Input", items: [row("Input", "Rumble", "On", mine(ON_OFF)), row("Input", "Stick dead zone", "10%", mine(["0%", "5%", "10%", "15%"]))] },
        ]
      : []),
  ]
}

/** The rows Korri owns, whatever the runner declares. */
function korriGroups(world: World, cpu = "100%"): SurfaceSettingGroup[] {
  const held = world.holder === undefined ? {} : { source: world.holder }
  const locked = world.unreachable === undefined ? {} : { fixed: world.unreachable }
  return [
    {
      title: "Performance",
      items: [
        row("Performance", "CPU limit", cpu, { choices: CPU_LIMITS, ...held, ...locked }),
        row("Performance", "CPU governor", "Powersave", { choices: GOVERNORS, ...held, ...locked }),
      ],
    },
    {
      title: "Runs with",
      items: [
        row("Runs with", "Runner", "WiiCompiled", { ...held, fixed: world.unreachable ?? "This game has one runner." }),
        row("Runs with", "Gamescope", "On", { choices: ON_OFF, ...held, ...locked }),
      ],
    },
  ]
}

const KEEP_PLACES = [
  { value: "launch", label: "This launch", says: "The changes end when you stop playing." },
  { value: "game", label: "This game", says: "Used every time you play this game." },
  { value: "device", label: "Every game", says: "Used by every game that has no value of its own." },
  { value: "system:wii", label: "System Wii", says: "Used by every Wii game that has no value of its own." },
  { value: "release:usa", label: "Release USA", says: "Used by this release of the game." },
  { value: "profile:battery", label: "Profile Battery saver", says: "Used while this profile is chosen." },
] as const

/** The keep row for `count` changes, on `place` (This game unless said). */
export function keepRow(count: number, place: (typeof KEEP_PLACES)[number]["value"] = "game"): SurfaceSettingItem {
  const chosen = KEEP_PLACES.find(candidate => candidate.value === place)!
  return {
    id: "launch:keep",
    label: `Keep ${count} ${count === 1 ? "change" : "changes"}`,
    value: chosen.label,
    description: chosen.says,
    interaction: { kind: "choice", choices: KEEP_PLACES.map(({ value, label }) => ({ value, label })), selected: place },
  }
}

const base = (title: string, runsOn: string): Pick<Content, "gameTitle" | "runsOn" | "profiles" | "notes"> => ({ gameTitle: title, runsOn, profiles: [], notes: [] })

const MARIO = "Mario Kart Wii: Retro Rewind"
const ZELDA = "The Legend of Zelda: Echoes of Wisdom"

const changed = (...rows: readonly (readonly [string, string, string])[]) =>
  rows.map(([section, label, was]) => ({ id: launchRowId(section, label), was }))

/** No changes: a game that runs on this device. */
const local = open("Ready", { ...base(MARIO, "Runs here"), groups: groups({}), changes: [], actions: [RELOAD, CLOSE] })

/** Two changes and the keep row. */
const changes = open("Ready", {
  ...base(MARIO, "Runs here"),
  groups: groups({ scale: "2x", aspect: "Original" }),
  changes: changed(["Video", "Render scale", "4x"], ["Video", "Aspect ratio", "Fill view"]),
  keep: keepRow(2),
  actions: [RELOAD, CLOSE],
})

/** A streamed game: the same list, with this device's own Stream section; two changes on the device that runs it. */
const streamed = open("Ready", {
  ...base(MARIO, "Runs on aka"),
  groups: groups({ scale: "2x", aspect: "Original", holder: ON_AKA, stream: {} }),
  changes: changed(["Video", "Render scale", "4x"], ["Video", "Aspect ratio", "Fill view"]),
  keep: keepRow(2),
  actions: [RELOAD, CLOSE],
})

/** A streamed game with no changes. */
const streamedClean = open("Ready", {
  ...base(MARIO, "Runs on aka"),
  groups: groups({ holder: ON_AKA, stream: {} }),
  changes: [],
  actions: [RELOAD, CLOSE],
})

/** The device that runs the game cannot be reached: its rows cannot change, this device's Stream rows still can. */
const unreachable = failed("Error", "aka cannot be reached. Its settings cannot change now.", {
  ...base(MARIO, "Runs on aka"),
  groups: groups({ holder: ON_AKA, stream: { frameRate: "30", bitrate: "20 Mbps" }, unreachable: "aka cannot be reached." }),
  changes: changed(["Stream", "Stream frame rate", "60"], ["Stream", "Bitrate", "40 Mbps"]),
  keep: keepRow(2),
  actions: [RETRY, CLOSE],
})

/** The longest title, a long label, a long value, a list longer than any screen, and many changes. */
const long = open("Ready", {
  ...base(ZELDA, "Runs on aka"),
  groups: groups({ scale: "2x", aspect: "Original", holder: ON_AKA, stream: { frameRate: "120", bitrate: "80 Mbps" }, long: true }),
  changes: changed(
    ["Video", "Render scale", "4x"],
    ["Video", "Aspect ratio", "Fill view"],
    ["Video", "Texture filter", "Nearest"],
    ["Stream", "Stream frame rate", "60"],
    ["Stream", "Bitrate", "40 Mbps"],
    ["Language", "Language", "English"],
    ["Input", "Stick dead zone", "5%"],
  ),
  keep: keepRow(7, "launch"),
  notes: ["FPS limit isn't applied: 300 is above the maximum of 240."],
  actions: [RELOAD, CLOSE],
})

/** A game whose runner declares no settings: only the rows Korri owns. One change. */
const korriOnly = open("Ready", {
  ...base("Celeste Classic", "Runs here"),
  groups: korriGroups({}, "75%"),
  changes: changed(["Performance", "CPU limit", "100%"]),
  keep: keepRow(1),
  actions: [RELOAD, CLOSE],
})

/** Korri gives no group at all: the page shows no fault. */
const empty = open("Ready", { ...base("Celeste Classic", "Runs here"), groups: [], changes: [], actions: [RELOAD, CLOSE] })

const loading = open("Loading", { gameTitle: MARIO, groups: [], changes: [], profiles: [], notes: [], actions: [CLOSE] })

const conflict = failed("Conflict", "Launch settings changed elsewhere. Reload to see them before you change them.", {
  gameTitle: MARIO,
  runsOn: "Runs here",
  groups: [],
  changes: [],
  profiles: [],
  notes: [],
  actions: [RELOAD, CLOSE],
})

/** Korri could not read the settings at all: its message and Retry, no rows. */
const error = failed("Error", "Korri could not read launch settings.", { gameTitle: MARIO, groups: [], changes: [], profiles: [], notes: [], actions: [RETRY, CLOSE] })

export const launchFixtures = { local, changes, streamed, streamedClean, unreachable, long, korriOnly, empty, loading, conflict, error } as const
export type LaunchFixture = keyof typeof launchFixtures

/** A model with the editor open over the catalog. */
export const withLaunchSettings = (model: SurfaceModel, launch: SurfaceLaunchSettings): SurfaceModel => ({ ...model, launchSettings: launch })
