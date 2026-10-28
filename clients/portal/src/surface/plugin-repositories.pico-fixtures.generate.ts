/**
 * Writes surfaces/pico/src/fixtures/plugin-settings.ts: Pico's copy of the
 * rows `pluginSettingGroups` publishes for each preview state
 * (`plugin-repositories.pico-states.ts`). Run from clients/portal:
 *
 *   bun run src/surface/plugin-repositories.pico-fixtures.generate.ts
 *
 * Development only; nothing the portal ships imports it.
 */
import { writeFileSync } from "node:fs"
import { resolve } from "node:path"
import {
  ADD_REPOSITORY_SETTING,
  pluginSettingGroups,
  publisherRowId,
  releaseRowId,
  removeRepositoryActionId,
  reviewActionId,
  revokeActionId,
  toggleSettingId,
  trustActionId,
} from "./plugin-repositories"
import { RETRO } from "./plugin-repositories.fixtures"
import { chiptune, dither, miner, picoPluginStates } from "./plugin-repositories.pico-states"

export const picoPluginIds = {
  add: ADD_REPOSITORY_SETTING,
  chiptuneRow: releaseRowId(chiptune),
  chiptuneReview: reviewActionId(chiptune),
  chiptuneTrustKey: trustActionId(chiptune, "@ada"),
  chiptuneTrustLib: trustActionId(chiptune, "@lib"),
  minerRow: releaseRowId(miner),
  minerTrust: trustActionId(miner, "@mallory"),
  ditherRow: releaseRowId(dither),
  ditherReview: reviewActionId(dither),
  netplay: toggleSettingId("@ada:netplay"),
  mgba: toggleSettingId("@korri:mgba"),
  removeRetro: removeRepositoryActionId(RETRO),
  adaPublisher: publisherRowId("@ada"),
  revokeAda: revokeActionId("@ada"),
}

const json = (value: unknown) => JSON.stringify(value, null, 2)

const groups = Object.entries(picoPluginStates).map(([name, state]) =>
  `/** ${state.note} */\nconst ${name}Groups: readonly SurfaceSettingGroup[] = ${json(pluginSettingGroups(state.facts, state.installed))}\n`)

const file = `/**
 * Settings → Plugins as the portal publishes it, for previews and tests.
 *
 * Generated from clients/portal/src/surface/plugin-repositories.ts
 * (\`pluginSettingGroups\`) and the states in
 * plugin-repositories.pico-states.ts, by
 * plugin-repositories.pico-fixtures.generate.ts: a surface imports nothing
 * from the portal, so the output is copied, not imported. Regenerate it when
 * the portal's plugin rows change; the portal's tests fail until it is.
 */
import type { SurfaceModel, SurfaceSettingGroup, SurfaceSettingsStatus } from "@contracts/surface/korri-surface"
import { fixtureModel } from "./fixture-host"

/** Row and action ids the previews put a status or a question on. */
export const pluginIds = ${json(picoPluginIds)} as const

${groups.join("\n")}
const deviceGroup = fixtureModel.settings.find(group => group.title === "Device")

/** A settings model with Device first, then the plugin groups, as on a device. */
const withPlugins = (groups: readonly SurfaceSettingGroup[], settingsStatus: SurfaceSettingsStatus = { _tag: "Idle" }): SurfaceModel => ({
  ...fixtureModel,
  settings: [...(deviceGroup === undefined ? [] : [deviceGroup]), ...groups],
  settingsStatus,
})

export const pluginModels = {
  officialOnly: withPlugins(officialOnlyGroups),
  adding: withPlugins(officialOnlyGroups, { _tag: "Saving", settingId: pluginIds.add }),
  invalidAddress: withPlugins(officialOnlyGroups, { _tag: "Problem", settingId: pluginIds.add, message: "Use an https:// address" }),
  unreachableAddress: withPlugins(officialOnlyGroups, {
    _tag: "Problem",
    settingId: pluginIds.add,
    message: "Could not reach plugins.example.org: no catalog at that address (HTTP 404). Nothing was saved.",
  }),
  alreadyAdded: withPlugins(repositoriesGroups, { _tag: "Problem", settingId: pluginIds.add, message: "Already added" }),
  repositories: withPlugins(repositoriesGroups),
  inspecting: withPlugins(repositoriesGroups, { _tag: "Saving", settingId: pluginIds.chiptuneRow }),
  reviewing: withPlugins(reviewingGroups),
  installing: withPlugins(reviewingGroups, { _tag: "Saving", settingId: pluginIds.chiptuneRow }),
  needsReview: withPlugins(needsReviewGroups),
  toggleRefused: withPlugins(repositoriesGroups, {
    _tag: "Problem",
    settingId: pluginIds.mgba,
    message: "mGBA could not be turned off: a game is running.",
  }),
  trustNew: withPlugins(trustNewGroups),
  trusting: withPlugins(trustNewGroups, { _tag: "Saving", settingId: pluginIds.minerRow }),
  trustRefused: withPlugins(trustNewGroups, {
    _tag: "Problem",
    settingId: pluginIds.minerRow,
    message: "remove the unsigned @mallory plugins first: @mallory:coins",
  }),
  keyChanged: withPlugins(keyChangedGroups),
  bringsNew: withPlugins(bringsNewGroups),
  unsigned: withPlugins(unsignedGroups),
  publishers: withPlugins(publishersGroups),
  revoking: withPlugins(publishersGroups, { _tag: "Saving", settingId: pluginIds.adaPublisher }),
  many: withPlugins(manyGroups),
} satisfies Record<string, SurfaceModel>
`

if (import.meta.main) {
  const target = resolve(import.meta.dir, "../../../../surfaces/pico/src/fixtures/plugin-settings.ts")
  writeFileSync(target, file)
  console.log(`wrote ${target}`)
}
