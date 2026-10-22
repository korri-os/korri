/**
 * The in-memory brain's launch settings, for browser dev and tests.
 *
 * Shapes are korrid's generated wire types (`app.launch-settings.*`) and
 * nothing else. Resolution follows CONTEXT.md "Launch settings": for each
 * value the most specific layer that sets it wins, and a profile changes only
 * the values it names. Device facts are absent by default, as in a browser
 * that cannot read cpufreq; seed them to show a device's real clock steps.
 */
import type {
  LaunchDeviceFacts,
  LaunchProfileRemoveRequest,
  LaunchProfileSaveRequest,
  LaunchProfileSelectRequest,
  LaunchResolvedValue,
  LaunchSettingsLayer,
  LaunchSettingsProfile,
  LaunchSettingsRouteLayers,
  LaunchSettingsScope,
  LaunchSettingsSetRequest,
  LaunchSettingsSnapshotOutcome,
  LaunchSettingsWriteOutcome,
  RpcFailure,
} from "@contracts/generated/korrid"

/** The layers that apply to one game, below Device, most general first. */
export interface InMemoryLaunchGame {
  readonly systemId?: string
  readonly runner?: { readonly id: string; readonly title: string }
  readonly releaseId?: string
  /** Whether korrid could give the route's layers. Absent: Present. */
  readonly routeLayers?: LaunchSettingsRouteLayers
}

export interface InMemoryLaunchSettingsConfig {
  readonly deviceFacts?: LaunchDeviceFacts
  /** Keyed by korrid game id. A game not listed gets Device and Game layers. */
  readonly games?: Readonly<Record<string, InMemoryLaunchGame>>
  /** Seeded layer values, keyed by the layer id `scopeKey` gives. */
  readonly layers?: Readonly<Record<string, Pick<LaunchSettingsLayer, "cpu" | "gpu" | "governor">>>
  readonly profiles?: Readonly<Record<string, readonly LaunchSettingsProfile[]>>
  readonly selected?: Readonly<Record<string, string>>
  readonly failures?: Partial<Record<"snapshot" | "set" | "profile", RpcFailure>>
  readonly delayMs?: number
}

export interface LaunchSettingsMethods {
  launchSettingsSnapshot(gameId?: string): Promise<LaunchSettingsSnapshotOutcome>
  setLaunchSetting(request: LaunchSettingsSetRequest): Promise<LaunchSettingsWriteOutcome>
  saveLaunchProfile(request: LaunchProfileSaveRequest): Promise<LaunchSettingsWriteOutcome>
  removeLaunchProfile(request: LaunchProfileRemoveRequest): Promise<LaunchSettingsWriteOutcome>
  selectLaunchProfile(request: LaunchProfileSelectRequest): Promise<LaunchSettingsWriteOutcome>
}

/** A stable key for one scope. The portal's layer ids use the same form. */
export function scopeKey(scope: LaunchSettingsScope): string {
  switch (scope._tag) {
    case "Device": return "device"
    case "System": return `system:${scope.id}`
    case "Runner": return `runner:${scope.id}`
    case "Game": return `game:${scope.id}`
    case "Release": return `release:${scope.id.gameId}:${scope.id.releaseId}`
    case "Profile": return `profile:${scope.id.gameId}:${scope.id.profileId}`
  }
}

type Values = { min?: number; max?: number }
type Stored = { cpu: Values; gpu: Values; governor?: string }

const BROWSER_DEFAULTS = { cpu: { min: 0, max: 100 }, gpu: { min: 0, max: 100 }, governor: "schedutil" }

export function createInMemoryLaunchSettings(config: InMemoryLaunchSettingsConfig = {}): LaunchSettingsMethods {
  const layers = new Map<string, Stored>(
    Object.entries(config.layers ?? {}).map(([key, value]) => [key, structuredClone({ cpu: value.cpu, gpu: value.gpu, ...(value.governor === undefined ? {} : { governor: value.governor }) })]),
  )
  const profiles = new Map<string, LaunchSettingsProfile[]>(
    Object.entries(config.profiles ?? {}).map(([gameId, list]) => [gameId, structuredClone([...list])]),
  )
  const selected = new Map<string, string>(Object.entries(config.selected ?? {}))
  let device = 1
  let games = 1
  let nextProfile = 1
  const revisions = () => ({ device: `device-${device}`, games: `games-${games}` })
  const failure = (code: string, message: string) => ({ _tag: "Err" as const, payload: { code, message } })
  const wait = async () => {
    if (config.delayMs) await new Promise(resolve => setTimeout(resolve, config.delayMs))
  }
  const deviceScoped = (scope: LaunchSettingsScope) =>
    scope._tag === "Device" || scope._tag === "System" || scope._tag === "Runner"
  const stale = (scope: LaunchSettingsScope, expected: string) =>
    expected !== (deviceScoped(scope) ? revisions().device : revisions().games)

  const scopesFor = (gameId?: string): { scope: LaunchSettingsScope; title: string }[] => {
    const scopes: { scope: LaunchSettingsScope; title: string }[] = [{ scope: { _tag: "Device" }, title: "Device" }]
    if (gameId === undefined) return scopes
    const game = config.games?.[gameId] ?? {}
    // Without the route, only Device, Game and the profiles: korrid's rule.
    if ((game.routeLayers?._tag ?? "Present") !== "Present") {
      scopes.push({ scope: { _tag: "Game", id: gameId }, title: "Game" })
      return scopes
    }
    if (game.systemId !== undefined) scopes.push({ scope: { _tag: "System", id: game.systemId }, title: `System ${game.systemId}` })
    if (game.runner !== undefined) scopes.push({ scope: { _tag: "Runner", id: game.runner.id }, title: `Runner ${game.runner.title}` })
    scopes.push({ scope: { _tag: "Game", id: gameId }, title: "Game" })
    if (game.releaseId !== undefined) {
      scopes.push({ scope: { _tag: "Release", id: { gameId, releaseId: game.releaseId } }, title: `Release ${game.releaseId}` })
    }
    return scopes
  }

  /** A layer's title as korrid names it in messages. */
  const scopeTitle = (scope: LaunchSettingsScope): string => {
    switch (scope._tag) {
      case "Device": return "Device"
      case "System": return `System ${scope.id}`
      case "Runner": {
        const runner = Object.values(config.games ?? {}).find(game => game.runner?.id === scope.id)?.runner
        return `Runner ${runner?.title ?? scope.id}`
      }
      case "Game": return "Game"
      case "Release": return `Release ${scope.id.releaseId}`
      case "Profile": {
        const profile = profiles.get(scope.id.gameId)?.find(candidate => candidate.id === scope.id.profileId)
        return `Profile ${profile?.title ?? scope.id.profileId}`
      }
    }
  }

  return {
    async launchSettingsSnapshot(gameId) {
      await wait()
      if (config.failures?.snapshot) return { _tag: "Err", payload: config.failures.snapshot }
      const scopes = scopesFor(gameId)
      const layerList: LaunchSettingsLayer[] = scopes.map(({ scope, title }) => {
        const stored = layers.get(scopeKey(scope))
        return {
          scope,
          title,
          cpu: { ...stored?.cpu },
          gpu: { ...stored?.gpu },
          ...(scope._tag === "Device" && stored?.governor !== undefined ? { governor: stored.governor } : {}),
        }
      })
      const gameProfiles = gameId === undefined ? [] : profiles.get(gameId) ?? []
      const pick = gameId === undefined ? undefined : selected.get(gameId)
      const chosen = gameProfiles.find(profile => profile.id === pick)
      const defaults = config.deviceFacts?.defaults ?? BROWSER_DEFAULTS
      // Most specific last: the layers in order, then the chosen profile.
      // A shipped default ranks below every layer.
      const resolve = (kind: "cpu" | "gpu", bound: "min" | "max"): { readonly rank: number; readonly resolved: LaunchResolvedValue } => {
        if (gameId !== undefined && chosen?.[kind][bound] !== undefined) {
          return {
            rank: layerList.length,
            resolved: { value: chosen[kind][bound], source: { _tag: "Profile", id: { gameId, profileId: chosen.id } }, deviceDefault: false, adjusted: false },
          }
        }
        for (let rank = layerList.length - 1; rank >= 0; rank--) {
          const layer = layerList[rank]
          const value = layer?.[kind][bound]
          if (layer !== undefined && value !== undefined) return { rank, resolved: { value, source: layer.scope, deviceDefault: false, adjusted: false } }
        }
        return { rank: -1, resolved: { value: defaults[kind][bound], source: { _tag: "Device" }, deviceDefault: true, adjusted: false } }
      }
      const range = (kind: "cpu" | "gpu") => {
        const min = resolve(kind, "min")
        const max = resolve(kind, "max")
        if (min.resolved.value <= max.resolved.value) return { min: min.resolved, max: max.resolved }
        // Across layers the more specific value wins; the other moves to meet it.
        return min.rank > max.rank
          ? { min: min.resolved, max: { ...max.resolved, value: min.resolved.value, adjusted: true } }
          : { min: { ...min.resolved, value: max.resolved.value, adjusted: true }, max: max.resolved }
      }
      const governor = layerList[0]?.governor ?? config.deviceFacts?.defaults.governor
      return {
        _tag: "Ok",
        payload: {
          routeLayers: (gameId === undefined ? undefined : config.games?.[gameId]?.routeLayers) ?? { _tag: "Present" },
          layers: layerList,
          resolved: { cpu: range("cpu"), gpu: range("gpu"), ...(governor === undefined ? {} : { governor }) },
          profiles: structuredClone(gameProfiles),
          ...(pick === undefined ? {} : { selectedProfile: pick }),
          ...(config.deviceFacts === undefined ? {} : { deviceFacts: structuredClone(config.deviceFacts) }),
          revisions: revisions(),
        },
      }
    },
    async setLaunchSetting({ scope, setting, value, expectedRevision }) {
      await wait()
      if (config.failures?.set) return { _tag: "Err", payload: config.failures.set }
      if (stale(scope, expectedRevision)) return failure("SettingsConflict", "Launch settings changed. Reload before saving.")
      if (setting === "cpu.governor" && scope._tag !== "Device") return failure("InvalidSetting", "The CPU governor is set on the device only.")
      const [kind, bound] = setting.split(".") as ["cpu" | "gpu", "min" | "max" | "governor"]
      if (bound !== "governor" && value !== undefined) {
        const low = bound === "min" ? 0 : 1
        if (typeof value !== "number" || !Number.isInteger(value) || value < low || value > 100) {
          return failure("InvalidSetting", `A ${kind.toUpperCase()} ${bound} is a whole percent from ${low} to 100.`)
        }
        // The same layer may not set its floor above its ceiling.
        const own = scope._tag === "Profile"
          ? profiles.get(scope.id.gameId)?.find(candidate => candidate.id === scope.id.profileId)?.[kind]
          : layers.get(scopeKey(scope))?.[kind]
        const min = bound === "min" ? value : own?.min
        const max = bound === "max" ? value : own?.max
        if (min !== undefined && max !== undefined && min > max) {
          const name = kind.toUpperCase()
          return failure("InvalidSetting", `${scopeTitle(scope)} sets ${name} min ${min} % above its ${name} max ${max} %.`)
        }
      }
      if (scope._tag === "Profile") {
        const profile = profiles.get(scope.id.gameId)?.find(candidate => candidate.id === scope.id.profileId)
        if (!profile || bound === "governor") return failure("ProfileNotFound", "That profile no longer exists.")
        if (value === undefined) delete profile[kind][bound]
        else profile[kind][bound] = value as number
      } else {
        const key = scopeKey(scope)
        const stored = layers.get(key) ?? { cpu: {}, gpu: {} }
        if (bound === "governor") {
          if (value === undefined) delete stored.governor
          else stored.governor = String(value)
        } else if (value === undefined) delete stored[kind][bound]
        else stored[kind][bound] = value as number
        layers.set(key, stored)
      }
      if (deviceScoped(scope)) device++
      else games++
      return { _tag: "Ok", payload: revisions() }
    },
    async saveLaunchProfile({ gameId, profileId, title, expectedRevision }) {
      await wait()
      if (config.failures?.profile) return { _tag: "Err", payload: config.failures.profile }
      if (expectedRevision !== revisions().games) return failure("SettingsConflict", "Profiles changed. Reload before saving.")
      if (title.trim() === "") return failure("InvalidProfile", "A profile needs a name.")
      const list = profiles.get(gameId) ?? []
      if (profileId === undefined) list.push({ id: `profile-${nextProfile++}`, title: title.trim(), cpu: {}, gpu: {} })
      else {
        const profile = list.find(candidate => candidate.id === profileId)
        if (!profile) return failure("ProfileNotFound", "That profile no longer exists.")
        profile.title = title.trim()
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
      if (selected.get(gameId) === profileId) selected.delete(gameId)
      games++
      return { _tag: "Ok", payload: revisions() }
    },
    async selectLaunchProfile({ gameId, profileId, expectedRevision }) {
      await wait()
      if (config.failures?.profile) return { _tag: "Err", payload: config.failures.profile }
      if (expectedRevision !== revisions().games) return failure("SettingsConflict", "Profiles changed. Reload before saving.")
      if (profileId === undefined) selected.delete(gameId)
      else if (!(profiles.get(gameId) ?? []).some(profile => profile.id === profileId)) {
        return failure("ProfileNotFound", "That profile no longer exists.")
      }
      else selected.set(gameId, profileId)
      games++
      return { _tag: "Ok", payload: revisions() }
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
