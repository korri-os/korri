/**
 * The in-memory brain's launch settings, for browser dev and tests.
 *
 * Shapes are korrid's generated wire types (`app.launch-settings.*`) and
 * nothing else. Resolution follows ADR 0028: a place is a layer (Device,
 * System, Game, Release, Profile) limited to a runner scope (any, a family,
 * a runner), merged in the ADR's order, later wins; the runner is chosen
 * first from the places that may name one. Device facts are absent by
 * default, as in a browser that cannot read cpufreq; seed them to show a
 * device's real clock steps.
 *
 * The runner choice the runner chooser reads (`gameRoutes`) comes from the
 * same places, so the chooser and the editor never disagree.
 *
 * Plugin settings are nested (ADR 0037) and fold one leaf at a time; `null`
 * at a leaf is a Cleared setting, so the plugin's schema default holds. A
 * resolved value that its plugin's schema does not take is kept and named
 * in a SettingSkipped notice, as korrid warns and skips it. A runner seeded
 * `choosable: false` (Moonlight) is listed on every snapshot, the Device
 * settings' too, and is never a game's runner.
 */
import type {
  GameRoute,
  LaunchDeviceFacts,
  LaunchLayer,
  LaunchContribution,
  LaunchNotice,
  LaunchPath,
  LaunchPlace,
  LaunchPlaceSnapshot,
  LaunchProfileRemoveRequest,
  LaunchProfileSaveRequest,
  LaunchProfileSelectRequest,
  LaunchRunnerFacts,
  LaunchRunnerScope,
  LaunchRunnerSnapshot,
  LaunchSetRequest,
  LaunchSettingsRouteLayers,
  LaunchSettingsSnapshotOutcome,
  LaunchSettingsWriteOutcome,
  LaunchSnapshot,
  LaunchSourcedValue,
  LaunchValuePath,
  RpcFailure,
  RunnerChoiceRevisions,
} from "@contracts/generated/korrid"
import { placeKey } from "./launch-place-key"

/** One installed runner as browser dev seeds it. */
export interface InMemoryLaunchRunner {
  readonly title: string
  readonly family?: string
  /** Its settings.describe JSON Schema. Absent: no settings. */
  readonly schema?: unknown
  /** Its dry-run launch.prepare. Absent: nothing set. */
  readonly prepare?: LaunchRunnerFacts
  /** False for a runner that is never a game's runner (Moonlight). Absent: true. */
  readonly choosable?: boolean
}

/** The places that apply to one game, beside Device. */
export interface InMemoryLaunchGame {
  readonly systemId?: string
  readonly releaseId?: string
  /** Installed runners able to play it, by id. Absent: the routes' runners. */
  readonly runners?: readonly string[]
  /** Whether korrid could give the route's places. Absent: Present. */
  readonly routeLayers?: LaunchSettingsRouteLayers
}

export interface InMemoryLaunchProfile {
  readonly id: string
  readonly title: string
}

/** One installed, enabled launch modifier as browser dev seeds it. */
export interface InMemoryLaunchModifier {
  readonly title: string
  /** Its settings JSON Schema. Absent: it has no settings. */
  readonly schema?: unknown
}

export interface InMemoryLaunchSettingsConfig {
  readonly deviceFacts?: LaunchDeviceFacts
  readonly runners?: Readonly<Record<string, InMemoryLaunchRunner>>
  /** device.yaml's routines: named hook lists any place may `use`. */
  readonly routines?: Readonly<Record<string, { readonly before?: readonly unknown[]; readonly after?: readonly unknown[] }>>
  /** Installed, enabled launch modifiers, by plugin id. */
  readonly modifiers?: Readonly<Record<string, InMemoryLaunchModifier>>
  /** Installed launch services (Sunshine), by plugin id: a modifier's shape, no switch. */
  readonly services?: Readonly<Record<string, InMemoryLaunchModifier>>
  /** Keyed by korrid game id. A game not listed gets Device and Game places. */
  readonly games?: Readonly<Record<string, InMemoryLaunchGame>>
  /** Seeded place bodies, keyed by `placeKey`. */
  readonly bodies?: Readonly<Record<string, Body>>
  readonly profiles?: Readonly<Record<string, readonly InMemoryLaunchProfile[]>>
  readonly selected?: Readonly<Record<string, string>>
  readonly failures?: Partial<Record<"snapshot" | "set" | "profile", RpcFailure>>
  readonly delayMs?: number
}

export interface LaunchSettingsMethods {
  launchSettingsSnapshot(gameId?: string): Promise<LaunchSettingsSnapshotOutcome>
  setLaunch(request: LaunchSetRequest): Promise<LaunchSettingsWriteOutcome>
  saveLaunchProfile(request: LaunchProfileSaveRequest): Promise<LaunchSettingsWriteOutcome>
  removeLaunchProfile(request: LaunchProfileRemoveRequest): Promise<LaunchSettingsWriteOutcome>
  selectLaunchProfile(request: LaunchProfileSelectRequest): Promise<LaunchSettingsWriteOutcome>
}

/** A place body: the editable kinds and anything a file may hold. */
export interface Body {
  runner?: string
  cpu?: { min?: number; max?: number; governor?: string }
  gpu?: { min?: number; max?: number }
  modifiers?: Record<string, { enabled?: boolean; settings?: Record<string, unknown>; config?: unknown }>
  services?: Record<string, { settings?: Record<string, unknown> }>
  /** Nested by section: `{ video: { fps: 30 } }`; `null` at a leaf clears it. */
  settings?: Record<string, unknown>
  env?: Record<string, string | null>
  cwd?: string
  args?: { prepend?: string[]; append?: string[] }
  hooks?: { before?: unknown[]; after?: unknown[]; use?: string[] }
  config?: unknown
}

const BROWSER_DEFAULTS = { cpu: { min: 0, max: 100 }, gpu: { min: 0, max: 100 }, governor: "schedutil" }
const ANY_KINDS = ["runner", "cpu", "gpu", "modifiers", "services"]
const RUNNER_KINDS = ["cpu", "gpu", "modifiers", "settings", "services"]

type Json = Record<string, unknown>
const record = (value: unknown): Json | undefined =>
  typeof value === "object" && value !== null && !Array.isArray(value) ? value as Json : undefined
const isObjectNode = (node: Json) => node.type === "object" || (node.type === undefined && record(node.properties) !== undefined)

/** A nested schema's leaves by dotted path. */
function schemaLeaves(schema: unknown, prefix: readonly string[] = []): Map<string, Json> {
  const leaves = new Map<string, Json>()
  for (const [key, raw] of Object.entries(record(record(schema)?.properties) ?? {})) {
    const node = record(raw)
    if (node === undefined) continue
    if (isObjectNode(node)) for (const [path, leaf] of schemaLeaves(node, [...prefix, key])) leaves.set(path, leaf)
    else leaves.set([...prefix, key].join("."), node)
  }
  return leaves
}

/** A nested settings object's leaves by dotted path; a name with a dot is skipped. */
function valueLeaves(settings: unknown, prefix: readonly string[] = []): Map<string, unknown> {
  const leaves = new Map<string, unknown>()
  for (const [key, value] of Object.entries(record(settings) ?? {})) {
    if (key.includes(".")) continue
    const nested = record(value)
    if (nested !== undefined) for (const [path, leaf] of valueLeaves(nested, [...prefix, key])) leaves.set(path, leaf)
    else leaves.set([...prefix, key].join("."), value)
  }
  return leaves
}

/** Settings with one leaf written (`null` clears it) or taken away (`undefined`); empty objects go. */
function withLeaf(settings: Record<string, unknown> | undefined, segments: readonly string[], value: unknown): Record<string, unknown> {
  const [head, ...rest] = segments
  const next: Record<string, unknown> = { ...settings }
  if (head === undefined) return next
  if (rest.length === 0) {
    if (value === undefined) delete next[head]
    else next[head] = value
    return next
  }
  const child = withLeaf(record(next[head]), rest, value)
  if (Object.keys(child).length === 0) delete next[head]
  else next[head] = child
  return next
}

/** Why a value doesn't fit its schema leaf, or undefined when it does. */
function misfit(leaf: Json, value: unknown): string | undefined {
  const values = Array.isArray(leaf.enum) ? leaf.enum as unknown[] : undefined
  if (values !== undefined) {
    return values.some(item => JSON.stringify(item) === JSON.stringify(value)) ? undefined : `${JSON.stringify(value)} is not one of ${values.map(item => String(item)).join(", ")}`
  }
  const type = leaf.type
  const wrong = type === "boolean" ? typeof value !== "boolean"
    : type === "integer" ? !Number.isInteger(value)
      : type === "number" ? typeof value !== "number"
        : type === "string" ? typeof value !== "string" : false
  if (wrong) return `${JSON.stringify(value)} is not ${type === "integer" ? "a whole number" : type === "boolean" ? "on or off" : `a ${String(type)}`}`
  if (typeof value === "number" && typeof leaf.minimum === "number" && value < leaf.minimum) return `${value} is below the minimum of ${leaf.minimum}`
  if (typeof value === "number" && typeof leaf.maximum === "number" && value > leaf.maximum) return `${value} is above the maximum of ${leaf.maximum}`
  return undefined
}

type ListKey = "args.prepend" | "args.append" | "hooks.before" | "hooks.after"
const LIST_PATH = {
  "args.prepend": "ArgsPrepend",
  "args.append": "ArgsAppend",
  "hooks.before": "HooksBefore",
  "hooks.after": "HooksAfter",
} as const satisfies Record<ListKey, LaunchValuePath["_tag"]>


export interface InMemoryRunnerChoices {
  readonly gameRunner?: string
  readonly systemRunners: Record<string, string>
  readonly revisions: RunnerChoiceRevisions
  /** The runner the places choose among `candidates`; undefined: Choose. */
  readonly selected?: string
}

export function createInMemoryLaunchSettings(
  config: InMemoryLaunchSettingsConfig = {},
  routesOf: (gameId: string) => readonly GameRoute[] = () => [],
) {
  const bodies = new Map<string, Body>(Object.entries(structuredClone(config.bodies ?? {})))
  const profiles = new Map<string, InMemoryLaunchProfile[]>(
    Object.entries(config.profiles ?? {}).map(([gameId, list]) => [gameId, structuredClone([...list])]),
  )
  const selectedProfiles = new Map<string, string>(Object.entries(config.selected ?? {}))
  let device = 1
  let games = 1
  let nextProfile = 1
  const revisions = (): RunnerChoiceRevisions => ({ device: `device-${device}`, games: `games-${games}` })
  const failure = (code: string, message: string) => ({ _tag: "Err" as const, payload: { code, message } })
  const wait = async () => {
    if (config.delayMs) await new Promise(resolve => setTimeout(resolve, config.delayMs))
  }
  const deviceFile = (layer: LaunchLayer) => layer._tag === "Device" || layer._tag === "System"
  const body = (place: LaunchPlace): Body => bodies.get(placeKey(place)) ?? {}

  /** Runners korrid never offers for a game (Moonlight): listed on every snapshot. */
  const streamRunners = Object.entries(config.runners ?? {}).filter(([, runner]) => runner.choosable === false).map(([id]) => id)
  const choosable = (id: string) => config.runners?.[id]?.choosable !== false

  /** Installed runners able to play a game, then the stream runners, as korrid lists them. */
  const runnersOf = (gameId: string | undefined): readonly { id: string; family?: string; title: string; runner?: InMemoryLaunchRunner }[] => {
    const routes = gameId === undefined ? [] : routesOf(gameId)
    const playable = gameId === undefined ? [] : (config.games?.[gameId]?.runners ?? [...new Set(routes.map(route => route.runnerId))]).filter(choosable)
    const ids = [...playable, ...streamRunners]
    return ids.map(id => {
      const runner = config.runners?.[id]
      const family = runner?.family ?? routes.find(route => route.runnerId === id)?.familyId
      return { id, title: runner?.title ?? id, ...(family === undefined ? {} : { family }), ...(runner === undefined ? {} : { runner }) }
    })
  }
  const systemOf = (gameId: string) => config.games?.[gameId]?.systemId ?? routesOf(gameId)[0]?.systemId

  /** Layers for a game, most general first (a profile only when chosen). */
  const layersOf = (gameId: string | undefined, withProfiles: "all" | "selected"): { layer: LaunchLayer; title: string }[] => {
    const layers: { layer: LaunchLayer; title: string }[] = [{ layer: { _tag: "Device" }, title: "Device" }]
    if (gameId === undefined) return layers
    const game = config.games?.[gameId] ?? {}
    const present = (game.routeLayers?._tag ?? "Present") === "Present"
    const systemId = systemOf(gameId)
    if (present && systemId !== undefined) layers.push({ layer: { _tag: "System", id: systemId }, title: `System ${systemId}` })
    layers.push({ layer: { _tag: "Game", id: gameId }, title: "Game" })
    if (present && game.releaseId !== undefined) {
      layers.push({ layer: { _tag: "Release", id: { gameId, releaseId: game.releaseId } }, title: `Release ${game.releaseId}` })
    }
    for (const profile of profiles.get(gameId) ?? []) {
      if (withProfiles === "selected" && selectedProfiles.get(gameId) !== profile.id) continue
      layers.push({ layer: { _tag: "Profile", id: { gameId, profileId: profile.id } }, title: `Profile ${profile.title}` })
    }
    return layers
  }

  /**
   * Runner first: the places that may name a runner, most specific first.
   * The first naming an installed runner wins; each more specific one naming
   * a runner that is not installed adds a notice.
   */
  const chooseRunner = (gameId: string | undefined, installed: readonly string[]) => {
    const notices: LaunchNotice[] = []
    const candidates = layersOf(gameId, "selected").reverse()
    for (const { layer } of candidates) {
      const place: LaunchPlace = { layer, scope: { _tag: "Any" } }
      const named = body(place).runner
      if (named === undefined) continue
      if (installed.includes(named) && choosable(named)) return { runner: named, notices }
      notices.push({ _tag: "RunnerUnavailable", payload: { runnerId: named, place } })
    }
    // Only when no place names a runner does a sole candidate choose itself.
    const games = installed.filter(choosable)
    return { runner: notices.length === 0 && games.length === 1 ? games[0] : undefined, notices }
  }

  /** Every place in ADR 0028's merge order for the runner in use, later wins. */
  const mergeOrder = (gameId: string | undefined, runner: { id: string; family?: string } | undefined): LaunchPlace[] => {
    const layers = layersOf(gameId, "selected").map(entry => entry.layer)
    const scoped = (layer: LaunchLayer): LaunchPlace[] => runner === undefined ? [] : [
      ...(runner.family === undefined ? [] : [{ layer, scope: { _tag: "Family" as const, id: runner.family } }]),
      { layer, scope: { _tag: "Runner" as const, id: runner.id } },
    ]
    const any = (layer: LaunchLayer): LaunchPlace => ({ layer, scope: { _tag: "Any" } })
    const deviceLayer = layers.find(layer => layer._tag === "Device")
    const systemLayer = layers.find(layer => layer._tag === "System")
    const rest = layers.filter(layer => layer._tag !== "Device" && layer._tag !== "System")
    return [
      ...(deviceLayer === undefined ? [] : [any(deviceLayer)]),
      ...(systemLayer === undefined ? [] : [any(systemLayer)]),
      ...(deviceLayer === undefined ? [] : scoped(deviceLayer)),
      ...(systemLayer === undefined ? [] : scoped(systemLayer)),
      ...rest.flatMap(layer => [any(layer), ...scoped(layer)]),
    ]
  }

  const resolve = (gameId: string | undefined, runner: { id: string; family?: string; runner?: InMemoryLaunchRunner } | undefined) => {
    // Keyed by korrid's dotted path text; each value also names its path.
    const values: Record<string, LaunchSourcedValue> = {}
    const ranks: Record<string, number> = {}
    const editable = (path: LaunchPath): LaunchValuePath => ({ _tag: "Editable", payload: path })
    const LIMITS = { "cpu.min": { _tag: "CpuMin" }, "cpu.max": { _tag: "CpuMax" }, "gpu.min": { _tag: "GpuMin" }, "gpu.max": { _tag: "GpuMax" } } as const
    const defaults = config.deviceFacts?.defaults ?? BROWSER_DEFAULTS
    for (const kind of ["cpu", "gpu"] as const) {
      for (const bound of ["min", "max"] as const) {
        const key = `${kind}.${bound}` as const
        values[key] = { path: editable(LIMITS[key]), value: defaults[kind][bound], source: { _tag: "DeviceDefault" }, adjusted: false }
        ranks[key] = -1
      }
    }
    if (defaults.governor !== undefined) {
      values["cpu.governor"] = { path: editable({ _tag: "CpuGovernor" }), value: defaults.governor, source: { _tag: "DeviceDefault" }, adjusted: false }
    }
    // Each plugin's schema defaults are the base layer a Cleared setting falls back to.
    const pluginDefaults = new Map<string, LaunchSourcedValue>()
    const seedDefaults = (prefix: string, pluginId: string, schema: unknown, pathOf: (key: string) => LaunchPath) => {
      for (const [key, leaf] of schemaLeaves(schema)) {
        if (!("default" in leaf)) continue
        const value: LaunchSourcedValue = { path: editable(pathOf(key)), value: leaf.default, source: { _tag: "PluginDefault", payload: { pluginId } }, adjusted: false }
        pluginDefaults.set(`${prefix}${key}`, value)
        values[`${prefix}${key}`] = value
      }
    }
    if (runner !== undefined) seedDefaults("settings.", runner.id, runner.runner?.schema, key => ({ _tag: "Setting", payload: { key } }))
    for (const [id, modifier] of Object.entries(config.modifiers ?? {})) {
      seedDefaults(`modifiers.${id}.settings.`, id, modifier.schema, key => ({ _tag: "ModifierSetting", payload: { id, key } }))
    }
    for (const [id, service] of Object.entries(config.services ?? {})) {
      seedDefaults(`services.${id}.settings.`, id, service.schema, key => ({ _tag: "ServiceSetting", payload: { id, key } }))
    }
    const lists: Record<ListKey, LaunchContribution[]> = { "args.prepend": [], "args.append": [], "hooks.before": [], "hooks.after": [] }
    mergeOrder(gameId, runner).forEach((place, rank) => {
      const own = body(place)
      const set = (key: string, path: LaunchValuePath, value: unknown) => {
        if (value === undefined) return
        values[key] = { path, value, source: { _tag: "Place", payload: place }, adjusted: false }
        ranks[key] = rank
      }
      /** One plugin setting leaf: a value wins; `null` clears what less specific places set. */
      const leaf = (key: string, path: LaunchPath, value: unknown) => {
        if (value !== null) return set(key, editable(path), value)
        const fallback = pluginDefaults.get(key)
        if (fallback === undefined) delete values[key]
        else values[key] = fallback
        ranks[key] = rank
      }
      const anyScope = place.scope._tag === "Any"
      if (anyScope) set("runner", editable({ _tag: "Runner" }), own.runner)
      set("cpu.min", editable(LIMITS["cpu.min"]), own.cpu?.min)
      set("cpu.max", editable(LIMITS["cpu.max"]), own.cpu?.max)
      if (place.layer._tag === "Device" && anyScope) set("cpu.governor", editable({ _tag: "CpuGovernor" }), own.cpu?.governor)
      set("gpu.min", editable(LIMITS["gpu.min"]), own.gpu?.min)
      set("gpu.max", editable(LIMITS["gpu.max"]), own.gpu?.max)
      for (const [key, value] of valueLeaves(own.settings)) {
        if (!anyScope) leaf(`settings.${key}`, { _tag: "Setting", payload: { key } }, value)
      }
      for (const [id, modifier] of Object.entries(own.modifiers ?? {})) {
        set(`modifiers.${id}.enabled`, editable({ _tag: "ModifierEnabled", payload: { id } }), modifier.enabled)
        for (const [key, value] of valueLeaves(modifier.settings)) {
          leaf(`modifiers.${id}.settings.${key}`, { _tag: "ModifierSetting", payload: { id, key } }, value)
        }
      }
      for (const [id, service] of Object.entries(own.services ?? {})) {
        for (const [key, value] of valueLeaves(service.settings)) {
          leaf(`services.${id}.settings.${key}`, { _tag: "ServiceSetting", payload: { id, key } }, value)
        }
      }
      for (const [key, value] of Object.entries(own.env ?? {})) set(`env.${key}`, { _tag: "Env", payload: { key } }, value)
      if (own.cwd !== undefined) set("cwd", { _tag: "Cwd" }, own.cwd)
      // List kinds concatenate in merge order; a place's routines come before its own steps.
      const add = (key: ListKey, value: readonly unknown[] | undefined, routine?: string) => {
        if (value === undefined || value.length === 0) return
        lists[key].push({ source: { _tag: "Place", payload: place }, value: [...value], ...(routine === undefined ? {} : { routine }) })
      }
      add("args.prepend", own.args?.prepend)
      add("args.append", own.args?.append)
      for (const phase of ["before", "after"] as const) {
        for (const name of own.hooks?.use ?? []) add(`hooks.${phase}`, config.routines?.[name]?.[phase], name)
        add(`hooks.${phase}`, own.hooks?.[phase])
      }
    })
    for (const [key, contributions] of Object.entries(lists) as [ListKey, LaunchContribution[]][]) {
      if (contributions.length === 0) continue
      values[key] = {
        path: { _tag: LIST_PATH[key] },
        value: contributions.flatMap(part => Array.isArray(part.value) ? part.value : [part.value]),
        contributions,
        adjusted: false,
      }
    }
    // Across places the more specific value wins; the other moves to meet it.
    for (const kind of ["cpu", "gpu"] as const) {
      const min = values[`${kind}.min`]
      const max = values[`${kind}.max`]
      if (min === undefined || max === undefined || typeof min.value !== "number" || typeof max.value !== "number" || min.value <= max.value) continue
      if ((ranks[`${kind}.min`] ?? -1) > (ranks[`${kind}.max`] ?? -1)) values[`${kind}.max`] = { ...max, value: min.value, adjusted: true }
      else values[`${kind}.min`] = { ...min, value: max.value, adjusted: true }
    }
    return { values, skipped: skippedOf(values, runner) }
  }

  /**
   * Warn and skip: each plugin setting in force that its plugin's schema
   * does not describe or does not take. The value stays in force here, as
   * it stays in the file; korrid leaves it out of what the plugin receives.
   */
  const skippedOf = (values: Record<string, LaunchSourcedValue>, runner: { id: string; runner?: InMemoryLaunchRunner } | undefined): LaunchNotice[] =>
    Object.values(values).flatMap((resolved): LaunchNotice[] => {
      if (resolved.path._tag !== "Editable" || resolved.source?._tag !== "Place") return []
      const path = resolved.path.payload
      const owner = path._tag === "Setting" ? { title: runner?.runner?.title ?? runner?.id ?? "", schema: runner?.runner?.schema }
        : path._tag === "ModifierSetting" ? config.modifiers?.[path.payload.id]
          : path._tag === "ServiceSetting" ? config.services?.[path.payload.id] : undefined
      if (owner === undefined || path.payload === undefined || !("key" in path.payload)) return []
      const leaf = schemaLeaves(owner.schema).get(path.payload.key)
      const message = leaf === undefined ? `${owner.title} has no setting ${path.payload.key}` : misfit(leaf, resolved.value)
      if (message === undefined) return []
      // A runner setting's skip names its runner; a modifier's or a service's names none.
      return [{ _tag: "SettingSkipped", payload: { path, message, ...(path._tag === "Setting" && runner !== undefined ? { runnerId: runner.id } : {}) } }]
    })

  /**
   * A stream runner's settings as the stream viewer resolves them: its
   * schema defaults, then its Device cells, then on a game page the game's
   * system, game and selected-profile cells, later wins; `null` clears.
   */
  const resolveStream = (gameId: string | undefined, runnerId: string): Record<string, LaunchSourcedValue> => {
    const values: Record<string, LaunchSourcedValue> = {}
    const runner = config.runners?.[runnerId]
    const path = (key: string): LaunchValuePath => ({ _tag: "Editable", payload: { _tag: "Setting", payload: { key } } })
    const defaults = new Map<string, LaunchSourcedValue>()
    for (const [key, leaf] of schemaLeaves(runner?.schema)) {
      if (!("default" in leaf)) continue
      const value: LaunchSourcedValue = { path: path(key), value: leaf.default, source: { _tag: "PluginDefault", payload: { pluginId: runnerId } }, adjusted: false }
      defaults.set(key, value)
      values[`settings.${key}`] = value
    }
    const layers = layersOf(gameId, "selected").map(entry => entry.layer).filter(layer => layer._tag !== "Release")
    for (const layer of layers) {
      const scopes: LaunchRunnerScope[] = [
        ...(runner?.family === undefined ? [] : [{ _tag: "Family" as const, id: runner.family }]),
        { _tag: "Runner", id: runnerId },
      ]
      for (const scope of scopes) {
        const place: LaunchPlace = { layer, scope }
        for (const [key, value] of valueLeaves(body(place).settings)) {
          const fallback = defaults.get(key)
          if (value !== null) values[`settings.${key}`] = { path: path(key), value, source: { _tag: "Place", payload: place }, adjusted: false }
          else if (fallback === undefined) delete values[`settings.${key}`]
          else values[`settings.${key}`] = fallback
        }
      }
    }
    return values
  }

  const placeTitle = (layerTitle: string, scope: LaunchRunnerScope, runners: readonly { id: string; title: string }[]) =>
    scope._tag === "Any" ? layerTitle
      : scope._tag === "Family" ? `${layerTitle} for family ${scope.id}`
        : `${layerTitle} for ${runners.find(runner => runner.id === scope.id)?.title ?? scope.id}`

  const snapshot = (gameId: string | undefined): LaunchSnapshot => {
    const runners = runnersOf(gameId)
    const choice = chooseRunner(gameId, runners.map(runner => runner.id))
    const inUse = runners.find(runner => runner.id === choice.runner)
    const families = [...new Set(runners.flatMap(runner => runner.family === undefined ? [] : [runner.family]))]
    const places: LaunchPlaceSnapshot[] = layersOf(gameId, "all").flatMap(({ layer, title }) => {
      const scopes: LaunchRunnerScope[] = [
        { _tag: "Any" },
        ...families.map(family => ({ _tag: "Family" as const, id: family })),
        ...runners.map(runner => ({ _tag: "Runner" as const, id: runner.id })),
      ]
      return scopes.map(scope => {
        const place: LaunchPlace = { layer, scope }
        return { place, title: placeTitle(title, scope, runners), body: structuredClone(body(place)), editableKinds: scope._tag === "Any" ? ANY_KINDS : RUNNER_KINDS }
      })
    })
    const gameProfiles = gameId === undefined ? [] : profiles.get(gameId) ?? []
    const pick = gameId === undefined ? undefined : selectedProfiles.get(gameId)
    const routeLayers = (gameId === undefined ? undefined : config.games?.[gameId]?.routeLayers)
      ?? (gameId !== undefined && inUse === undefined && runners.filter(runner => choosable(runner.id)).length > 1 ? { _tag: "NeedsRunnerChoice" as const } : { _tag: "Present" as const })
    const resolved = resolve(gameId, inUse)
    const streams = streamRunners.map(runnerId => ({ runnerId, values: resolveStream(gameId, runnerId) }))
    // A stream runner's own settings are checked against its schema too.
    const streamSkips = streams.flatMap(({ runnerId, values }) => {
      const runner = config.runners?.[runnerId]
      return skippedOf(values, { id: runnerId, ...(runner === undefined ? {} : { runner }) })
    })
    return {
      places,
      resolved: { values: resolved.values },
      runnerResolved: streams,
      runners: runners.map((runner): LaunchRunnerSnapshot => ({
        id: runner.id,
        ...(runner.family === undefined ? {} : { family: runner.family }),
        title: runner.title,
        selected: runner.id === inUse?.id,
        choosable: choosable(runner.id),
        schema: runner.runner?.schema ?? { type: "object", properties: {} },
        revision: "describe-1",
        prepare: runner.runner?.prepare ?? { _tag: "Ready", payload: { env: {}, args: [] } },
      })),
      modifiers: Object.entries(config.modifiers ?? {}).map(([id, modifier]) => ({
        id,
        title: modifier.title,
        ...(modifier.schema === undefined ? {} : { schema: modifier.schema, revision: "describe-1" }),
      })),
      services: Object.entries(config.services ?? {}).map(([id, service]) => ({
        id,
        title: service.title,
        ...(service.schema === undefined ? {} : { schema: service.schema, revision: "describe-1" }),
      })),
      profiles: gameProfiles.map(profile => ({
        id: profile.id,
        title: profile.title,
        body: structuredClone(body({ layer: { _tag: "Profile", id: { gameId: gameId ?? "", profileId: profile.id } }, scope: { _tag: "Any" } })),
      })),
      ...(pick === undefined ? {} : { selectedProfile: pick }),
      ...(config.deviceFacts === undefined ? {} : { deviceFacts: structuredClone(config.deviceFacts) }),
      notices: [...choice.notices, ...resolved.skipped, ...streamSkips],
      routeLayers,
      revisions: revisions(),
    }
  }

  const setLaunch = async ({ place, path, value, clear: cleared = false, expectedRevision }: LaunchSetRequest): Promise<LaunchSettingsWriteOutcome> => {
    await wait()
    if (config.failures?.set) return { _tag: "Err", payload: config.failures.set }
    const { layer, scope } = place
    if (layer._tag === "Launch") return failure("InvalidPlace", "One launch is not saved.")
    if (expectedRevision !== (deviceFile(layer) ? revisions().device : revisions().games)) {
      return failure("SettingsConflict", "Launch settings changed. Reload before saving.")
    }
    if (layer._tag === "Profile" && !(profiles.get(layer.id.gameId) ?? []).some(profile => profile.id === layer.id.profileId)) {
      return failure("ProfileNotFound", "That profile no longer exists.")
    }
    const key = placeKey(place)
    const stored: Body = structuredClone(bodies.get(key) ?? {})
    if (cleared && value !== undefined) return failure("InvalidSetting", "A cleared setting takes no value.")
    if (cleared && !["Setting", "ModifierSetting", "ServiceSetting"].includes(path._tag)) return failure("InvalidSetting", "Only a plugin setting can be cleared.")
    const clear = value === undefined
    switch (path._tag) {
      case "Runner": {
        if (scope._tag !== "Any") return failure("InvalidSetting", "A runner is chosen for any runner only.")
        if (!clear && typeof value !== "string") return failure("InvalidSetting", "A runner is named by its id.")
        if (!clear && !choosable(value as string)) return failure("InvalidSetting", `${String(value)} runs no game; it views streams.`)
        if (clear) delete stored.runner
        else stored.runner = value as string
        break
      }
      case "CpuMin":
      case "CpuMax":
      case "GpuMin":
      case "GpuMax": {
        const kind = path._tag.startsWith("Cpu") ? "cpu" : "gpu"
        const bound = path._tag.endsWith("Min") ? "min" : "max"
        const limits: { min?: number; max?: number } = { ...stored[kind] }
        if (clear) delete limits[bound]
        else {
          const low = bound === "min" ? 0 : 1
          if (typeof value !== "number" || !Number.isInteger(value) || value < low || value > 100) {
            return failure("InvalidSetting", `A ${kind.toUpperCase()} ${bound} is a whole percent from ${low} to 100.`)
          }
          limits[bound] = value
        }
        // The same place may not set its floor above its ceiling.
        if (limits.min !== undefined && limits.max !== undefined && limits.min > limits.max) {
          const name = kind.toUpperCase()
          return failure("InvalidSetting", `This place sets ${name} min ${limits.min} % above its ${name} max ${limits.max} %.`)
        }
        stored[kind] = limits
        break
      }
      case "CpuGovernor": {
        if (layer._tag !== "Device" || scope._tag !== "Any") return failure("InvalidSetting", "The CPU governor is set on the device for any runner only.")
        const cpu = { ...stored.cpu }
        if (clear) delete cpu.governor
        else cpu.governor = String(value)
        stored.cpu = cpu
        break
      }
      case "ModifierEnabled": {
        const installed = config.modifiers?.[path.payload.id]
        if (installed === undefined) return failure("InvalidSetting", `No installed modifier ${path.payload.id}.`)
        if (!clear && typeof value !== "boolean") return failure("InvalidSetting", "A modifier is on or off.")
        const modifiers = { ...stored.modifiers }
        const modifier = { ...modifiers[path.payload.id] }
        if (clear) delete modifier.enabled
        else modifier.enabled = value as boolean
        modifiers[path.payload.id] = modifier
        stored.modifiers = modifiers
        break
      }
      case "ModifierSetting":
      case "ServiceSetting":
      case "Setting": {
        // A leaf of the plugin's schema, by its dotted path; a family place
        // takes any installed runner's of the family.
        const schemas = path._tag === "ModifierSetting" ? [config.modifiers?.[path.payload.id]]
          : path._tag === "ServiceSetting" ? [config.services?.[path.payload.id]]
            : Object.entries(config.runners ?? {})
              .filter(([id, runner]) => scope._tag === "Runner" ? id === scope.id : scope._tag === "Family" && runner.family === scope.id)
              .map(([, runner]) => runner)
        if (path._tag === "Setting" && scope._tag === "Any") return failure("InvalidSetting", "Runner settings are set for a family or a runner.")
        if (path._tag !== "Setting" && schemas[0] === undefined) return failure("InvalidSetting", `No installed plugin ${path.payload.id}.`)
        const segments = path.payload.key.split(".")
        if (segments.some(segment => segment === "")) return failure("InvalidSetting", `${path.payload.key} is not a setting path.`)
        const leaf = schemas.map(plugin => schemaLeaves(plugin?.schema).get(path.payload.key)).find(found => found !== undefined)
        if (leaf === undefined) return failure("InvalidSetting", `No installed plugin here has a setting ${path.payload.key}.`)
        if (!clear && !cleared && misfit(leaf, value) !== undefined) {
          return failure("InvalidSetting", `That value doesn't fit ${String(leaf.title ?? path.payload.key)}.`)
        }
        const written = cleared ? null : value
        if (path._tag === "Setting") stored.settings = withLeaf(stored.settings, segments, written)
        else if (path._tag === "ModifierSetting") {
          const modifiers = { ...stored.modifiers }
          const modifier = { ...modifiers[path.payload.id] }
          modifier.settings = withLeaf(modifier.settings, segments, written)
          modifiers[path.payload.id] = modifier
          stored.modifiers = modifiers
        } else {
          const services = { ...stored.services }
          services[path.payload.id] = { ...services[path.payload.id], settings: withLeaf(services[path.payload.id]?.settings, segments, written) }
          stored.services = services
        }
        break
      }
    }
    bodies.set(key, stored)
    if (deviceFile(layer)) device++
    else games++
    return { _tag: "Ok", payload: revisions() }
  }

  const methods: LaunchSettingsMethods = {
    async launchSettingsSnapshot(gameId) {
      await wait()
      if (config.failures?.snapshot) return { _tag: "Err", payload: config.failures.snapshot }
      return { _tag: "Ok", payload: snapshot(gameId) }
    },
    setLaunch,
    async saveLaunchProfile({ gameId, profileId, title, expectedRevision }) {
      await wait()
      if (config.failures?.profile) return { _tag: "Err", payload: config.failures.profile }
      if (expectedRevision !== revisions().games) return failure("SettingsConflict", "Profiles changed. Reload before saving.")
      if (title.trim() === "") return failure("InvalidProfile", "A profile needs a name.")
      const list = profiles.get(gameId) ?? []
      if (profileId === undefined) list.push({ id: `profile-${nextProfile++}`, title: title.trim() })
      else {
        const profile = list.find(candidate => candidate.id === profileId)
        if (!profile) return failure("ProfileNotFound", "That profile no longer exists.")
        list[list.indexOf(profile)] = { ...profile, title: title.trim() }
      }
      profiles.set(gameId, list)
      games++
      return { _tag: "Ok", payload: revisions() }
    },
    async removeLaunchProfile({ gameId, profileId, expectedRevision }) {
      await wait()
      if (config.failures?.profile) return { _tag: "Err", payload: config.failures.profile }
      if (expectedRevision !== revisions().games) return failure("SettingsConflict", "Profiles changed. Reload before saving.")
      const list = profiles.get(gameId) ?? []
      if (!list.some(profile => profile.id === profileId)) return failure("ProfileNotFound", "That profile no longer exists.")
      profiles.set(gameId, list.filter(profile => profile.id !== profileId))
      for (const key of [...bodies.keys()]) if (key.startsWith(`profile:${profileId}/`)) bodies.delete(key)
      if (selectedProfiles.get(gameId) === profileId) selectedProfiles.delete(gameId)
      games++
      return { _tag: "Ok", payload: revisions() }
    },
    async selectLaunchProfile({ gameId, profileId, expectedRevision }) {
      await wait()
      if (config.failures?.profile) return { _tag: "Err", payload: config.failures.profile }
      if (expectedRevision !== revisions().games) return failure("SettingsConflict", "Profiles changed. Reload before saving.")
      if (profileId === undefined) selectedProfiles.delete(gameId)
      else if (!(profiles.get(gameId) ?? []).some(profile => profile.id === profileId)) {
        return failure("ProfileNotFound", "That profile no longer exists.")
      }
      else selectedProfiles.set(gameId, profileId)
      games++
      return { _tag: "Ok", payload: revisions() }
    },
  }

  return {
    methods,
    /** The runner choices `gameRoutes` reports, from the same places. */
    runnerChoices(gameId: string, systemIds: readonly string[], installed: readonly string[]): InMemoryRunnerChoices {
      const gameRunner = body({ layer: { _tag: "Game", id: gameId }, scope: { _tag: "Any" } }).runner
      const systemRunners = Object.fromEntries(systemIds.flatMap(systemId => {
        const named = body({ layer: { _tag: "System", id: systemId }, scope: { _tag: "Any" } }).runner
        return named === undefined ? [] : [[systemId, named]]
      }))
      const selected = chooseRunner(gameId, installed).runner
      return { ...(gameRunner === undefined ? {} : { gameRunner }), systemRunners, revisions: revisions(), ...(selected === undefined ? {} : { selected }) }
    },
    /** Seed a runner choice, as an existing device.yaml or library would hold it. */
    seedRunner(place: LaunchPlace, runnerId: string) {
      const key = placeKey(place)
      if (bodies.get(key)?.runner === undefined) bodies.set(key, { ...bodies.get(key), runner: runnerId })
    },
  }
}

/** The RG353M's single CPU cluster (rk3566) and its Mali GPU, as cpufreq
 * (kHz) and devfreq (Hz) report them. Browser dev seeds it with `?launch=rg353m`. */
export const RG353M_LAUNCH_FACTS: LaunchDeviceFacts = {
  defaults: { cpu: { min: 0, max: 100 }, gpu: { min: 0, max: 100 }, governor: "schedutil" },
  cpuPolicies: [{
    name: "policy0",
    cpus: [0, 1, 2, 3],
    cpuinfoMaxFreq: 1_800_000,
    availableFrequencies: [408_000, 600_000, 816_000, 1_104_000, 1_416_000, 1_608_000, 1_800_000],
    availableGovernors: ["conservative", "ondemand", "userspace", "powersave", "performance", "schedutil"],
  }],
  gpuDevfreq: [{
    name: "fde60000.gpu",
    availableFrequencies: [200_000_000, 300_000_000, 400_000_000, 600_000_000, 700_000_000, 800_000_000],
  }],
}
