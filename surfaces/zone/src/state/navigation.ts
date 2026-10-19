/**
 * Where the player is in Zone, as one value, and the one function that
 * changes it.
 *
 *     tab      which page: GAMES, OPTIONS or SAVE DATA
 *     zone     where the cursor is on the games page: the tabs, the cards
 *              or the action row under the focused card
 *     card     the focused card's index in the catalog
 *     action   the focused button's index in the action row
 *     layers   what is drawn over the page, bottom to top; empty is none
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
import { ask, stay, type ZoneStep } from "./requests"

export type ZoneTab = "games" | "options" | "savedata"
export type ZoneFocusZone = "tabs" | "cards" | "actions"
export type ZoneLayer = "gameopts" | "keyboard" | "busy" | "problem" | "overlay" | "onboarding" | "idle"

/** The tabs in strip order. */
export const ZONE_TABS: readonly ZoneTab[] = ["games", "options", "savedata"]

/** Milliseconds without input before the idle layer shows. */
export const ZONE_IDLE_AFTER_MS = 30_000

export interface ZoneNavigation {
  readonly tab: ZoneTab
  /** The tab the cursor is on while `zone` is `tabs`. */
  readonly tabCursor: ZoneTab
  readonly zone: ZoneFocusZone
  readonly card: number
  readonly action: number
  readonly layers: readonly ZoneLayer[]
  /** Where OPTIONS lands when opened by `system`: its SYSTEM group. The
   * OPTIONS page clears it once it has put the cursor there. */
  readonly optionsGroup: "system" | undefined
}

export const ZONE_START: ZoneNavigation = {
  tab: "games",
  tabCursor: "games",
  zone: "cards",
  card: 0,
  action: 0,
  layers: [],
  optionsGroup: undefined,
}

export const topLayer = (nav: ZoneNavigation): ZoneLayer | "none" => nav.layers.at(-1) ?? "none"

/** The layers Zone pushes and pops itself. */
const OWN_LAYERS: readonly ZoneLayer[] = ["gameopts", "keyboard", "idle"]

/** The number of cards the catalog offers; zero while it is not Ready. */
export function cardCount(korri: SurfaceModel): number {
  return korri.catalog._tag === "Ready" ? korri.catalog.games.length : 0
}

export function update(nav: ZoneNavigation, message: ZoneMessage, korri: SurfaceModel): ZoneStep<ZoneNavigation> {
  const result = step(nav, message, korri)
  return { model: settle(result.model, korri), requests: result.requests }
}

function step(nav: ZoneNavigation, message: ZoneMessage, korri: SurfaceModel): ZoneStep<ZoneNavigation> {
  // Idle is a screensaver: any press or focus move wakes it and is otherwise
  // ignored. Korri publishing a model (a clock tick) is not a press.
  if (topLayer(nav) === "idle") {
    return message._tag === "IdleElapsed" || message._tag === "KorriPublished" ? stay(nav) : stay(pop(nav))
  }
  switch (message._tag) {
    case "PressedBack":
      return back(nav)
    case "PressedOptions":
      // GAME OPTS. for the focused card, when there is one and nothing is over the page.
      return topLayer(nav) === "none" && nav.tab === "games" && cardCount(korri) > 0
        ? stay(push(nav, "gameopts")) : stay(nav)
    case "PressedMenu":
      return topLayer(nav) === "none"
        ? stay({ ...nav, zone: "tabs", tabCursor: nav.tab }) : stay(nav)
    case "PressedSystem":
      return topLayer(nav) === "none"
        ? stay({ ...nav, tab: "options", tabCursor: "options", optionsGroup: "system" }) : stay(nav)
    case "KorriPublished":
      return stay(nav)
    case "IdleElapsed":
      return nav.layers.length === 0 ? stay(push(nav, "idle")) : stay(nav)
    case "Woke":
      return stay(nav)
    case "FocusedTab":
      return stay({ ...nav, zone: "tabs", tabCursor: message.tab })
    case "ChoseTab":
      return stay({ ...nav, tab: message.tab, tabCursor: message.tab, zone: message.tab === "games" ? "cards" : "tabs" })
    case "FocusedCard":
      return stay({ ...nav, zone: "cards", card: clampCard(message.index, korri), action: 0 })
    case "ChoseCard":
      return stay({ ...nav, zone: "actions", card: clampCard(message.index, korri), action: 0 })
    case "FocusedAction":
      return stay({ ...nav, zone: "actions", action: Math.max(0, message.index) })
    case "ChoseLaunch":
      return ask(nav, message.locationId === undefined
        ? { _tag: "LaunchGame", gameId: message.gameId }
        : { _tag: "LaunchGame", gameId: message.gameId, locationId: message.locationId })
  }
}

/** Back acts on the top layer, then on the page. */
function back(nav: ZoneNavigation): ZoneStep<ZoneNavigation> {
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
    case "keyboard":
    case "idle":
      return stay(pop(nav))
    case "none":
      if (nav.zone !== "cards" && nav.tab === "games") return stay({ ...nav, zone: "cards" })
      if (nav.tab !== "games") return stay({ ...nav, tab: "games", tabCursor: "games", zone: "cards", optionsGroup: undefined })
      return stay(nav)
  }
}

const push = (nav: ZoneNavigation, layer: ZoneLayer): ZoneNavigation =>
  ({ ...nav, layers: [...nav.layers, layer] })

const pop = (nav: ZoneNavigation): ZoneNavigation =>
  ({ ...nav, layers: nav.layers.slice(0, -1) })

function clampCard(index: number, korri: SurfaceModel): number {
  return Math.min(Math.max(0, index), Math.max(0, cardCount(korri) - 1))
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
  const card = clampCard(nav.card, korri)
  const same = card === nav.card && layers.length === nav.layers.length
    && layers.every((layer, index) => layer === nav.layers[index])
  return same ? nav : { ...nav, card, layers }
}
