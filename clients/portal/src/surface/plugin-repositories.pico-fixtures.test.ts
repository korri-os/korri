/**
 * Pico previews Settings → Plugins from a copy of this module's output
 * (surfaces/pico/src/fixtures/plugin-settings.ts): a surface may not import
 * the portal. This test keeps the copy equal to what the portal publishes, so
 * the screenshots show the real rows.
 */
import { describe, expect, it } from "bun:test"
import { pluginModels } from "../../../../surfaces/pico/src/fixtures/plugin-settings"
import {
  pluginSettingGroups,
  withNeedsReview,
  withReview,
  type PluginFacts,
} from "./plugin-repositories"
import { BROKEN, OFFICIAL, REMOVED, RETRO, inspection, readyFacts, repositories, retroCatalog } from "./plugin-repositories.fixtures"

const installed = [
  { id: "@ada:netplay", title: "Netplay", enabled: true },
  { id: "@old:shader", title: "Shader pack", enabled: false },
  { id: "@korri:mgba", title: "mGBA", enabled: true },
  { id: "@korri:retroarch", title: "RetroArch", enabled: false },
]
const base: PluginFacts = readyFacts({
  repositories: { _tag: "Ready", value: repositories({
    userAdded: [RETRO, BROKEN],
    repositoryInstalls: [
      { id: "@ada:netplay", sourceUrl: RETRO, releaseVersion: "0.4.0", enabled: true },
      { id: "@old:shader", sourceUrl: REMOVED, releaseVersion: "3", enabled: false },
      { id: "@old:pocket-dither-and-scanline-filter-collection", sourceUrl: REMOVED, releaseVersion: "1", enabled: true },
    ],
  }) },
  catalogs: {
    [RETRO]: { _tag: "Ready", entries: retroCatalog },
    [BROKEN]: { _tag: "Unreachable", message: "Could not reach broken.example: the catalog is not valid JSON (line 1, column 7)." },
  },
})
const chiptune = { sourceUrl: RETRO, pluginId: "@ada:chiptune", releaseVersion: "1.0.0" }
const longRepos = [
  "https://github.com/ada/retro-plugins",
  "https://codeberg.org/pixel-collective/korri-handheld-plugins-for-small-screens",
  "https://plugins.example.org",
  "https://raw.githubusercontent.com/someone/averyveryverylongrepositorynamewithoutanybreakpoints/main/catalog.json",
  "https://gitlab.com/mira/korri",
  "https://cdn.example.net/k/plugins/v1",
  "https://homebrew.example/korri",
]
const many: PluginFacts = readyFacts({
  repositories: { _tag: "Ready", value: repositories({ official: OFFICIAL, userAdded: longRepos }) },
  catalogs: {
    [OFFICIAL]: { _tag: "Ready", entries: [
      { pluginId: "@korri:tailscale", title: "Tailscale", description: "Reach this device from your other devices", releaseVersion: "1.76.1", publisherTrusted: true },
      { pluginId: "@korri:syncthing", title: "Syncthing", description: "Keep saves in step with your other devices", releaseVersion: "1.27.0", installed: "Enabled", publisherTrusted: true },
    ] },
    ...Object.fromEntries(longRepos.map((url, index) => [url,
      index === 0 ? { _tag: "Ready" as const, entries: retroCatalog }
      : index === 2 ? { _tag: "Checking" as const }
      : index === 4 ? { _tag: "Unreachable" as const, message: "Could not reach gitlab.com" }
      : { _tag: "Ready" as const, entries: retroCatalog.slice(0, index % 3 + 1) }])),
  },
})

// Pico's models put its fixture Device group first.
const pluginGroupsOf = (model: { readonly settings: readonly unknown[] }) => model.settings.slice(1)

describe("Pico's plugin previews", () => {
  it("are exactly the rows the portal publishes", () => {
    expect(pluginGroupsOf(pluginModels.officialOnly)).toEqual(pluginSettingGroups(readyFacts(), [{ id: "@korri:mgba", title: "mGBA", enabled: true }]))
    expect(pluginGroupsOf(pluginModels.repositories)).toEqual(pluginSettingGroups(base, installed))
    expect(pluginGroupsOf(pluginModels.reviewing)).toEqual(pluginSettingGroups(withReview(base, chiptune, inspection), installed))
    expect(pluginGroupsOf(pluginModels.needsReview)).toEqual(pluginSettingGroups(withNeedsReview(base, "@ada:netplay"), installed))
    expect(pluginGroupsOf(pluginModels.many)).toEqual(pluginSettingGroups(many, installed))
  })
})
