/**
 * THROWAWAY SPIKE: Zone's logic for the surface shell (../../CONTRACT.md).
 *
 * Zone's real program (surfaces/zone/src/state/surface.ts) runs unchanged.
 * In the browser the host moves DOM focus and the page reports `Focused…`
 * facts; here no DOM exists, so a thin adapter turns the shell's
 * `{ _tag: "Pressed", action }` into the same facts, by where the cursor is:
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
 *     back, menu              Zone's own PressedBack and PressedMenu
 *
 * Confirm on a card or a button, and options (GAME OPTS.), do nothing: the
 * launch title card, GAME OPTS. and the other pages are not drawn by this
 * package, and the shell performs no requests.
 *
 * `view` is plain JSON for main.slint: every word, game, fact and readout it
 * shows, in Zone's own case (main.slint sets Zone's words in capitals, as
 * Zone's caps do). Sizes, colours and motion stay in main.slint.
 *
 * Pure: no timers, no Date, no randomness. The facts line's day is computed
 * from Korri's epoch milliseconds in UTC (Zone uses the person's time zone;
 * the fixture's instants are the same day in every zone from -11 to +11).
 */
import type { SurfaceGame, SurfaceModel, SurfaceNetworkConnection } from "../../../../contracts/surface/korri-surface"
import { launchChoices, playtime, type ZoneLaunchChoice } from "../../../../surfaces/zone/src/state/games"
import { bandGames, ZONE_TABS, type ZoneNavMessage, type ZoneTab } from "../../../../surfaces/zone/src/state/navigation"
import { update as zoneUpdate, zoneShown, zoneSurfaceStart, type ZoneSurfaceMessage, type ZoneSurfaceState } from "../../../../surfaces/zone/src/state/surface"

export type Action = "up" | "down" | "left" | "right" | "confirm" | "back" | "menu" | "options"
export type Message = { readonly _tag: "Pressed"; readonly action: Action }
export type State = ZoneSurfaceState

// ---- init and update ----

/**
 * Zone's start in the Modern look, then the Caliper part's opening
 * (ZoneGames.page.part.tsx, `Ready`: FocusedCard 1), so the first frame is
 * the reference state: Hollow Knight focused.
 */
export function init(model: SurfaceModel): State {
  const start = zoneSurfaceStart(model, "modern")
  return games(start, { _tag: "FocusedCard", index: 1 }, model)
}

const games = (state: State, message: ZoneNavMessage, model: SurfaceModel): State =>
  run(state, { _tag: "Games", message }, model)

const run = (state: State, message: ZoneSurfaceMessage, model: SurfaceModel): State =>
  zoneUpdate(state, message, model).model

export function update(state: State, message: Message, model: SurfaceModel): State {
  if (message._tag !== "Pressed") return state
  const nav = state.nav
  switch (message.action) {
    case "back":
      return run(state, { _tag: "PressedBack" }, model)
    case "menu":
      return run(state, { _tag: "PressedMenu" }, model)
    case "options":
    case "up":
    case "down":
    case "left":
    case "right":
    case "confirm":
      break
  }
  // Directions and confirm move Zone's cursor only on GAMES with nothing over it.
  if (zoneShown(state, model) !== "games" || nav.layers.length > 0) return state
  const action = message.action
  switch (nav.zone) {
    case "tabs": {
      const at = ZONE_TABS.indexOf(nav.tabCursor)
      if (action === "left" || action === "right") {
        const next = ZONE_TABS[clamp(at + (action === "left" ? -1 : 1), 0, ZONE_TABS.length - 1)]!
        return next === nav.tabCursor ? state : games(state, { _tag: "FocusedTab", tab: next }, model)
      }
      if (action === "down" && nav.tab === "games") return games(state, { _tag: "FocusedCard", index: nav.card }, model)
      if (action === "confirm" && nav.tabCursor === "games") return games(state, { _tag: "ChoseTab", tab: "games" }, model)
      return state
    }
    case "find":
      if (action === "left" || action === "up") return games(state, { _tag: "FocusedTab", tab: "savedata" }, model)
      if (action === "down") return games(state, { _tag: "FocusedCard", index: nav.card }, model)
      return state
    case "get":
      if (action === "down") return games(state, { _tag: "FocusedCard", index: nav.card }, model)
      return state
    case "cards": {
      if (action === "left" || action === "right") {
        const count = bandGames(nav, model).length
        const next = clamp(nav.card + (action === "left" ? -1 : 1), 0, Math.max(0, count - 1))
        return next === nav.card ? state : games(state, { _tag: "FocusedCard", index: next }, model)
      }
      if (action === "up") return games(state, { _tag: "FocusedTab", tab: nav.tab }, model)
      if (action === "down") {
        const game = bandGames(nav, model)[nav.card]
        if (game === undefined) return state
        return games(state, { _tag: "FocusedAction", index: 0 }, model)
      }
      return state
    }
    case "actions": {
      if (action === "left" || action === "right") {
        const game = bandGames(nav, model)[nav.card]
        if (game === undefined) return state
        // The launch buttons, then OPTS.
        const last = launchChoices(game).length
        const next = clamp(nav.action + (action === "left" ? -1 : 1), 0, last)
        return next === nav.action ? state : games(state, { _tag: "FocusedAction", index: next }, model)
      }
      if (action === "up") return games(state, { _tag: "FocusedCard", index: nav.card }, model)
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

export function view(state: State, model: SurfaceModel) {
  const nav = state.nav
  const band = bandGames(nav, model)
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
    page: zoneShown(state, model),
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
    facts,
    actions: actions.map((entry, index) => ({ label: entry.label, lit: index === lit, focused: index === focusedAction })),
    timeLabel: "Time",
    clock: model.clockLabel ?? "",
    signal,
    build: model.buildLabel ?? "",
  }
}

globalThis.surface = { init, update, view }

declare global {
  // The one global the contract allows.
  var surface: { init: typeof init; update: typeof update; view: typeof view }
}
