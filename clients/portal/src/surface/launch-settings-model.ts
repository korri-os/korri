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
 *
 * Plugin settings (a runner's, a modifier's, a launch service's) are nested
 * in sections (ADR 0037). Each top-level section of a plugin's schema is one
 * group, "Moonlight · Video", with its own Add setting row; a row names one
 * leaf by its dotted setting path. Every such row offers Default, which
 * writes a Cleared setting: the plugin's own default holds there.
 */
import type {
  LaunchDeviceFacts,
  LaunchLayer,
  LaunchModifierSnapshot,
  LaunchPath,
  LaunchPlace,
  LaunchPlaceSnapshot,
  LaunchRunnerScope,
  LaunchRunnerSnapshot,
  LaunchServiceSnapshot,
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
import { layerKey, placeKey } from "../korrid/launch-place-key"

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
  /** A Device-settings row of one place's plugin settings (Moonlight, Sunshine). */
  deviceSet: (placeId: string, path: string) => `launch-device:set:${placeId}#${path}`,
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
export const KIND = { runner: "runner", cpu: "cpu", gpu: "gpu", modifiers: "modifiers", settings: "settings", services: "services" } as const

/** Choice value for a Cleared setting: the plugin's default holds here.
 * Never a JSON-encoded value (those are quoted, or numbers and booleans). */
export const DEFAULT = "default"

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

/** What a changed row writes. `encode` turns the surface's text into what
 * to send: no value takes this place's value away (Inherit), `clear` writes
 * a Cleared setting (Default), and `null` refuses the text. */
export interface LaunchWrite {
  readonly place: LaunchPlace
  readonly path: LaunchPath
  readonly encode: (text: string) => LaunchEncoded | null
}
export type LaunchEncoded = { readonly value?: unknown; readonly clear?: true }

/** A setting added with Add setting and not yet given a value: a runner's,
 * a modifier's or a launch service's, so they never meet. `key` is its
 * dotted setting path. */
export type PendingSetting =
  | { readonly _tag: "Runner"; readonly key: string }
  | { readonly _tag: "Modifier"; readonly id: string; readonly key: string }
  | { readonly _tag: "Service"; readonly id: string; readonly key: string }

export const samePending = (a: PendingSetting, b: PendingSetting): boolean =>
  a.key === b.key && a._tag === b._tag && (a._tag === "Runner" || (b._tag !== "Runner" && a.id === b.id))

/** A row's change that adds a pending setting rather than writing. */
export interface LaunchAdd {
  readonly placeId: string
  /** The pending setting a chosen key makes. */
  readonly pendingOf: (key: string) => PendingSetting
}

export interface LaunchEditor {
  readonly places: readonly SurfaceLaunchSettingsPlace[]
  readonly notes: readonly string[]
  readonly writes: ReadonlyMap<string, LaunchWrite>
  /** Add setting rows, by row id. */
  readonly adds: ReadonlyMap<string, LaunchAdd>
  /** Rows added with Add setting and not yet set, by row id. */
  readonly pending: ReadonlyMap<string, { readonly placeId: string; readonly setting: PendingSetting }>
  /** Inherit rows of string settings, by row id: the setting row each clears. */
  readonly clears: ReadonlyMap<string, string>
  /** Default rows of number and string settings, by row id: the setting row
   * each writes a Cleared setting for. */
  readonly defaults: ReadonlyMap<string, string>
}

// --- places ----------------------------------------------------------------

/** A stable place id, unique within one snapshot. */
export const placeIdOf = placeKey

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

/** The places the editor shows, most general first: by layer, then scope. */
export function orderedPlaces(snapshot: LaunchSnapshot, gameId?: string): readonly LaunchPlaceSnapshot[] {
  const runnerRank = new Map(snapshot.runners.map((runner, index) => [runner.id, index]))
  const familyRank = new Map<string, number>()
  snapshot.runners.forEach((runner, index) => {
    if (runner.family !== undefined && !familyRank.has(runner.family)) familyRank.set(runner.family, index)
  })
  const profileRank = new Map(snapshot.profiles.map((profile, index) => [profile.id, index]))
  // korrid lists every place, a profile's too; one launch is not edited here.
  const all = snapshot.places.filter(place => place.place.layer._tag !== "Launch")
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
const DEFAULT_CHOICE: SurfaceSettingChoice = { value: DEFAULT, label: "Default" }
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
  /** No game: the Device settings page reads this snapshot. */
  readonly devicePage: boolean
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
  /** Why korrid does not apply the value in force at a path, when it says so. */
  readonly skipped: (path: LaunchPath) => string | undefined
}

function contextOf(snapshot: LaunchSnapshot, gameId?: string): Context {
  const selected = snapshot.runners.find(runner => runner.selected && runner.choosable)
  const byPath = new Map(Object.values(snapshot.resolved.values).map(value => [valuePathKey(value.path), value]))
  const skipped = new Map(snapshot.notices.flatMap(notice => notice._tag === "SettingSkipped"
    ? [[editablePathKey(notice.payload.path), notice.payload.message] as const]
    : []))
  return {
    resolved: path => byPath.get(valuePathKey(editable(path))),
    list: kind => byPath.get(valuePathKey({ _tag: kind })),
    skipped: path => skipped.get(editablePathKey(path)),
    snapshot,
    devicePage: gameId === undefined,
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
    case "PluginDefault":
      return `${pluginTitle(context, source.payload.pluginId)} (default)`
    case "DeviceDefault":
      return "Device (default)"
  }
}

/** A plugin's title by its id: a runner's, a modifier's or a service's. */
function pluginTitle(context: Context, pluginId: string): string {
  const runner = context.snapshot.runners.find(candidate => candidate.id === pluginId)
  if (runner !== undefined) return context.runnerTitles.get(runner.id) ?? runner.title
  return [...context.snapshot.modifiers, ...context.snapshot.services].find(candidate => candidate.id === pluginId)?.title ?? pluginId
}

/** A runner korrid never offers as a game's runner (Moonlight): its
 * settings apply when this device streams a game from another device. */
const streamRunner = (context: Context, scope: LaunchRunnerScope): boolean =>
  scope._tag === "Runner" && context.snapshot.runners.some(runner => runner.id === scope.id && !runner.choosable)

const streamLine = (context: Context) =>
  context.devicePage ? "Applies when this device streams a game" : "Applies when this device streams the game"

/** Whether a place's scope applies to the runner this game launches with. */
function applies(context: Context, scope: LaunchRunnerScope): boolean {
  switch (scope._tag) {
    case "Any": return true
    case "Family": return context.selected?.family === scope.id
    case "Runner": return context.selected?.id === scope.id
  }
}

/** When a place's values would apply, said instead of a source while they do not. */
function notInForceScope(context: Context, scope: LaunchRunnerScope): string | undefined {
  if (streamRunner(context, scope)) return streamLine(context)
  if (applies(context, scope)) return undefined
  const name = context.selected === undefined ? undefined : context.runnerTitles.get(context.selected.id) ?? context.selected.title
  return name === undefined ? "Applies once a runner is chosen" : `Not in force: ${name} runs this game`
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
  const waiting = notInForceScope(context, here.scope)
  if (waiting !== undefined) return waiting
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
  readonly pending: Map<string, { readonly placeId: string; readonly setting: PendingSetting }>
  readonly clears: Map<string, string>
  readonly defaults: Map<string, string>
}

const emptyRows = (): Rows => ({ writes: new Map(), adds: new Map(), pending: new Map(), clears: new Map(), defaults: new Map() })

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

/** The runner choice, on places that may hold one. A runner korrid never
 * offers for a game (Moonlight) is no choice here. */
function runnerGroup(context: Context, rows: Rows, placeId: string, place: LaunchPlace, body: Json): SurfaceSettingGroup {
  const own = typeof body.runner === "string" ? body.runner : undefined
  const id = LAUNCH_ID.set(placeId, "runner")
  const installed = context.snapshot.runners.filter(runner => runner.choosable).map(runner => ({ value: runner.id, label: context.runnerTitles.get(runner.id) ?? runner.title }))
  const missing = own !== undefined && !installed.some(choice => choice.value === own)
  const streamer = own === undefined ? undefined : context.snapshot.runners.find(runner => runner.id === own && !runner.choosable)
  const missingLabel = streamer === undefined ? `${own} (not installed)` : `${context.runnerTitles.get(streamer.id) ?? streamer.title} (streams only)`
  const choices = [INHERIT_CHOICE, ...installed, ...(missing ? [{ value: own, label: missingLabel }] : [])]
  rows.writes.set(id, { place, path: { _tag: "Runner" }, encode: text => text === INHERIT ? {} : { value: text } })
  const source = sourceText(context, place, context.resolved({ _tag: "Runner" }), value =>
    typeof value === "string" ? context.runnerTitles.get(value) ?? value : valueText(value))
  return {
    title: "Runner",
    items: [{
      id,
      label: "Runner",
      value: choices.find(choice => choice.value === (own ?? INHERIT))?.label ?? "Inherit",
      description: streamer !== undefined
        ? "This runner only views streams; it runs no game. Korri skips it here."
        : missing
        ? "This runner isn't installed. Korri skips it and its runner settings here."
        : "The most specific place that names an installed runner wins.",
      ...(source === undefined ? {} : { source }),
      scope: "shared",
      interaction: { kind: "choice", choices, selected: own ?? INHERIT },
    }],
  }
}

// --- plugin settings -------------------------------------------------------

/** One setting a plugin's schema describes: a leaf, named by its dotted path. */
interface Leaf {
  readonly path: string
  /** The row's label, from the schema's titles of the leaf and the objects
   * between it and its section ("HDR", "Resolution width"). */
  readonly label: string
  readonly type?: string
  readonly description?: string
  readonly enum?: readonly unknown[]
  readonly minimum?: number
  readonly maximum?: number
  /** The plugin's own default, when its schema states one. */
  readonly default?: { readonly value: unknown }
  readonly device: boolean
}

/** A top-level section of a plugin's schema; `key` "" holds leaves outside any section. */
interface Section {
  readonly key: string
  readonly title: string
  readonly leaves: readonly Leaf[]
}

/** Short words a schema key spells in capitals when it has no title. */
const ACRONYMS = new Set(["fps", "hdr", "sdr", "ip", "qp", "fec", "av1", "hevc", "ds4", "ds5", "vk", "cpu", "gpu", "id", "url"])

/** A key as words, when its schema gives no title: "max-nits" is "Max nits". */
function humanize(key: string): string {
  const words = key.split(/[-_]/).filter(word => word !== "").map(word => ACRONYMS.has(word) ? word.toUpperCase() : word)
  const text = words.join(" ")
  return text === "" ? key : `${text.charAt(0).toUpperCase()}${text.slice(1)}`
}

/** A title inside a label: "Max nits" reads "max nits" after "HDR"; "SDR nits" keeps its capitals. */
const inner = (title: string) => /^[A-Z][a-z]/.test(title) ? `${title.charAt(0).toLowerCase()}${title.slice(1)}` : title

/**
 * A leaf's label from its own title and the titles of the objects between
 * it and its section. An `enable` leaf is named by its feature ("HDR"); a
 * leaf whose title already names its object keeps its title alone.
 */
function leafLabel(objects: readonly string[], key: string, title: string | undefined): string {
  const own = title ?? humanize(key)
  if (objects.length === 0) return own
  if (key === "enable" && (title === undefined || /^enabled?$/i.test(title))) return [objects[0] ?? "", ...objects.slice(1).map(inner)].join(" ")
  const named = objects.filter(object => !own.toLowerCase().includes(object.toLowerCase()))
  const [first, ...rest] = [...named, own]
  return [first ?? own, ...rest.map(inner)].join(" ")
}

const typeOf = (node: Json): string | undefined => {
  const type = Array.isArray(node.type) ? node.type.find(item => item !== "null") : node.type
  return typeof type === "string" ? type : undefined
}
const isObjectNode = (node: Json) => typeOf(node) === "object" || (node.type === undefined && record(node.properties) !== undefined)
const titleOf = (node: Json, key: string) => typeof node.title === "string" ? node.title : humanize(key)

function leafOf(node: Json, segments: readonly string[], objects: readonly string[]): Leaf {
  const key = segments[segments.length - 1] ?? ""
  const type = typeOf(node)
  return {
    path: segments.join("."),
    label: leafLabel(objects, key, typeof node.title === "string" ? node.title : undefined),
    ...(type === undefined ? {} : { type }),
    ...(typeof node.description === "string" ? { description: node.description } : {}),
    ...(Array.isArray(node.enum) ? { enum: node.enum } : {}),
    ...(typeof node.minimum === "number" ? { minimum: node.minimum } : {}),
    ...(typeof node.maximum === "number" ? { maximum: node.maximum } : {}),
    ...("default" in node ? { default: { value: node.default } } : {}),
    device: node["x-korri-scope"] === "device",
  }
}

/** Every leaf under an object, in schema order. A name never holds a dot;
 * one that does is skipped, as korrid skips it. */
function leavesOf(node: Json, segments: readonly string[], objects: readonly string[]): Leaf[] {
  return Object.entries(record(node.properties) ?? {}).flatMap(([key, raw]) => {
    const child = record(raw)
    if (child === undefined || key.includes(".") || key === "") return []
    return isObjectNode(child)
      ? leavesOf(child, [...segments, key], [...objects, titleOf(child, key)])
      : [leafOf(child, [...segments, key], objects)]
  })
}

/** A plugin's nested settings.describe schema as sections of leaves, in schema order. */
export function sectionsOf(schema: unknown): readonly Section[] {
  const loose: Leaf[] = []
  const sections: Section[] = []
  for (const [key, raw] of Object.entries(record(record(schema)?.properties) ?? {})) {
    const node = record(raw)
    if (node === undefined || key.includes(".") || key === "") continue
    if (isObjectNode(node)) sections.push({ key, title: titleOf(node, key), leaves: leavesOf(node, [key], []) })
    else loose.push(leafOf(node, [key], []))
  }
  return loose.length === 0 ? sections : [{ key: "", title: "", leaves: loose }, ...sections]
}

/** Several schemas as one: a section by its key, a leaf by its path, the first to name one wins. */
function mergeSections(all: readonly (readonly Section[])[]): readonly Section[] {
  const merged = new Map<string, { title: string; leaves: Map<string, Leaf> }>()
  for (const sections of all) {
    for (const section of sections) {
      const entry = merged.get(section.key) ?? { title: section.title, leaves: new Map<string, Leaf>() }
      for (const leaf of section.leaves) if (!entry.leaves.has(leaf.path)) entry.leaves.set(leaf.path, leaf)
      merged.set(section.key, entry)
    }
  }
  return [...merged].map(([key, entry]) => ({ key, title: entry.title, leaves: [...entry.leaves.values()] }))
}

/**
 * A place's own nested settings as leaves by dotted path, in file order. A
 * `null` leaf is a Cleared setting. A name with a dot is skipped, as korrid
 * skips it.
 */
function ownLeaves(settings: unknown, prefix: readonly string[] = []): Map<string, unknown> {
  const leaves = new Map<string, unknown>()
  for (const [key, value] of Object.entries(record(settings) ?? {})) {
    if (key.includes(".") || key === "") continue
    const nested = record(value)
    if (nested !== undefined) for (const [path, leaf] of ownLeaves(nested, [...prefix, key])) leaves.set(path, leaf)
    else leaves.set([...prefix, key].join("."), value)
  }
  return leaves
}

/** The section a dotted path belongs to: its first name, or "" for a loose leaf. */
const sectionKeyOf = (path: string) => path.includes(".") ? path.slice(0, path.indexOf(".")) : ""

/** The settings a runner place may set: its runner's schema, or for a family
 * place every installed runner of the family (the first to name a leaf wins). */
function schemaFor(context: Context, scope: LaunchRunnerScope): readonly Section[] {
  if (scope._tag === "Any") return []
  const runners = context.snapshot.runners.filter(runner => scope._tag === "Runner" ? runner.id === scope.id : runner.family === scope.id)
  return mergeSections(runners.map(runner => sectionsOf(runner.schema)))
}

const choiceValue = (value: unknown) => JSON.stringify(value)
/** What every plugin setting's choice offers first. */
const KEEP_CHOICES: readonly SurfaceSettingChoice[] = [INHERIT_CHOICE, DEFAULT_CHOICE]

/**
 * How one value is changed, from its schema leaf or else its own type. A
 * choice offers Inherit and Default before its values; a number or a string
 * gets its Default from a row of its own (settingRows).
 */
function valueInteraction(leaf: Leaf | undefined, own: unknown, cleared: boolean): {
  readonly interaction: SurfaceSettingInteraction
  readonly encode: (text: string) => LaunchEncoded | null
} {
  const keep = (text: string): LaunchEncoded | undefined => text === INHERIT ? {} : text === DEFAULT ? { clear: true } : undefined
  const selectedOf = (value: string) => cleared ? DEFAULT : own === undefined ? INHERIT : value
  if (leaf?.enum !== undefined && leaf.enum.length > 0) {
    const values = leaf.enum
    return {
      interaction: {
        kind: "choice",
        choices: [...KEEP_CHOICES, ...values.map(value => ({ value: choiceValue(value), label: valueText(value) }))],
        selected: selectedOf(choiceValue(own)),
      },
      encode: text => {
        const kept = keep(text)
        if (kept !== undefined) return kept
        const found = values.find(value => choiceValue(value) === text)
        return found === undefined ? null : { value: found }
      },
    }
  }
  const type = leaf?.type ?? (typeof own === "boolean" ? "boolean" : typeof own === "number" ? "number" : "string")
  if (type === "boolean") {
    return {
      interaction: {
        kind: "choice",
        choices: [...KEEP_CHOICES, { value: "true", label: "On" }, { value: "false", label: "Off" }],
        selected: selectedOf(own === true ? "true" : "false"),
      },
      encode: text => keep(text) ?? (text === "true" ? { value: true } : text === "false" ? { value: false } : null),
    }
  }
  if (type === "number" || type === "integer") {
    const integer = type === "integer"
    return {
      interaction: {
        kind: "number",
        ...(typeof own === "number" ? { value: own } : {}),
        ...(leaf?.minimum === undefined ? {} : { min: leaf.minimum }),
        ...(leaf?.maximum === undefined ? {} : { max: leaf.maximum }),
        ...(integer ? { step: 1 } : {}),
        clearable: true,
      },
      encode: text => {
        if (text.trim() === "") return {}
        const number = Number(text)
        if (!Number.isFinite(number) || (integer && !Number.isInteger(number))) return null
        if (leaf?.minimum !== undefined && number < leaf.minimum) return null
        if (leaf?.maximum !== undefined && number > leaf.maximum) return null
        return { value: number }
      },
    }
  }
  // A string: what is typed is the value, an empty one too. Its Inherit is
  // a row of its own (settingRows), because "" is a value here.
  return {
    interaction: { kind: "text", placeholder: leaf?.label ?? "Value", allowEmpty: true },
    encode: text => ({ value: text }),
  }
}

/** Whose settings a run of groups holds, and how its rows name, write and add them. */
interface Owner {
  readonly placeId: string
  readonly place: LaunchPlace
  /** The plugin's title, the first half of each group's title. */
  readonly title: string
  readonly sections: readonly Section[]
  /** This place's own nested settings for this plugin. */
  readonly own: unknown
  /** Paths added with Add setting and not yet given a value. */
  readonly added: readonly string[]
  readonly pathOf: (key: string) => LaunchPath
  /** A row's id from its path within the place, such as "setting:video.fps". */
  readonly rowId: (path: string) => string
  readonly rowPath: (key: string) => string
  readonly addPath: (section: string) => string
  readonly pendingOf: (key: string) => PendingSetting
  readonly scopeOf: (leaf: Leaf | undefined) => "device" | "shared"
  /** Said on a value the schema no longer describes. */
  readonly missing: string
  /** The plugin's default when korrid did not resolve the path (Moonlight on a game page). */
  readonly defaultOf: (leaf: Leaf | undefined) => string | undefined
}

/**
 * Where a setting's value in force comes from, from one place's row. A
 * Cleared setting names the plugin's default; a value korrid skips says it
 * is not applied and why.
 */
function settingSource(context: Context, owner: Owner, path: string, leaf: Leaf | undefined, own: { readonly set: boolean; readonly cleared: boolean }): string | undefined {
  const fallback = () => owner.defaultOf(leaf) ?? `${owner.title} decides`
  if (streamRunner(context, owner.place.scope)) return own.cleared ? fallback() : streamLine(context)
  const waiting = notInForceScope(context, owner.place.scope)
  if (waiting !== undefined) return waiting
  const writePath = owner.pathOf(path)
  const resolved = context.resolved(writePath)
  const source = resolved?.source
  const skipped = context.skipped(writePath)
  const here = source?._tag === "Place" && samePlace(source.payload, owner.place)
  if (skipped !== undefined && (here || (source === undefined && own.set && !own.cleared))) return `Not applied: ${skipped.replace(/\.$/, "")}`
  if (resolved === undefined || source === undefined || resolved.value === null) return own.cleared ? fallback() : undefined
  const title = sourceTitle(context, source)
  const value = valueText(resolved.value)
  if (skipped !== undefined) return `${value} from ${title} isn't applied: ${skipped.replace(/\.$/, "")}`
  if (resolved.adjusted) return `${value} from ${title}, adjusted`
  return here ? `from ${title}` : `${value} from ${title}`
}

/**
 * One setting's rows on one place: the setting, then for a string its
 * Inherit row (an empty string is a value of its own), then for a number or
 * a string a Default row, unless the place already clears it.
 */
function settingRows(context: Context, rows: Rows, owner: Owner, path: string, leaf: Leaf | undefined, own: ReadonlyMap<string, unknown>): SurfaceSettingItem[] {
  const set = own.has(path)
  const value = own.get(path)
  const cleared = set && value === null
  const id = owner.rowId(owner.rowPath(path))
  const { interaction, encode } = valueInteraction(leaf, set && !cleared ? value : undefined, cleared)
  rows.writes.set(id, { place: owner.place, path: owner.pathOf(path), encode })
  if (!set) rows.pending.set(id, { placeId: owner.placeId, setting: owner.pendingOf(path) })
  const source = settingSource(context, owner, path, leaf, { set, cleared })
  const label = leaf?.label ?? humanize(path.slice(path.lastIndexOf(".") + 1))
  const text = interaction.kind === "text"
  const shown: Pick<SurfaceSettingItem, "value"> = cleared
    ? text ? {} : { value: "Default" }
    : text
      ? typeof value === "string" && value !== "" && set ? { value } : {}
      : { value: set ? valueText(value) : "Not set here" }
  const description = leaf?.description ?? (leaf === undefined ? owner.missing : undefined)
  const row: SurfaceSettingItem = {
    id,
    label,
    ...shown,
    ...(description === undefined ? {} : { description }),
    ...(source === undefined ? {} : { source }),
    scope: owner.scopeOf(leaf),
    // The label for an empty string stays out of its value: the editor starts from the raw text.
    interaction: text && interaction.kind === "text"
      ? { ...interaction, placeholder: cleared ? "Default" : set ? "Empty" : "Not set here" }
      : interaction,
  }
  const extra: SurfaceSettingItem[] = []
  if (text) {
    const clear = owner.rowId(`clear:${owner.rowPath(path)}`)
    rows.clears.set(clear, id)
    extra.push({
      id: clear,
      label: `Inherit ${label}`,
      value: "Clear",
      description: "Takes the value set here away, so the value in force comes from elsewhere.",
      interaction: { kind: "action", actionId: clear },
    })
  }
  if (interaction.kind !== "choice" && !cleared) {
    const reset = owner.rowId(`default:${owner.rowPath(path)}`)
    rows.defaults.set(reset, id)
    extra.push({
      id: reset,
      label: `Default ${label}`,
      value: "Use",
      description: `${owner.title}'s own default holds here, whatever less specific places set.`,
      interaction: { kind: "action", actionId: reset },
    })
  }
  return [row, ...extra]
}

/**
 * One plugin's settings on one place: a group per section of its schema,
 * "Moonlight · Video", each holding the leaves set here, then those added
 * with Add setting and not yet given a value, then its own Add setting row
 * for the leaves left. Leaves set here that the schema does not describe
 * follow in their own sections.
 */
function settingGroups(context: Context, rows: Rows, owner: Owner): SurfaceSettingGroup[] {
  const own = ownLeaves(owner.own)
  const described = new Map(owner.sections.flatMap(section => section.leaves.map(leaf => [leaf.path, leaf] as const)))
  const extraKeys = [...new Set([...own.keys(), ...owner.added].filter(path => !described.has(path)).map(sectionKeyOf))]
    .filter(key => !owner.sections.some(section => section.key === key))
  const sections: readonly Section[] = [...owner.sections, ...extraKeys.map(key => ({ key, title: key === "" ? "" : humanize(key), leaves: [] }))]
  return sections.flatMap((section): SurfaceSettingGroup[] => {
    const inSection = (path: string) => sectionKeyOf(path) === section.key
    const ownHere = [
      ...section.leaves.filter(leaf => own.has(leaf.path)).map(leaf => leaf.path),
      ...[...own.keys()].filter(path => inSection(path) && !described.has(path)),
    ]
    const addedHere = owner.added.filter(path => inSection(path) && !own.has(path))
    const shown = [...ownHere, ...addedHere]
    const items = shown.flatMap(path => settingRows(context, rows, owner, path, described.get(path), own))
    const unset = section.leaves.filter(leaf => !shown.includes(leaf.path))
    if (unset.length > 0) {
      const id = owner.rowId(owner.addPath(section.key))
      rows.adds.set(id, { placeId: owner.placeId, pendingOf: owner.pendingOf })
      items.push({
        id,
        label: "Add setting",
        value: "Choose a setting",
        description: `${unset.length} more ${unset.length === 1 ? "setting" : "settings"} ${owner.title} offers here.`,
        interaction: { kind: "choice", choices: unset.map(leaf => ({ value: leaf.path, label: leaf.label })), selected: "" },
      })
    }
    if (items.length === 0) return []
    return [{ title: section.key === "" ? owner.title : `${owner.title} · ${section.title}`, items }]
  })
}

/** "h264 from Moonlight (default)", from a schema's default. */
const schemaDefault = (title: string) => (leaf: Leaf | undefined) =>
  leaf?.default === undefined ? undefined : `${valueText(leaf.default.value)} from ${title} (default)`

/** The title a runner place's groups carry: its runner, or its family. */
function runnerOwnerTitle(context: Context, scope: LaunchRunnerScope): string {
  if (scope._tag === "Runner") return context.runnerTitles.get(scope.id) ?? scope.id
  return scopeLabel(context, scope)
}

/** A runner place's settings: one group per section of its runner's schema. */
function runnerSettingGroups(
  context: Context,
  rows: Rows,
  placeId: string,
  place: LaunchPlace,
  body: Json,
  pending: readonly PendingSetting[],
  rowId: (path: string) => string,
): SurfaceSettingGroup[] {
  const title = runnerOwnerTitle(context, place.scope)
  return settingGroups(context, rows, {
    placeId,
    place,
    title,
    sections: schemaFor(context, place.scope),
    own: body.settings,
    added: pending.flatMap(setting => setting._tag === "Runner" ? [setting.key] : []),
    pathOf: key => ({ _tag: "Setting", payload: { key } }),
    rowId,
    rowPath: key => `setting:${key}`,
    addPath: section => `add-setting:${section}`,
    pendingOf: key => ({ _tag: "Runner", key }),
    scopeOf: leaf => leaf?.device === true || streamRunner(context, place.scope) ? "device" : "shared",
    missing: "This runner no longer describes this setting.",
    defaultOf: schemaDefault(title),
  })
}

const ON_OFF = [INHERIT_CHOICE, { value: "true", label: "On" }, { value: "false", label: "Off" }]

interface Plugin { readonly id: string; readonly title: string; readonly schema?: unknown; readonly listed: boolean }

/**
 * Plugins a place body names under one kind (modifiers, services): every
 * installed one korrid lists, then any a place names that it does not list.
 */
function pluginsOf(context: Context, listed: readonly (LaunchModifierSnapshot | LaunchServiceSnapshot)[], kind: "modifiers" | "services"): readonly Plugin[] {
  const known = listed.map(plugin => ({ id: plugin.id, title: plugin.title, ...(plugin.schema === undefined ? {} : { schema: plugin.schema }), listed: true }))
  const named = new Set<string>()
  for (const place of context.places) for (const id of Object.keys(record(record(place.body)?.[kind]) ?? {})) named.add(id)
  const unlisted = [...named].filter(id => !known.some(plugin => plugin.id === id)).sort()
  return [...known, ...unlisted.map(id => ({ id, title: id, listed: false }))]
}

/**
 * Per modifier: its on, off or Inherit row in a group of its own, then its
 * settings as a runner's are, one group per section. A modifier with no
 * schema has no settings to show.
 */
function modifierGroups(context: Context, rows: Rows, placeId: string, place: LaunchPlace, body: Json, pending: readonly PendingSetting[], rowId: (path: string) => string): SurfaceSettingGroup[] {
  const own = record(body.modifiers) ?? {}
  return pluginsOf(context, context.snapshot.modifiers, "modifiers").flatMap((modifier): SurfaceSettingGroup[] => {
    const authored = record(own[modifier.id]) ?? {}
    const enabled = typeof authored.enabled === "boolean" ? authored.enabled : undefined
    const id = rowId(`modifier:${modifier.id}:enabled`)
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
    const settings = settingGroups(context, rows, {
      placeId,
      place,
      title: modifier.title,
      sections: modifier.schema === undefined ? [] : sectionsOf(modifier.schema),
      own: authored.settings,
      added: pending.flatMap(setting => setting._tag === "Modifier" && setting.id === modifier.id ? [setting.key] : []),
      pathOf: key => ({ _tag: "ModifierSetting", payload: { id: modifier.id, key } }),
      rowId,
      rowPath: key => `modifier:${modifier.id}:setting:${key}`,
      addPath: section => `modifier:${modifier.id}:add-setting:${section}`,
      pendingOf: key => ({ _tag: "Modifier", id: modifier.id, key }),
      scopeOf: () => "device",
      missing: "This modifier no longer describes this setting.",
      defaultOf: schemaDefault(modifier.title),
    })
    return [{ title: `Modifier ${modifier.title}`, items: [enabledRow] }, ...settings]
  })
}

/**
 * Launch services (Sunshine): their settings, one group per section, on
 * every place. They stay on this device, as modifier settings do, and take
 * effect before the game starts.
 */
function serviceGroups(context: Context, rows: Rows, placeId: string, place: LaunchPlace, body: Json, pending: readonly PendingSetting[], rowId: (path: string) => string): SurfaceSettingGroup[] {
  const own = record(body.services) ?? {}
  return pluginsOf(context, context.snapshot.services, "services").flatMap(service => settingGroups(context, rows, {
    placeId,
    place,
    title: service.title,
    sections: service.schema === undefined ? [] : sectionsOf(service.schema),
    own: record(own[service.id])?.settings,
    added: pending.flatMap(setting => setting._tag === "Service" && setting.id === service.id ? [setting.key] : []),
    pathOf: key => ({ _tag: "ServiceSetting", payload: { id: service.id, key } }),
    rowId,
    rowPath: key => `service:${service.id}:setting:${key}`,
    addPath: section => `service:${service.id}:add-setting:${section}`,
    pendingOf: key => ({ _tag: "Service", id: service.id, key }),
    scopeOf: () => "device",
    missing: service.listed ? "This service no longer describes this setting." : "This service isn't installed on this device.",
    defaultOf: schemaDefault(service.title),
  }))
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
  const unavailable = snapshot.notices.flatMap(notice => {
    if (notice._tag !== "RunnerUnavailable") return []
    const place = sourceTitle(context, { _tag: "Place", payload: notice.payload.place })
    return [`${place} names runner ${notice.payload.runnerId}, which isn't installed. Korri skips it and its runner settings there${inUse === undefined ? "." : ` and uses ${inUse}.`}`]
  })
  const skipped = snapshot.notices.flatMap(notice => notice._tag === "SettingSkipped"
    ? [`${skippedName(context, notice.payload.path)} isn't applied: ${notice.payload.message.replace(/\.$/, "")}.`]
    : [])
  const facts = snapshot.runners.flatMap(runner => runner.prepare._tag === "Unavailable"
    ? [`${context.runnerTitles.get(runner.id) ?? runner.title} facts are unavailable: ${runner.prepare.payload.replace(/\.$/, "")}.`]
    : [])
  return [...route, ...unavailable, ...skipped, ...facts]
}

/** A skipped setting in words: its label and its plugin, "FPS (Moonlight)". */
function skippedName(context: Context, path: LaunchPath): string {
  const named = (title: string, schema: unknown, key: string) => {
    const leaf = sectionsOf(schema).flatMap(section => section.leaves).find(candidate => candidate.path === key)
    return `${leaf?.label ?? key} (${title})`
  }
  switch (path._tag) {
    case "Setting": {
      const runner = context.selected
      return runner === undefined ? path.payload.key : named(context.runnerTitles.get(runner.id) ?? runner.title, runner.schema, path.payload.key)
    }
    case "ModifierSetting":
    case "ServiceSetting": {
      const { id, key } = path.payload
      const plugin = [...context.snapshot.modifiers, ...context.snapshot.services].find(candidate => candidate.id === id)
      return named(plugin?.title ?? id, plugin?.schema, key)
    }
    default:
      return path._tag
  }
}

/**
 * One game's editor: a place per layer and runner scope, most general first.
 * `pending` holds settings the person added with Add setting, by place id,
 * shown until they are set or the editor closes.
 */
export function launchEditorFrom(
  snapshot: LaunchSnapshot,
  gameId: string | undefined,
  pending: ReadonlyMap<string, readonly PendingSetting[]> = new Map(),
): LaunchEditor {
  const context = contextOf(snapshot, gameId)
  const rows = emptyRows()
  const places = context.places.map((entry): SurfaceLaunchSettingsPlace => {
    const { place } = entry
    const placeId = placeIdOf(place)
    const body = record(entry.body) ?? {}
    const kinds = entry.editableKinds
    const any = place.scope._tag === "Any"
    const added = pending.get(placeId) ?? []
    const rowId = (path: string) => LAUNCH_ID.set(placeId, path)
    const groups: (SurfaceSettingGroup | undefined)[] = [
      // What a place is for first: the runner choice, or the runner's settings.
      any && kinds.includes(KIND.runner) ? runnerGroup(context, rows, placeId, place, body) : undefined,
      ...(kinds.includes(KIND.settings) ? runnerSettingGroups(context, rows, placeId, place, body, added, rowId) : []),
      ...limitGroups(context, rows, place, body, kinds, key => LAUNCH_ID.set(placeId, key)),
      ...(kinds.includes(KIND.modifiers) ? modifierGroups(context, rows, placeId, place, body, added, rowId) : []),
      ...(kinds.includes(KIND.services) ? serviceGroups(context, rows, placeId, place, body, added, rowId) : []),
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
  return { places, notes: launchNotesFrom(snapshot, gameId), ...rows }
}

/** Device settings' launch groups and how each of their rows writes. */
export interface DeviceLaunch extends Omit<LaunchEditor, "places" | "notes"> {
  readonly groups: readonly SurfaceSettingGroup[]
}

/**
 * Device settings' launch groups, from a snapshot without a game: "Launch
 * settings" (the device's clock limits), then the settings of each runner
 * korrid never offers for a game (Moonlight) from its Device place, then
 * each launch service's (Sunshine) from the Device place for any runner.
 * Each plugin has a group per section, with Add setting rows as in a game's
 * editor; `pending` holds the settings added and not yet set, by place id.
 */
export function deviceLaunchFrom(snapshot: LaunchSnapshot, pending: ReadonlyMap<string, readonly PendingSetting[]> = new Map()): DeviceLaunch {
  const context = contextOf(snapshot)
  const rows = emptyRows()
  const entry = context.places.find(place => place.place.layer._tag === "Device" && place.place.scope._tag === "Any")
  const place: LaunchPlace = entry?.place ?? { layer: { _tag: "Device" }, scope: { _tag: "Any" } }
  const limits = limitGroups(context, rows, place, record(entry?.body) ?? {}, [KIND.cpu, KIND.gpu], LAUNCH_ID.device)
  const items = limits.flatMap(group => group.items)
  const governor = items.find(item => item.id === LAUNCH_ID.device("cpu.governor"))
  const launch: SurfaceSettingGroup = { title: "Launch settings", items: [...items.filter(item => item !== governor), ...(governor === undefined ? [] : [governor])] }
  const streams = context.places.filter(candidate => candidate.place.layer._tag === "Device" && streamRunner(context, candidate.place.scope) && candidate.editableKinds.includes(KIND.settings))
    .flatMap(candidate => {
      const placeId = placeIdOf(candidate.place)
      return runnerSettingGroups(context, rows, placeId, candidate.place, record(candidate.body) ?? {}, pending.get(placeId) ?? [], path => LAUNCH_ID.deviceSet(placeId, path))
    })
  const services = entry === undefined || !entry.editableKinds.includes(KIND.services) ? [] : (() => {
    const placeId = placeIdOf(entry.place)
    return serviceGroups(context, rows, placeId, entry.place, record(entry.body) ?? {}, pending.get(placeId) ?? [], path => LAUNCH_ID.deviceSet(placeId, path))
  })()
  return { groups: [launch, ...streams, ...services], ...rows }
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
