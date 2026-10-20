import { describe, expect, test } from "bun:test"
import { artifacts, games, handlers, runners } from "./plugin"

const dataPck =
  "sha256:eb6fc0155b1996c5fcc5b42d6ec3f9c3aea4ba1e9aca5389cd7da8070310bd41"
const gameDirectory = "/var/lib/korri/cache/mega-man-rock-n-roll/game/1.3"

const launchInput = () => ({
  runnerId: "@korri:mega-man-rock-n-roll/fex",
  program: "/nix/store/mmrnr/bin/mega-man-rock-n-roll-fex",
  contentPath: `${gameDirectory}/data.pck`,
  accountRoot: "/var/lib/korri/accounts/a",
  cacheRoot: "/var/lib/korri/cache",
  files: {
    "fex-launch": "/nix/store/mmrnr/bin/mega-man-rock-n-roll-fex",
    "game-unpack": "/nix/store/mmrnr/bin/mega-man-rock-n-roll-unpack",
    "@korri:fex/config": "/nix/store/fex/share/fex-emu",
    "@korri:fex/fex-unpack": "/nix/store/fex/bin/fex-unpack",
  },
  artifacts: {
    "@korri:fex/rootfs": "/var/lib/korri/cache/fex/rootfs/archlinux-2026-01-08-mesa-26.1.2",
  },
})

describe("Mega Man Rock N Roll declaration", () => {
  test("the release is data.pck inside the game artifact, by its hash", () => {
    const release = games["mega-man-rock-n-roll"].releases["1.3"]
    expect(release).toEqual({
      hashes: [dataPck],
      artifacts: [artifacts.game.id],
      file: { artifact: artifacts.game.id, path: "data.pck" },
    })
  })

  test("the runner accepts that release and needs the FEX rootfs", () => {
    expect(runners.fex.releases).toEqual([dataPck])
    expect(runners.fex.artifacts).toEqual(["@korri:fex/rootfs"])
  })
})

describe("Mega Man Rock N Roll launch", () => {
  test("runs the launcher on the game directory with its home and FEX paths", () => {
    const input = launchInput()
    expect(handlers["launch.prepare"](input)).toEqual({
      command: input.program,
      args: [gameDirectory],
      env: {
        HOME: "/var/lib/korri/accounts/a/mega-man-rock-n-roll",
        KORRI_FEX_CONFIG: "/nix/store/fex/share/fex-emu",
        KORRI_FEX_ROOTFS: "/var/lib/korri/cache/fex/rootfs/archlinux-2026-01-08-mesa-26.1.2",
      },
      cwd: gameDirectory,
      directories: ["/var/lib/korri/accounts/a/mega-man-rock-n-roll"],
      envUnset: [],
    })
  })

  test("refuses a launch korrid cannot supply", () => {
    const refused: [string, Record<string, unknown>][] = [
      ["requires a content path", { contentPath: undefined }],
      ["requires an absolute content path", { contentPath: "data.pck" }],
      ["loads no core", { corePath: "/nix/store/core.so" }],
      ["no raw config override", { overrides: { config: { append: [] } } }],
      ["does not implement typed settings", { overrides: { settings: { scale: 2 } } }],
      ["requires @korri:fex/config", { files: {} }],
      ["requires @korri:fex/rootfs", { artifacts: {} }],
    ]
    for (const [message, change] of refused) {
      expect(() => handlers["launch.prepare"]({ ...launchInput(), ...change } as never)).toThrow(
        message,
      )
    }
  })

  test("accepts empty overrides", () => {
    const plan = handlers["launch.prepare"]({ ...launchInput(), overrides: { settings: {} } })
    expect(plan.cwd).toBe(gameDirectory)
  })
})
