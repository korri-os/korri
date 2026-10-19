import { describe, expect, test } from "bun:test"
import { discovery, handlers, runners } from "./plugin"

const manifest = (fields = "") =>
  `"AppState" { "appid" "401710" "StateFlags" "4" "name" "Flinthook" ${fields} }`
const discover = (text: string) => handlers["discovery.discover"]({
  files: [{ relativePath: "steamapps/appmanifest_401710.acf", text }],
})

const launchInput = () => ({
  runnerId: "@korri:steam/steam",
  program: "/nix/store/steam/bin/steam-session",
  ref: "401710",
  accountRoot: "/var/lib/korri/accounts/a",
  cacheRoot: "/var/lib/korri/cache",
  files: {
    "steam-session": "/nix/store/steam/bin/steam-session",
    "@korri:fex/config": "/nix/store/fex/share/fex-emu",
    "@korri:fex/fex-unpack": "/nix/store/fex/bin/fex-unpack",
    "@korri:proton/cachyos-unpack": "/nix/store/proton/bin/cachyos-unpack",
  },
  artifacts: {
    "@korri:fex/rootfs": "/var/lib/korri/cache/fex/rootfs/archlinux-2026-01-08-mesa-26.1.2",
    "@korri:proton/cachyos": "/var/lib/korri/cache/proton/cachyos/11.0-20260601-slr-arm64",
  },
})

describe("Steam launch declaration", () => {
  test("lists the FEX rootfs and Proton-CachyOS artifacts korrid must ready first", () => {
    expect(runners.steam.artifacts).toEqual(["@korri:fex/rootfs", "@korri:proton/cachyos"])
  })

  test("hands the session the FEX config file and the ready artifact directories", () => {
    const input = launchInput()
    expect(handlers["launch.prepare"](input)).toEqual({
      command: input.program,
      args: ["401710"],
      env: {
        STEAM_HOME: "/var/lib/korri/accounts/a/steam",
        KORRI_FEX_CONFIG: "/nix/store/fex/share/fex-emu",
        KORRI_FEX_ROOTFS: "/var/lib/korri/cache/fex/rootfs/archlinux-2026-01-08-mesa-26.1.2",
        KORRI_PROTON_CACHYOS: "/var/lib/korri/cache/proton/cachyos/11.0-20260601-slr-arm64",
      },
      directories: ["/var/lib/korri/accounts/a/steam"],
      envUnset: [],
    })
  })

  test("refuses the launch when a required file or artifact path is missing", () => {
    for (const key of ["@korri:fex/rootfs", "@korri:proton/cachyos"]) {
      const input = launchInput()
      delete input.artifacts[key]
      expect(() => handlers["launch.prepare"](input)).toThrow(`Steam requires ${key}`)
    }
    const noFile = launchInput()
    delete noFile.files["@korri:fex/config"]
    expect(() => handlers["launch.prepare"](noFile)).toThrow("Steam requires @korri:fex/config")
    const noArtifacts = { ...launchInput(), artifacts: undefined as unknown as Record<string, string> }
    expect(() => handlers["launch.prepare"](noArtifacts)).toThrow("Steam requires @korri:fex/rootfs")
  })
})

describe("Steam installed manifest declaration", () => {
  test("declares its closed provider and names the installed game from VDF", () => {
    expect(discovery).toEqual([{ id: "@korri:steam/installed-apps", title: "Steam installed apps" }])
    expect(discover(manifest())).toEqual([{
      kind: "provider-ref-release",
      confidence: "high",
      source: { relativePath: "steamapps/appmanifest_401710.acf" },
      target: { provider: "@korri:steam", ref: "401710" },
      release: { id: "steam", title: "Flinthook", system: "steam" },
      launch: { use: "@korri:steam/steam" },
    }])
  })

  test("skips partial, corrupt, mismatched and unnamed manifests", () => {
    for (const text of [
      manifest().replace('"4"', '"1026"'),
      manifest().replace('"401710"', '"401711"'),
      manifest().replace('"name" "Flinthook"', ""),
      manifest().replace('"Flinthook"', '"   "'),
      '"AppState" {',
    ]) expect(discover(text)).toEqual([])
    expect(handlers["discovery.discover"]({files: []})).toEqual([])
  })

  test("excludes explicit non-game types but accepts missing type and nested VDF", () => {
    for (const type of ["Tool", "Proton", "Steam Runtime", "Steamworks Redistributables"])
      expect(discover(manifest(`"type" "${type}"`))).toEqual([])
    expect(discover(manifest('"type" "gAmE" "InstalledDepots" { "1" { "manifest" "2" } }'))).toHaveLength(1)
    expect(discover(manifest().replace('"4"', '"04"'))).toHaveLength(1)
    expect(discover(manifest().replace('"Flinthook"', '"Flint \\"hook\\""'))[0]?.release.title).toBe('Flint "hook"')
  })
})
