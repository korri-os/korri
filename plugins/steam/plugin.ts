import type {
  PluginLaunchInput,
  PluginLaunchOutput,
  ReleaseDiscoveryInput,
  ProviderRefReleaseObservation,
} from "../../contracts/generated/korrid"

export const name = "steam"
export const title = "Steam"
export const description = "Runs Steam content through the native session program."

export const discovery = [
  { id: "@korri:steam/installed-apps", title: "Steam installed apps" },
]

export const systems = {
  steam: { id: "steam", title: "Steam" },
}

export const runners = {
  steam: {
    id: "@korri:steam/steam",
    program: "steam-session",
    systems: ["steam"],
    // korrid downloads these before it starts the runner and passes the
    // ready directories in `artifacts`.
    artifacts: ["@korri:fex/rootfs", "@korri:proton/cachyos"],
  },
}

// Until contracts/generated/korrid.ts carries `artifacts` (ADR 0012): ready
// artifact directories keyed `<pluginId>/<key>`, like `files`.
type LaunchInputWithArtifacts = PluginLaunchInput & {
  artifacts: Record<string, string>
}

function requiredPath(
  paths: Record<string, string> | undefined,
  key: string,
): string {
  const path = paths?.[key]
  if (!path) {
    throw new Error(`Steam requires ${key} from its required plugin`)
  }
  return path
}

function prepareLaunch(input: LaunchInputWithArtifacts): PluginLaunchOutput {
  if (!input.ref) {
    throw new Error("Steam requires a provider ref")
  }
  const overrides = input.overrides
  if (overrides?.config !== undefined) {
    throw new Error("Steam takes no raw config override")
  }
  if (
    overrides?.settings !== undefined &&
    Object.keys(overrides.settings).length > 0
  ) {
    throw new Error("Steam runner does not implement typed settings yet")
  }
  const steamHome = `${input.accountRoot}/steam`
  // The exact required @korri:fex and @korri:proton builds: their files and
  // ready artifacts, as korrid passes them under `<pluginId>/<key>`.
  const env: Record<string, string> = {
    STEAM_HOME: steamHome,
    KORRI_FEX_CONFIG: requiredPath(input.files, "@korri:fex/config"),
    KORRI_FEX_ROOTFS: requiredPath(input.artifacts, "@korri:fex/rootfs"),
    KORRI_PROTON_CACHYOS: requiredPath(input.artifacts, "@korri:proton/cachyos"),
  }
  return {
    command: input.program,
    args: [input.ref],
    env,
    directories: [steamHome],
    envUnset: [],
  }
}

// Ported from legacy product/plugins/steam/src/discovery.ts and
// state-materializer.ts. korrid reads only the declared first-party manifests.
type VdfObject = { [key: string]: string | VdfObject }

function parseVdf(content: string): VdfObject {
  const tokens: string[] = []
  let index = 0
  while (index < content.length) {
    const char = content[index]
    if (/\s/.test(char)) {
      index += 1
      continue
    }
    if (char === "/" && content[index + 1] === "/") {
      while (index < content.length && content[index] !== "\n") index += 1
      continue
    }
    if (char === "{" || char === "}") {
      tokens.push(char)
      index += 1
      continue
    }
    if (char !== '"') throw new Error("invalid VDF token")
    index += 1
    let value = ""
    let closed = false
    while (index < content.length) {
      const next = content[index++]
      if (next === "\\") {
        value += content[index++] ?? ""
        continue
      }
      if (next === '"') {
        closed = true
        break
      }
      value += next
    }
    if (!closed) throw new Error("unterminated VDF string")
    tokens.push(value)
  }
  index = 0
  const object = (nested: boolean, depth: number): VdfObject => {
    if (depth > 64) throw new Error("VDF nesting limit")
    const result: VdfObject = Object.create(null)
    while (index < tokens.length) {
      const key = tokens[index++]
      if (key === "}") {
        if (nested) return result
        throw new Error("unexpected closing brace")
      }
      if (key === "{") throw new Error("unexpected opening brace")
      const value = tokens[index++]
      if (value === undefined || value === "}") throw new Error("missing VDF value")
      result[key] = value === "{" ? object(true, depth + 1) : value
    }
    if (nested) throw new Error("missing closing brace")
    return result
  }
  return object(false, 0)
}

function discoverInstalledApps(
  input: ReleaseDiscoveryInput,
): ProviderRefReleaseObservation[] {
  const observations: ProviderRefReleaseObservation[] = []
  for (const file of input.files) {
    const match = /^steamapps\/appmanifest_(\d+)\.acf$/.exec(file.relativePath)
    if (!match) continue
    try {
      const parsed = parseVdf(file.text)
      const state = typeof parsed.AppState === "object" ? parsed.AppState : parsed
      const appid = state.appid ?? match[1]
      if (appid !== match[1]) continue
      if (
        typeof state.StateFlags !== "string" ||
        !/^\d+$/.test(state.StateFlags) ||
        Number(state.StateFlags) !== 4
      ) continue
      // Legacy excludes every explicit non-Game type, including Tool, Proton,
      // Steam Runtime and Steamworks redistributables. Missing type is valid.
      if (
        typeof state.type === "string" && state.type.toLowerCase() !== "game"
      ) continue
      const title = typeof state.name === "string" ? state.name.trim() : ""
      if (!title) continue
      observations.push({
        kind: "provider-ref-release",
        confidence: "high",
        source: { relativePath: file.relativePath },
        target: { provider: "@korri:steam", ref: appid },
        release: { id: "steam", title, system: "steam" },
        launch: { use: "@korri:steam/steam" },
      })
    } catch {
      // A partial manifest is not an installed game.
      continue
    }
  }
  return observations
}

export const handlers = {
  "discovery.discover": discoverInstalledApps,
  "launch.prepare": prepareLaunch,
}
