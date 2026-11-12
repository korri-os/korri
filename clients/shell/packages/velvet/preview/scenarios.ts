/**
 * Velvet's preview scenarios: SurfaceModels for the fixture korrid, and the
 * presses that bring the package to each screen and layer.
 *
 *     cd clients/shell && bun packages/velvet/preview/scenarios.ts
 *
 * It writes preview/scenarios/*.json, which preview/shots.sh replays through
 * the real shell under Xvfb, tests/velvet_package.rs walks at five sizes and
 * preview/probe.sh hands to the shell's probe.
 *
 * The library is made up for this: titles, facts and pictures (preview/art,
 * drawn by preview/art/build.py). It holds every mix of art a real library
 * has: a hero, a logo and a poster; a hero and a logo; a logo and a poster;
 * a hero alone; a poster alone; a logo alone; a square cover alone; nothing
 * on a system Velvet knows; nothing on one it does not; nothing and no
 * system. Korri's other slices (settings, the network, a runner choice,
 * launch settings, the store, identity, first boot, the overlay, statuses)
 * are in preview/fixtures/korri.json.
 *
 * Steps: up down left right confirm back menu options system, `model N`
 * (publish the scenario's Nth model), `wait SECONDS`.
 */
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { join } from "node:path"

const here = import.meta.dir
const korri = JSON.parse(readFileSync(join(here, "fixtures/korri.json"), "utf8"))

/** The moment every scenario is published: Wednesday 11 November 2026, 21:47 UTC. */
const NOW = Date.UTC(2026, 10, 11, 21, 47)
const MINUTE = 60_000
const HOUR = 60 * MINUTE
const DAY = 24 * HOUR

type Media = { role: "hero" | "logo" | "poster"; url: string; width: number; height: number; color?: string }
const hero = (name: string, color: string): Media => ({ role: "hero", url: `preview/art/hero-${name}.png`, width: 1920, height: 620, color })
const poster = (name: string, color: string): Media => ({ role: "poster", url: `preview/art/poster-${name}.png`, width: 600, height: 900, color })
const logo = (name: string, width: number, height: number): Media => ({ role: "logo", url: `preview/art/logo-${name}.png`, width, height })

interface Fixture {
  id: string
  title: string
  systemId?: string
  system?: string
  section: string
  ago?: number
  plays?: number
  hours?: number
  media?: Media[]
  cover?: string
  resumable?: boolean
  places?: { id: string; label: string }[]
  extra?: object
}

const HERE = "This device"
const fixtures: Fixture[] = [
  { id: "ember-road", title: "Ember Road", systemId: "windows", system: "PC", section: HERE, ago: 2 * HOUR, plays: 46, hours: 61.33, resumable: true, media: [hero("ember", "#b0452a"), logo("ember", 785, 338), poster("ember", "#b0452a")] },
  { id: "tide-pool", title: "Tide Pool", systemId: "gba", system: "GBA", section: HERE, ago: 1 * DAY + 3 * HOUR, plays: 18, hours: 26.66, media: [hero("tide", "#2a7a8c"), logo("tide", 2364, 704)], places: [{ id: "here", label: HERE }, { id: "zao", label: "zao" }] },
  { id: "moss-and-stone", title: "Moss and Stone", systemId: "snes", system: "SNES", section: HERE, ago: 3 * DAY, plays: 112, hours: 19.1, media: [logo("moss", 1936, 1243), poster("moss", "#4a7a3a")] },
  { id: "dusk-runner", title: "Dusk Runner", systemId: "windows", system: "PC", section: HERE, ago: 4 * DAY, plays: 37, hours: 22.2, media: [hero("dusk", "#5a3a8a"), logo("dusk", 830, 355), poster("dusk", "#5a3a8a")] },
  { id: "sand-king", title: "Sand King", systemId: "gbc", system: "GBC", section: HERE, ago: 5 * DAY, plays: 14, hours: 6.5, media: [poster("sand", "#c9a45a")] },
  { id: "comet-courier", title: "Comet Courier", systemId: "gb", system: "Game Boy", section: HERE, ago: 9 * DAY, plays: 7, hours: 5.2, cover: "preview/art/cover-ember.png" },
  { id: "lantern-keep", title: "Lantern Keep", systemId: "nes", system: "NES", section: HERE, ago: 10 * DAY, plays: 4, hours: 3.5 },
  { id: "bramble-run", title: "Bramble Run", systemId: "md", system: "Mega Drive", section: HERE, ago: 16 * DAY, plays: 140, hours: 96.75 },
  { id: "petal-quest", title: "Petal Quest", systemId: "pico8", system: "PICO-8", section: HERE, ago: 21 * DAY, plays: 31, hours: 14.2, media: [logo("sand", 770, 304)] },
  { id: "quartz-drift", title: "Quartz Drift", systemId: "n64", system: "Nintendo 64", section: HERE, ago: 23 * DAY, plays: 3, hours: 2.2, media: [hero("moss", "#4a7a3a")] },
  { id: "iron-orchard", title: "Iron Orchard", systemId: "psx", system: "PlayStation", section: HERE, ago: 38 * DAY, plays: 41, hours: 18.4 },
  { id: "clockmaker", title: "The Long Winter of the Clockmaker's Apprentice", systemId: "snes", system: "SNES", section: HERE, ago: 42 * DAY, plays: 64, hours: 88.25 },
  { id: "ucity", title: "µCity", systemId: "gbc", system: "GBC", section: HERE, ago: 63 * DAY, plays: 22, hours: 12.1 },
  { id: "signal-nine", title: "Signal Nine", systemId: "wasm4", system: "WASM-4", section: HERE, ago: 122 * DAY, plays: 22, hours: 11.8 },
  { id: "harbor-lights", title: "Harbor Lights", systemId: "tic80", system: "TIC-80", section: HERE, ago: 244 * DAY, plays: 9, hours: 22.75, cover: "preview/art/cover-tide.png" },
  { id: "paper-kites", title: "Paper Kites", systemId: "nds", system: "Nintendo DS", section: HERE },
  { id: "night-ferry", title: "Night Ferry", section: HERE },
  { id: "antidisestablishmentarianism", title: "Antidisestablishmentarianism", systemId: "gba", system: "GBA", section: HERE, extra: { download: { bytes: 1_240_000_000 } } },
  { id: "glass-garden", title: "Glass Garden", section: "zao", ago: 9 * DAY, plays: 12, hours: 7.3 },
  { id: "vesper", title: "Vesper", section: "zao", ago: 151 * DAY, plays: 12, hours: 7.3, extra: { saves: { _tag: "Split", choose: { id: "save-choose", label: "Choose a save", enabled: true } } } },
  { id: "copper-sky", title: "Copper Sky", section: "zao" },
]

function game(fixture: Fixture, art: boolean): object {
  return {
    id: fixture.id,
    title: fixture.title,
    section: fixture.section,
    ...(fixture.systemId === undefined ? {} : { systemId: fixture.systemId, system: fixture.system, subtitle: fixture.system }),
    ...(fixture.ago === undefined ? {} : { lastPlayedAt: NOW - fixture.ago, playCount: fixture.plays, totalPlaytimeSeconds: Math.round((fixture.hours ?? 0) * 3600) }),
    ...(fixture.resumable === true ? { resumable: true } : {}),
    ...(fixture.places === undefined ? {} : { launchLocations: fixture.places }),
    ...(art && fixture.media !== undefined ? { media: fixture.media } : {}),
    ...(art && fixture.cover !== undefined ? { coverArtUrl: fixture.cover } : {}),
    ...(fixture.extra ?? {}),
  }
}

const base = {
  presentation: { kind: "catalog" },
  status: { _tag: "Browsing" },
  actions: korri.actions,
  settings: korri.settings,
  settingsStatus: { _tag: "Idle" },
  network: korri.network,
  buildLabel: korri.buildLabel,
  clockLabel: "21:47",
  publishedAt: NOW,
  store: { providers: korri.store.results.providers, search: { _tag: "Idle" }, details: { _tag: "Closed" } },
  identityManagement: korri.identity.idle,
}

const library = (art: boolean, list: Fixture[] = fixtures) => ({ ...base, catalog: { _tag: "Ready", games: list.map(fixture => game(fixture, art)) } })
const ready = library(true)
const bare = library(false)
/** A library nobody has played yet, and one with a single game. */
const fresh = library(true, fixtures.map(({ ago: _ago, plays: _plays, hours: _hours, resumable: _resumable, ...rest }) => rest))
const one = library(true, [fixtures[1]!])
/** The other device holds more games than a shelf shows: its shelf ends in "See all". */
const crowd = library(true, [...fixtures, ...["Salt Road", "Winter Kiln", "Low Tide Radio", "Paper Moon", "Brass Owl", "Hollow Pine"].map((title, index): Fixture => ({ id: `zao-${index}`, title, section: "zao", ...(index % 2 === 0 ? { systemId: "snes", system: "SNES" } : {}), ago: (30 + index * 11) * DAY, plays: 3 + index, hours: 2 + index }))])
/** A week away: nothing played in the last seven days, so the pick is the most played. */
const stale = { ...library(true, fixtures.map(fixture => (fixture.ago === undefined ? fixture : { ...fixture, ago: fixture.ago + 18 * DAY, resumable: false }))) }
/** No time from Korri: the facts say a date. */
const { publishedAt: _published, ...undated } = ready

const withStatus = (status: object, model: object = ready) => ({ ...model, status })
const withActions = (model: object) => ({ ...model, focusedGame: { gameId: "", actions: korri.gameActions } })
const identity = (state: object) => ({ ...ready, identityManagement: state })
const store = (state: object) => ({ ...ready, store: state })
const overlay = (presentation: object, more: object = {}) => ({ ...ready, presentation, ...more })
const firstBoot = (more: object = {}) => ({ ...ready, catalog: { _tag: "Empty" }, settings: korri.onboarding.settings, network: korri.onboarding.network, onboarding: korri.onboarding.onboarding, ...more })

const invites = [{
  id: "invite-1", personName: "Mika", gameTitle: "Glass Garden", deviceLabel: "zao",
  actions: [{ id: "invite-1:stream", label: "Play together", description: "Joins the session on zao", enabled: true }, { id: "invite-1:dismiss", label: "Not now", enabled: true }],
}]

const saveHistory = {
  _tag: "Ready", gameId: "tide-pool",
  entries: [
    { snapshotId: "s3", device: "mini", endedAt: NOW - 1 * DAY - 3 * HOUR, durationSeconds: 4380, interrupted: false, current: true },
    { snapshotId: "s2", device: "zao", endedAt: NOW - 3 * DAY, durationSeconds: 1260, interrupted: true, current: false, restore: { id: "save-restore:s2", label: "Go back to this save", description: "This device continues from it. Newer saves stay in the history.", enabled: true } },
    { snapshotId: "s1", device: "mini", endedAt: NOW - 9 * DAY, durationSeconds: 7500, interrupted: false, current: false, restore: { id: "save-restore:s1", label: "Go back to this save", enabled: true } },
  ],
  actions: [{ id: "save-history:close", label: "Close", enabled: true }],
}

const chooseSave = {
  _tag: "ChooseSave", gameId: "vesper", kicker: "Two saves of Vesper", detail: "This device and zao each played on their own. Choose the one to keep going from; the other stays in the history.",
  sides: [
    { actionId: "save-pick:mini", device: "mini", endedAt: NOW - 2 * DAY, durationSeconds: 5400 },
    { actionId: "save-pick:zao", device: "zao", endedAt: NOW - 1 * DAY, durationSeconds: 1500 },
  ],
  actions: [{ id: "save-pick:cancel", label: "Not now", enabled: true }], declineActionId: "save-pick:cancel",
}

const paused = { _tag: "Paused", kicker: "zao paused the game", detail: "Glass Garden is waiting on zao. It comes back when zao resumes.", actions: [{ id: "stream-play:leave", label: "Leave", enabled: true }], declineActionId: "stream-play:leave", gameId: "glass-garden" }
const ended = { _tag: "Ended", kicker: "The session ended", reason: "zao closed Glass Garden.", gameId: "glass-garden", gameTitle: "Glass Garden" }
const busySteps = { ...korri.status.steps, kicker: "Starting Tide Pool", gameId: "tide-pool" }
const busyProgress = { ...korri.status.progress, gameId: "tide-pool" }
const failedSetting = { _tag: "Problem", settingId: "device-name", message: "That name is already used by another device." }
const wifiFailed = { ...korri.networkFailed, wifi: { ...korri.networkFailed.wifi, attempt: { _tag: "Failed", ssid: "Attic 5G", message: "The password was not accepted.", passwordRejected: false } } }

interface Scenario {
  readonly name: string
  readonly models: readonly unknown[]
  readonly steps: readonly string[]
}

const toMenu = ["menu"]
const toSettings = ["system"]
const toWifi = ["menu", "down", "down", "confirm"]
const toIdentity = ["menu", "down", "down", "down", "confirm"]
const toStore = ["menu", "confirm"]
/** First boot: past the controllers, and past Wi-Fi with "Skip for now". */
const pastWifi = ["confirm", "down", "down", "down", "down", "down", "confirm"]
const toLobby = ["down", "down"]
const toSet = [...toLobby, "right", "right", "right", "right", "right", "right", "confirm"]
const toSystems = [...toLobby, "down", "down", "down"]

const scenarios: Scenario[] = [
  // The home.
  { name: "home", models: [ready], steps: [] },
  { name: "home-no-art", models: [bare], steps: [] },
  { name: "home-menu-pill", models: [ready], steps: ["up"] },
  { name: "home-stale", models: [stale], steps: [] },
  { name: "home-fresh", models: [fresh], steps: [] },
  { name: "home-one", models: [one], steps: [] },
  { name: "home-undated", models: [undated], steps: [] },
  { name: "row", models: [ready], steps: ["down"] },
  { name: "row-no-art", models: [bare], steps: ["down", "right"] },
  { name: "row-all", models: [ready], steps: ["down", "right", "right", "right", "right"] },
  // The library.
  { name: "lobby", models: [ready], steps: toLobby },
  { name: "lobby-no-art", models: [bare], steps: [...toLobby, "right"] },
  { name: "lobby-not-started", models: [ready], steps: [...toLobby, "down"] },
  { name: "lobby-peer", models: [ready], steps: [...toLobby, "down", "down"] },
  { name: "lobby-systems", models: [ready], steps: toSystems },
  { name: "set", models: [ready], steps: toSet },
  { name: "set-no-art", models: [bare], steps: [...toSet, "right", "down"] },
  { name: "set-pill", models: [ready], steps: [...toSet, "up"] },
  { name: "set-sort", models: [ready], steps: [...toSet, "options"] },
  { name: "set-sorted", models: [ready], steps: [...toSet, "options", "down", "confirm", "back", "down"] },
  { name: "set-system", models: [ready], steps: [...toSystems, "confirm"] },
  { name: "set-deep", models: [ready], steps: [...toSet, "down", "down", "down", "right"] },
  { name: "set-peer", models: [crowd], steps: [...toLobby, "down", "down", "right", "right", "right", "right", "right", "confirm", "right"] },
  { name: "detail-peer", models: [ready], steps: [...toLobby, "down", "down", "right", "right", "confirm"] },
  // The detail.
  { name: "detail", models: [ready], steps: ["confirm"] },
  { name: "detail-no-art", models: [bare], steps: ["confirm"] },
  { name: "detail-two-places", models: [ready], steps: ["down", "confirm"] },
  { name: "detail-poster", models: [ready], steps: ["down", "right", "confirm"] },
  { name: "detail-actions", models: [withActions(ready)], steps: ["confirm", "options"] },
  { name: "detail-actions-asking", models: [withActions(ready)], steps: ["confirm", "options", "down", "confirm"] },
  { name: "detail-long-title", models: [bare], steps: [...toSet, "options", "down", "confirm", "back", "down", "down", "down", "down", "down", "right", "right", "confirm"] },
  // The library with no pick.
  { name: "empty", models: [{ ...base, catalog: { _tag: "Empty" } }], steps: [] },
  { name: "empty-no-store", models: [{ ...base, store: undefined, catalog: { _tag: "Empty" } }], steps: [] },
  { name: "loading", models: [{ ...base, catalog: { _tag: "Loading" } }], steps: [] },
  { name: "error", models: [{ ...base, catalog: { _tag: "Error", message: "Korri could not read the library on this device. The storage it is on may be missing." } }], steps: [] },
  // The house menu and its pages.
  { name: "menu", models: [ready], steps: toMenu },
  { name: "menu-invite", models: [{ ...ready, invites }], steps: toMenu },
  { name: "invites", models: [{ ...ready, invites }], steps: ["menu", "down", "confirm"] },
  { name: "invite-notice", models: [ready, { ...ready, invites }], steps: ["model 1"] },
  { name: "settings", models: [ready], steps: toSettings },
  { name: "settings-choice", models: [ready], steps: [...toSettings, "down"] },
  { name: "settings-text", models: [ready], steps: [...toSettings, "confirm"] },
  { name: "settings-text-symbols", models: [ready], steps: [...toSettings, "confirm", "down", "down", "down", "down", "left", "left", "left", "left", "left", "left", "left", "left", "confirm"] },
  { name: "settings-secret", models: [ready], steps: [...toSettings, "down", "down", "down", "confirm", "confirm", "confirm"] },
  { name: "settings-destructive", models: [ready], steps: [...toSettings, "down", "down", "down", "down", "down", "down", "confirm"] },
  { name: "settings-problem", models: [{ ...ready, settingsStatus: failedSetting }], steps: toSettings },
  { name: "settings-saving", models: [{ ...ready, settingsStatus: { _tag: "Saving", settingId: "host.preferences.playerCount" } }], steps: [...toSettings, "down"] },
  { name: "settings-foot", models: [ready], steps: [...toSettings, "down", "down", "down", "down", "down", "down", "down", "down", "down", "down", "down", "down", "down", "down"] },
  { name: "wifi", models: [ready], steps: toWifi },
  { name: "wifi-password", models: [ready], steps: [...toWifi, "down", "down", "confirm"] },
  { name: "wifi-hidden", models: [ready], steps: [...toWifi, "down", "down", "down", "down", "down", "confirm"] },
  { name: "wifi-failed", models: [{ ...ready, network: wifiFailed }], steps: [...toWifi, "down", "down"] },
  { name: "wifi-none", models: [{ ...ready, network: { connection: { _tag: "Wired" }, wifi: { _tag: "NoAdapter" } } }], steps: toWifi },
  { name: "identity", models: [ready], steps: toIdentity },
  { name: "identity-last", models: [ready], steps: [...toIdentity, "down", "down"] },
  { name: "identity-retired", models: [identity(korri.identity.retired)], steps: [...toIdentity, "down", "down", "down", "confirm"] },
  { name: "identity-password", models: [ready], steps: [...toIdentity, "confirm"] },
  { name: "identity-backup", models: [identity(korri.identity.backup)], steps: toIdentity },
  { name: "identity-working", models: [identity(korri.identity.working)], steps: toIdentity },
  { name: "identity-problem", models: [identity(korri.identity.problem)], steps: toIdentity },
  { name: "identity-switched", models: [identity({ ...korri.identity.idle, status: { _tag: "Switched", ownerPublicKey: "npub13h8y59c8ql6ctesyg4zq9xnu3zntx0vxc9uw5px2g8nmt6fajva29st0rt" } })], steps: toIdentity },
  // Getting games.
  { name: "store", models: [ready], steps: toStore },
  { name: "store-none", models: [store({ providers: { _tag: "Ready", providers: [] }, search: { _tag: "Idle" }, details: { _tag: "Closed" } })], steps: toStore },
  { name: "store-search", models: [ready], steps: [...toStore, "confirm", "confirm"] },
  { name: "store-results", models: [store(korri.store.results)], steps: [...toStore, "confirm", "down"] },
  { name: "store-offer", models: [store(korri.store.offer)], steps: [...toStore, "confirm", "down", "confirm"] },
  { name: "store-downloading", models: [store(korri.store.downloading)], steps: [...toStore, "confirm", "down", "confirm"] },
  { name: "store-confirm", models: [store(korri.store.confirm)], steps: [...toStore, "confirm", "down", "confirm"] },
  // Korri's status, over the home.
  { name: "busy-steps", models: [withStatus(busySteps)], steps: [] },
  { name: "busy-progress", models: [withStatus(busyProgress)], steps: [] },
  { name: "busy-plain", models: [withStatus({ _tag: "Busy", kicker: "Checking saves" })], steps: [] },
  { name: "problem", models: [withStatus({ ...korri.status.problem, gameId: "tide-pool", gameTitle: "Tide Pool" })], steps: [] },
  { name: "problem-long", models: [withStatus(korri.status.problemLong)], steps: ["down", "down"] },
  { name: "confirm", models: [withStatus(korri.status.confirm)], steps: [] },
  { name: "choose-save", models: [withStatus(chooseSave)], steps: [] },
  { name: "paused-stream", models: [withStatus(paused)], steps: [] },
  { name: "ended", models: [withStatus(ended)], steps: [] },
  { name: "running", models: [withStatus({ _tag: "Running", kicker: "Playing now", gameId: "tide-pool" })], steps: [] },
  // Korri's own pages.
  { name: "saves", models: [{ ...ready, saveHistory }], steps: [] },
  { name: "saves-restore", models: [{ ...ready, saveHistory }], steps: ["down", "confirm"] },
  { name: "saves-empty", models: [{ ...ready, saveHistory: { ...saveHistory, entries: [] } }], steps: [] },
  { name: "runner", models: [{ ...ready, runnerChoice: korri.runnerChoice }], steps: [] },
  { name: "launch-settings", models: [{ ...ready, launchSettings: korri.launchSettings }], steps: ["down"] },
  { name: "first-boot", models: [firstBoot()], steps: [] },
  { name: "first-boot-wifi", models: [firstBoot()], steps: ["confirm"] },
  { name: "first-boot-names", models: [firstBoot()], steps: [...pastWifi] },
  { name: "first-boot-ready", models: [firstBoot()], steps: [...pastWifi, "down", "confirm"] },
  { name: "first-boot-starter", models: [firstBoot({ onboarding: { ...korri.onboarding.onboarding, starterPack: { _tag: "Offered", actions: [{ id: "starter-pack:install", label: "Get the Starter pack", enabled: true }] } } })], steps: [...pastWifi, "down", "confirm"] },
  // The gameplay overlay.
  { name: "overlay", models: [overlay(korri.overlay.plain)], steps: [] },
  { name: "overlay-groups", models: [overlay(korri.overlay.groups)], steps: ["down", "down"] },
  { name: "overlay-stream", models: [overlay(korri.overlay.stream, { clockLabel: "21:47" })], steps: ["down", "down", "down"] },
  { name: "overlay-asking", models: [overlay(korri.overlay.plain)], steps: ["down", "confirm"] },
]

const out = join(here, "scenarios")
rmSync(out, { recursive: true, force: true })
mkdirSync(out, { recursive: true })
for (const scenario of scenarios) writeFileSync(join(out, `${scenario.name}.json`), `${JSON.stringify(scenario)}\n`)
console.log(`${scenarios.length} scenarios`)
