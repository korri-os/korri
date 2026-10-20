/**
 * Where the player is in Zone, as one value, and the one function that
 * changes it.
 *
 *     tab      which page: GAMES, OPTIONS or SAVE DATA
 *     zone     where the cursor is on the games page: the tabs, the FIND
 *              key, the cards or the action row under the focused card.
 *              On OPTIONS and SAVE DATA only `tabs` or `cards` (the page's
 *              own rows), so `menu` knows whether to move it
 *     card     the focused card's index in the band (the filtered catalog)
 *     action   the focused button's index in the action row
 *     layers   what is drawn over the page, bottom to top; empty is none
 *     find     the find draft (the band's filter) and the keyboard's cursor
 *     opts     the cursor in GAME OPTS. and the question it is asking
 *     idleCard which game the idle title card shows, in catalog order
 *
 * Layers come from two owners. Korri's (`onboarding`, `busy`, `problem`,
 * `overlay`) follow its model and are settled on every message, so they can
 * never be stale. Zone's own (`gameopts`, `keyboard`, `idle`) are pushed and
 * popped by messages. Back acts on the top layer only, so it can never
 * withdraw something the player cannot see.
 *
 * Korri's SurfaceModel is not stored here. Korri owns it and replaces it
 * whole; `update` receives it beside this value.
 */
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import type { ZoneMessage } from "./messages"
import {
  catalogGames,
  deleteBack,
  launchChoices,
  pressFindKey,
  visibleGames,
  ZONE_FIND_START,
  type ZoneFind,
} from "./games"
import { ask, stay, type ZoneStep } from "./requests"

export type ZoneTab = "games" | "options" | "savedata"
export type ZoneFocusZone = "tabs" | "find" | "cards" | "actions"
export type ZoneLayer = "gameopts" | "keyboard" | "busy" | "problem" | "overlay" | "onboarding" | "idle"

/** The tabs in strip order. */
export const ZONE_TABS: readonly ZoneTab[] = ["games", "options", "savedata"]

/** Milliseconds without input before the idle layer shows. */
export const ZONE_IDLE_AFTER_MS = 30_000
/** Milliseconds each game's title card holds while idle. */
export const ZONE_IDLE_CYCLE_MS = 4_000

/** GAME OPTS.: the focused row, and the destructive action awaiting NO / YES. */
export interface ZoneGameOpts {
  readonly row: number
  readonly asking: { readonly gameId: string; readonly actionId: string } | undefined
  /** NO is 0, YES is 1. */
  readonly answer: number
}

export const ZONE_OPTS_START: ZoneGameOpts = { row: 0, asking: undefined, answer: 0 }

export interface ZoneNavigation {
  readonly tab: ZoneTab
  /** The tab the cursor is on while `zone` is `tabs`. */
  readonly tabCursor: ZoneTab
  readonly zone: ZoneFocusZone
  readonly card: number
  readonly action: number
  readonly layers: readonly ZoneLayer[]
  /** Where OPTIONS lands when opened by `system`: its SYSTEM group. The
   * wire stage (state/surface.ts) puts the OPTIONS cursor there and clears
   * it with LandedOptions. */
  readonly optionsGroup: "system" | undefined
  readonly find: ZoneFind
  readonly opts: ZoneGameOpts
  readonly idleCard: number
}

export const ZONE_START: ZoneNavigation = {
  tab: "games",
  tabCursor: "games",
  zone: "cards",
  card: 0,
  action: 0,
  layers: [],
  optionsGroup: undefined,
  find: ZONE_FIND_START,
  opts: ZONE_OPTS_START,
  idleCard: 0,
}

export const topLayer = (nav: ZoneNavigation): ZoneLayer | "none" => nav.layers.at(-1) ?? "none"

/** The layers Zone pushes and pops itself. */
const OWN_LAYERS: readonly ZoneLayer[] = ["gameopts", "keyboard", "idle"]

/** The number of cards the catalog offers; zero while it is not Ready. */
export function cardCount(korri: SurfaceModel): number {
  return catalogGames(korri.catalog).length
}

/** The games in the band now: the catalog through the find draft. */
export const bandGames = (nav: ZoneNavigation, korri: SurfaceModel) => visibleGames(korri.catalog, nav.find.draft)

/** The game under the cursor, if the band has one. */
export const focusedGame = (nav: ZoneNavigation, korri: SurfaceModel) => bandGames(nav, korri)[nav.card]

/**
 * The navigation's messages: every fact a page or the host reports, and two
 * the wire stage reports for the pages that do not speak to navigation
 * themselves (OPTIONS, SAVE DATA).
 */
export type ZoneNavMessage =
  | ZoneMessage
  /** The cursor is on a row or card of OPTIONS or SAVE DATA, off the tabs. */
  | { readonly _tag: "FocusedPage" }
  /** OPTIONS has put its cursor on the SYSTEM group that `system` asked for. */
  | { readonly _tag: "LandedOptions" }

export function update(nav: ZoneNavigation, message: ZoneNavMessage, korri: SurfaceModel): ZoneStep<ZoneNavigation> {
  const result = step(nav, message, korri)
  return { model: settle(result.model, korri), requests: result.requests }
}

function step(nav: ZoneNavigation, message: ZoneNavMessage, korri: SurfaceModel): ZoneStep<ZoneNavigation> {
  // Idle is a screensaver: any press or focus move wakes it and is otherwise
  // ignored, so the cursor comes back exactly where it was. Korri publishing
  // a model (a clock tick) is not a press; the cycle's own tick moves the card.
  if (topLayer(nav) === "idle") {
    if (message._tag === "IdleTicked") return stay({ ...nav, idleCard: (nav.idleCard + 1) % Math.max(1, cardCount(korri)) })
    return message._tag === "IdleElapsed" || message._tag === "KorriPublished" ? stay(nav) : stay(pop(nav))
  }
  switch (message._tag) {
    case "PressedBack":
      return back(nav, korri)
    case "PressedOptions":
    case "ChoseGameOpts":
      // GAME OPTS. for the focused card, when there is one and nothing is over the page.
      // OPTS. in the action row opens it too, so d-pad and A alone reach it.
      return topLayer(nav) === "none" && nav.tab === "games" && bandGames(nav, korri).length > 0
        ? stay(push({ ...nav, opts: ZONE_OPTS_START }, "gameopts")) : stay(nav)
    case "PressedMenu":
      // Once to the tabs; again, on GAMES, to the FIND key at their right end.
      if (topLayer(nav) !== "none") return stay(nav)
      return nav.zone === "tabs" && nav.tab === "games"
        ? stay({ ...nav, zone: "find" }) : stay({ ...nav, zone: "tabs", tabCursor: nav.tab })
    case "PressedSystem":
      // The cursor goes into the page, never onto the tabs: SYSTEM is a row.
      return topLayer(nav) === "none"
        ? stay({ ...nav, tab: "options", tabCursor: "options", zone: "cards", optionsGroup: "system" }) : stay(nav)
    case "FocusedPage":
      // OPTIONS and SAVE DATA: the cursor left the tabs for the page's own rows.
      return nav.tab === "games" || nav.zone !== "tabs" ? stay(nav) : stay({ ...nav, zone: "cards" })
    case "LandedOptions":
      return nav.optionsGroup === undefined ? stay(nav) : stay({ ...nav, optionsGroup: undefined })
    case "KorriPublished":
      return stay(nav)
    case "IdleElapsed": {
      if (nav.layers.length > 0) return stay(nav)
      const shown = focusedGame(nav, korri)
      const start = shown === undefined ? 0 : Math.max(0, catalogGames(korri.catalog).indexOf(shown))
      return stay(push({ ...nav, idleCard: start }, "idle"))
    }
    case "IdleTicked":
    case "Woke":
      return stay(nav)
    case "FocusedTab":
      return stay({ ...nav, zone: "tabs", tabCursor: message.tab })
    case "ChoseTab":
      return stay({ ...nav, tab: message.tab, tabCursor: message.tab, zone: message.tab === "games" ? "cards" : "tabs" })
    case "FocusedCard":
      return stay({ ...nav, zone: "cards", card: clampCard(message.index, nav, korri), action: 0 })
    case "ChoseCard": {
      // One place: A on the card is A on its one lit button. A choice of
      // places: the cursor drops into the row and nothing launches.
      const card = clampCard(message.index, nav, korri)
      const game = bandGames(nav, korri)[card]
      const choices = game === undefined ? [] : launchChoices(game)
      if (game !== undefined && choices.length === 1) {
        return ask({ ...nav, zone: "cards", card }, launch(game.id, choices[0]!.locationId))
      }
      return stay({ ...nav, zone: "actions", card, action: 0 })
    }
    case "FocusedAction":
      return stay({ ...nav, zone: "actions", action: Math.max(0, message.index) })
    case "ChoseLaunch":
      return ask(nav, launch(message.gameId, message.locationId))
    case "FocusedFind":
      return stay({ ...nav, zone: "find" })
    case "ChoseFind":
      return topLayer(nav) === "none" && nav.tab === "games"
        ? stay(push({ ...nav, zone: "find", find: { ...nav.find, caret: nav.find.draft.length } }, "keyboard"))
        : stay(nav)
    case "FocusedKey":
      return topLayer(nav) === "keyboard" ? stay({ ...nav, find: { ...nav.find, key: message.id } }) : stay(nav)
    case "PressedKey": {
      if (topLayer(nav) !== "keyboard") return stay(nav)
      if (message.key.kind === "done") return stay({ ...pop(nav), zone: "cards" })
      return stay(refilter(nav, pressFindKey(nav.find, message.key), korri))
    }
    case "FocusedGameOpt":
      return topLayer(nav) === "gameopts" && nav.opts.asking === undefined
        ? stay({ ...nav, opts: { ...nav.opts, row: Math.max(0, message.index) } }) : stay(nav)
    case "ChoseGameAction": {
      if (topLayer(nav) !== "gameopts" || !message.enabled) return stay(nav)
      if (message.destructive) {
        return stay({ ...nav, opts: { ...nav.opts, asking: { gameId: message.gameId, actionId: message.actionId }, answer: 0 } })
      }
      return ask(closeOpts(nav), { _tag: "RunGameAction", gameId: message.gameId, actionId: message.actionId })
    }
    case "FocusedAnswer":
      return nav.opts.asking === undefined ? stay(nav) : stay({ ...nav, opts: { ...nav.opts, answer: message.index === 1 ? 1 : 0 } })
    case "AnsweredGameAction": {
      const asking = nav.opts.asking
      if (topLayer(nav) !== "gameopts" || asking === undefined) return stay(nav)
      return message.yes
        ? ask(closeOpts(nav), { _tag: "RunGameAction", gameId: asking.gameId, actionId: asking.actionId })
        : stay({ ...nav, opts: { ...nav.opts, asking: undefined, answer: 0 } })
    }
    case "ChoseReload":
      return korri.catalog._tag === "Error" ? ask(nav, { _tag: "Reload" }) : stay(nav)
  }
}

const launch = (gameId: string, locationId: string | undefined) => locationId === undefined
  ? { _tag: "LaunchGame", gameId } as const
  : { _tag: "LaunchGame", gameId, locationId } as const

const closeOpts = (nav: ZoneNavigation): ZoneNavigation =>
  ({ ...nav, layers: nav.layers.filter(layer => layer !== "gameopts"), opts: ZONE_OPTS_START })

/**
 * A new draft: the band is filtered again, and the cursor stays on the game
 * it was on when that game is still shown, or goes to the first match.
 */
function refilter(nav: ZoneNavigation, find: ZoneFind, korri: SurfaceModel): ZoneNavigation {
  if (find.draft === nav.find.draft) return { ...nav, find }
  const was = focusedGame(nav, korri)
  const now = visibleGames(korri.catalog, find.draft)
  const card = was === undefined ? 0 : Math.max(0, now.indexOf(was))
  return { ...nav, find, card, action: 0 }
}

/** Back acts on the top layer, then on the page. */
function back(nav: ZoneNavigation, korri: SurfaceModel): ZoneStep<ZoneNavigation> {
  switch (topLayer(nav)) {
    case "overlay":
      return ask(nav, { _tag: "DismissGameplayOverlay" })
    case "problem":
      return ask(nav, { _tag: "DismissProblem" })
    case "busy":
      // Korri's work is cancelled by the action it publishes, never by Back.
      return stay(nav)
    case "onboarding":
      return stay(nav)
    case "gameopts":
      // B on the question is NO; otherwise it closes GAME OPTS.
      return nav.opts.asking === undefined
        ? stay(closeOpts(nav)) : stay({ ...nav, opts: { ...nav.opts, asking: undefined, answer: 0 } })
    case "keyboard":
      // B deletes; on an empty draft it drops the keyboard and the filter with it.
      return nav.find.draft === ""
        ? stay({ ...pop(nav), zone: "cards", find: ZONE_FIND_START })
        : stay(refilter(nav, deleteBack(nav.find.caret === 0 ? { ...nav.find, caret: nav.find.draft.length } : nav.find), korri))
    case "idle":
      return stay(pop(nav))
    case "none":
      if (nav.zone !== "cards" && nav.tab === "games") return stay({ ...nav, zone: "cards" })
      if (nav.tab !== "games") {
        return stay({ ...nav, tab: "games", tabCursor: "games", zone: "cards", optionsGroup: undefined })
      }
      // On the band with a filter kept: B lets go of it, the cursor on the same game.
      if (nav.find.draft !== "") return stay({ ...refilter(nav, { ...nav.find, draft: "", caret: 0 }, korri), find: ZONE_FIND_START })
      return stay(nav)
  }
}

const push = (nav: ZoneNavigation, layer: ZoneLayer): ZoneNavigation =>
  ({ ...nav, layers: [...nav.layers, layer] })

const pop = (nav: ZoneNavigation): ZoneNavigation =>
  ({ ...nav, layers: nav.layers.slice(0, -1) })

function clampCard(index: number, nav: ZoneNavigation, korri: SurfaceModel): number {
  return Math.min(Math.max(0, index), Math.max(0, bandGames(nav, korri).length - 1))
}

/**
 * Korri's layers follow its model: `onboarding` under everything while first
 * boot is unfinished, `busy` or `problem` over the page while its status
 * says so, `overlay` over everything while it presents gameplay. Zone's own
 * layers keep their order between them. A card index past the catalog is
 * brought back in.
 */
function settle(nav: ZoneNavigation, korri: SurfaceModel): ZoneNavigation {
  const own = nav.layers.filter(layer => OWN_LAYERS.includes(layer))
  const status = korri.status._tag
  const layers: ZoneLayer[] = [
    ...(korri.onboarding === undefined ? [] : ["onboarding" as const]),
    ...own.filter(layer => layer !== "idle"),
    ...(status === "Busy" ? ["busy" as const] : []),
    ...(status === "Problem" ? ["problem" as const] : []),
    ...(korri.presentation.kind === "gameplay-overlay" ? ["overlay" as const] : []),
    ...(own.includes("idle") && own.length === 1 && status === "Browsing"
      && korri.presentation.kind === "catalog" && korri.onboarding === undefined ? ["idle" as const] : []),
  ]
  const card = clampCard(nav.card, nav, korri)
  const same = card === nav.card && layers.length === nav.layers.length
    && layers.every((layer, index) => layer === nav.layers[index])
  return same ? nav : { ...nav, card, layers }
}
