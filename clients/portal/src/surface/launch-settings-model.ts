/**
 * Launch settings as settings rows: korrid's snapshot in, surface layers out.
 *
 * Pure. Every word a surface shows about a launch setting is decided here:
 * the value or "Inherit", where the value in force comes from, the
 * device-only mark (ADR 0018: every CPU and GPU setting is device-only) and
 * "Applies at next launch". Percent choices are this device's real clock
 * steps (ADR 0019: a percent maps to each cluster's top clock, rounded to a
 * real step); without device facts they are 10 % steps, marked as such.
 */
import type {
  LaunchDeviceFacts,
  LaunchResolvedValue,
  LaunchSettingsScope,
  LaunchSettingsSnapshot,
  LaunchSettingsValues,
} from "@contracts/generated/korrid"
import type {
  SurfaceLaunchSettingsLayer,
  SurfaceSettingChoice,
  SurfaceSettingGroup,
  SurfaceSettingItem,
} from "@contracts/surface/korri-surface"

export type LaunchSettingKey = "cpu.min" | "cpu.max" | "gpu.min" | "gpu.max" | "cpu.governor"
type Kind = "cpu" | "gpu"
type Bound = "min" | "max"

/** Choice value for "no value on this layer". Never a percent or governor. */
export const INHERIT = "inherit"
export const NO_PROFILE = "none"
const PROFILE_CHOICE = "profile:"

/** Opaque ids the host publishes and routes. Surfaces never parse them. */
export const LAUNCH_ID = {
  /** A Device-settings row; the device page reads its own snapshot. */
  device: (setting: LaunchSettingKey) => `launch-device:${setting}`,
  /** An editor row: one setting on one layer. */
  set: (layerId: string, setting: LaunchSettingKey) => `launch:set:${layerId}:${setting}`,
  pick: "launch:profile:pick",
  create: "launch:profile:new",
  rename: (profileId: string) => `launch:profile:rename:${profileId}`,
  remove: (profileId: string) => `launch:profile:remove:${profileId}`,
  reload: "launch:reload",
  retry: "launch:retry",
  cancel: "launch:cancel",
} as const

const SETTINGS: readonly LaunchSettingKey[] = ["cpu.min", "cpu.max", "gpu.min", "gpu.max", "cpu.governor"]
const LABELS: Record<LaunchSettingKey, string> = {
  "cpu.min": "CPU min",
  "cpu.max": "CPU max",
  "gpu.min": "GPU min",
  "gpu.max": "GPU max",
  "cpu.governor": "CPU governor",
}

export type LaunchSettingIdParts =
  | { readonly _tag: "Device"; readonly setting: LaunchSettingKey }
  | { readonly _tag: "Set"; readonly layerId: string; readonly setting: LaunchSettingKey }
  | { readonly _tag: "Pick" }
  | { readonly _tag: "Create" }
  | { readonly _tag: "Rename"; readonly profileId: string }
  | { readonly _tag: "Remove"; readonly profileId: string }

const isSetting = (value: string): value is LaunchSettingKey => (SETTINGS as readonly string[]).includes(value)

/** The host's reading of an id it published. Undefined: not a launch id. */
export function parseLaunchId(id: string): LaunchSettingIdParts | undefined {
  if (id.startsWith("launch-device:")) {
    const setting = id.slice("launch-device:".length)
    return isSetting(setting) ? { _tag: "Device", setting } : undefined
  }
  if (id === LAUNCH_ID.pick) return { _tag: "Pick" }
  if (id === LAUNCH_ID.create) return { _tag: "Create" }
  if (id.startsWith("launch:profile:rename:")) return { _tag: "Rename", profileId: id.slice("launch:profile:rename:".length) }
  if (id.startsWith("launch:profile:remove:")) return { _tag: "Remove", profileId: id.slice("launch:profile:remove:".length) }
  if (id.startsWith("launch:set:")) {
    const rest = id.slice("launch:set:".length)
    const at = rest.lastIndexOf(":")
    const setting = rest.slice(at + 1)
    return at > 0 && isSetting(setting) ? { _tag: "Set", layerId: rest.slice(0, at), setting } : undefined
  }
  return undefined
}

/** A stable layer id for a scope, unique within one game's snapshot. */
export function layerIdOf(scope: LaunchSettingsScope): string {
  switch (scope._tag) {
    case "Device": return "device"
    case "System": return `system:${scope.id}`
    case "Runner": return `runner:${scope.id}`
    case "Game": return "game"
    case "Release": return `release:${scope.id.releaseId}`
    case "Profile": return `profile:${scope.id.profileId}`
  }
}

/** Device, System and Runner live in device.yaml; the rest in the library. */
export const revisionOf = (scope: LaunchSettingsScope): "device" | "games" =>
  scope._tag === "Device" || scope._tag === "System" || scope._tag === "Runner" ? "device" : "games"

/** The scope a layer id names in this snapshot, profiles included. */
export function scopeForLayer(
  snapshot: LaunchSettingsSnapshot,
  gameId: string | undefined,
  layerId: string,
): LaunchSettingsScope | undefined {
  const layer = snapshot.layers.find(candidate => layerIdOf(candidate.scope) === layerId)
  if (layer) return layer.scope
  if (gameId === undefined) return undefined
  const profile = snapshot.profiles.find(candidate => `profile:${candidate.id}` === layerId)
  return profile ? { _tag: "Profile", id: { gameId, profileId: profile.id } } : undefined
}

/** The value a write sends for a choice. Undefined clears it (Inherit). */
export function launchValueFrom(setting: LaunchSettingKey, choice: string): number | string | undefined {
  if (choice === INHERIT) return undefined
  if (setting === "cpu.governor") return choice
  const percent = Number(choice)
  return Number.isInteger(percent) && percent >= 0 && percent <= 100 ? percent : Number.NaN
}

/** The profile a pick chooses. `null` is "No profile"; undefined is not a pick. */
export function profileFromPick(choice: string): string | null | undefined {
  if (choice === NO_PROFILE) return null
  return choice.startsWith(PROFILE_CHOICE) ? choice.slice(PROFILE_CHOICE.length) : undefined
}

// --- clock steps -----------------------------------------------------------

interface Steps {
  readonly choices: readonly SurfaceSettingChoice[]
  /** True when the device did not say: 10 % steps, not real clocks. */
  readonly guessed: boolean
  readonly labelOf: (percent: number) => string
}

const ghz = (value: number) => {
  const text = value.toFixed(2).replace(/0$/, "").replace(/\.0$/, "")
  return text
}

/**
 * Real steps, as percents of each cluster's top clock, every cluster's clock
 * at that percent named in GHz. `unitsPerGhz` is 1e6 for cpufreq (kHz) and
 * 1e9 for devfreq (Hz).
 */
function clockSteps(clusters: readonly { readonly top: number; readonly steps: readonly number[] }[], unitsPerGhz: number): Steps {
  const usable = clusters.filter(cluster => cluster.top > 0 && cluster.steps.length > 0)
  if (usable.length === 0) {
    const percents = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100]
    const labelOf = (percent: number) => `${percent} %`
    return { guessed: true, labelOf, choices: percents.map(percent => ({ value: String(percent), label: labelOf(percent) })) }
  }
  const percentOf = (step: number, top: number) => Math.round((step / top) * 100)
  const percents = [...new Set(usable.flatMap(cluster => cluster.steps.map(step => percentOf(step, cluster.top))))]
    .filter(percent => percent > 0 && percent <= 100)
    .sort((a, b) => a - b)
  const labelOf = (percent: number) => {
    const clocks = usable.map(cluster => {
      const target = (cluster.top * percent) / 100
      // The real step this percent rounds to on this cluster.
      const nearest = [...cluster.steps].sort((a, b) => Math.abs(a - target) - Math.abs(b - target))[0] ?? target
      return ghz(nearest / unitsPerGhz)
    })
    return `${percent} % (${[...new Set(clocks)].join(" / ")} GHz)`
  }
  return { guessed: false, labelOf, choices: percents.map(percent => ({ value: String(percent), label: labelOf(percent) })) }
}

export function cpuSteps(facts: LaunchDeviceFacts | undefined): Steps {
  return clockSteps(
    (facts?.cpuPolicies ?? []).map(policy => ({ top: policy.cpuinfoMaxFreq || Math.max(0, ...policy.availableFrequencies), steps: policy.availableFrequencies })),
    1e6,
  )
}

export function gpuSteps(facts: LaunchDeviceFacts | undefined): Steps {
  return clockSteps(
    (facts?.gpuDevfreq ?? []).map(device => ({ top: Math.max(0, ...device.availableFrequencies), steps: device.availableFrequencies })),
    1e9,
  )
}

/** Governors every cluster offers. Empty when the device did not say. */
export function governors(facts: LaunchDeviceFacts | undefined): readonly string[] {
  const policies = facts?.cpuPolicies ?? []
  const [first, ...rest] = policies
  if (first === undefined) return []
  return first.availableGovernors.filter(governor => rest.every(policy => policy.availableGovernors.includes(governor)))
}

// --- rows ------------------------------------------------------------------

const INHERIT_CHOICE: SurfaceSettingChoice = { value: INHERIT, label: "Inherit" }
const NEXT_LAUNCH = "Applies at next launch."

interface Context {
  readonly snapshot: LaunchSettingsSnapshot
  readonly cpu: Steps
  readonly gpu: Steps
  readonly governors: readonly string[]
  readonly titleOf: (scope: LaunchSettingsScope) => string
  readonly deviceValues: { readonly cpu: LaunchSettingsValues; readonly gpu: LaunchSettingsValues; readonly governor?: string }
}

function contextOf(snapshot: LaunchSettingsSnapshot): Context {
  const device = snapshot.layers.find(layer => layer.scope._tag === "Device")
  return {
    snapshot,
    cpu: cpuSteps(snapshot.deviceFacts),
    gpu: gpuSteps(snapshot.deviceFacts),
    governors: governors(snapshot.deviceFacts),
    deviceValues: { cpu: device?.cpu ?? {}, gpu: device?.gpu ?? {}, ...(device?.governor === undefined ? {} : { governor: device.governor }) },
    titleOf: scope => {
      if (scope._tag === "Profile") {
        const profile = snapshot.profiles.find(candidate => candidate.id === scope.id.profileId)
        return profile === undefined ? "a profile" : `Profile ${profile.title}`
      }
      return snapshot.layers.find(layer => layerIdOf(layer.scope) === layerIdOf(scope))?.title ?? scope._tag
    },
  }
}

/** Where the value in force comes from, said from one layer's row. */
function sourceText(context: Context, layerId: string, kind: Kind, bound: Bound, resolved: LaunchResolvedValue): string {
  const steps = context[kind]
  const fromId = layerIdOf(resolved.source)
  const defaulted = resolved.source._tag === "Device" && context.deviceValues[kind][bound] === undefined
  const title = defaulted ? "Device (default)" : context.titleOf(resolved.source)
  const value = steps.labelOf(resolved.value)
  if (resolved.adjusted) return `${value} from ${title}, adjusted`
  return fromId === layerId && !defaulted ? `from ${title}` : `${value} from ${title}`
}

function percentRow(
  context: Context,
  id: string,
  layerId: string,
  setting: Extract<LaunchSettingKey, `${Kind}.${Bound}`>,
  own: number | undefined,
): SurfaceSettingItem {
  const [kind, bound] = setting.split(".") as [Kind, Bound]
  const steps = context[kind]
  const choices = own === undefined || steps.choices.some(choice => choice.value === String(own))
    ? steps.choices
    : [...steps.choices, { value: String(own), label: steps.labelOf(own) }].sort((a, b) => Number(a.value) - Number(b.value))
  return {
    id,
    label: LABELS[setting],
    value: own === undefined ? "Inherit" : steps.labelOf(own),
    description: steps.guessed ? `${NEXT_LAUNCH} Clock steps unknown.` : NEXT_LAUNCH,
    source: sourceText(context, layerId, kind, bound, context.snapshot.resolved[kind][bound]),
    scope: "device",
    interaction: { kind: "choice", choices: [INHERIT_CHOICE, ...choices] },
  }
}

function governorRow(context: Context, id: string): SurfaceSettingItem {
  const own = context.deviceValues.governor
  const known = context.governors
  const offered = own === undefined || known.includes(own) ? known : [...known, own]
  const inForce = context.snapshot.resolved.governor ?? context.snapshot.deviceFacts?.defaults.governor
  return {
    id,
    label: LABELS["cpu.governor"],
    value: own ?? "Inherit",
    description: known.length === 0 ? `${NEXT_LAUNCH} Governors unknown.` : NEXT_LAUNCH,
    source: own !== undefined ? "from Device" : inForce === undefined ? "from Device (default)" : `${inForce} from Device (default)`,
    scope: "device",
    interaction: { kind: "choice", choices: [INHERIT_CHOICE, ...offered.map(governor => ({ value: governor, label: governor }))] },
  }
}

function valueRows(
  context: Context,
  layerId: string,
  values: { readonly cpu: LaunchSettingsValues; readonly gpu: LaunchSettingsValues },
  withGovernor: boolean,
  idOf: (setting: LaunchSettingKey) => string,
): { readonly cpu: SurfaceSettingItem[]; readonly gpu: SurfaceSettingItem[] } {
  return {
    cpu: [
      percentRow(context, idOf("cpu.min"), layerId, "cpu.min", values.cpu.min),
      percentRow(context, idOf("cpu.max"), layerId, "cpu.max", values.cpu.max),
      ...(withGovernor ? [governorRow(context, idOf("cpu.governor"))] : []),
    ],
    gpu: [
      percentRow(context, idOf("gpu.min"), layerId, "gpu.min", values.gpu.min),
      percentRow(context, idOf("gpu.max"), layerId, "gpu.max", values.gpu.max),
    ],
  }
}

/** Device settings' "Launch settings" group, from a snapshot without a game. */
export function deviceLaunchGroup(snapshot: LaunchSettingsSnapshot): SurfaceSettingGroup {
  const context = contextOf(snapshot)
  const rows = valueRows(context, "device", context.deviceValues, false, LAUNCH_ID.device)
  return {
    title: "Launch settings",
    items: [...rows.cpu, ...rows.gpu, governorRow(context, LAUNCH_ID.device("cpu.governor"))],
  }
}

const MAX_PROFILE_TITLE = 40

/** One game's editor: a layer per scope, most general first, then each profile. */
export function launchLayersFrom(snapshot: LaunchSettingsSnapshot): readonly SurfaceLaunchSettingsLayer[] {
  const context = contextOf(snapshot)
  const selected = snapshot.profiles.find(profile => profile.id === snapshot.selectedProfile)
  const profileRows = (): SurfaceSettingGroup => ({
    title: "Profile",
    items: [
      {
        id: LAUNCH_ID.pick,
        label: "Profile at next launch",
        value: selected?.title ?? "No profile",
        description: "Remembered for this game. Applies at next launch.",
        interaction: {
          kind: "choice",
          choices: [
            { value: NO_PROFILE, label: "No profile" },
            ...snapshot.profiles.map(profile => ({ value: `${PROFILE_CHOICE}${profile.id}`, label: profile.title })),
          ],
        },
      },
      {
        id: LAUNCH_ID.create,
        label: "New profile",
        description: "Starts with every value inherited.",
        interaction: { kind: "text", placeholder: "Profile name", maxLength: MAX_PROFILE_TITLE },
      },
    ],
  })
  const layers = snapshot.layers
    .filter(layer => layer.scope._tag !== "Profile")
    .map((layer): SurfaceLaunchSettingsLayer => {
      const layerId = layerIdOf(layer.scope)
      const rows = valueRows(context, layerId, layer, layer.scope._tag === "Device", setting => LAUNCH_ID.set(layerId, setting))
      return {
        id: layerId,
        title: layer.title,
        groups: [
          { title: "CPU", items: rows.cpu },
          { title: "GPU", items: rows.gpu },
          ...(layer.scope._tag === "Game" ? [profileRows()] : []),
        ],
      }
    })
  const profiles = snapshot.profiles.map((profile): SurfaceLaunchSettingsLayer => {
    const layerId = `profile:${profile.id}`
    const rows = valueRows(context, layerId, profile, false, setting => LAUNCH_ID.set(layerId, setting))
    return {
      id: layerId,
      title: `Profile ${profile.title}`,
      groups: [
        { title: "CPU", items: rows.cpu },
        { title: "GPU", items: rows.gpu },
        {
          title: "Profile",
          items: [
            {
              id: LAUNCH_ID.rename(profile.id),
              label: "Name",
              value: profile.title,
              description: profile.id === snapshot.selectedProfile ? "Used at next launch." : "Not used at next launch.",
              interaction: { kind: "text", placeholder: "Profile name", maxLength: MAX_PROFILE_TITLE },
            },
            {
              id: LAUNCH_ID.remove(profile.id),
              label: "Delete profile",
              interaction: {
                kind: "action",
                actionId: LAUNCH_ID.remove(profile.id),
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
  })
  return [...layers, ...profiles]
}
