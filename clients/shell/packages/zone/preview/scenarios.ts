/**
 * The preview scenarios: Zone's own fixtures (surfaces/zone/src/fixtures,
 * the data its Caliper parts draw) as SurfaceModels for the fixture korrid,
 * and the presses that bring the package to each page and layer.
 *
 * Run once from surfaces/zone before the cut (it imports Zone's fixtures):
 *
 *     cd surfaces/zone && bun ../../clients/shell/packages/zone/preview/scenarios.ts
 *
 * It wrote preview/scenarios/*.json, which preview/shots.sh replays through
 * the real shell under Xvfb. `ref` names the Caliper part and state each
 * scenario stands beside.
 */
import { mkdirSync, writeFileSync } from "node:fs"
import { fixtureGetGamesModel, fixtureDownloadModel, fixtureEmptyModel, fixtureErrorModel, fixtureGameActions, fixtureLoadingModel, fixtureReadyModel } from "../../../../../surfaces/zone/src/fixtures/zone-games"
import { fixtureNetworkMany, fixtureOptions } from "../../../../../surfaces/zone/src/fixtures/fixture-options"
import { fixtureRunnerReady, fixtureRunnerStale, fixtureRunnerWarnings } from "../../../../../surfaces/zone/src/fixtures/fixture-options"
import {
  controllerPad, controllersFour, networkOnline, onboardingModel, overlayDestructive, overlayGroups, overlayDefault, statusBusyDetail,
  statusConfirm, statusProblem, statusProgressKnown, statusRunning,
} from "../../../../../surfaces/zone/src/fixtures/fixture-layers"
import { fixtureEncryptedSecret, fixtureIdentity, fixtureRetiredKeys } from "../../../../../surfaces/zone/src/fixtures/fixture-save-data"
import { fixtureStore, fixtureStoreDetails, fixtureStoreFresh, fixtureStoreGets } from "../../../../../surfaces/zone/src/fixtures/fixture-store"
import { launchSettingsFixtures, withLaunchSettings } from "../../../../../surfaces/zone/src/fixtures/fixture-launch-settings"

type Step = string
interface Scenario {
  readonly name: string
  /** The Caliper part (file stem) and state this stands beside. */
  readonly ref: readonly [string, string]
  readonly models: readonly unknown[]
  readonly steps: readonly Step[]
}

const ready = fixtureReadyModel
const options = { ...ready, ...fixtureOptions }
const withActions = (model: object) => ({ ...model, focusedGame: { gameId: "", actions: fixtureGameActions } })
const identity = (status: object = { _tag: "Idle" }, overrides: object = {}) =>
  ({ ...ready, identityManagement: fixtureIdentity(status as never, overrides as never) })
const toOptions = ["up", "right", "confirm"]
const toSave = ["up", "right", "right", "confirm"]

const scenarios: Scenario[] = [
  // GAMES
  { name: "games", ref: ["ZoneGames", "default"], models: [ready], steps: ["right"] },
  { name: "games-loading", ref: ["ZoneGames", "Loading"], models: [fixtureLoadingModel], steps: [] },
  { name: "games-error", ref: ["ZoneGames", "Error"], models: [fixtureErrorModel], steps: [] },
  { name: "games-empty", ref: ["ZoneGames", "Empty"], models: [fixtureEmptyModel], steps: [] },
  { name: "games-two-places", ref: ["ZoneGames", "TwoPlaceChoice"], models: [ready], steps: ["right", "right", "right", "confirm"] },
  { name: "games-download", ref: ["ZoneGames", "DownloadFocused"], models: [fixtureDownloadModel], steps: ["right"] },
  { name: "games-gameopts", ref: ["ZoneGames", "GameOpts"], models: [withActions(ready)], steps: ["right", "right", "right", "options"] },
  { name: "games-gameopts-asking", ref: ["ZoneGames", "GameOptsAsking"], models: [withActions(ready)], steps: ["right", "right", "right", "options", "down", "confirm"] },
  { name: "games-find", ref: ["ZoneGames", "FindOpen"], models: [ready], steps: ["menu", "menu", "confirm"] },
  { name: "games-filtered", ref: ["ZoneGames", "Filtered"], models: [ready], steps: ["menu", "menu", "confirm", "right", "right", "right", "right", "right", "right", "right", "right", "right", "down", "confirm", "down", "confirm", "down", "right", "right", "right", "right", "right", "right", "confirm"] },
  { name: "games-get", ref: ["ZoneGames", "GetGames"], models: [fixtureGetGamesModel], steps: ["right", "menu", "menu", "menu"] },
  { name: "games-busy", ref: ["ZoneGames", "BusyOverHome"], models: [{ ...ready, status: statusBusyDetail }], steps: [] },
  { name: "games-title-card", ref: ["ZoneLaunchCard", "default"], models: [ready], steps: ["right", "confirm"] },
  {
    name: "games-tally",
    ref: ["ZoneTally", "Counted"],
    models: [
      { ...ready, status: { _tag: "Running", kicker: "Playing", gameId: "hollow-knight" } },
      { ...ready, catalog: { _tag: "Ready", games: (ready.catalog as { games: { id: string; playCount?: number; totalPlaytimeSeconds?: number }[] }).games.map(game => game.id === "hollow-knight" ? { ...game, playCount: 8, totalPlaytimeSeconds: (game.totalPlaytimeSeconds ?? 0) + 1_985 } : game) } },
    ],
    steps: ["right", "model 1", "wait 2.5"],
  },
  { name: "games-attract", ref: ["ZoneAttract", "default"], models: [ready], steps: ["right", "wait 31.5"] },
  { name: "status-running", ref: ["ZoneStatusLayer", "Running"], models: [{ ...ready, status: statusRunning }], steps: ["right"] },
  { name: "status-problem", ref: ["ZoneStatusLayer", "Problem"], models: [{ ...ready, status: statusProblem }], steps: [] },
  { name: "status-confirm", ref: ["ZoneStatusLayer", "Confirm"], models: [{ ...ready, status: statusConfirm }], steps: [] },
  { name: "status-progress", ref: ["ZoneStatusLayer", "BusyProgress"], models: [{ ...ready, status: statusProgressKnown }], steps: [] },
  // OPTIONS
  { name: "options", ref: ["ZoneOptions", "default"], models: [options], steps: [...toOptions, "down"] },
  { name: "options-choice", ref: ["ZoneOptions", "Choice"], models: [options], steps: [...toOptions, "down", "down", "down"] },
  { name: "options-text", ref: ["ZoneOptions", "Text"], models: [options], steps: [...toOptions, "down", "confirm"] },
  { name: "options-system", ref: ["ZoneOptions", "SystemFocused"], models: [options], steps: ["system"] },
  { name: "options-destructive", ref: ["ZoneOptions", "Destructive"], models: [options], steps: ["system", "down", "down", "down", "confirm"] },
  { name: "options-look", ref: ["ZoneOptions", "LookFocused"], models: [options], steps: ["system", "down", "down", "down", "down"] },
  { name: "options-wifi", ref: ["ZoneOptions", "WifiList"], models: [{ ...options, network: fixtureNetworkMany }], steps: ["system", "down", "down", "down", "down", "down", "down", "down", "down"] },
  // SAVE DATA
  { name: "savedata", ref: ["ZoneSaveData", "default"], models: [identity()], steps: [...toSave, "down"] },
  { name: "savedata-password", ref: ["ZoneSaveData", "Password"], models: [identity()], steps: [...toSave, "down", "confirm"] },
  { name: "savedata-backup", ref: ["ZoneSaveData", "BackupReady"], models: [identity({ _tag: "BackupReady", encryptedSecret: fixtureEncryptedSecret })], steps: [...toSave, "down"] },
  { name: "savedata-retired", ref: ["ZoneSaveData", "RetiredActions"], models: [identity({ _tag: "Idle" }, { retiredPublicKeys: fixtureRetiredKeys })], steps: [...toSave, "down", "right", "right", "right", "confirm"] },
  { name: "savedata-delete", ref: ["ZoneSaveData", "ConfirmDelete"], models: [identity({ _tag: "Idle" }, { retiredPublicKeys: fixtureRetiredKeys })], steps: [...toSave, "down", "right", "right", "right", "confirm", "right", "confirm"] },
  { name: "savedata-working", ref: ["ZoneSaveData", "Working"], models: [identity({ _tag: "Working", operation: "Making a backup" })], steps: [...toSave] },
  { name: "savedata-problem", ref: ["ZoneSaveData", "Problem"], models: [identity({ _tag: "Problem", message: "The password does not open this backup. Check it and try again." })], steps: [...toSave] },
  { name: "savedata-none", ref: ["ZoneSaveData", "NotAvailable"], models: [ready], steps: [...toSave, "down"] },
  // PAUSED (the Overlay)
  { name: "paused", ref: ["ZonePaused", "default"], models: [{ ...ready, presentation: overlayDefault }], steps: [] },
  { name: "paused-groups", ref: ["ZonePaused", "Groups"], models: [{ ...ready, presentation: overlayGroups }], steps: [] },
  { name: "paused-asking", ref: ["ZonePaused", "Destructive"], models: [{ ...ready, presentation: overlayDestructive }], steps: ["down", "confirm"] },
  // First boot
  { name: "onboarding", ref: ["ZoneOnboarding", "OneHeard"], models: [onboardingModel([controllerPad])], steps: [] },
  { name: "onboarding-four", ref: ["ZoneOnboarding", "FourControllers"], models: [onboardingModel(controllersFour)], steps: [] },
  { name: "onboarding-wifi", ref: ["ZoneOnboarding", "WifiStep"], models: [onboardingModel([controllerPad])], steps: ["confirm"] },
  { name: "onboarding-name", ref: ["ZoneOnboarding", "NameStep"], models: [onboardingModel([controllerPad], networkOnline)], steps: ["confirm", "down", "down", "down", "down", "confirm"] },
  { name: "onboarding-typing", ref: ["ZoneOnboarding", "NameTyping"], models: [onboardingModel([controllerPad], networkOnline)], steps: ["confirm", "down", "down", "down", "down", "confirm", "confirm"] },
  { name: "onboarding-identity", ref: ["ZoneOnboarding", "IdentityStep"], models: [onboardingModel([controllerPad], networkOnline)], steps: ["confirm", "down", "down", "down", "down", "confirm", "down", "confirm"] },
  // RUN WITH
  { name: "runwith", ref: ["ZoneRunWith", "default"], models: [{ ...options, runnerChoice: fixtureRunnerReady }], steps: [] },
  { name: "runwith-warnings", ref: ["ZoneRunWith", "Warnings"], models: [{ ...options, runnerChoice: fixtureRunnerWarnings }], steps: [] },
  { name: "runwith-stale", ref: ["ZoneRunWith", "Stale"], models: [{ ...options, runnerChoice: fixtureRunnerStale }], steps: [] },
  // GET GAMES
  { name: "store", ref: ["ZoneStore", "default"], models: [{ ...ready, store: fixtureStoreFresh }], steps: ["right", "menu", "menu", "menu", "confirm"] },
  { name: "store-results", ref: ["ZoneStore", "Results"], models: [{ ...ready, store: fixtureStore() }], steps: ["right", "menu", "menu", "menu", "confirm", "down", "confirm"] },
  { name: "store-offer", ref: ["ZoneStore", "Offer"], models: [{ ...ready, store: fixtureStore() }], steps: ["right", "menu", "menu", "menu", "confirm", "down", "confirm", "down", "down", "confirm"] },
  { name: "store-downloading", ref: ["ZoneStore", "Downloading"], models: [{ ...ready, store: fixtureStore({ details: fixtureStoreDetails(fixtureStoreGets.downloading) }) }], steps: ["right", "menu", "menu", "menu", "confirm", "down", "confirm", "down", "down", "confirm"] },
  { name: "store-confirm", ref: ["ZoneStore", "Confirm"], models: [{ ...ready, store: fixtureStore({ details: fixtureStoreDetails(fixtureStoreGets.confirm) }) }], steps: ["right", "menu", "menu", "menu", "confirm", "down", "confirm", "down", "down", "confirm"] },
  // LAUNCH SETTINGS
  { name: "launch", ref: ["ZoneLaunchSettings", "default"], models: [withLaunchSettings(options as never, launchSettingsFixtures.ready)], steps: ["down"] },
  { name: "launch-conflict", ref: ["ZoneLaunchSettings", "Conflict"], models: [withLaunchSettings(options as never, launchSettingsFixtures.conflict)], steps: [] },
]

const out = new URL("./scenarios/", import.meta.url).pathname
mkdirSync(out, { recursive: true })
for (const scenario of scenarios) writeFileSync(`${out}${scenario.name}.json`, `${JSON.stringify(scenario, null, 1)}\n`)
console.log(`${scenarios.length} scenarios in ${out}`)
