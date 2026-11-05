/**
 * The preview scenarios: one Korri model each, the keys that bring Pico to
 * the state, and the Caliper reference it is compared with
 * (`<part>/<state>`, rendered from surfaces/pico before it was removed).
 *
 *   bun preview/scenarios.ts OUT_DIR    writes OUT_DIR/<name>.json and OUT_DIR/scenarios.json
 *
 * Keys are xdotool names, as the shell maps them: arrows, Return (confirm),
 * Escape (back), m (menu), o (options). `then` publishes further models in
 * order, for the clock (attract) and for a model that changes.
 */
import { mkdirSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import type { SurfaceModel } from "../src/model"
import { fixtureGameActions, fixtureModel, fixtureOverlay } from "../test/fixtures/fixture-host"
import { launchSettingsFixtures, withLaunchSettings } from "../test/fixtures/launch-settings"
import {
  failedSettingsModel,
  fixtureGame,
  fixtureInvite,
  fixtureInvites,
  inviteModel,
  noArtworkGame,
  streamEndedStatus,
  streamPausedStatus,
} from "../test/fixtures/named-states"
import { fixtureControllers, fixtureNetworks, onboardingModel } from "../test/fixtures/onboarding"
import { pluginModels } from "../test/fixtures/plugin-settings"
import { runnerChoice } from "../test/fixtures/runner-choice"
import { rgdsDisplays } from "../test/fixtures/displays"
import { STORE_GETS, storeDetails, storeFixture, STORE_ENTRIES } from "../test/fixtures/store"

interface Scenario {
  readonly name: string
  readonly ref: string
  readonly model: SurfaceModel & { readonly fixtureGameActions?: Record<string, unknown> }
  readonly keys?: readonly string[]
  readonly then?: readonly SurfaceModel[]
  readonly displays?: unknown
}

/** Every game's actions, as korrid publishes them for the focused game. */
const actions = { hollow: fixtureGameActions("hollow") }
const base = { ...fixtureModel, fixtureGameActions: actions }

const toSettings = ["Down", "Return", "Down", "Return"]
const toStore = ["Down", "Return", "Down", "Return"]
const search = (word: string): string[] => {
  // From A (row 0, column 0) to each letter of the search keyboard and back.
  const rows = ["ABCDEFGHIJ", "KLMNOPQRST", "UVWXYZ0123", "456789"]
  const keys: string[] = []
  let at = { r: 0, c: 0 }
  for (const letter of word) {
    const r = rows.findIndex(row => row.includes(letter))
    const c = rows[r]?.indexOf(letter) ?? 0
    for (; at.r < r; at.r += 1) keys.push("Down")
    for (; at.r > r; at.r -= 1) keys.push("Up")
    for (; at.c < c; at.c += 1) keys.push("Right")
    for (; at.c > c; at.c -= 1) keys.push("Left")
    keys.push("Return")
  }
  return keys
}

const withStatus = (status: SurfaceModel["status"]): SurfaceModel => ({ ...base, status })

export const SCENARIOS: readonly Scenario[] = [
  // Home
  { name: "home-ready", ref: "PicoHome.page/Ready", model: base },
  { name: "home-loading", ref: "PicoHome.page/Loading", model: { ...base, catalog: { _tag: "Loading" } } },
  { name: "home-empty", ref: "PicoHome.page/Empty", model: { ...base, catalog: { _tag: "Empty" } } },
  { name: "home-error", ref: "PicoHome.page/Error", model: { ...base, catalog: { _tag: "Error", message: "The library could not be read." } } },
  { name: "home-no-artwork", ref: "PicoHome.page/NoArtwork", model: { ...base, catalog: { _tag: "Ready", games: [noArtworkGame] } } },
  { name: "home-grid", ref: "PicoHome.page/Grid", model: base, keys: ["m"] },
  { name: "home-hero", ref: "PicoHome.page/Hero", model: base, keys: ["m", "m"] },
  { name: "home-menu", ref: "PicoHome.page/MenuOpen", model: base, keys: ["Down", "Return"] },
  { name: "home-starting", ref: "PicoHome.page/Starting", model: withStatus({ _tag: "Busy", kicker: "STARTING", detail: "Preparing the game.", gameId: "hollow" }) },
  {
    name: "home-starting-cancel",
    ref: "PicoHome.page/StartingWithCancel",
    model: withStatus({ _tag: "Busy", kicker: "Starting Hollow…", detail: "Opening your session", gameId: "hollow", actions: [{ id: "cancel", label: "Cancel", enabled: true }] }),
  },
  {
    // Amendment A4: a launch's named steps. Written after the cut: no Caliper reference.
    name: "home-starting-steps",
    ref: "PicoLaunchStage.organism/Steps",
    model: withStatus({
      _tag: "Busy",
      kicker: "Starting Hollow…",
      steps: { labels: ["Checking saves", "Downloading", "Getting the game ready", "Starting the game"], current: 2 },
      gameId: "hollow",
      actions: [{ id: "cancel-launch", label: "Cancel", enabled: true }],
      declineActionId: "cancel-launch",
    }),
  },
  { name: "home-running", ref: "PicoHome.page/Running", model: withStatus({ _tag: "Running", kicker: "PLAYING", gameId: "hollow" }) },
  {
    name: "home-needs-download",
    ref: "PicoHome.page/NeedsDownload",
    model: withStatus({
      _tag: "Confirm",
      kicker: "Needs a download",
      detail: "FEX rootfs and Proton-CachyOS, 1.5 GB",
      gameId: "hollow",
      actions: [{ id: "acquisition:1:download", label: "Download", enabled: true }, { id: "acquisition:1:cancel", label: "Cancel", enabled: true }],
    }),
  },
  {
    name: "home-downloading",
    ref: "PicoHome.page/Downloading",
    model: withStatus({
      _tag: "Busy",
      kicker: "Downloading FEX rootfs",
      detail: "116 MB of 1.2 GB · 6.4 MB/s · 2:43 left",
      progress: { done: 116_000_000, total: 1_200_000_000 },
      gameId: "hollow",
      actions: [{ id: "acquisition:1:cancel", label: "Cancel", enabled: true }],
    }),
  },
  { name: "home-problem", ref: "PicoHome.page/Problem", model: withStatus({ _tag: "Problem", kicker: "COULD NOT START", reason: "No installed runner can play this game.", canRetry: false, gameTitle: "Tetris" }) },
  { name: "home-retry", ref: "PicoHome.page/RetryableProblem", model: withStatus({ _tag: "Problem", kicker: "COULD NOT START", reason: "The device did not respond.", canRetry: true, gameTitle: "Tetris" }) },
  { name: "home-playing-now", ref: "PicoHome.page/PlayingNowThree", model: { ...inviteModel(fixtureInvites), fixtureGameActions: actions }, keys: ["Right"] },
  { name: "home-invite-notice", ref: "PicoHome.page/InviteNotice", model: base, then: [{ ...inviteModel([fixtureInvite]), fixtureGameActions: actions } as SurfaceModel] },
  { name: "home-stream-paused", ref: "PicoHome.page/StreamPaused", model: withStatus(streamPausedStatus as SurfaceModel["status"]) },
  { name: "home-stream-ended", ref: "PicoHome.page/StreamEnded", model: withStatus(streamEndedStatus as SurfaceModel["status"]) },
  // A game's own screen
  { name: "detail-default", ref: "PicoGameDetail.page/default", model: base, keys: ["Right", "Return"] },
  { name: "detail-choose-location", ref: "PicoGameDetail.page/ChooseLocation", model: base, keys: ["Right", "Right", "Return", "Return"] },
  { name: "detail-confirm-removal", ref: "PicoGameDetail.page/ConfirmRemoval", model: base, keys: ["Right", "Return", "Down", "Down", "Return"] },
  { name: "detail-no-art", ref: "PicoGameDetail.page/NoArtOrHistory", model: base, keys: ["Right", "Right", "Right", "Right", "Right", "Right", "Right", "Right", "Return"] },
  {
    name: "detail-downloading",
    ref: "PicoGameDetail.page/Downloading",
    model: {
      ...base,
      catalog: {
        _tag: "Ready",
        games: (base.catalog._tag === "Ready" ? base.catalog.games : []).map(game =>
          game.id === "celeste" ? { ...game, download: { bytes: 1_500_000_000, progress: { done: 620_000_000, total: 1_500_000_000 } } } : game),
      },
      fixtureGameActions: { celeste: [{ id: "cancel-download", label: "Cancel download", enabled: true }] },
    },
    keys: ["Return"],
  },
  // Find
  { name: "find-default", ref: "PicoLibrary.page/default", model: base, keys: ["o"] },
  { name: "find-search", ref: "PicoLibrary.page/SearchResults", model: base, keys: ["o", ...search("HOLLOW")] },
  // Settings
  { name: "settings-default", ref: "PicoSettings.page/default", model: base, keys: toSettings },
  { name: "settings-wifi", ref: "PicoSettings.page/Wifi", model: { ...base, settings: [], network: { connection: { _tag: "Wifi", ssid: "Home-5G", strengthPercent: 72 }, wifi: { _tag: "Available", scanning: false, networks: [...fixtureNetworks], attempt: { _tag: "Idle" } } } }, keys: toSettings },
  { name: "settings-edit-name", ref: "PicoSettings.page/EditName", model: base, keys: [...toSettings, "Down", "Return"] },
  { name: "settings-error", ref: "PicoSettings.page/Error", model: { ...failedSettingsModel, fixtureGameActions: actions }, keys: toSettings },
  { name: "settings-plugins", ref: "PicoSettings.page/PluginsHub", model: { ...pluginModels.repositories, fixtureGameActions: actions }, keys: [...toSettings, "Right"] },
  { name: "settings-identity", ref: "PicoSettings.page/IdentityActions", model: { ...base, settings: [], identityManagement: { localBackupAvailable: true, retiredPublicKeys: [], status: { _tag: "Idle" } } }, keys: toSettings },
  // Launch settings and the runner picker
  { name: "launch-default", ref: "PicoLaunchSettings.page/default", model: { ...withLaunchSettings(base, launchSettingsFixtures.ready), fixtureGameActions: actions } },
  { name: "launch-loading", ref: "PicoLaunchSettings.page/Loading", model: { ...withLaunchSettings(base, launchSettingsFixtures.loading as never), fixtureGameActions: actions } },
  { name: "runner-ready", ref: "PicoRunnerPicker.page/Ready", model: { ...base, runnerChoice: { ...runnerChoice, _tag: "Ready" } } },
  // The store
  { name: "store-providers", ref: "PicoStore.page/default", model: { ...base, store: storeFixture() }, keys: toStore },
  {
    name: "store-results",
    ref: "PicoStore.page/Results",
    model: { ...base, store: storeFixture({ search: { _tag: "Ready", providerId: "@korri:pico8", query: "celeste", entries: [...STORE_ENTRIES] } }) },
    keys: [...toStore, "Return"],
  },
  {
    name: "store-entry",
    ref: "PicoStore.page/Entry",
    model: { ...base, store: storeFixture({ search: { _tag: "Ready", providerId: "@korri:pico8", query: "celeste", entries: [...STORE_ENTRIES] }, details: storeDetails(STORE_GETS.offer) }) },
    keys: [...toStore, "Return", "Return"],
  },
  // First-boot setup
  { name: "setup-hello", ref: "PicoOnboarding.page/default", model: onboardingModel },
  { name: "setup-wifi", ref: "PicoOnboarding.page/WifiNetworks", model: { ...onboardingModel, network: { connection: { _tag: "Offline" }, wifi: { _tag: "Available", scanning: false, networks: [...fixtureNetworks], attempt: { _tag: "Idle" } } } }, keys: ["Return"] },
  { name: "setup-password", ref: "PicoOnboarding.page/WifiPassword", model: { ...onboardingModel, network: { connection: { _tag: "Offline" }, wifi: { _tag: "Available", scanning: false, networks: [...fixtureNetworks], attempt: { _tag: "Idle" } } } }, keys: ["Return", "Return"] },
  { name: "setup-pads", ref: "PicoOnboarding.page/Controllers", model: { ...onboardingModel, network: { connection: { _tag: "Wired" }, wifi: { _tag: "NoAdapter" } }, onboarding: { ...onboardingModel.onboarding!, controllers: [...fixtureControllers] } }, keys: ["Return", "Return", "Return"] },
  // The gameplay overlay
  { name: "overlay-default", ref: "PicoOverlay.page/default", model: { ...base, presentation: fixtureOverlay } },
  { name: "overlay-confirm-quit", ref: "PicoOverlay.page/ConfirmQuit", model: { ...base, presentation: fixtureOverlay }, keys: ["Down", "Return"] },
  // Two displays: the showcase on the main one
  { name: "showcase", ref: "PicoDisplays.template/Shelf", model: base, displays: rgdsDisplays },
  // Attract: the shell's Tick reaches 45 s with no press (wait 46 s)
  { name: "attract", ref: "PicoAttract.organism/default", model: base },
]

if (import.meta.main) {
  const out = process.argv[2] ?? "preview/gen"
  mkdirSync(out, { recursive: true })
  const list = SCENARIOS.map(scenario => {
    writeFileSync(join(out, `${scenario.name}.json`), JSON.stringify(scenario.model))
    const then = (scenario.then ?? []).map((model, i) => {
      const file = join(out, `${scenario.name}.then${i}.json`)
      writeFileSync(file, JSON.stringify(model))
      return file
    })
    return { name: scenario.name, ref: scenario.ref, keys: scenario.keys ?? [], then, displays: scenario.displays ?? null }
  })
  writeFileSync(join(out, "scenarios.json"), JSON.stringify(list, null, 2))
  console.log(`${list.length} scenarios in ${out}`)
}

void fixtureGame
