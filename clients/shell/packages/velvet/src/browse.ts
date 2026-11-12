/**
 * The library: the home (the pick, the row under it), the lobby of shelves
 * one level down, a set one level deeper, and the detail.
 *
 * The home and the lobby are one tall page the view moves over. Its focus
 * regions are all in every view, so a press goes where the plan draws it
 * (contract 8.4):
 *
 *     menu            the pill at the top; down returns to the pick
 *     pick            Play; up to the menu, down to the row
 *     row             the pick again, the row's games, "All games"; up to
 *                     the pick, down to the first shelf
 *     shelf:<key>     a shelf's games and its way further in; up and down to
 *                     the shelves beside it, the first one up to the row
 *
 * A set is a grid over one catalog query. Its focus targets are the places
 * of the window Korri gave (`marks`); the tiles it draws are the rows near
 * the window's view (`tiles`), so 10,000 games cost what a screenful does.
 * Up from the first row reaches the Sort and filter pill.
 *
 * Confirm on a game opens the detail with Play focused; Confirm again
 * launches. Back closes each level.
 */
import type { CatalogQuery, SurfaceCatalog, SurfaceModel as Model } from "../../../../../contracts/generated/surface"
import { factLines, factsOf, gameCard, gameTile, hasHero, marqueeOf, mediaOf, navTile, NO_IMAGE, NO_MARQUEE, NO_TILE, systemTile, titleLines, waitingTile, type FactView, type MarqueeView, type TileView } from "./art"
import { fitCaps, textWidth } from "./fit"
import { gridMove, lineMove } from "./focus"
import { homeOf, sameQuery, shelvesOf, type ShelfItem, type ShelfSpec } from "./lists"
import { sortWords } from "./pages"
import { TILE_RATIO, type Plan } from "./plan"
import { NONE, send, type Act, type Direction, type Game, type Request, type SetState, type State } from "./types"
import { ago, bytes, caps, games as gamesWords } from "./words"

// ---- The home's row ----

export type RowItem = { readonly kind: "game"; readonly game: Game; readonly pick: boolean } | { readonly kind: "all" }

/** The row's places: the pick (shown once the row is focused), its games, then "All games". */
export function rowItems(state: State, model: Model): readonly RowItem[] {
  const home = homeOf(state.lists, model.publishedAt)
  if (home.pick === undefined) return []
  return [{ kind: "game", game: home.pick, pick: true }, ...home.row.map((game): RowItem => ({ kind: "game", game, pick: false })), { kind: "all" }]
}

export const shelfRegion = (key: string): string => `shelf:${key}`

/** The shelf focus is on, and its place among the shelves. */
export function shelfAt(state: State, shelves: readonly ShelfSpec[]): number {
  const at = shelves.findIndex(shelf => shelf.key === state.shelf)
  return at < 0 ? 0 : at
}

const columnOf = (state: State, shelf: ShelfSpec): number => Math.max(0, Math.min(state.columns[shelf.key] ?? 0, shelf.items.length - 1))

/** The game focus is on in the library, if it is on a game. */
export function focusedGame(state: State, model: Model): Game | undefined {
  if (state.detail !== undefined) return state.detail.game
  if (state.set !== undefined) return state.set.pill ? undefined : setGame(state.set, model.catalog, state.set.at)
  const home = homeOf(state.lists, model.publishedAt)
  if (state.onMenu) return home.pick
  if (state.depth === 0) return home.pick
  if (state.depth === 1) {
    const item = rowItems(state, model)[state.rowAt]
    return item?.kind === "game" ? item.game : home.pick
  }
  const shelves = shelvesOf(state.lists, model)
  const shelf = shelves[shelfAt(state, shelves)]
  const item = shelf?.items[columnOf(state, shelf)]
  return item?.kind === "game" ? item.game : undefined
}

// ---- The home: presses ----

export interface Step {
  readonly state: State
  readonly act?: Act
}

const stay = (state: State): Step => ({ state })

/** A press on the home or the lobby. */
export function pressHome(state: State, button: Direction | "confirm" | "back", model: Model): Step {
  const home = homeOf(state.lists, model.publishedAt)
  const row = rowItems(state, model)
  const shelves = shelvesOf(state.lists, model)
  if (state.onMenu) {
    if (button === "down" || button === "back") return stay({ ...state, onMenu: false })
    if (button === "confirm") return { state, act: { t: "sheet", sheet: "menu" } }
    return stay(state)
  }
  if (home.pick === undefined) return stay(state)
  if (state.depth === 0) {
    if (button === "up") return stay({ ...state, onMenu: true })
    if (button === "down" && row.length > 1) return stay({ ...state, depth: 1, rowAt: Math.max(1, Math.min(state.rowAt, row.length - 1)) })
    if (button === "confirm") return { state, act: { t: "detail", game: home.pick } }
    return stay(state)
  }
  if (state.depth === 1) {
    const at = Math.max(0, Math.min(state.rowAt, row.length - 1))
    if (button === "left" || button === "right") {
      const next = lineMove(at, row.length, button === "left" ? -1 : 1)
      return stay(next === undefined ? state : { ...state, rowAt: next })
    }
    if (button === "up" || button === "back") return stay({ ...state, depth: 0, rowAt: at })
    if (button === "down") return stay(shelves.length === 0 ? state : { ...state, depth: 2, shelf: shelves[shelfAt(state, shelves)]!.key })
    const item = row[at]
    if (item === undefined) return stay(state)
    return item.kind === "all" ? { state, act: { t: "lobby" } } : { state, act: { t: "detail", game: item.game } }
  }
  const at = shelfAt(state, shelves)
  const shelf = shelves[at]
  if (shelf === undefined) return stay({ ...state, depth: 0 })
  const column = columnOf(state, shelf)
  switch (button) {
    case "left":
    case "right": {
      const next = lineMove(column, shelf.items.length, button === "left" ? -1 : 1)
      return stay(next === undefined ? state : { ...state, columns: { ...state.columns, [shelf.key]: next } })
    }
    case "up":
      return stay(at === 0 ? { ...state, depth: 1 } : { ...state, shelf: shelves[at - 1]!.key })
    case "down":
      return stay(at >= shelves.length - 1 ? state : { ...state, shelf: shelves[at + 1]!.key })
    case "back":
      return stay({ ...state, depth: 0 })
    case "confirm": {
      const item = shelf.items[column]
      if (item === undefined) return stay(state)
      return item.kind === "game" ? { state, act: { t: "detail", game: item.game } } : { state, act: { t: "set", set: item.set } }
    }
  }
}

/** A finger on a tile of the home or the lobby: it takes focus; on a game that had it already, it opens. */
export function tapHome(state: State, target: string, model: Model): Step | undefined {
  if (target === "menu") return { state: { ...state, onMenu: true }, act: { t: "sheet", sheet: "menu" } }
  if (target === "play") return pressHome({ ...state, onMenu: false, depth: 0 }, "confirm", model)
  if (target.startsWith("row:")) {
    const at = Number(target.slice(4))
    const item = rowItems(state, model)[at]
    if (item === undefined) return undefined
    const had = !state.onMenu && state.depth === 1 && state.rowAt === at
    const next: State = { ...state, onMenu: false, depth: 1, rowAt: at }
    return item.kind === "game" && !had ? stay(next) : pressHome(next, "confirm", model)
  }
  if (target.startsWith("shelf:")) {
    const cut = target.lastIndexOf(":")
    const key = target.slice(6, cut)
    const at = Number(target.slice(cut + 1))
    const shelf = shelvesOf(state.lists, model).find(candidate => candidate.key === key)
    const item = shelf?.items[at]
    if (shelf === undefined || item === undefined) return undefined
    const had = !state.onMenu && state.depth === 2 && state.shelf === key && columnOf(state, shelf) === at
    const next: State = { ...state, onMenu: false, depth: 2, shelf: key, columns: { ...state.columns, [key]: at } }
    return item.kind === "game" && !had ? stay(next) : pressHome(next, "confirm", model)
  }
  return undefined
}

// ---- The home: view ----

export interface PillView {
  readonly shown: boolean
  readonly label: string
  readonly icon: string
  readonly note: string
  readonly quiet: boolean
  readonly dim: boolean
  readonly danger: boolean
  readonly focusId?: string
  readonly focused?: boolean
  readonly focusRegion?: string
}

export const NO_PILL: PillView = { shown: false, label: "", icon: "", note: "", quiet: false, dim: false, danger: false }

export const pill = (label: string, region: string, focusId: string, focused: boolean, more: Partial<PillView> = {}): PillView =>
  ({ ...NO_PILL, shown: true, label, focusId, focused, focusRegion: region, ...more })

/** Whether a card already says the game's name (so the marquee beside it does not repeat it). */
const cardNames = (game: Game): boolean => mediaOf(game, "poster") === undefined

function shelfTile(item: ShelfItem, region: string, id: string, focused: boolean, at: number): TileView {
  switch (item.kind) {
    case "game":
      return gameTile(item.game, region, id, focused, at)
    case "more":
      return navTile(item.label, item.count === undefined ? "" : String(item.count), region, id, focused, at)
    case "system":
      return systemTile(item.systemId, item.system, gamesWords(item.count), region, id, focused, at)
  }
}

/** The line at the library's head: the focused thing's name and one fact. */
function nowOf(state: State, model: Model, shelves: readonly ShelfSpec[]): { title: string; fact: string } {
  if (state.depth < 2) return { title: "", fact: "" }
  const shelf = shelves[shelfAt(state, shelves)]
  const item = shelf?.items[columnOf(state, shelf)]
  if (item === undefined) return { title: "", fact: "" }
  if (item.kind === "game") return { title: item.game.title, fact: ago(item.game.lastPlayedAt, model.publishedAt) }
  if (item.kind === "system") return { title: item.system, fact: gamesWords(item.count) }
  return { title: item.set.title, fact: item.count === undefined ? "" : gamesWords(item.count) }
}

export function homeView(state: State, model: Model, plan: Plan, live: boolean) {
  const home = homeOf(state.lists, model.publishedAt)
  const pick = home.pick
  const shelves = shelvesOf(state.lists, model)
  const row = rowItems(state, model)
  const depth = pick === undefined ? 0 : state.depth
  const hero = hasHero(pick)
  const onPick = live && !state.onMenu && depth === 0
  // The mark's logo first, then the row's tiles from the focused one outwards (the pixel budget, src/art.ts).
  const mark = pick === undefined ? NO_MARQUEE : marqueeOf(pick, {
    width: hero ? plan.heroW : plan.textW, logoH: hero ? Math.round(plan.markH * 1.25) : plan.markH, title: hero ? plan.title : plan.pickTitle, fact: plan.fact,
    mark: hero || !cardNames(pick), badge: true,
  }, model.publishedAt)
  const rowGame = depth === 1 ? row[state.rowAt] : undefined
  const rowMark = rowGame === undefined ? NO_MARQUEE : rowGame.kind === "game"
    ? marqueeOf(rowGame.game, { width: plan.rowMarkW, logoH: Math.max(24, plan.rowMarkH - plan.fact * 3.2), title: Math.min(plan.title, Math.max(24, (plan.rowMarkH - plan.fact * 3.2) / 2)), fact: plan.fact, mark: true, badge: false, lines: 2 }, model.publishedAt)
    : { ...NO_MARQUEE, key: "all", ...fitMark("Library", plan.rowMarkW, plan), facts: factLines([{ icon: "grid", text: gamesWords(state.lists.total), strong: false }], plan.fact, plan.rowMarkW) }
  const cards = pick === undefined || hero ? [] : [gameCard(pick)]
  const rowTiles: TileView[] = row.map(() => NO_TILE)
  for (const at of nearest(row.length, depth === 1 ? state.rowAt : 0)) {
    const item = row[at]!
    const focused = live && !state.onMenu && depth === 1 && state.rowAt === at
    if (item.kind === "all") rowTiles[at] = { ...navTile("All games", String(state.lists.total), "row", `row:${at}`, focused, at), rests: depth === 0 }
    // The pick takes its place in the row once the row is the shelf.
    else if (item.pick && depth === 0) rowTiles[at] = { ...waitingTile("row", `row:${at}`, focused, at), shown: false }
    else rowTiles[at] = { ...gameTile(item.game, "row", `row:${at}`, focused, at), rests: depth === 0 }
  }
  const shelfAtNow = shelfAt(state, shelves)
  const now = nowOf(state, model, shelves)
  // The shelves' tiles, the focused shelf first.
  const tiles: TileView[] = []
  // Only what is on the stage is sent: the shelves in sight, and on each the tiles in sight.
  // Every tile keeps its place in the list all the same, so a view row is always the same
  // tile and nothing slides from one tile's place to another's; a tile out of sight is its
  // place and no more (`shown: false`, no focus target).
  const across = Math.ceil((plan.width - plan.padX) / (plan.shelfH * TILE_RATIO + plan.shelfGap)) + 1
  const starts: number[] = []
  shelves.forEach((shelf, index) => {
    starts.push(tiles.length)
    for (let at = 0; at < shelf.items.length; at++) tiles.push({ shown: false, focusId: "", at, shelf: index } as unknown as TileView)
  })
  for (const index of nearest(shelves.length, shelfAtNow)) {
    if (!shelfShown(depth, index, shelfAtNow)) continue
    const shelf = shelves[index]!
    const region = shelfRegion(shelf.key)
    const column = columnOf(state, shelf)
    const first = state.scrolled?.[region] ?? column
    for (const at of nearest(shelf.items.length, column)) {
      if (at !== column && Math.abs(at - first) > across) continue
      const id = `${region}:${at}`
      const focused = live && !state.onMenu && depth === 2 && index === shelfAtNow && column === at
      tiles[starts[index]! + at] = { ...shelfTile(shelf.items[at]!, region, id, focused, at), shelf: index, rests: depth !== 2 || index !== shelfAtNow }
    }
  }
  const heads = shelves.map(shelf => ({ key: shelf.key, label: shelf.label, at: state.scrolled?.[shelfRegion(shelf.key)] ?? columnOf(state, shelf), count: shelf.items.length }))
  const rowAt = Math.max(0, Math.min(state.rowAt, Math.max(0, row.length - 1)))
  return {
    home: {
      shown: true,
      depth,
      hero,
      mark,
      play: pick === undefined ? NO_PILL : pill(pick.resumable === true ? "Resume" : "Play", "pick", "play", onPick, { icon: "play" }),
      menu: pill("Menu", "menu", "menu", live && state.onMenu, { icon: "menu", quiet: true }),
      clock: model.clockLabel ?? "",
      rowAt: state.scrolled?.row ?? rowAt,
      rowMark,
      shelfAt: shelfAtNow,
      libTitle: "LIBRARY",
      libCount: gamesWords(state.lists.total),
      nowTitle: now.title,
      nowFact: now.fact,
    },
    cards,
    row: rowTiles,
    shelves: heads,
    shelfTiles: tiles,
  }
}

/** Places 0 to count - 1, the one nearest `from` first. */
export function nearest(count: number, from: number): number[] {
  // At the same distance, the one after: it is the one in sight (a focused shelf stands at the top).
  return Array.from({ length: count }, (_, index) => index).sort((a, b) => Math.abs(a - from) - Math.abs(b - from) || b - a)
}

function fitMark(text: string, width: number, plan: Plan) {
  const fit = fitCaps(text, width, plan.title * 2, plan.title, 18)
  return { ...titleLines(fit.lines), size: fit.size }
}

/** The exits of the home's regions (contract 8.4). */
/**
 * Whether a shelf's tiles are in the view. Home shows none of the library;
 * the row shows the shelf under it; the lobby shows the focused shelf and the
 * one each side of it. A view holds what is on the stage and no more: every
 * byte of it is read on every call (the call budget).
 */
export function shelfShown(depth: number, index: number, at: number): boolean {
  return depth === 2 ? Math.abs(index - at) <= 1 : depth === 1 && index === at
}

export function homeExits(state: State, model: Model): Record<string, Record<string, { to: string; entry: string }>> {
  const shelves = shelvesOf(state.lists, model)
  const hasPick = homeOf(state.lists, model.publishedAt).pick !== undefined
  const exits: Record<string, Record<string, { to: string; entry: string }>> = {}
  if (!hasPick) return exits
  exits.menu = { down: { to: "pick", entry: "first" } }
  exits.pick = { up: { to: "menu", entry: "first" }, down: { to: "row", entry: "remembered" } }
  const at = shelfAt(state, shelves)
  const shown = (index: number) => shelfShown(state.depth, index, at)
  exits.row = { up: { to: "pick", entry: "first" }, ...(shelves.length > 0 && shown(at) ? { down: { to: shelfRegion(shelves[at]!.key), entry: "remembered" } } : {}) }
  // An exit leads only to a region with targets in this view; the next view names the next one.
  shelves.forEach((shelf, index) => {
    if (!shown(index)) return
    exits[shelfRegion(shelf.key)] = {
      ...(index === 0 ? { up: { to: "row", entry: "remembered" } } : shown(index - 1) ? { up: { to: shelfRegion(shelves[index - 1]!.key), entry: "remembered" } } : {}),
      ...(index < shelves.length - 1 && shown(index + 1) ? { down: { to: shelfRegion(shelves[index + 1]!.key), entry: "remembered" } } : {}),
    }
  })
  return exits
}

// ---- A set ----

/** Games a set's window asks Korri for, and how near its edge focus may come before the window moves. */
export const SET_WINDOW = 60
const SET_EDGE_ROWS = 3

export function setQuery(set: SetState): CatalogQuery {
  return { text: "", order: set.order, ...(set.section === undefined ? {} : { section: set.section }), ...(set.systemId === undefined ? {} : { systemId: set.systemId }) }
}

type Ready = Extract<SurfaceCatalog, { _tag: "Ready" }>

/** The window, when it answers this set's query. */
export function setWindow(set: SetState, catalog: SurfaceCatalog): Ready | undefined {
  return catalog._tag === "Ready" && sameQuery(catalog.query, setQuery(set)) ? catalog : undefined
}

export function setGame(set: SetState, catalog: SurfaceCatalog, at: number): Game | undefined {
  const window = setWindow(set, catalog)
  return window?.window[at - window.firstIndex]
}

/** The first question for a set: its start, or the end of the recent order for Not started. */
export function openSet(set: SetState): Request {
  return { _tag: "MoveCatalog", query: setQuery(set), from: set.fromEnd ? { _tag: "End" } : { _tag: "Start" }, count: SET_WINDOW }
}

/**
 * The set against the window Korri gave: its lowest place (Not started
 * begins after the last played game the window shows), focus held inside it
 * and on the game it was on, the rows shown, and a new window when focus
 * comes near the edge of this one.
 */
export function settleSet(set: SetState, catalog: SurfaceCatalog, plan: Plan): { readonly set: SetState; readonly request?: Request } {
  const window = setWindow(set, catalog)
  if (window === undefined) return { set }
  let next: SetState = set.moving ? { ...set, moving: false } : set
  let low = 0
  if (set.fromEnd) {
    let played = -1
    for (let i = window.window.length - 1; i >= 0; i--) {
      if (window.window[i]!.lastPlayedAt !== undefined) {
        played = i
        break
      }
    }
    low = played >= 0 ? window.firstIndex + played + 1 : window.before === undefined ? window.firstIndex : 0
  }
  const last = Math.max(low, window.total - 1)
  let at = next.at
  // The window moved under the cursor: stay on the game.
  if (next.gameId !== undefined) {
    const found = window.window.findIndex(game => game.id === next.gameId)
    if (found >= 0) at = window.firstIndex + found
  }
  if (set.fromEnd && set.moving && set.gameId === undefined) at = low
  at = Math.max(low, Math.min(at, last))
  const gameId = window.window[at - window.firstIndex]?.id
  const row = Math.floor((at - low) / plan.columns)
  let first = next.first
  if (row < first) first = row
  if (row >= first + plan.gridRows) first = row - plan.gridRows + 1
  first = Math.max(0, first)
  if (at !== next.at || low !== next.low || first !== next.first || gameId !== next.gameId) next = { ...next, at, low, first, ...(gameId === undefined ? {} : { gameId }) }
  // Near an edge that has more beyond it: a window around the focused game.
  const edge = SET_EDGE_ROWS * plan.columns
  const start = window.firstIndex
  const end = window.firstIndex + window.window.length
  const nearStart = window.before !== undefined && at - start < edge && start > low
  const nearEnd = window.after !== undefined && end - 1 - at < edge
  if (!next.moving && gameId !== undefined && (nearStart || nearEnd)) {
    return { set: { ...next, moving: true }, request: { _tag: "MoveCatalog", query: setQuery(next), from: { _tag: "Around", gameId }, count: SET_WINDOW } }
  }
  return { set: next }
}

/** Places a set holds, from its lowest. */
export function setCount(set: SetState, catalog: SurfaceCatalog): number {
  const window = setWindow(set, catalog)
  return window === undefined ? 0 : Math.max(0, window.total - set.low)
}

/** Whether the set can be sorted and filtered (Not started is one fixed list). */
export const setSorts = (set: SetState): boolean => !set.fromEnd

export function pressSet(state: State, set: SetState, button: Direction | "confirm" | "back", model: Model, plan: Plan): Step {
  const count = setCount(set, model.catalog)
  if (set.pill) {
    if (button === "down" && count > 0) return stay({ ...state, set: { ...set, pill: false } })
    if (button === "confirm") return { state, act: { t: "sheet", sheet: "sort" } }
    if (button === "back") return stay({ ...state, set: undefined, depth: 2 })
    return stay(state)
  }
  if (button === "back") return stay({ ...state, set: undefined, depth: 2 })
  if (count === 0) return button === "up" && setSorts(set) ? stay({ ...state, set: { ...set, pill: true } }) : stay(state)
  if (button === "confirm") {
    const game = setGame(set, model.catalog, set.at)
    return game === undefined ? stay(state) : { state, act: { t: "detail", game } }
  }
  const place = set.at - set.low
  const next = gridMove(place, count, plan.columns, button)
  if (next === undefined) return button === "up" && setSorts(set) ? stay({ ...state, set: { ...set, pill: true, scrolled: undefined } }) : stay(state)
  // Focus moves only onto a place the window holds; beyond it, the next window is on its way.
  const game = setGame(set, model.catalog, set.low + next)
  if (game === undefined) return stay(state)
  return stay({ ...state, set: { ...set, at: set.low + next, gameId: game.id, scrolled: undefined } })
}

export function tapSet(state: State, set: SetState, target: string, model: Model): Step | undefined {
  if (target === "pill") return setSorts(set) ? { state: { ...state, set: { ...set, pill: true } }, act: { t: "sheet", sheet: "sort" } } : undefined
  if (!target.startsWith("g:")) return undefined
  const at = Number(target.slice(2))
  const game = setGame(set, model.catalog, at)
  if (game === undefined) return undefined
  const had = !set.pill && set.at === at
  // A game opens on the second tap: the first only chooses it (ADR 0055).
  return had ? { state, act: { t: "detail", game } } : stay({ ...state, set: { ...set, pill: false, at, gameId: game.id } })
}

/** A finger dragged the row or a shelf `by` whole tiles (ADR 0055): it scrolls, focus stays. */
export function dragHome(state: State, region: string, by: number, model: Model): State {
  const count = region === "row" ? rowItems(state, model).length : shelvesOf(state.lists, model).find(shelf => shelfRegion(shelf.key) === region)?.items.length
  if (count === undefined || count === 0) return state
  const focused = region === "row" ? state.rowAt : state.columns[region.slice(6)] ?? 0
  const from = state.scrolled?.[region] ?? focused
  const to = Math.max(0, Math.min(count - 1, from + by))
  return to === from ? state : { ...state, scrolled: { ...state.scrolled, [region]: to } }
}

/** A finger dragged the set's rows `by` whole rows (ADR 0055): the rows scroll, focus stays. */
export function dragSet(set: SetState, by: number, model: Model, plan: Plan): SetState {
  const rows = Math.ceil(setCount(set, model.catalog) / plan.columns)
  const from = set.scrolled ?? set.first
  const to = Math.max(0, Math.min(Math.max(0, rows - plan.gridRows), from + by))
  return to === from ? set : { ...set, scrolled: to }
}

export function setView(state: State, set: SetState, model: Model, plan: Plan, live: boolean) {
  const window = setWindow(set, model.catalog)
  const count = setCount(set, model.catalog)
  const first = set.scrolled ?? set.first
  const pool = plan.poolRows * plan.columns
  const from = Math.max(0, first - 1) * plan.columns
  // The rows in sight (the whole ones and the one that shows under them), one above and one below: the rows a move can bring in.
  const until = Math.min(count, from + pool, (first + plan.gridRows + 2) * plan.columns)
  const tiles: TileView[] = []
  for (let slot = 0; slot < pool; slot++) tiles.push(NO_TILE)
  const focusedAt = set.at - set.low
  if (window !== undefined) {
    // The focused row asks for its pictures first, then the rows beside it (the pixel budget, src/art.ts).
    const places: number[] = []
    for (let place = from; place < until; place++) places.push(place)
    const row = (place: number) => Math.floor(place / plan.columns)
    // Rows in sight (the whole ones and the one that shows under them) before a row that waits out of sight.
    const away = (place: number) => (row(place) >= first && row(place) <= first + plan.gridRows ? 0 : 1)
    places.sort((a, b) => away(a) - away(b) || Math.abs(row(a) - row(focusedAt)) - Math.abs(row(b) - row(focusedAt)) || a - b)
    for (const place of places) {
      const game = window.window[set.low + place - window.firstIndex]
      const lit = live && !set.pill && place === focusedAt
      // Each place keeps its slot while it is near the view, so a row that scrolls keeps its tiles.
      tiles[place % pool] = game === undefined ? { ...NO_TILE, shown: true, at: place } : { ...gameCard(game), at: place, lit, badge: lit ? game.system ?? "" : "" } as TileView
    }
  }
  // A focus target for each place a tile is sent for: the rows in sight and one more each way.
  const marks = window === undefined ? [] : window.window.flatMap((game, index) => {
    const place = window.firstIndex + index - set.low
    return place < from || place >= until ? [] : [{ focusId: `g:${set.low + place}`, focused: live && !set.pill && place === focusedAt, focusRegion: "grid", at: place, label: game.title }]
  })
  const focused = set.pill ? undefined : setGame(set, model.catalog, set.at)
  const system = set.key === "every" && set.systemId !== undefined ? model.systems?.find(item => item.systemId === set.systemId)?.system : undefined
  const waiting = window === undefined
  return {
    set: {
      shown: true,
      title: caps(system ?? set.title),
      count: waiting ? "" : `${gamesWords(count)}${setSorts(set) ? `, ${sortWords(set.order)}` : ""}`,
      pill: setSorts(set) ? pill("Sort and filter", "pill", "pill", live && set.pill, { icon: "sliders", quiet: true }) : NO_PILL,
      firstRow: first,
      rows: Math.ceil(count / plan.columns),
      nowTitle: focused?.title ?? "",
      nowFact: focused === undefined ? "" : ago(focused.lastPlayedAt, model.publishedAt),
      state: waiting ? "loading" : count === 0 ? "none" : "",
      words: waiting ? "Finding these games." : count === 0 ? "No game is here yet." : "",
    },
    tiles,
    marks,
  }
}

export function setExits(set: SetState, model: Model): Record<string, Record<string, { to: string; entry: string }>> {
  if (!setSorts(set)) return {}
  return setCount(set, model.catalog) > 0 && setWindow(set, model.catalog) !== undefined
    ? { pill: { down: { to: "grid", entry: "remembered" } }, grid: { up: { to: "pill", entry: "first" } } }
    : {}
}

// ---- The detail ----

export interface DetailButton {
  readonly id: string
  readonly label: string
  readonly icon: string
  readonly quiet: boolean
  readonly act: Act
}

/** The detail's buttons: Play first, then each other place it plays, a save to choose, and what else Korri offers. */
export function detailButtons(game: Game, model: Model): readonly DetailButton[] {
  const verb = game.resumable === true ? "Resume" : "Play"
  const buttons: DetailButton[] = []
  const places = game.launchLocations ?? []
  if (places.length > 1) {
    places.forEach((place, index) => buttons.push({
      id: `d:place:${index}`, label: `${index === 0 ? verb : "Play"} on ${place.label}`, icon: index === 0 ? "play" : "peer", quiet: index > 0,
      act: send({ _tag: "LaunchGame", gameId: game.id, launchLocationId: place.id }),
    }))
  } else {
    const only = places[0]
    buttons.push({ id: "d:play", label: verb, icon: "play", quiet: false, act: send(only === undefined ? { _tag: "LaunchGame", gameId: game.id } : { _tag: "LaunchGame", gameId: game.id, launchLocationId: only.id }) })
  }
  if (game.saves?._tag === "Split") {
    const choose = game.saves.choose
    buttons.push({ id: "d:save", label: choose.label, icon: "save", quiet: true, act: choose.enabled ? send({ _tag: "RunAction", actionId: choose.id }) : NONE })
  }
  const more = model.focusedGame
  if (more !== undefined && more.gameId === game.id && more.actions.length > 0) buttons.push({ id: "d:more", label: "More", icon: "menu", quiet: true, act: { t: "sheet", sheet: "actions" } })
  return buttons
}

/** The game the detail shows: Korri's newest copy when it knows one. */
export function detailGame(state: State, model: Model): Game | undefined {
  const detail = state.detail
  if (detail === undefined) return undefined
  if (model.catalog._tag === "Ready") {
    const fresh = model.catalog.window.find(game => game.id === detail.game.id)
    if (fresh !== undefined) return fresh
  }
  for (const key in state.lists.kept) {
    const fresh = state.lists.kept[key]!.find(game => game.id === detail.game.id)
    if (fresh !== undefined) return fresh
  }
  return detail.game
}

function savesWords(game: Game): string | undefined {
  switch (game.saves?._tag) {
    case "Synced":
      return "Saves in sync"
    case "NotSynced":
      return "Saves not in sync"
    case "SyncedBySteam":
      return "Saves kept by Steam"
    case "Split":
      return "Two saves to choose from"
    default:
      return undefined
  }
}

function downloadWords(game: Game): string {
  const download = game.download
  if (download === undefined) return ""
  if (download.queued === true) return "Waiting for another download to finish."
  if (download.progress !== undefined) {
    return download.progress.total === undefined ? `Downloading: ${bytes(download.progress.done)} so far.` : `Downloading: ${bytes(download.progress.done)} of ${bytes(download.progress.total)}.`
  }
  return download.bytes === undefined ? "Needs a download before it can start." : `Needs a ${bytes(download.bytes)} download before it can start.`
}

export function pressDetail(state: State, button: Direction | "confirm" | "back", model: Model, plan: Plan): Step {
  const game = detailGame(state, model)
  if (state.detail === undefined || game === undefined) return stay({ ...state, detail: undefined })
  if (button === "back") return stay({ ...state, detail: undefined })
  const buttons = detailButtons(game, model)
  const at = Math.max(0, Math.min(state.detail.at, buttons.length - 1))
  const stacked = detailStacks(buttons, plan)
  if (button === "confirm") return { state, act: buttons[at]?.act ?? NONE }
  const along = stacked ? (button === "up" ? -1 : button === "down" ? 1 : 0) : button === "left" ? -1 : button === "right" ? 1 : 0
  if (along === 0) return stay(state)
  const next = lineMove(at, buttons.length, along)
  return stay(next === undefined ? state : { ...state, detail: { ...state.detail, at: next } })
}

/** Whether the detail's buttons stand one under the other: a narrow window, or more words than one row holds. */
export function detailStacks(buttons: readonly DetailButton[], plan: Plan): boolean {
  if (plan.stack) return true
  const width = buttons.reduce((sum, button) => sum + plan.button * (textWidth(button.label, true) + 3.6), 0) + (buttons.length - 1) * plan.button * 0.8
  return width > plan.width - 2 * plan.padX
}

export function detailView(state: State, model: Model, plan: Plan, live: boolean) {
  const game = detailGame(state, model)
  if (state.detail === undefined || game === undefined) return undefined
  const buttons = detailButtons(game, model)
  const at = Math.max(0, Math.min(state.detail.at, buttons.length - 1))
  const hero = hasHero(game)
  const extra: FactView[] = [
    ...(game.activeProfile === undefined ? [] : [{ icon: "sliders", text: `Profile: ${game.activeProfile}`, strong: false }]),
    ...(savesWords(game) === undefined ? [] : [{ icon: "save", text: savesWords(game)!, strong: false }]),
  ]
  const facts = [...factsOf(game, model.publishedAt, true), ...extra]
  const mark: MarqueeView = marqueeOf(game, { width: hero ? plan.detailLogoW : plan.textW, logoH: hero ? plan.detailLogoH : plan.markH, title: hero ? plan.detailTitle : plan.pickTitle, fact: plan.detailFact, mark: hero || !cardNames(game), badge: false }, model.publishedAt, [])
  return {
    detail: {
      shown: true,
      hero,
      mark,
      stack: detailStacks(buttons, plan),
      badge: game.system ?? "",
      facts: factLines(facts, plan.detailFact, plan.width - 2 * plan.padX, game.system === undefined ? 0 : plan.detailFact * (1.6 + textWidth(game.system, true))),
      note: downloadWords(game),
    },
    cards: hero ? [] : [gameCard(game)],
    buttons: buttons.map((button, index) => pill(button.label, "detail", button.id, live && index === at, { icon: button.icon, quiet: button.quiet })),
  }
}

export { NO_IMAGE }
