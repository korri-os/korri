/**
 * Launch settings as settings rows: korrid's snapshot in, surface places out.
 *
 * Pure. Every word a surface shows about a launch setting is decided here:
 * the value or "Inherit", where the value in force comes from, the
 * device-only mark and "Applies at next launch". A place is a layer (Device,
 * System, Game, Release, Profile) limited to a runner scope (any, a family, a
 * runner); ADR 0028 merges them. Percent choices are this device's real
 * clock steps (ADR 0019); without device facts they are 10 % steps, marked
 * as such.
 *
 * Each row the person may change is published with a write: the place and
 * the typed `set` path it names, and how its choice becomes a JSON value.
 * The controller looks a changed row up by id; nothing parses an id.
 */
import type {
  LaunchDeviceFacts,
  LaunchLayer,
  LaunchPath,
  LaunchPlace,
  LaunchPlaceSnapshot,
  LaunchRunnerScope,
  LaunchRunnerSnapshot,
  LaunchSnapshot,
  LaunchSource,
  LaunchSourcedValue,
  LaunchValuePath,
} from "@contracts/generated/korrid"
import type {
  SurfaceLaunchSettingsPlace,
  SurfaceSettingChoice,
  SurfaceSettingGroup,
  SurfaceSettingInteraction,
  SurfaceSettingItem,
} from "@contracts/surface/korri-surface"

/** Choice value for "no value on this place". Never a percent or runner id. */
export const INHERIT = "inherit"
/** The pick's value for "No profile". A profile id is never empty. */
export const NO_PROFILE = ""

export type LimitKey = "cpu.min" | "cpu.max" | "gpu.min" | "gpu.max" | "cpu.governor"

/** Opaque ids the host publishes and routes. Surfaces never parse them. */
export const LAUNCH_ID = {
  /** A Device-settings row; the device page reads its own snapshot. */
  device: (setting: LimitKey) => `launch-device:${setting}`,
  /** An editor row: one value on one place. */
  set: (placeId: string, path: string) => `launch:set:${placeId}#${path}`,
  pick: "launch:profile:pick",
  create: "launch:profile:new",
  rename: (profileId: string) => `launch:profile:rename:${profileId}`,
  remove: (profileId: string) => `launch:profile:remove:${profileId}`,
  reload: "launch:reload",
  /** Reads the device's launch settings again after a failed read. */
  deviceRetry: "launch-device:retry",
  /** Opens the runner chooser for this game, when its page offers one. */
  runner: "launch:runner",
  retry: "launch:retry",
  cancel: "launch:cancel",
} as const

/** The `editableKinds` strings korrid uses, in one place. */
export const KIND = { runner: "runner", cpu: "cpu", gpu: "gpu", modifiers: "modifiers", settings: "settings" } as const

/**
 * A resolved value's path as one comparable string. korrid names each value
 * by a structured path; the dotted key of `resolved.values` is never split,
 * because plugin ids and setting keys may hold dots.
 */
export function valuePathKey(path: LaunchValuePath): string {
  const payload = path._tag === "Editable" ? editablePathKey(path.payload) : path.payload === undefined ? "" : JSON.stringify(Object.entries(path.payload).sort())
  return `${path._tag}\u0000${payload}`
}
const editablePathKey = (path: LaunchPath): string =>
  `${path._tag}\u0000${path.payload === undefined ? "" : JSON.stringify(Object.entries(path.payload).sort())}`
const editable = (path: LaunchPath): LaunchValuePath => ({ _tag: "Editable", payload: path })

const LIMIT_PATH: Record<LimitKey, LaunchPath> = {
  "cpu.min": { _tag: "CpuMin" },
  "cpu.max": { _tag: "CpuMax" },
  "gpu.min": { _tag: "GpuMin" },
  "gpu.max": { _tag: "GpuMax" },
  "cpu.governor": { _tag: "CpuGovernor" },
}

/** What a changed row writes. `encode` turns the surface's text into the
 * value to send: `undefined` clears, `null` refuses the text. */
export interface LaunchWrite {
  readonly place: LaunchPlace
  readonly path: LaunchPath
  readonly encode: (text: string) => { readonly value?: unknown } | null
}

/** A row's change that adds a pending setting rather than writing. */
export interface LaunchAdd {
  readonly placeId: string
  /** The pending entry a chosen key makes: a runner setting or a modifier's. */
  readonly tokenOf: (key: string) => string
}

/** Pending entries, one string each, so runner and modifier settings never meet. */
export const PENDING = {
  setting: (key: string) => `setting\u0000${key}`,
  modifier: (id: string, key: string) => `modifier\u0000${id}\u0000${key}`,
} as const

export interface LaunchEditor {
  readonly places: readonly SurfaceLaunchSettingsPlace[]
  readonly notes: readonly string[]
  readonly writes: ReadonlyMap<string, LaunchWrite>
  /** Add setting rows, by row id. */
  readonly adds: ReadonlyMap<string, LaunchAdd>
  /** Rows added with Add setting and not yet set, by row id. */
  readonly pending: ReadonlyMap<string, { readonly placeId: string; readonly token: string }>
}

// --- places ----------------------------------------------------------------

export function layerKey(layer: LaunchLayer): string {
  switch (layer._tag) {
    case "Device": return "device"
    case "System": return `system:${layer.id}`
    case "Game": return `game:${layer.id}`
    case "Release": return `release:${layer.id.releaseId}`
    case "Profile": return `profile:${layer.id.profileId}`
    case "Launch": return "launch"
  }
}

export function scopeKey(scope: LaunchRunnerScope): string {
  switch (scope._tag) {
    case "Any": return "any"
    case "Family": return `family:${scope.id}`
    case "Runner": return `runner:${scope.id}`
  }
}

/** A stable place id, unique within one snapshot. */
export const placeIdOf = (place: LaunchPlace): string => `${layerKey(place.layer)}/${scopeKey(place.scope)}`

const samePlace = (a: LaunchPlace, b: LaunchPlace) => placeIdOf(a) === placeIdOf(b)

/** Device and System live in device.yaml; the rest in the library. */
export const revisionOf = (layer: LaunchLayer): "device" | "games" =>
  layer._tag === "Device" || layer._tag === "System" ? "device" : "games"

const fileOf = (layer: LaunchLayer) => revisionOf(layer) === "device" ? "device.yaml" : "library.yaml"

const LAYER_RANK: Record<LaunchLayer["_tag"], number> = { Device: 0, System: 1, Game: 2, Release: 3, Profile: 4, Launch: 5 }
const SCOPE_RANK: Record<LaunchRunnerScope["_tag"], number> = { Any: 0, Family: 1, Runner: 2 }

/**
 * Each profile's title as the editor shows it. Titles may repeat; a repeat
 * is told apart by its place among its namesakes ("Quiet", "Quiet (2)").
 */
export function profileTitles(profiles: readonly { readonly id: string; readonly title: string }[]): ReadonlyMap<string, string> {
  const seen = new Map<string, number>()
  return new Map(profiles.map(profile => {
    const count = (seen.get(profile.title) ?? 0) + 1
    seen.set(profile.title, count)
    return [profile.id, count === 1 ? profile.title : `${profile.title} (${count})`]
  }))
}

/** Runner titles, told apart by id when two runners share one. */
function runnerTitles(runners: readonly LaunchRunnerSnapshot[]): ReadonlyMap<string, string> {
  const counts = new Map<string, number>()
  for (const runner of runners) counts.set(runner.title, (counts.get(runner.title) ?? 0) + 1)
  return new Map(runners.map(runner => [runner.id, (counts.get(runner.title) ?? 0) > 1 ? `${runner.title} (${runner.id})` : runner.title]))
}

type Json = Readonly<Record<string, unknown>>
const strings = (value: unknown): readonly string[] =>
  Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : []
const record = (value: unknown): Json | undefined =>
  typeof value === "object" && value !== null && !Array.isArray(value) ? value as Json : undefined

/**
 * Profile places. korrid lists them in `places`; a profile it lists only in
 * `profiles` gets its places from its body: any runner, then each installed
 * runner's family and the runner itself.
 */
function profilePlaces(snapshot: LaunchSnapshot, gameId: string | undefined): readonly LaunchPlaceSnapshot[] {
  if (gameId === undefined) return []
  const titles = profileTitles(snapshot.profiles)
  const titlesOf = runnerTitles(snapshot.runners)
  return snapshot.profiles.flatMap(profile => {
    if (snapshot.places.some(place => place.place.layer._tag === "Profile" && place.place.layer.id.profileId === profile.id)) return []
    const layer: LaunchLayer = { _tag: "Profile", id: { gameId, profileId: profile.id } }
    const body = record(profile.body) ?? {}
    const name = `Profile ${titles.get(profile.id) ?? profile.title}`
    const families = [...new Set(snapshot.runners.flatMap(runner => runner.family === undefined ? [] : [runner.family]))]
    return [
      { place: { layer, scope: { _tag: "Any" } }, title: name, body, editableKinds: [KIND.runner, KIND.cpu, KIND.gpu, KIND.modifiers] },
      ...families.map(family => ({
        place: { layer, scope: { _tag: "Family" as const, id: family } },
        title: `${name} for ${family}`,
        body: record(body.families)?.[family] ?? {},
        editableKinds: [KIND.cpu, KIND.gpu, KIND.modifiers, KIND.settings],
      })),
      ...snapshot.runners.map(runner => ({
        place: { layer, scope: { _tag: "Runner" as const, id: runner.id } },
        title: `${name} for ${titlesOf.get(runner.id) ?? runner.title}`,
        body: record(body.runners)?.[runner.id] ?? {},
        editableKinds: [KIND.cpu, KIND.gpu, KIND.modifiers, KIND.settings],
      })),
    ]
  })
}

/** The places the editor shows, most general first: by layer, then scope. */
export function orderedPlaces(snapshot: LaunchSnapshot, gameId?: string): readonly LaunchPlaceSnapshot[] {
  const runnerRank = new Map(snapshot.runners.map((runner, index) => [runner.id, index]))
  const familyRank = new Map<string, number>()
  snapshot.runners.forEach((runner, index) => {
    if (runner.family !== undefined && !familyRank.has(runner.family)) familyRank.set(runner.family, index)
  })
  const profileRank = new Map(snapshot.profiles.map((profile, index) => [profile.id, index]))
  const all = [...snapshot.places, ...profilePlaces(snapshot, gameId)].filter(place => place.place.layer._tag !== "Launch")
  const rank = (place: LaunchPlace): readonly number[] => [
    LAYER_RANK[place.layer._tag],
    place.layer._tag === "Profile" ? profileRank.get(place.layer.id.profileId) ?? 0 : 0,
    SCOPE_RANK[place.scope._tag],
    place.scope._tag === "Runner" ? runnerRank.get(place.scope.id) ?? 0 : place.scope._tag === "Family" ? familyRank.get(place.scope.id) ?? 0 : 0,
  ]
  return all
    .map((place, index) => ({ place, index, rank: rank(place.place) }))
    .sort((a, b) => {
      for (let at = 0; at < a.rank.length; at++) {
        const delta = (a.rank[at] ?? 0) - (b.rank[at] ?? 0)
        if (delta !== 0) return delta
      }
      return a.index - b.index
    })
    .map(entry => entry.place)
}

// --- clock steps -----------------------------------------------------------

interface Steps {
  readonly choices: readonly SurfaceSettingChoice[]
  /** True when the device did not say: 10 % steps, not real clocks. */
  readonly guessed: boolean
  /** The device says it has none of these clocks (no devfreq GPU). */
  readonly unavailable?: boolean
  readonly labelOf: (percent: number) => string
}

const GUESSED_PERCENTS = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100]

const guessed: Steps = {
  guessed: true,
  labelOf: percent => `${percent} %`,
  choices: GUESSED_PERCENTS.map(percent => ({ value: String(percent), label: `${percent} %` })),
}

/**
 * Real steps, by the launch helper's rule (ADR 0020): a step's percent is
 * `Math.round(step * 100 / top)`, each distinct percent once, labelled with
 * the GHz of the step it maps back to. `unitsPerGhz` is 1e6 for cpufreq
 * (kHz) and 1e9 for devfreq (Hz). A percent that is no step (a value saved
 * on another device, a default) is named by its percent alone.
 */
function clockSteps(top: number, steps: readonly number[], unitsPerGhz: number): Steps {
  const byPercent = new Map<number, number>()
  for (const step of [...steps].sort((a, b) => a - b)) {
    const percent = Math.round((step * 100) / top)
    if (percent > 0 && percent <= 100 && !byPercent.has(percent)) byPercent.set(percent, step)
  }
  const labelOf = (percent: number) => {
    const step = byPercent.get(percent)
    return step === undefined ? `${percent} %` : `${percent} % (${(step / unitsPerGhz).toFixed(2)} GHz)`
  }
  return {
    guessed: false,
    labelOf,
    choices: [...byPercent.keys()].map(percent => ({ value: String(percent), label: labelOf(percent) })),
  }
}

/** The CPU policy with the highest top clock; 10 % steps when the device states none. */
export function cpuSteps(facts: LaunchDeviceFacts | undefined): Steps {
  const policy = [...(facts?.cpuPolicies ?? [])]
    .filter(candidate => candidate.cpuinfoMaxFreq > 0 && candidate.availableFrequencies.length > 0)
    .sort((a, b) => b.cpuinfoMaxFreq - a.cpuinfoMaxFreq)[0]
  return policy === undefined ? guessed : clockSteps(policy.cpuinfoMaxFreq, policy.availableFrequencies, 1e6)
}

/**
 * The GPU's steps against its highest frequency. Facts with no devfreq GPU
 * mean this device has no GPU limits; no facts at all mean 10 % steps.
 */
export function gpuSteps(facts: LaunchDeviceFacts | undefined): Steps {
  if (facts === undefined) return guessed
  const device = [...facts.gpuDevfreq]
    .filter(candidate => candidate.availableFrequencies.length > 0)
    .sort((a, b) => Math.max(...b.availableFrequencies) - Math.max(...a.availableFrequencies))[0]
  if (device === undefined) return { ...guessed, guessed: false, unavailable: true, choices: [] }
  return clockSteps(Math.max(...device.availableFrequencies), device.availableFrequencies, 1e9)
}

/** Governors every cluster offers. Empty when the device did not say. */
export function governors(facts: LaunchDeviceFacts | undefined): readonly string[] {
  const policies = facts?.cpuPolicies ?? []
  const [first, ...rest] = policies
  if (first === undefined) return []
  return first.availableGovernors.filter(governor => rest.every(policy => policy.availableGovernors.includes(governor)))
}

// --- words -----------------------------------------------------------------

const INHERIT_CHOICE: SurfaceSettingChoice = { value: INHERIT, label: "Inherit" }
const NO_FLOOR: SurfaceSettingChoice = { value: "0", label: "0 % (no floor)" }
const NEXT_LAUNCH = "Applies at next launch."
const MAX_PROFILE_TITLE = 40

/** A JSON value as one line of display text. */
export function valueText(value: unknown): string {
  if (value === null || value === undefined) return "Unset"
  if (typeof value === "boolean") return value ? "On" : "Off"
  if (typeof value === "string") return value === "" ? "(empty)" : value
  if (typeof value === "number") return String(value)
  if (Array.isArray(value)) return value.map(valueText).join(" ")
  return JSON.stringify(value)
}

interface Context {
  readonly snapshot: LaunchSnapshot
  readonly places: readonly LaunchPlaceSnapshot[]
  readonly cpu: Steps
  readonly gpu: Steps
  readonly governors: readonly string[]
  readonly runnerTitles: ReadonlyMap<string, string>
  readonly selected?: LaunchRunnerSnapshot
  /** The value in force at a path, for the runner in use. */
  readonly resolved: (path: LaunchPath) => LaunchSourcedValue | undefined
  /** A list kind in force (arguments, hooks), with each place's part. */
  readonly list: (kind: ListKind) => LaunchSourcedValue | undefined
}

function contextOf(snapshot: LaunchSnapshot, gameId?: string): Context {
  const selected = snapshot.runners.find(runner => runner.selected)
  const byPath = new Map(Object.values(snapshot.resolved.values).map(value => [valuePathKey(value.path), value]))
  return {
    resolved: path => byPath.get(valuePathKey(editable(path))),
    list: kind => byPath.get(valuePathKey({ _tag: kind })),
    snapshot,
    places: orderedPlaces(snapshot, gameId),
    cpu: cpuSteps(snapshot.deviceFacts),
    gpu: gpuSteps(snapshot.deviceFacts),
    governors: governors(snapshot.deviceFacts),
    runnerTitles: runnerTitles(snapshot.runners),
    ...(selected === undefined ? {} : { selected }),
  }
}

function sourceTitle(context: Context, source: LaunchSource): string {
  switch (source._tag) {
    case "Place": {
      const place = context.places.find(candidate => samePlace(candidate.place, source.payload))
      return place === undefined ? layerTitle(context, source.payload.layer) : placeTitle(context, place)
    }
    case "PluginDefault": {
      const runner = context.snapshot.runners.find(candidate => candidate.id === source.payload.pluginId)
      return `${runner === undefined ? source.payload.pluginId : context.runnerTitles.get(runner.id) ?? runner.title} (default)`
    }
    case "DeviceDefault":
      return "Device (default)"
  }
}

/** Whether a place's scope applies to the runner this game launches with. */
function applies(context: Context, scope: LaunchRunnerScope): boolean {
  switch (scope._tag) {
    case "Any": return true
    case "Family": return context.selected?.family === scope.id
    case "Runner": return context.selected?.id === scope.id
  }
}

/**
 * Where the value in force comes from, said from one place's row. A place
 * whose scope is not the runner in use says when it would apply instead:
 * korrid resolves runner-scoped values for the selected runner only.
 */
function sourceText(
  context: Context,
  here: LaunchPlace,
  resolved: LaunchSourcedValue | undefined,
  show: (value: unknown) => string,
): string | undefined {
  if (!applies(context, here.scope)) {
    const name = context.selected === undefined ? undefined : context.runnerTitles.get(context.selected.id) ?? context.selected.title
    return name === undefined ? "Applies once a runner is chosen" : `Not in force: ${name} runs this game`
  }
  // A list kind has no one source: its contributions name theirs.
  const source = resolved?.source
  if (resolved === undefined || source === undefined) return undefined
  const title = sourceTitle(context, source)
  const value = show(resolved.value)
  if (resolved.adjusted) return `${value} from ${title}, adjusted`
  return source._tag === "Place" && samePlace(source.payload, here) ? `from ${title}` : `${value} from ${title}`
}

function layerTitle(context: Context, layer: LaunchLayer): string {
  switch (layer._tag) {
    case "Device": return "Device"
    case "System": return `System ${layer.id}`
    case "Game": return "Game"
    case "Release": return `Release ${layer.id.releaseId}`
    case "Profile": return `Profile ${profileTitles(context.snapshot.profiles).get(layer.id.profileId) ?? layer.id.profileId}`
    case "Launch": return "This launch"
  }
}

/**
 * A place's title: korrid's, except a profile's, which names the profile as
 * the editor does, so two profiles with one title stay apart.
 */
function placeTitle(context: Context, entry: LaunchPlaceSnapshot): string {
  const { layer, scope } = entry.place
  if (layer._tag !== "Profile") return entry.title
  const title = layerTitle(context, layer)
  return scope._tag === "Any" ? title : `${title} for ${scopeLabel(context, scope)}`
}

function scopeLabel(context: Context, scope: LaunchRunnerScope): string {
  switch (scope._tag) {
    case "Any": return "Any runner"
    case "Family": return `Family ${scope.id}`
    case "Runner": return context.runnerTitles.get(scope.id) ?? scope.id
  }
}

// --- rows ------------------------------------------------------------------

interface Rows {
  readonly writes: Map<string, LaunchWrite>
  readonly adds: Map<string, LaunchAdd>
  readonly pending: Map<string, { readonly placeId: string; readonly token: string }>
}

const percentOf = (value: unknown): number | undefined =>
  typeof value === "number" && Number.isFinite(value) ? value : undefined

function percentRow(
  context: Context,
  rows: Rows,
  id: string,
  place: LaunchPlace,
  key: Exclude<LimitKey, "cpu.governor">,
  own: number | undefined,
  describe: string,
): SurfaceSettingItem {
  const [kind, bound] = key.split(".") as ["cpu" | "gpu", "min" | "max"]
  const steps = context[kind]
  // A minimum may be no floor at all; a maximum is always above 0 (korrid
  // takes min 0 to 100, max 1 to 100).
  const floor = bound === "min" ? [NO_FLOOR] : []
  const offered = [...floor, ...steps.choices]
  const choices = own === undefined || offered.some(choice => choice.value === String(own))
    ? offered
    : [...offered, { value: String(own), label: steps.labelOf(own) }].sort((a, b) => Number(a.value) - Number(b.value))
  const path = LIMIT_PATH[key]
  rows.writes.set(id, {
    place,
    path,
    encode: text => {
      if (text === INHERIT) return {}
      const percent = Number(text)
      return Number.isInteger(percent) && percent >= 0 && percent <= 100 ? { value: percent } : null
    },
  })
  const label = `${kind.toUpperCase()} ${bound}`
  const source = sourceText(context, place, context.resolved(LIMIT_PATH[key]), value => {
    const percent = percentOf(value)
    return percent === undefined ? valueText(value) : steps.labelOf(percent)
  })
  return {
    id,
    label,
    value: own === undefined ? "Inherit" : choices.find(choice => choice.value === String(own))?.label ?? steps.labelOf(own),
    description: steps.guessed ? `${describe} Clock steps unknown.` : describe,
    ...(source === undefined ? {} : { source }),
    scope: "device",
    interaction: { kind: "choice", choices: [INHERIT_CHOICE, ...choices], selected: own === undefined ? INHERIT : String(own) },
  }
}

function governorRow(context: Context, rows: Rows, id: string, place: LaunchPlace, own: string | undefined): SurfaceSettingItem {
  const known = context.governors
  const offered = own === undefined || known.includes(own) ? known : [...known, own]
  const resolved = context.resolved(LIMIT_PATH["cpu.governor"])
  const fallback = context.snapshot.deviceFacts?.defaults.governor
  rows.writes.set(id, { place, path: LIMIT_PATH["cpu.governor"], encode: text => text === INHERIT ? {} : { value: text } })
  return {
    id,
    label: "CPU governor",
    value: own ?? "Inherit",
    description: known.length === 0 ? `${NEXT_LAUNCH} Governors unknown.` : NEXT_LAUNCH,
    source: own !== undefined
      ? "from Device"
      : resolved !== undefined
        ? sourceText(context, place, resolved, valueText) ?? "from Device (default)"
        : fallback === undefined ? "from Device (default)" : `${fallback} from Device (default)`,
    scope: "device",
    interaction: {
      kind: "choice",
      choices: [INHERIT_CHOICE, ...offered.map(governor => ({ value: governor, label: governor }))],
      selected: own ?? INHERIT,
    },
  }
}

/** CPU and GPU rows for one place. Only Device for any runner sets the
 * device's clock limits; every other place limits the game's session. */
function limitGroups(context: Context, rows: Rows, place: LaunchPlace, body: Json, kinds: readonly string[], ids: (key: LimitKey) => string): SurfaceSettingGroup[] {
  const cpu = record(body.cpu) ?? {}
  const gpu = record(body.gpu) ?? {}
  const hostWide = place.layer._tag === "Device" && place.scope._tag === "Any"
  const describe = hostWide ? `${NEXT_LAUNCH} Sets the device's clock limits.` : `${NEXT_LAUNCH} Limits the game's session.`
  const groups: SurfaceSettingGroup[] = []
  if (kinds.includes(KIND.cpu)) {
    groups.push({
      title: "CPU",
      items: [
        percentRow(context, rows, ids("cpu.min"), place, "cpu.min", percentOf(cpu.min), describe),
        percentRow(context, rows, ids("cpu.max"), place, "cpu.max", percentOf(cpu.max), describe),
        ...(hostWide ? [governorRow(context, rows, ids("cpu.governor"), place, typeof cpu.governor === "string" ? cpu.governor : undefined)] : []),
      ],
    })
  }
  if (kinds.includes(KIND.gpu)) {
    groups.push({
      title: "GPU",
      items: context.gpu.unavailable === true
        ? [{ id: `${ids("gpu.min")}:unavailable`, label: "GPU limits aren't available on this device" }]
        : [
            percentRow(context, rows, ids("gpu.min"), place, "gpu.min", percentOf(gpu.min), describe),
            percentRow(context, rows, ids("gpu.max"), place, "gpu.max", percentOf(gpu.max), describe),
          ],
    })
  }
  return groups
}

/** The runner choice, on places that may hold one. */
function runnerGroup(context: Context, rows: Rows, placeId: string, place: LaunchPlace, body: Json): SurfaceSettingGroup {
  const own = typeof body.runner === "string" ? body.runner : undefined
  const id = LAUNCH_ID.set(placeId, "runner")
  const installed = context.snapshot.runners.map(runner => ({ value: runner.id, label: context.runnerTitles.get(runner.id) ?? runner.title }))
  const missing = own !== undefined && !installed.some(choice => choice.value === own)
  const choices = [INHERIT_CHOICE, ...installed, ...(missing ? [{ value: own, label: `${own} (not installed)` }] : [])]
  rows.writes.set(id, { place, path: { _tag: "Runner" }, encode: text => text === INHERIT ? {} : { value: text } })
  const source = sourceText(context, place, context.resolved({ _tag: "Runner" }), value =>
    typeof value === "string" ? context.runnerTitles.get(value) ?? value : valueText(value))
  return {
    title: "Runner",
    items: [{
      id,
      label: "Runner",
      value: choices.find(choice => choice.value === (own ?? INHERIT))?.label ?? "Inherit",
      description: missing
        ? "This runner isn't installed. Korri skips it and its runner settings here."
        : "The most specific place that names an installed runner wins.",
      ...(source === undefined ? {} : { source }),
      scope: "shared",
      interaction: { kind: "choice", choices, selected: own ?? INHERIT },
    }],
  }
}

// --- runner settings -------------------------------------------------------

interface Property {
  readonly type?: string
  readonly title?: string
  readonly description?: string
  readonly enum?: readonly unknown[]
  readonly minimum?: number
  readonly maximum?: number
  readonly device: boolean
}

function propertiesOf(schema: unknown): ReadonlyMap<string, Property> {
  const properties = record(record(schema)?.properties) ?? {}
  return new Map(Object.entries(properties).flatMap(([key, raw]) => {
    const property = record(raw)
    if (property === undefined) return []
    const type = Array.isArray(property.type) ? property.type.find(item => item !== "null") : property.type
    return [[key, {
      ...(typeof type === "string" ? { type } : {}),
      ...(typeof property.title === "string" ? { title: property.title } : {}),
      ...(typeof property.description === "string" ? { description: property.description } : {}),
      ...(Array.isArray(property.enum) ? { enum: property.enum } : {}),
      ...(typeof property.minimum === "number" ? { minimum: property.minimum } : {}),
      ...(typeof property.maximum === "number" ? { maximum: property.maximum } : {}),
      device: property["x-korri-scope"] === "device",
    } satisfies Property]]
  }))
}

/** The settings a runner place may set: its runner's schema, or for a family
 * place every installed runner of the family (the first to name a key wins). */
function schemaFor(context: Context, scope: LaunchRunnerScope): ReadonlyMap<string, Property> {
  if (scope._tag === "Any") return new Map()
  const runners = context.snapshot.runners.filter(runner => scope._tag === "Runner" ? runner.id === scope.id : runner.family === scope.id)
  const merged = new Map<string, Property>()
  for (const runner of runners) {
    for (const [key, property] of propertiesOf(runner.schema)) if (!merged.has(key)) merged.set(key, property)
  }
  return merged
}

const choiceValue = (value: unknown) => JSON.stringify(value)

/** How one value is changed, from its schema property or else its own type. */
function valueInteraction(property: Property | undefined, own: unknown): {
  readonly interaction: SurfaceSettingInteraction
  readonly encode: (text: string) => { readonly value?: unknown } | null
} {
  if (property?.enum !== undefined && property.enum.length > 0) {
    const choices = property.enum.map(value => ({ value: choiceValue(value), label: valueText(value) }))
    return {
      interaction: { kind: "choice", choices: [INHERIT_CHOICE, ...choices], selected: own === undefined ? INHERIT : choiceValue(own) },
      encode: text => {
        if (text === INHERIT) return {}
        const found = property.enum?.find(value => choiceValue(value) === text)
        return found === undefined ? null : { value: found }
      },
    }
  }
  const type = property?.type ?? (typeof own === "boolean" ? "boolean" : typeof own === "number" ? "number" : "string")
  if (type === "boolean") {
    return {
      interaction: {
        kind: "choice",
        choices: [INHERIT_CHOICE, { value: "true", label: "On" }, { value: "false", label: "Off" }],
        selected: own === true ? "true" : own === false ? "false" : INHERIT,
      },
      encode: text => text === INHERIT ? {} : text === "true" ? { value: true } : text === "false" ? { value: false } : null,
    }
  }
  if (type === "number" || type === "integer") {
    const integer = type === "integer"
    return {
      interaction: {
        kind: "number",
        ...(typeof own === "number" ? { value: own } : {}),
        ...(property?.minimum === undefined ? {} : { min: property.minimum }),
        ...(property?.maximum === undefined ? {} : { max: property.maximum }),
        ...(integer ? { step: 1 } : {}),
        clearable: true,
      },
      encode: text => {
        if (text.trim() === "") return {}
        const number = Number(text)
        if (!Number.isFinite(number) || (integer && !Number.isInteger(number))) return null
        if (property?.minimum !== undefined && number < property.minimum) return null
        if (property?.maximum !== undefined && number > property.maximum) return null
        return { value: number }
      },
    }
  }
  return {
    interaction: { kind: "text", placeholder: property?.title ?? "Value" },
    encode: text => ({ value: text }),
  }
}

function settingsGroup(context: Context, rows: Rows, placeId: string, place: LaunchPlace, body: Json, pending: readonly string[]): SurfaceSettingGroup | undefined {
  const schema = schemaFor(context, place.scope)
  const own = record(body.settings) ?? {}
  const added = schemaKeys(pending, PENDING.setting)
  const keys = [...Object.keys(own), ...added.filter(key => !(key in own))]
  const items: SurfaceSettingItem[] = keys.map(key => {
    const property = schema.get(key)
    const value = own[key]
    const set = key in own
    const id = LAUNCH_ID.set(placeId, `setting:${key}`)
    const { interaction, encode } = valueInteraction(property, set ? value : undefined)
    rows.writes.set(id, { place, path: { _tag: "Setting", payload: { key } }, encode })
    if (!set) rows.pending.set(id, { placeId, token: PENDING.setting(key) })
    const source = sourceText(context, place, context.resolved({ _tag: "Setting", payload: { key } }), valueText)
    return {
      id,
      label: property?.title ?? key,
      value: set ? valueText(value) : "Not set here",
      ...(property?.description === undefined
        ? property === undefined ? { description: "This runner no longer describes this setting." } : {}
        : { description: property.description }),
      ...(source === undefined ? {} : { source }),
      scope: property?.device === true ? "device" : "shared",
      interaction,
    }
  })
  const unset = [...schema].filter(([key]) => !keys.includes(key))
  if (unset.length > 0) {
    const id = LAUNCH_ID.set(placeId, "add-setting")
    rows.adds.set(id, { placeId, tokenOf: PENDING.setting })
    items.push({
      id,
      label: "Add setting",
      value: "Choose a setting",
      description: `${unset.length} more ${unset.length === 1 ? "setting" : "settings"} this runner offers.`,
      interaction: {
        kind: "choice",
        choices: unset.map(([key, property]) => ({ value: key, label: property.title ?? key })),
        selected: "",
      },
    })
  }
  return items.length === 0 ? undefined : { title: "Runner settings", items }
}

/** The keys pending entries name for one owner, in the order they were added. */
function schemaKeys(pending: readonly string[], tokenOf: (key: string) => string): readonly string[] {
  const prefix = tokenOf("")
  return pending.filter(token => token.startsWith(prefix)).map(token => token.slice(prefix.length))
}

const ON_OFF = [INHERIT_CHOICE, { value: "true", label: "On" }, { value: "false", label: "Off" }]

/**
 * The modifiers a place can turn on or off: every installed, enabled one
 * korrid lists, then any a place names that it does not list.
 */
function modifiersOf(context: Context): readonly { readonly id: string; readonly title: string; readonly schema?: unknown; readonly listed: boolean }[] {
  const listed = context.snapshot.modifiers.map(modifier => ({
    id: modifier.id,
    title: modifier.title,
    ...(modifier.schema === undefined ? {} : { schema: modifier.schema }),
    listed: true,
  }))
  const named = new Set<string>()
  for (const place of context.places) for (const id of Object.keys(record(record(place.body)?.modifiers) ?? {})) named.add(id)
  const unlisted = [...named].filter(id => !listed.some(modifier => modifier.id === id)).sort()
  return [...listed, ...unlisted.map(id => ({ id, title: id, listed: false }))]
}

/**
 * One group per modifier: on, off or Inherit, then its settings as runner
 * settings are (what is set here, then Add setting from its schema). A
 * modifier with no schema has no settings to show.
 */
function modifierGroups(context: Context, rows: Rows, placeId: string, place: LaunchPlace, body: Json, pending: readonly string[]): SurfaceSettingGroup[] {
  const own = record(body.modifiers) ?? {}
  return modifiersOf(context).map((modifier): SurfaceSettingGroup => {
    const authored = record(own[modifier.id]) ?? {}
    const enabled = typeof authored.enabled === "boolean" ? authored.enabled : undefined
    const id = LAUNCH_ID.set(placeId, `modifier:${modifier.id}:enabled`)
    rows.writes.set(id, {
      place,
      path: { _tag: "ModifierEnabled", payload: { id: modifier.id } },
      encode: text => text === INHERIT ? {} : text === "true" ? { value: true } : text === "false" ? { value: false } : null,
    })
    const source = sourceText(context, place, context.resolved({ _tag: "ModifierEnabled", payload: { id: modifier.id } }), valueText)
    const enabledRow: SurfaceSettingItem = {
      id,
      label: modifier.title,
      value: enabled === undefined ? "Inherit" : enabled ? "On" : "Off",
      description: modifier.listed ? NEXT_LAUNCH : "This modifier isn't installed or is turned off on this device.",
      ...(source === undefined ? {} : { source }),
      scope: "device",
      interaction: { kind: "choice", choices: ON_OFF, selected: enabled === undefined ? INHERIT : String(enabled) },
    }
    const schema = modifier.schema === undefined ? new Map<string, Property>() : propertiesOf(modifier.schema)
    const settings = record(authored.settings) ?? {}
    const tokenOf = (key: string) => PENDING.modifier(modifier.id, key)
    const added = schemaKeys(pending, tokenOf)
    const keys = [...Object.keys(settings), ...added.filter(key => !(key in settings))]
    const settingRows = keys.map((key): SurfaceSettingItem => {
      const property = schema.get(key)
      const set = key in settings
      const rowId = LAUNCH_ID.set(placeId, `modifier:${modifier.id}:setting:${key}`)
      const { interaction, encode } = valueInteraction(property, set ? settings[key] : undefined)
      rows.writes.set(rowId, { place, path: { _tag: "ModifierSetting", payload: { id: modifier.id, key } }, encode })
      if (!set) rows.pending.set(rowId, { placeId, token: tokenOf(key) })
      const from = sourceText(context, place, context.resolved({ _tag: "ModifierSetting", payload: { id: modifier.id, key } }), valueText)
      return {
        id: rowId,
        label: property?.title ?? key,
        value: set ? valueText(settings[key]) : "Not set here",
        ...(property?.description === undefined ? {} : { description: property.description }),
        ...(from === undefined ? {} : { source: from }),
        scope: "device",
        interaction,
      }
    })
    const unset = [...schema].filter(([key]) => !keys.includes(key))
    const addId = LAUNCH_ID.set(placeId, `modifier:${modifier.id}:add-setting`)
    if (unset.length > 0) rows.adds.set(addId, { placeId, tokenOf })
    const addRow: SurfaceSettingItem[] = unset.length === 0 ? [] : [{
      id: addId,
      label: `Add ${modifier.title} setting`,
      value: "Choose a setting",
      description: `${unset.length} more ${unset.length === 1 ? "setting" : "settings"} ${modifier.title} offers.`,
      interaction: { kind: "choice", choices: unset.map(([key, property]) => ({ value: key, label: property.title ?? key })), selected: "" },
    }]
    return { title: `Modifier ${modifier.title}`, items: [enabledRow, ...settingRows, ...addRow] }
  })
}

/** Why a place's list parts are not in force, in the person's words. */
function notInForceText(context: Context, place: LaunchPlace): string {
  if (!applies(context, place.scope)) return sourceText(context, place, undefined, valueText) ?? "Not in force"
  if (place.layer._tag === "Profile" && place.layer.id.profileId !== context.snapshot.selectedProfile) {
    return "Not in force: this profile isn't picked"
  }
  return "Not in force"
}

/** The four list kinds korrid concatenates across places, in merge order. */
type ListKind = "ArgsPrepend" | "ArgsAppend" | "HooksBefore" | "HooksAfter"
const LISTS: readonly { readonly kind: ListKind; readonly label: string; readonly key: string }[] = [
  { kind: "ArgsPrepend", label: "Arguments before", key: "args:prepend" },
  { kind: "ArgsAppend", label: "Arguments after", key: "args:append" },
  { kind: "HooksBefore", label: "Before launch", key: "hooks:before" },
  { kind: "HooksAfter", label: "After the session", key: "hooks:after" },
]

/** A part of a list as one line: arguments by spaces, hook steps by what they run. */
function partText(value: unknown): string {
  if (!Array.isArray(value)) {
    const run = record(value)?.run
    return typeof run === "string" ? run : valueText(value)
  }
  const words = value.every(item => typeof item === "string")
  return value.map(partText).join(words ? " " : "; ")
}

/** A place's own part of a list kind, from its body. */
function ownPart(body: Json, kind: ListKind): unknown {
  const args = record(body.args) ?? {}
  const hooks = record(body.hooks) ?? {}
  switch (kind) {
    case "ArgsPrepend": return args.prepend
    case "ArgsAppend": return args.append
    case "HooksBefore": return hooks.before
    case "HooksAfter": return hooks.after
  }
}

const isEmptyPart = (value: unknown) => value === undefined || (Array.isArray(value) && value.length === 0)

/**
 * Values set only in files: shown with where they are, never changed here.
 * A list kind (arguments before and after, hooks before and after) shows
 * only this place's parts: the routines it uses, then its own steps, each
 * marked In force or Not in force. The whole list in force is the places'
 * parts read in order.
 */
function fileGroup(context: Context, place: LaunchPlaceSnapshot, placeId: string): SurfaceSettingGroup | undefined {
  const body = record(place.body) ?? {}
  const here = placeTitle(context, place)
  const describeOf = (layer: LaunchLayer) => `Set in ${fileOf(layer)}. Change it there.`
  const row = (key: string, label: string, value: string, source = `from ${here}`, description = describeOf(place.place.layer)): SurfaceSettingItem =>
    ({ id: LAUNCH_ID.set(placeId, `file:${key}`), label, value, source, scope: "device", description })
  const items: SurfaceSettingItem[] = []
  for (const [key, value] of Object.entries(record(body.env) ?? {})) {
    items.push(row(`env:${key}`, `Environment ${key}`, value === null ? "Unset" : valueText(value)))
  }
  if (typeof body.cwd === "string") items.push(row("cwd", "Working folder", body.cwd))
  // A place shows only its own parts of each list kind. korrid's
  // contributions say which are in force: the parts this place gave the
  // launch, its routines first. A place that gave none is not in force, and
  // its parts come from its own body.
  const ownContributions = (kind: ListKind) => (context.list(kind)?.contributions ?? [])
    .filter(part => part.source._tag === "Place" && samePlace(part.source.payload, place.place) && !isEmptyPart(part.value))
  const inForce = LISTS.some(list => ownContributions(list.kind).length > 0)
  const notInForce = notInForceText(context, place.place)
  for (const list of LISTS) {
    if (!inForce) {
      const own = ownPart(body, list.kind)
      if (!isEmptyPart(own)) items.push(row(`${list.key}:own`, list.label, partText(own), notInForce))
      continue
    }
    ownContributions(list.kind).forEach((part, index) => {
      // A routine lives in device.yaml; a place's own part in its place's file.
      const routine = part.routine
      items.push(row(
        `${list.key}:${index}`,
        list.label,
        partText(part.value),
        routine === undefined ? `In force: from ${here}` : `In force: routine ${routine}, used by ${here}`,
        routine === undefined ? describeOf(place.place.layer) : "Set in device.yaml. Change it there.",
      ))
    })
  }
  // Routines a place uses that is not in force: their steps are in
  // device.yaml, and korrid gives them only for the parts in force.
  if (!inForce) {
    for (const name of strings(record(body.hooks)?.use)) {
      items.push(row(`hooks:use:${name}`, "Routine", name, notInForce, "Set in device.yaml. Change it there."))
    }
  }
  const config = record(body.config)
  if (config !== undefined && Object.keys(config).length > 0) items.push(row("config", "Runner config", valueText(config)))
  for (const [modifier, raw] of Object.entries(record(body.modifiers) ?? {})) {
    const modifierConfig = record(raw)?.config
    if (modifierConfig !== undefined) items.push(row(`modifier-config:${modifier}`, `${modifier} config`, valueText(modifierConfig)))
  }
  return items.length === 0 ? undefined : { title: "From files", items }
}

/** What the runner plugin itself sets, from korrid's dry run, or that it could not say. */
function runnerFactsGroup(context: Context, placeId: string, scope: LaunchRunnerScope): SurfaceSettingGroup | undefined {
  if (scope._tag !== "Runner") return undefined
  const runner = context.snapshot.runners.find(candidate => candidate.id === scope.id)
  if (runner === undefined) return undefined
  const title = context.runnerTitles.get(runner.id) ?? runner.title
  const from = `from ${title} (default)`
  const row = (key: string, label: string, value: string): SurfaceSettingItem =>
    ({ id: LAUNCH_ID.set(placeId, `runner-facts:${key}`), label, value, source: from, scope: "device" })
  if (runner.prepare._tag === "Unavailable") {
    return {
      title: "From the runner",
      items: [{
        id: LAUNCH_ID.set(placeId, "runner-facts:unavailable"),
        label: "Runner facts unavailable",
        value: runner.prepare.payload,
        description: `Korri could not ask ${title} what it sets. Its environment, arguments and folder are not shown.`,
      }],
    }
  }
  const facts = runner.prepare.payload
  const items = [
    ...Object.entries(facts.env).map(([key, value]) => row(`env:${key}`, `Environment ${key}`, value)),
    ...(facts.args.length === 0 ? [] : [row("args", "Arguments", facts.args.join(" "))]),
    ...(facts.cwd === undefined ? [] : [row("cwd", "Working folder", facts.cwd)]),
  ]
  return items.length === 0 ? undefined : { title: "From the runner", items }
}

function profileRows(context: Context, kind: "pick" | "own", profileId?: string): SurfaceSettingGroup {
  const snapshot = context.snapshot
  const titles = profileTitles(snapshot.profiles)
  if (kind === "own" && profileId !== undefined) {
    const profile = snapshot.profiles.find(candidate => candidate.id === profileId)
    const title = profile?.title ?? profileId
    const shown = titles.get(profileId) ?? title
    return {
      title: "Profile",
      items: [
        {
          id: LAUNCH_ID.rename(profileId),
          label: "Name",
          value: title,
          description: profileId === snapshot.selectedProfile ? "Used at next launch." : "Not used at next launch.",
          interaction: { kind: "text", placeholder: "Profile name", maxLength: MAX_PROFILE_TITLE },
        },
        {
          id: LAUNCH_ID.remove(profileId),
          label: "Delete profile",
          interaction: {
            kind: "action",
            actionId: LAUNCH_ID.remove(profileId),
            destructive: true,
            confirmation: {
              title: `Delete ${shown}?`,
              message: "Its values go. The game's other launch settings stay.",
              confirmLabel: "Delete profile",
            },
          },
        },
      ],
    }
  }
  const selected = snapshot.profiles.find(profile => profile.id === snapshot.selectedProfile)
  return {
    title: "Profile",
    items: [
      {
        id: LAUNCH_ID.pick,
        label: "Profile at next launch",
        value: selected === undefined ? "No profile" : titles.get(selected.id) ?? selected.title,
        description: "Remembered for this game on this device. Applies at next launch.",
        interaction: {
          kind: "choice",
          choices: [
            { value: NO_PROFILE, label: "No profile" },
            ...snapshot.profiles.map(profile => ({ value: profile.id, label: titles.get(profile.id) ?? profile.title })),
          ],
          selected: selected?.id ?? NO_PROFILE,
        },
      },
      {
        id: LAUNCH_ID.create,
        label: "New profile",
        description: "Starts with every value inherited.",
        interaction: { kind: "text", placeholder: "Profile name", maxLength: MAX_PROFILE_TITLE },
      },
    ],
  }
}

/**
 * Plain lines about the whole editor: places korrid could not give, runners a
 * place names that are not installed, and runners whose facts it could not read.
 */
export function launchNotesFrom(snapshot: LaunchSnapshot, gameId?: string): readonly string[] {
  const context = contextOf(snapshot, gameId)
  const partial = "Values in force leave them out."
  const route = snapshot.routeLayers._tag === "NeedsRunnerChoice"
    ? [`System and release settings appear once you choose a runner. ${partial}`]
    : snapshot.routeLayers._tag === "Unavailable"
      ? [`System and release settings aren't available: ${snapshot.routeLayers.message.replace(/\.$/, "")}. ${partial}`]
      : []
  const inUse = context.selected === undefined ? undefined : context.runnerTitles.get(context.selected.id) ?? context.selected.title
  const unavailable = snapshot.notices.map(notice => {
    const place = sourceTitle(context, { _tag: "Place", payload: notice.payload.place })
    return `${place} names runner ${notice.payload.runnerId}, which isn't installed. Korri skips it and its runner settings there${inUse === undefined ? "." : ` and uses ${inUse}.`}`
  })
  const facts = snapshot.runners.flatMap(runner => runner.prepare._tag === "Unavailable"
    ? [`${context.runnerTitles.get(runner.id) ?? runner.title} facts are unavailable: ${runner.prepare.payload.replace(/\.$/, "")}.`]
    : [])
  return [...route, ...unavailable, ...facts]
}

/**
 * One game's editor: a place per layer and runner scope, most general first.
 * `pending` holds settings the person added with Add setting, by place id,
 * shown until they are set or the editor closes.
 */
export function launchEditorFrom(
  snapshot: LaunchSnapshot,
  gameId: string | undefined,
  pending: ReadonlyMap<string, readonly string[]> = new Map(),
): LaunchEditor {
  const context = contextOf(snapshot, gameId)
  const rows: Rows = { writes: new Map(), adds: new Map(), pending: new Map() }
  const places = context.places.map((entry): SurfaceLaunchSettingsPlace => {
    const { place } = entry
    const placeId = placeIdOf(place)
    const body = record(entry.body) ?? {}
    const kinds = entry.editableKinds
    const any = place.scope._tag === "Any"
    const groups: (SurfaceSettingGroup | undefined)[] = [
      // What a place is for first: the runner choice, or the runner's settings.
      any && kinds.includes(KIND.runner) ? runnerGroup(context, rows, placeId, place, body) : undefined,
      kinds.includes(KIND.settings) ? settingsGroup(context, rows, placeId, place, body, pending.get(placeId) ?? []) : undefined,
      ...limitGroups(context, rows, place, body, kinds, key => LAUNCH_ID.set(placeId, key)),
      ...(kinds.includes(KIND.modifiers) ? modifierGroups(context, rows, placeId, place, body, pending.get(placeId) ?? []) : []),
      place.layer._tag === "Game" && any ? profileRows(context, "pick") : undefined,
      place.layer._tag === "Profile" && any ? profileRows(context, "own", place.layer.id.profileId) : undefined,
      fileGroup(context, entry, placeId),
      runnerFactsGroup(context, placeId, place.scope),
    ]
    return {
      id: placeId,
      title: placeTitle(context, entry),
      layer: { id: layerKey(place.layer), title: layerTitle(context, place.layer) },
      scope: { kind: place.scope._tag === "Any" ? "any" : place.scope._tag === "Family" ? "family" : "runner", label: scopeLabel(context, place.scope) },
      groups: groups.filter((group): group is SurfaceSettingGroup => group !== undefined),
    }
  })
  return { places, notes: launchNotesFrom(snapshot, gameId), writes: rows.writes, adds: rows.adds, pending: rows.pending }
}

/** Device settings' "Launch settings" group, from a snapshot without a game. */
export function deviceLaunchGroup(snapshot: LaunchSnapshot): { readonly group: SurfaceSettingGroup; readonly writes: ReadonlyMap<string, LaunchWrite> } {
  const context = contextOf(snapshot)
  const rows: Rows = { writes: new Map(), adds: new Map(), pending: new Map() }
  const entry = context.places.find(place => place.place.layer._tag === "Device" && place.place.scope._tag === "Any")
  const place: LaunchPlace = entry?.place ?? { layer: { _tag: "Device" }, scope: { _tag: "Any" } }
  const groups = limitGroups(context, rows, place, record(entry?.body) ?? {}, [KIND.cpu, KIND.gpu], LAUNCH_ID.device)
  const items = groups.flatMap(group => group.items)
  const governor = items.find(item => item.id === LAUNCH_ID.device("cpu.governor"))
  return {
    group: { title: "Launch settings", items: [...items.filter(item => item !== governor), ...(governor === undefined ? [] : [governor])] },
    writes: rows.writes,
  }
}

/** The profile a pick chooses: its id, or `null` for "No profile". */
export function profileFromPick(choice: string): string | null {
  return choice === NO_PROFILE ? null : choice
}

/** The selected profile's title, as the editor shows it. Absent: none. */
export function activeProfileOf(snapshot: LaunchSnapshot): string | undefined {
  const selected = snapshot.profiles.find(profile => profile.id === snapshot.selectedProfile)
  return selected === undefined ? undefined : profileTitles(snapshot.profiles).get(selected.id) ?? selected.title
}
