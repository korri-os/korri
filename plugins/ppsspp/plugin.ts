import type {
  PluginLaunchInput,
  PluginLaunchOutput,
} from "../../contracts/generated/korrid"

// A hand-written runner that borrows nothing. No family, no shared helper, no
// generated file: this plugin is the proof that the contract a catalogue core
// uses is the whole contract, available to anyone.
export const name = "ppsspp"
export const title = "PPSSPP"
export const description = "Runs PlayStation Portable content with PPSSPP."

export const systems = {
  psp: { id: "psp", title: "PlayStation Portable" },
}

// No `family`. A family is a shared settings scope, and this runner shares its
// settings with nothing, so it declares none. Launching does not depend on one.
export const runners = {
  ppsspp: {
    id: "@korri:ppsspp/ppsspp",
    program: "ppsspp",
    systems: ["psp"],
  },
}

export const discovery = {
  fileReleases: {
    "psp-files": {
      id: "@korri:ppsspp/psp-files",
      title: "PlayStation Portable files",
      extensions: ["iso", "cso", "pbp"],
      system: "psp",
      runners: ["@korri:ppsspp/ppsspp"],
    },
  },
}

// No `sessionControls`. Effects are a closed first-party vocabulary, so a DIY
// runner gets no in-game menu. Declaring a RetroArch effect here would be a
// lie: PPSSPP does not answer the RetroArch network command protocol.
// See services/korrid/SCRIPTING.md, "Effects are a closed first-party
// vocabulary".

function prepareLaunch(input: PluginLaunchInput): PluginLaunchOutput {
  if (input.corePath !== undefined) {
    throw new Error("PPSSPP is a whole emulator and loads no core")
  }
  // PPSSPP keeps its memory stick, saves and configuration under one root. The
  // account root is korrid's fact; the layout inside it is this runner's.
  const memoryStick = `${input.accountRoot}/ppsspp`
  const overrides = input.overrides
  if (overrides?.config !== undefined) {
    throw new Error(
      "PPSSPP takes no raw config override; its settings are typed",
    )
  }
  // Settings are not implemented for this runner yet, and silently dropping
  // authored values would be worse than refusing them.
  if (
    overrides?.settings !== undefined &&
    Object.keys(overrides.settings).length > 0
  ) {
    throw new Error("PPSSPP runner does not implement typed settings yet")
  }
  return {
    command: input.program,
    args: ["--fullscreen", input.contentPath],
    directories: [memoryStick],
    env: {
      XDG_CONFIG_HOME: memoryStick,
      XDG_DATA_HOME: memoryStick,
    },
    envUnset: [],
  }
}

export const handlers = {
  "launch.prepare": prepareLaunch,
}
