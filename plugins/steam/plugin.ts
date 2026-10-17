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
  const steamHome = `${input.accountRoot}/steam`
  return {
    command: input.program,
    args: [input.ref],
    env: { STEAM_HOME: steamHome },
    directories: [steamHome],
    envUnset: [],
  }
}

export const handlers = {
  "launch.prepare": prepareLaunch,
}
