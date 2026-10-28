/**
 * The plugin facts behind Pico's Settings → Plugins previews
 * (surfaces/pico/src/fixtures/plugin-settings.ts). A surface may not import
 * the portal, so Pico keeps a copy of the rows these facts yield;
 * `plugin-repositories.pico-fixtures.test.ts` keeps the copy equal.
 *
 * To regenerate Pico's copy after the rows change:
 *
 *   bun run clients/portal/src/surface/plugin-repositories.pico-fixtures.generate.ts
 *
 * Test and preview data only; nothing the portal ships imports it.
 */
import type { PluginSetting } from "@contracts/generated/korrid"
import { PluginInstalledState, PluginPublisherState } from "@contracts/generated/korrid"
import {
  withNeedsReview,
  withReview,
  type PluginFacts,
  type ReleaseTarget,
} from "./plugin-repositories"
import {
  BROKEN,
  OFFICIAL,
  REMOVED,
  RETRO,
  bringsNewInspection,
  inspection,
  keyChangedInspection,
  newPublisherInspection,
  readyFacts,
  repositories,
  retroCatalog,
  trustedPublishers,
  unsignedInspection,
} from "./plugin-repositories.fixtures"

export const installed: PluginSetting[] = [
  { id: "@ada:netplay", title: "Netplay", enabled: true },
  { id: "@old:shader", title: "Shader pack", enabled: false },
  { id: "@korri:mgba", title: "mGBA", enabled: true },
  { id: "@korri:retroarch", title: "RetroArch", enabled: false },
]
const otherInstalls = [
  { id: "@korri:mgba", title: "mGBA", enabled: true },
  { id: "@korri:retroarch", title: "RetroArch", enabled: false },
]

export const base: PluginFacts = readyFacts({
  repositories: { _tag: "Ready", value: repositories({
    otherInstalls,
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

export const chiptune: ReleaseTarget = { sourceUrl: RETRO, pluginId: "@ada:chiptune", releaseVersion: "1.0.0" }
export const miner: ReleaseTarget = { sourceUrl: RETRO, pluginId: "@mallory:miner", releaseVersion: "9.9.9" }
export const dither: ReleaseTarget = { sourceUrl: RETRO, pluginId: "@pocket:dither", releaseVersion: "0.1.0" }

const longRepos = [
  "https://git.example/ada/retro-plugins",
  "https://forge.example/pixel-collective/korri-handheld-plugins-for-small-screens",
  "https://plugins.example.org",
  "https://raw.files.example/someone/averyveryverylongrepositorynamewithoutanybreakpoints/main/catalog.json",
  "https://lab.example/mira/korri",
  "https://cdn.example.net/k/plugins/v1",
  "https://homebrew.example/korri",
]

/** More publishers than fit, the longest namespace without a break among them. */
const manyPublishers = {
  entries: [
    ...trustedPublishers.entries,
    { namespace: "@averyveryverylongnamespacewithoutanybreakpointsatall", fingerprint: "averyveryverylongkeylabelwithoutanybreakpoints-2026 QXZl cnlW ZXJ5 TG9u", state: PluginPublisherState.Trusted, installed: ["@averyveryverylongnamespacewithoutanybreakpointsatall:x"] },
    ...["@mira", "@homebrew-hub", "@retro-friends", "@scanline", "@tiny"].map((namespace, index) => ({
      namespace,
      fingerprint: `${namespace.slice(1)}-1 S2V5 ${"IG5v dCBy ZWFs".slice(0, 4 + index)}`,
      state: PluginPublisherState.Trusted,
      installed: index % 2 === 0 ? [] : [`${namespace}:plugin`],
    })),
  ],
}

const many: PluginFacts = readyFacts({
  repositories: { _tag: "Ready", value: repositories({ official: OFFICIAL, userAdded: longRepos, otherInstalls }) },
  catalogs: {
    [OFFICIAL]: { _tag: "Ready", entries: [
      { pluginId: "@korri:tailscale", title: "Tailscale", description: "Reach this device from your other devices", releaseVersion: "1.76.1", publisher: { namespace: "@korri", state: PluginPublisherState.Bound } },
      { pluginId: "@korri:syncthing", title: "Syncthing", description: "Keep saves in step with your other devices", releaseVersion: "1.27.0", installed: PluginInstalledState.Enabled, publisher: { namespace: "@korri", state: PluginPublisherState.Bound } },
    ] },
    ...Object.fromEntries(longRepos.map((url, index) => [url,
      index === 0 ? { _tag: "Ready" as const, entries: retroCatalog }
      : index === 2 ? { _tag: "Checking" as const }
      : index === 4 ? { _tag: "Unreachable" as const, message: "Could not reach lab.example" }
      : { _tag: "Ready" as const, entries: retroCatalog.slice(0, index % 3 + 1) }])),
  },
  publishers: { _tag: "Ready", value: manyPublishers },
})

const withPublishers = (facts: PluginFacts): PluginFacts =>
  ({ ...facts, publishers: { _tag: "Ready", value: trustedPublishers } })

/** One preview's facts. Pico's models add the settings status. */
export interface PicoPluginState {
  readonly note: string
  readonly facts: PluginFacts
  readonly installed: readonly PluginSetting[]
}

const officialOnlyFacts = readyFacts({ repositories: { _tag: "Ready", value: repositories({ otherInstalls: [{ id: "@korri:mgba", title: "mGBA", enabled: true }] }) } })

/** Every set of rows Pico previews, by name. Pico's models pair them with a
 * settings status (saving, a problem), so several models share one set. */
export const picoPluginStates = {
  officialOnly: { note: "Official repository not set up, nothing added.", facts: officialOnlyFacts, installed: [] },
  repositories: { note: "Two added repositories: one read, one unreachable; installs from a removed one; plugins installed with the image.", facts: base, installed },
  reviewing: { note: "Chiptune checked: its publishers are trusted, so the next press shows the report.", facts: withReview(base, chiptune, inspection), installed },
  needsReview: { note: "Netplay's approval went stale.", facts: withNeedsReview(base, "@ada:netplay", true), installed },
  trustNew: { note: "Free coins checked: its publisher is not trusted yet, so the next press trusts it.", facts: withReview(base, miner, newPublisherInspection), installed },
  keyChanged: { note: "Chiptune checked: signed by another key than the one this device trusts.", facts: withReview(base, chiptune, keyChangedInspection), installed },
  bringsNew: { note: "Chiptune checked: it brings a plugin from a publisher not trusted yet.", facts: withReview(base, chiptune, bringsNewInspection), installed },
  unsigned: { note: "Pocket dither checked: not signed, so the approval opens with the disclosure.", facts: withReview(base, dither, unsignedInspection), installed },
  publishers: { note: "The publishers this device trusts: one configured, two trusted by its owner.", facts: withPublishers(base), installed },
  many: { note: "Seven added repositories with long names, and more publishers than fit.", facts: many, installed },
} satisfies Record<string, PicoPluginState>
