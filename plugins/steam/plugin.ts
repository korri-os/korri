import type {
  PluginLaunchInput,
  PluginLaunchOutput,
} from "../../contracts/generated/korrid"

export const name = "steam"
export const title = "Steam"
export const description = "Runs Steam content through the native session program."

export const systems = {
  steam: { id: "steam", title: "Steam" },
}

export const runners = {
  steam: {
    id: "@korri:steam/steam",
    program: "steam-session",
    systems: ["steam"],
  },
}

// Files of the exact required @korri:fex and @korri:proton builds, as korrid
// passes them under `<pluginId>/<key>`, and the variable that hands each one
// to the session program.
const runtimeFiles = {
  KORRI_FEX_PREPARE: "@korri:fex/fex-prepare",
  KORRI_FEX_CONFIG: "@korri:fex/config",
  KORRI_PROTON_CACHYOS_PREPARE: "@korri:proton/cachyos-prepare",
}

function prepareLaunch(input: PluginLaunchInput): PluginLaunchOutput {
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
  if (!input.cacheRoot) {
    throw new Error("Steam requires korrid's cache root for its runtime downloads")
  }
  const steamHome = `${input.accountRoot}/steam`
  const env: Record<string, string> = {
    STEAM_HOME: steamHome,
    KORRI_CACHE_ROOT: input.cacheRoot,
  }
  for (const [variable, key] of Object.entries(runtimeFiles)) {
    const path = input.files[key]
    if (!path) {
      throw new Error(`Steam requires ${key} from its required plugin`)
    }
    env[variable] = path
  }
  return {
    command: input.program,
    args: [input.ref],
    env,
    directories: [steamHome],
    envUnset: [],
  }
}

export const handlers = {
  "launch.prepare": prepareLaunch,
}
