import type {
  PluginLaunchInput,
  PluginLaunchOutput,
} from "../../contracts/generated/korrid"

// Dennis Engelhard's Mega Man Rock N Roll, public Linux 1.3 build: Godot
// 2.1.4 with an x86_64 executable. korrid downloads the ZIP as the artifact
// `game` and the game's release names `data.pck` in its ready directory
// (ADR 0013). The runner plays it under FEX from @korri:fex, which
// plugin.nix requires.
export const name = "mega-man-rock-n-roll"
export const title = "Mega Man Rock N Roll"
export const description =
  "Public Linux 1.3 ZIP for Dennis Engelhard's Mega Man Rock N Roll."

// The release identity: the SHA-256 of `data.pck`, not of the ZIP (ADR 0013).
// game/contents lists the same value for the unpack check.
const dataPck =
  "sha256:eb6fc0155b1996c5fcc5b42d6ec3f9c3aea4ba1e9aca5389cd7da8070310bd41"

// `bytes` is the size of the file downloaded on 2026-10-17 and 2026-10-20.
// korrid downloads and verifies it, then runs `game-unpack`, which extracts
// it and checks `data.pck` and `MegaMan_RocknRoll`.
export const artifacts = {
  game: {
    id: "@korri:mega-man-rock-n-roll/game",
    version: "1.3",
    files: [
      {
        url: "https://dennisengelhard.com/wp-content/uploads/2021/01/megaman_rocknroll_linux_1.3.zip",
        sha256: "sha256:af916d57c169ef9b964457f60638c553a6dd7e623cd4fca0c1d828c40919abd9",
        bytes: 97511960,
      },
    ],
    unpack: "game-unpack",
  },
}

export const games = {
  "mega-man-rock-n-roll": {
    title: "Mega Man Rock N Roll",
    releases: {
      "1.3": {
        hashes: [dataPck],
        artifacts: ["@korri:mega-man-rock-n-roll/game"],
        file: { artifact: "@korri:mega-man-rock-n-roll/game", path: "data.pck" },
      },
    },
  },
}

// No system: the runner accepts the release by its hash.
export const runners = {
  fex: {
    id: "@korri:mega-man-rock-n-roll/fex",
    program: "fex-launch",
    releases: [dataPck],
    artifacts: ["@korri:fex/rootfs"],
  },
}

function requiredPath(paths: Record<string, string>, key: string): string {
  const path = paths[key]
  if (!path) {
    throw new Error(`Mega Man Rock N Roll requires ${key} from @korri:fex`)
  }
  return path
}

function prepareLaunch(input: PluginLaunchInput): PluginLaunchOutput {
  if (!input.contentPath) {
    throw new Error("Mega Man Rock N Roll requires a content path")
  }
  if (input.corePath !== undefined) {
    throw new Error("Mega Man Rock N Roll is a whole game and loads no core")
  }
  const overrides = input.overrides
  if (overrides?.config !== undefined) {
    throw new Error("Mega Man Rock N Roll takes no raw config override")
  }
  if (
    overrides?.settings !== undefined &&
    Object.keys(overrides.settings).length > 0
  ) {
    throw new Error("Mega Man Rock N Roll runner does not implement typed settings")
  }
  // The content path is `data.pck`; the executable sits beside it.
  if (!input.contentPath.startsWith("/")) {
    throw new Error("Mega Man Rock N Roll requires an absolute content path")
  }
  const gameDirectory =
    input.contentPath.slice(0, input.contentPath.lastIndexOf("/")) || "/"
  // Godot keeps settings and saves under ~/.godot/app_userdata/Megaman_RnR.
  const home = `${input.accountRoot}/mega-man-rock-n-roll`
  return {
    command: input.program,
    args: [gameDirectory],
    env: {
      HOME: home,
      KORRI_FEX_CONFIG: requiredPath(input.files, "@korri:fex/config"),
      KORRI_FEX_ROOTFS: requiredPath(input.artifacts, "@korri:fex/rootfs"),
    },
    cwd: gameDirectory,
    directories: [home],
    envUnset: [],
  }
}

export const handlers = {
  "launch.prepare": prepareLaunch,
}
