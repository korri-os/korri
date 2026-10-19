import { describe, expect, test } from "bun:test"
import { discovery, handlers } from "./plugin"

const manifest = (fields = "") =>
  `"AppState" { "appid" "401710" "StateFlags" "4" "name" "Flinthook" ${fields} }`
const discover = (text: string) => handlers["discovery.discover"]({
  files: [{ relativePath: "steamapps/appmanifest_401710.acf", text }],
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
