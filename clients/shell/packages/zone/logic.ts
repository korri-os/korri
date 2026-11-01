/**
 * Zone's logic for the native shell (docs/briefs/2026-10-31-native-surfaces-contract.md,
 * section 3.1). Adapted from the surface-shell spike's Zone package.
 *
 * Zone's real program (surfaces/zone/src/state/surface.ts) runs unchanged.
 * In the browser the host moves DOM focus and the page reports `Focused…`
 * facts; here no DOM exists, so a thin adapter turns the shell's
 * `{ _tag: "Pressed", button }` into the same facts, by where the cursor is:
 *
 *     tabs     left, right    FocusedTab on the next tab
 *              down           FocusedCard on the band's focused card
 *              confirm        ChoseTab GAMES (the other pages are not drawn)
 *     find     left, up       FocusedTab SAVE DATA
 *              down           FocusedCard
 *     cards    left, right    FocusedCard one step along (Zone clamps it)
 *              up             FocusedTab on the shown tab
 *              down           FocusedAction on the lit button, or the first
 *     actions  left, right    FocusedAction one step along the row
 *              up             FocusedCard
 *     back, menu, system      Zone's own PressedBack, PressedMenu, PressedSystem
 *
 * The catalog is a cursor window (contract 3.4). Zone's state code reads
 * `catalog.games`, so the adapter hands it the window as that list. Near an
 * edge of the window the adapter asks for a window around the focused game
 * (`MoveCatalog`, `Around`), and after Korri publishes it, focuses the same
 * game again. Every change of focused game is reported (`FocusGame`).
 * Requests Zone's update makes are forwarded in the treaty's words.
 *
 * Pure: no timers, no Date, no randomness. The facts line's day is computed
 * from Korri's epoch milliseconds in UTC.
 */
// Zone's state code still speaks the TypeScript treaty it was written against;
// `legacy` below hands it the model in that shape.
import type { SurfaceGame, SurfaceModel, SurfaceNetworkConnection } from "../../../../contracts/surface/korri-surface"
import { launchChoices, playtime, type ZoneLaunchChoice } from "../../../../surfaces/zone/src/state/games"
import { bandGames, ZONE_TABS, type ZoneNavMessage, type ZoneTab } from "../../../../surfaces/zone/src/state/navigation"
import type { ZoneRequest } from "../../../../surfaces/zone/src/state/requests"
import { update as zoneUpdate, zoneShown, zoneSurfaceStart, type ZoneSurfaceMessage, type ZoneSurfaceState } from "../../../../surfaces/zone/src/state/surface"
import { isZoneLook, ZONE_DEFAULT_LOOK } from "../../../../surfaces/zone/src/plan/look"

import type {
  SurfaceButton as Button,
  SurfaceMessage as Message,
  SurfaceModel as Model,
  SurfaceRequest as Request,
} from "../../../../contracts/generated/surface"

export interface State {
  readonly zone: ZoneSurfaceState
  /** The focused game Korri last heard of (FocusGame). */
  readonly reported: string | null
  /** A MoveCatalog is out; no second one until Korri publishes. */
  readonly moving: boolean
}

const LOOK_KEY = "zone.look"
/** Cards from a window edge at which Zone asks for a window around the focus. */
const EDGE = 8
const WINDOW = 50

// ---- the adapter between the cursor catalog and Zone's state code ----

let cachedModel: Model | undefined
let cachedLegacy: SurfaceModel | undefined
/** Zone's state code reads `catalog.games`: give it the window. */
function legacy(model: Model): SurfaceModel {
  if (model === cachedModel && cachedLegacy !== undefined) return cachedLegacy
  const catalog = model.catalog._tag === "Ready" ? { _tag: "Ready" as const, games: model.catalog.window } : model.catalog
  cachedModel = model
  cachedLegacy = { ...model, catalog } as unknown as SurfaceModel
  return cachedLegacy
}

const focusedId = (state: ZoneSurfaceState, model: Model): string | null =>
  bandGames(state.nav, legacy(model))[state.nav.card]?.id ?? null

/** Zone's request in the treaty's words, or nothing for Zone's own. */
function treaty(request: ZoneRequest): Request | undefined {
  switch (request._tag) {
    case "LaunchGame":
      return request.locationId === undefined
        ? { _tag: "LaunchGame", gameId: request.gameId }
        : { _tag: "LaunchGame", gameId: request.gameId, launchLocationId: request.locationId }
    case "DismissProblem":
      return { _tag: "Dismiss" }
    case "RememberLook":
      return { _tag: "RememberPreference", key: LOOK_KEY, value: request.look }
    default:
      return request as unknown as Request
  }
}

// ---- init and update ----

/**
 * Zone's start in the stored look, then the Caliper part's opening
 * (ZoneGames.page.part.tsx, `Ready`: FocusedCard 1), so the first frame is
 * the reference state: Hollow Knight focused.
 */
export function init(model: Model, preferences: Readonly<Record<string, string>>): State {
  const stored = preferences[LOOK_KEY]
  const start = zoneSurfaceStart(legacy(model), isZoneLook(stored) ? stored : ZONE_DEFAULT_LOOK)
  const zone = run(start, { _tag: "Games", message: { _tag: "FocusedCard", index: 1 } }, model, []).zone
  return { zone, reported: null, moving: false }
}

/** One step of Zone's own program; its requests are added to `out`. */
function run(state: ZoneSurfaceState, message: ZoneSurfaceMessage, model: Model, out: Request[]): { zone: ZoneSurfaceState } {
  const step = zoneUpdate(state, message, legacy(model))
  for (const request of step.requests) {
    const sent = treaty(request)
    if (sent !== undefined) out.push(sent)
  }
  return { zone: step.model }
}

const games = (state: ZoneSurfaceState, message: ZoneNavMessage, model: Model, out: Request[]): ZoneSurfaceState =>
  run(state, { _tag: "Games", message }, model, out).zone

export function update(state: State, message: Message, model: Model): { state: State; requests: Request[] } {
  const out: Request[] = []
  let zone = state.zone
  let moving = state.moving
  switch (message._tag) {
    case "KorriPublished": {
      moving = false
      zone = run(zone, { _tag: "KorriPublished" }, model, out).zone
      // A new window: keep the focus on the same game.
      if (state.reported !== null && model.catalog._tag === "Ready") {
        const index = bandGames(zone.nav, legacy(model)).findIndex(game => game.id === state.reported)
        if (index !== -1 && index !== zone.nav.card) zone = games(zone, { _tag: "FocusedCard", index }, model, out)
      }
      break
    }
    case "PreferenceStored":
      break
    case "Pressed":
      zone = pressed(zone, message.button, model, out)
      break
  }
  const focused = focusedId(zone, model)
  if (focused !== state.reported) out.push(focused === null ? { _tag: "FocusGame" } : { _tag: "FocusGame", gameId: focused })
  const catalog = model.catalog
  if (!moving && focused !== null && catalog._tag === "Ready" && zone.nav.zone === "cards") {
    const card = zone.nav.card
    if ((catalog.after !== undefined && card >= catalog.window.length - EDGE) || (catalog.before !== undefined && card < EDGE)) {
      out.push({ _tag: "MoveCatalog", query: catalog.query, from: { _tag: "Around", gameId: focused }, count: WINDOW })
      moving = true
    }
  }
  return { state: { zone, reported: focused, moving }, requests: out }
}

function pressed(state: ZoneSurfaceState, button: Button, model: Model, out: Request[]): ZoneSurfaceState {
  const nav = state.nav
  switch (button) {
    case "back":
      return run(state, { _tag: "PressedBack" }, model, out).zone
    case "menu":
      return run(state, { _tag: "PressedMenu" }, model, out).zone
    case "system":
      return run(state, { _tag: "PressedSystem" }, model, out).zone
    case "options":
    case "up":
    case "down":
    case "left":
    case "right":
    case "confirm":
      break
  }
  // Directions and confirm move Zone's cursor only on GAMES with nothing over it.
  if (zoneShown(state, legacy(model)) !== "games" || nav.layers.length > 0) return state
  const band = () => bandGames(nav, legacy(model))
  const go = (message: ZoneNavMessage) => games(state, message, model, out)
  switch (nav.zone) {
    case "tabs": {
      const at = ZONE_TABS.indexOf(nav.tabCursor)
      if (button === "left" || button === "right") {
        const next = ZONE_TABS[clamp(at + (button === "left" ? -1 : 1), 0, ZONE_TABS.length - 1)]!
        return next === nav.tabCursor ? state : go({ _tag: "FocusedTab", tab: next })
      }
      if (button === "down" && nav.tab === "games") return go({ _tag: "FocusedCard", index: nav.card })
      if (button === "confirm" && nav.tabCursor === "games") return go({ _tag: "ChoseTab", tab: "games" })
      return state
    }
    case "find":
      if (button === "left" || button === "up") return go({ _tag: "FocusedTab", tab: "savedata" })
      if (button === "down") return go({ _tag: "FocusedCard", index: nav.card })
      return state
    case "get":
      if (button === "down") return go({ _tag: "FocusedCard", index: nav.card })
      return state
    case "cards": {
      if (button === "left" || button === "right") {
        const next = clamp(nav.card + (button === "left" ? -1 : 1), 0, Math.max(0, band().length - 1))
        return next === nav.card ? state : go({ _tag: "FocusedCard", index: next })
      }
      if (button === "up") return go({ _tag: "FocusedTab", tab: nav.tab })
      if (button === "down") return band()[nav.card] === undefined ? state : go({ _tag: "FocusedAction", index: 0 })
      return state
    }
    case "actions": {
      if (button === "left" || button === "right") {
        const game = band()[nav.card]
        if (game === undefined) return state
        // The launch buttons, then OPTS.
        const next = clamp(nav.action + (button === "left" ? -1 : 1), 0, launchChoices(game).length)
        return next === nav.action ? state : go({ _tag: "FocusedAction", index: next })
      }
      if (button === "up") return go({ _tag: "FocusedCard", index: nav.card })
      return state
    }
  }
}

const clamp = (value: number, low: number, high: number): number => Math.min(Math.max(value, low), high)

// ---- view ----

/** The tabs in strip order, in Zone's words (ZoneGames.tsx `TABS`). */
const TAB_WORDS: Readonly<Record<ZoneTab, string>> = { games: "Games", options: "Options", savedata: "Save data" }

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]

/** A day from epoch ms, in UTC, without Date (Hinnant's civil_from_days): 19 Oct 2026. */
export function dayOf(at: number): string {
  const z = Math.floor(at / 86_400_000) + 719_468
  const era = Math.floor(z / 146_097)
  const doe = z - era * 146_097
  const yoe = Math.floor((doe - Math.floor(doe / 1460) + Math.floor(doe / 36_524) - Math.floor(doe / 146_096)) / 365)
  const doy = doe - (365 * yoe + Math.floor(yoe / 4) - Math.floor(yoe / 100))
  const mp = Math.floor((5 * doy + 2) / 153)
  const d = doy - Math.floor((153 * mp + 2) / 5) + 1
  const m = mp < 10 ? mp + 3 : mp - 9
  const y = yoe + era * 400 + (m <= 2 ? 1 : 0)
  return `${d} ${MONTHS[m - 1] ?? ""} ${y}`
}

/** ZoneGames.tsx `hoursAndMinutes`. */
const hoursAndMinutes = ({ hours, minutes }: { readonly hours: number; readonly minutes: number }): string =>
  hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`

const NO_TIME = "--'--\"--"
const NO_VALUE = "--"

/** ZoneGames.tsx `launchLabel`: PLAY, CONTINUE, PLAY HERE, PLAY ON ZAO. */
const launchLabel = (launch: ZoneLaunchChoice): string => {
  const verb = launch.verb === "continue" ? "Continue" : "Play"
  if (launch.place === undefined) return verb
  return launch.place.here ? `${verb} here` : `${verb} on ${launch.place.label}`
}

/** Bytes as Zone's DOWNLOAD fact reads them (plan/bytes.ts would need its own import; one decimal is enough here). */
const bytesWord = (bytes: number): string =>
  bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB` : bytes >= 1e6 ? `${Math.round(bytes / 1e6)} MB` : `${Math.round(bytes / 1e3)} KB`

/**
 * A card's tags (ZoneCard.tsx `cardTags`, with use-zone-games.ts `placesOf`):
 * the system, each place (HERE for this device), CONTINUE, DOWNLOAD.
 * `korri` marks Korri's names, which keep their own case.
 */
function tagsOf(game: SurfaceGame): readonly { readonly label: string; readonly kind: string; readonly korri: boolean }[] {
  const locations = game.launchLocations ?? []
  const places = locations.length === 0 ? [{ label: "Here", here: true }]
    : locations.length === 1 ? [{ label: locations[0]!.label, here: false }]
    : locations.map((location, index) => ({ label: location.label, here: index === 0 }))
  return [
    ...(game.subtitle === undefined ? [] : [{ label: game.subtitle, kind: "system", korri: true }]),
    ...places.map(place => place.here
      ? { label: "Here", kind: "here", korri: false }
      : { label: place.label, kind: "remote", korri: true }),
    ...(game.resumable === true ? [{ label: "Continue", kind: "continue", korri: false }] : []),
    ...(game.download === undefined ? [] : [{
      label: game.download.bytes === undefined ? "Download" : `Download ${bytesWord(game.download.bytes)}`,
      kind: "download",
      korri: false,
    }]),
  ]
}

/** ZoneCoverWell.tsx `initialsOf`: a well with no art shows these, "The Legend of Zelda" reads LZ. */
const initialsOf = (title: string): string =>
  title.split(/\s+/)
    .filter(word => word.length > 0 && !/^(?:the|of|a|an|and)$/i.test(word))
    .map(word => word[0] ?? "")
    .join("")
    .slice(0, 2)
    .toUpperCase()

/** ZoneCoverWell.tsx `stripeSetOf`: which of the six stripe sets a well with no art wears. */
function stripeSetOf(title: string): number {
  let sum = 0
  for (const character of title) sum += character.codePointAt(0) ?? 0
  return sum % 6
}

/** ZoneBars.tsx `zoneBarsLit`: one bar per quarter, never none while connected. */
const barsLit = (strength: number | undefined, connected: boolean): number =>
  !connected ? 0 : strength === undefined ? 1 : Math.max(1, Math.min(4, Math.round(strength / 25)))

function signalOf(connection: SurfaceNetworkConnection | undefined) {
  if (connection === undefined) return { shown: false, name: "", korri: false, bars: 0, strength: "" }
  const strength = connection._tag === "Wifi" ? connection.strengthPercent : connection._tag === "Wired" ? 100 : undefined
  return {
    shown: true,
    name: connection._tag === "Wifi" ? connection.ssid : connection._tag === "Wired" ? "Wired" : "Offline",
    korri: connection._tag === "Wifi",
    bars: barsLit(strength, connection._tag !== "Offline"),
    strength: connection._tag === "Wifi" && connection.strengthPercent !== undefined ? `${connection.strengthPercent}%` : "",
  }
}

export function view(outer: State, model: Model) {
  const state = outer.zone
  const korri = legacy(model)
  const nav = state.nav
  const band = bandGames(nav, korri)
  const game = band[nav.card]
  const launches = game === undefined ? [] : launchChoices(game)
  const choice = launches.length > 1
  const actions = [...launches.map(launch => ({ label: launchLabel(launch) })), { label: "Opts." }]
  const focusedAction = nav.zone === "actions" ? Math.min(nav.action, actions.length - 1) : -1
  // One place: its button is the default and stays lit. A choice: the focused one.
  const lit = choice ? (focusedAction < launches.length ? focusedAction : -1) : 0
  const facts = game === undefined ? [] : [
    { label: "Last played", value: game.lastPlayedAt === undefined ? NO_VALUE : dayOf(game.lastPlayedAt), known: game.lastPlayedAt !== undefined },
    { label: "Plays", value: game.playCount === undefined ? NO_VALUE : String(game.playCount), known: game.playCount !== undefined },
    {
      label: "Time",
      value: game.totalPlaytimeSeconds === undefined ? NO_TIME : hoursAndMinutes(playtime(game.totalPlaytimeSeconds)),
      known: game.totalPlaytimeSeconds !== undefined,
    },
    ...(game.download === undefined ? [] : [{
      label: "Download",
      value: game.download.bytes === undefined ? "" : bytesWord(game.download.bytes),
      known: true,
    }]),
    ...(game.activeProfile === undefined ? [] : [{ label: "Profile", value: game.activeProfile, known: true }]),
  ]
  const signal = signalOf(model.network?.connection)
  return {
    page: zoneShown(state, korri),
    zone: nav.zone,
    tabs: ZONE_TABS.map(tab => ({ label: TAB_WORDS[tab], active: tab === nav.tab, focused: nav.zone === "tabs" && tab === nav.tabCursor })),
    findLabel: "Find",
    findShown: model.catalog._tag === "Ready",
    hints: { back: "Back", confirm: "Confirm" },
    cards: band.map(entry => ({
      title: entry.title,
      // A card with no cover leaves `art` out: its well stays empty.
      ...(entry.coverArtUrl === undefined ? {} : { art: { $image: entry.coverArtUrl } }),
      hasArt: entry.coverArtUrl !== undefined,
      initials: initialsOf(entry.title),
      stripes: stripeSetOf(entry.title),
      tags: tagsOf(entry),
    })),
    focusedCard: nav.card,
    firstIndex: model.catalog._tag === "Ready" ? model.catalog.firstIndex : 0,
    total: model.catalog._tag === "Ready" ? model.catalog.total : band.length,
    facts,
    actions: actions.map((entry, index) => ({ label: entry.label, lit: index === lit, focused: index === focusedAction })),
    timeLabel: "Time",
    clock: model.clockLabel ?? "",
    signal,
    build: model.buildLabel ?? "",
  }
}

const preferences = { [LOOK_KEY]: { mark: "device", initial: ZONE_DEFAULT_LOOK } } as const

// As the portal registers Zone (clients/portal/src/surface/surface-registry.tsx).
const displays = "main"

globalThis.surface = { displays, preferences, init, update, view }

declare global {
  // The one global the contract allows.
  var surface: { preferences: typeof preferences; init: typeof init; update: typeof update; view: typeof view }
}
