/**
 * The home's lists over Korri's one catalog window.
 *
 * A surface has one `model.catalog` window and one query at a time
 * (`MoveCatalog`). Velvet's home and lobby need several small lists, so the
 * logic asks for them one after another, tells each answer by the query the
 * window names (`catalog.query`) and where it stands, and keeps the few
 * games each list shows:
 *
 *     recent   order `recent`, from the start: the pick and "Lately"
 *     most     order `mostPlayed`, from the start: the pick when nothing was
 *              played this week, and the row under it
 *     korri    Korri's own order, from the start: the first game not yet
 *              started
 *     tail     order `recent`, from the end: a game never played sorts last,
 *              so this is "Not started" when the start of Korri's order
 *              holds none; read further back (`tail:1`...) while a window
 *              holds only unplayed games, to find where they begin
 *
 * "Not started" is in Korri's order: Korri sorts `recent` stably, so the
 * games with no session keep Korri's order among themselves, after every
 * played game. The first of them is the first game not yet started in
 * Korri's order (`freshGames`).
 *     ends     Korri's own order, from the end: a section's games arrive
 *              together and other devices' sections come last, so this
 *              names them
 *     peer:S   order `recent` within one such section: "Plays on S"
 *
 *     rest     `recent` from the start once more, last: the window Velvet
 *              rests on while nothing is asked
 *
 * Korri changes games Velvet already holds: artwork arrives after the
 * library is found, a few games a pass; a game is played, becomes resumable,
 * gets a download. A surface shows what the model says now, so on every
 * publication:
 *
 *   - every kept game that stands in the window Korri published is replaced
 *     by the window's (`refresh`, by id);
 *   - when one of them differed (`sameGame`), or the window Velvet rests on
 *     changed its games, the library changed under the lists: all of them
 *     are asked for again, and the old ones show meanwhile;
 *   - when the window is no longer the one Velvet rests on (a set held it),
 *     the lists are asked for again too.
 *
 * The treaty has no mark that says "the library changed", so a change is
 * seen only in the window Korri publishes. That is why Velvet rests on the
 * games it shows most (the newest sixty). A game outside it that changes is
 * picked up at the next of these: any change inside the window, a return
 * from a set or from play, a change of `systems`.
 *
 * No loop: a question is asked only when a list is stale, each answer is
 * kept and compared with nothing, and the last answer is the resting window,
 * so a model that changes nothing a list holds (the clock, the focused game)
 * asks nothing. At most one question goes out a publication.
 *
 * The lists also go stale when the library's systems change (`stampOf`) or a
 * game was just played. While a set holds the window (src/browse.ts) nothing
 * is asked here.
 */
import type { CatalogQuery, SurfaceCatalog, SurfaceModel as Model } from "../../../../../contracts/generated/surface"
import { ROW_SLOTS, SHELF_SLOTS } from "./plan"
import type { Game, Lists, Request, SetState, Want } from "./types"
import { within } from "./words"

/** Korri's captions for its own two sections (the treaty's SurfaceGame.section); any other section is another device. */
export const SECTION_CONTINUE = "Continue"
export const SECTION_HERE = "This device"

/** The pick is the last game played when that was within this many days. */
export const PICK_DAYS = 7
/** Ticks a question waits before it is asked again. */
const ASK_AGAIN_AFTER = 4
/** How often the end of Korri's order is paged back to find more devices. */
const PAGES_BACK = 3
/** How often the end of the recent order is paged back to find the first unplayed game, and how many games a page. */
const FRESH_PAGES = 5
const FRESH_COUNT = 200
const MOST_PEERS = 6

const query = (order: CatalogQuery["order"], extra: Partial<CatalogQuery> = {}): CatalogQuery => ({ text: "", order, ...extra })

/** Games in the window Velvet rests on: the newest played, which the pick, the row and Lately show. */
const REST_COUNT = 60

/** The window Velvet rests on: asked first (the pick shows soonest) and again last. */
const REST: Want = { key: "recent", query: query("recent"), from: "start", count: REST_COUNT }

const BASE: readonly Want[] = [
  REST,
  { key: "most", query: query("mostPlayed"), from: "start", count: 12 },
  { key: "korri", query: query("korri"), from: "start", count: 50 },
  { key: "tail", query: query("recent"), from: "end", count: 24 },
  { key: "ends", query: query("korri"), from: "end", count: 200 },
]

export const LISTS_START: Lists = { kept: {}, total: 0, peers: [], peerTotals: {}, waited: 0, done: 0, stamp: "", backs: [], tails: [] }

/** Every list wanted, in the order it is asked for. */
export function wants(lists: Lists): readonly Want[] {
  const backs = lists.backs.map((before, i): Want => ({ key: `ends:${i + 1}`, query: query("korri"), from: { before }, count: 200 }))
  const tails = lists.tails.map((before, i): Want => ({ key: `tail:${i + 1}`, query: query("recent"), from: { before }, count: FRESH_COUNT }))
  return [...BASE, ...backs, ...tails, ...lists.peers.map((section): Want => ({ key: `peer:${section}`, query: query("recent", { section }), from: "start", count: SHELF_SLOTS + 1 })), REST]
}

/**
 * Whether two publications of a game say the same: everything Velvet shows
 * of a game or decides by. Field by field, because Korri sends every game
 * anew with every model and this runs for each one kept.
 */
export function sameGame(a: Game, b: Game): boolean {
  if (a === b) return true
  if (
    a.id !== b.id || a.title !== b.title || a.section !== b.section || a.subtitle !== b.subtitle || a.systemId !== b.systemId || a.system !== b.system
    || a.coverArtUrl !== b.coverArtUrl || a.resumable !== b.resumable || a.activeProfile !== b.activeProfile
    || a.lastPlayedAt !== b.lastPlayedAt || a.playCount !== b.playCount || a.totalPlaytimeSeconds !== b.totalPlaytimeSeconds
    || a.saves?._tag !== b.saves?._tag
  ) return false
  const media = a.media ?? []
  const other = b.media ?? []
  if (media.length !== other.length) return false
  for (let i = 0; i < media.length; i++) {
    const m = media[i]!, o = other[i]!
    if (m.url !== o.url || m.role !== o.role || m.color !== o.color || m.width !== o.width || m.height !== o.height) return false
  }
  const places = a.launchLocations ?? []
  const others = b.launchLocations ?? []
  if (places.length !== others.length || places.some((place, i) => place.id !== others[i]!.id || place.label !== others[i]!.label)) return false
  // A download is rare and small: its whole value.
  if ((a.download === undefined) !== (b.download === undefined)) return false
  return a.download === undefined || JSON.stringify(a.download) === JSON.stringify(b.download)
}

/**
 * The kept games as the window Korri published has them: each one that
 * stands in the window is the window's. `changed`: one of them said
 * something else than the one kept.
 */
export function refresh(lists: Lists, window: readonly Game[]): { readonly lists: Lists; readonly changed: boolean } {
  if (window.length === 0) return { lists, changed: false }
  let byId: Map<string, Game> | undefined
  let kept: Record<string, readonly Game[]> | undefined
  let changed = false
  for (const key in lists.kept) {
    const list = lists.kept[key]!
    let next: Game[] | undefined
    for (let i = 0; i < list.length; i++) {
      const old = list[i]!
      if (byId === undefined) {
        byId = new Map()
        for (const game of window) byId.set(game.id, game)
      }
      const now = byId.get(old.id)
      if (now === undefined || now === old || sameGame(old, now)) continue
      next ??= [...list]
      next[i] = now
      changed = true
    }
    if (next !== undefined) (kept ??= { ...lists.kept })[key] = next
  }
  return kept === undefined ? { lists, changed } : { lists: { ...lists, kept }, changed }
}

/** Whether a window holds the games a kept list holds, in its order (each compared by `sameGame`). */
function sameList(kept: readonly Game[] | undefined, window: readonly Game[]): boolean {
  if (kept === undefined || kept.length !== window.length) return false
  for (let i = 0; i < window.length; i++) if (kept[i]!.id !== window[i]!.id || !sameGame(kept[i]!, window[i]!)) return false
  return true
}

export const sameQuery = (a: CatalogQuery, b: CatalogQuery): boolean =>
  a.text.trim() === b.text.trim() && a.order === b.order && (a.section ?? "") === (b.section ?? "") && (a.systemId ?? "") === (b.systemId ?? "")

type Ready = Extract<SurfaceCatalog, { _tag: "Ready" }>

/** Whether the window answers the want: its query, and where it stands. */
export function answers(catalog: Ready, want: Want): boolean {
  if (!sameQuery(catalog.query, want.query)) return false
  if (want.from === "start") return catalog.firstIndex === 0
  if (want.from === "end") return catalog.after === undefined
  return catalog.after !== undefined || catalog.firstIndex === 0
}

export function moveFor(want: Want): Request {
  const from = want.from === "start" ? { _tag: "Start" as const } : want.from === "end" ? { _tag: "End" as const } : { _tag: "Before" as const, cursor: want.from.before }
  return { _tag: "MoveCatalog", query: want.query, from, count: want.count }
}

/** What the lists were read against. */
export function stampOf(model: Model): string {
  return (model.systems ?? []).map(system => `${system.systemId}:${system.total}`).join(",")
}

const isPeer = (game: Game): boolean => game.section !== undefined && game.section !== SECTION_CONTINUE && game.section !== SECTION_HERE

/** The peer sections a window of Korri's order holds, in that order. */
function peersIn(games: readonly Game[], known: readonly string[]): string[] {
  const out = [...known]
  for (const game of games) if (isPeer(game) && !out.includes(game.section!)) out.push(game.section!)
  return out.slice(0, MOST_PEERS)
}

/**
 * The lists after Korri published, and the next question when one is due.
 * `free`: the home or the lobby holds the window (no set is open).
 */
export function settleLists(lists: Lists, model: Model, free: boolean): { readonly lists: Lists; readonly request?: Request } {
  const catalog = model.catalog
  if (catalog._tag === "Empty") {
    return lists.total === 0 && lists.done === BASE.length && lists.asked === undefined ? { lists } : { lists: { ...LISTS_START, done: BASE.length, stamp: stampOf(model) } }
  }
  if (catalog._tag !== "Ready") return { lists }
  let next = lists
  // The library changed under the lists: ask again from the first.
  const stamp = stampOf(model)
  if (next.stamp !== stamp) next = { ...next, stamp, done: 0, backs: [], tails: [], ...(next.asked === undefined ? {} : { asked: undefined }) }
  // Settled, and Korri published on its own. What the window says now decides.
  if (next.asked === undefined && next.done > 0 && next.done >= wants(next).length) {
    if (!answers(catalog, REST)) {
      // The window is not the one Velvet rests on (a set held it): read the lists again.
      next = staleLists(next)
    } else if (!sameList(next.kept[REST.key], catalog.window)) {
      // The resting window's games changed (artwork came, a game was played): so may every other list.
      next = staleLists({ ...next, kept: { ...next.kept, [REST.key]: catalog.window } })
    }
  }
  // Every kept game Korri published again is as it is now.
  const fresh = refresh(next, catalog.window)
  next = fresh.lists
  // The answer to the question out.
  if (next.asked !== undefined && answers(catalog, next.asked)) next = keep(next, next.asked, catalog)
  // The window Korri already shows may answer the next ones (its first model answers `korri`).
  for (let want = wants(next)[next.done]; next.asked === undefined && want !== undefined && answers(catalog, want); want = wants(next)[next.done]) next = keep(next, want, catalog)
  if (next.asked !== undefined || !free) return { lists: next }
  const want = wants(next)[next.done]
  if (want === undefined) return { lists: next }
  return { lists: { ...next, asked: want, waited: 0 }, request: moveFor(want) }
}

function keep(lists: Lists, want: Want, catalog: Ready): Lists {
  const kept = { ...lists.kept, [want.key]: catalog.window }
  let next: Lists = { ...lists, kept, asked: undefined, waited: 0, done: lists.done + 1 }
  if (want.query.section === undefined && want.query.systemId === undefined) next = { ...next, total: catalog.total }
  if (want.key === "ends" || want.key.startsWith("ends:")) {
    // A window further back holds sections that come before the ones known.
    const peers = want.key === "ends" ? peersIn(catalog.window, []) : peersIn([...catalog.window, ...lists.peers.map(section => ({ id: "", title: "", section }))], [])
    next = { ...next, peers }
    // Another device's games may reach past this window: read further back, a few times.
    const first = catalog.window[0]
    if (first !== undefined && isPeer(first) && catalog.before !== undefined && lists.backs.length < PAGES_BACK) next = { ...next, backs: [...lists.backs, catalog.before] }
  }
  if (want.key === "tail" || want.key.startsWith("tail:")) {
    // Only unplayed games here, more before them, and no list that shows the first one: read further back.
    const known = freshOf(kept.recent).length > 0 || freshOf(kept.korri).length > 0
    const onlyFresh = catalog.window.length > 0 && catalog.window.every(game => game.lastPlayedAt === undefined)
    if (!known && onlyFresh && catalog.before !== undefined && lists.tails.length < FRESH_PAGES) next = { ...next, tails: [...lists.tails, catalog.before] }
  }
  if (want.key.startsWith("peer:")) next = { ...next, peerTotals: { ...next.peerTotals, [want.key.slice(5)]: catalog.total } }
  return next
}

/** A Tick passed: a question that got no answer is asked again. */
export function tickLists(lists: Lists): { readonly lists: Lists; readonly request?: Request } {
  if (lists.asked === undefined) return { lists }
  if (lists.waited + 1 < ASK_AGAIN_AFTER) return { lists: { ...lists, waited: lists.waited + 1 } }
  return { lists: { ...lists, waited: 0 }, request: moveFor(lists.asked) }
}

/** The lists are to be read again (a game was just played). The kept ones show meanwhile. */
export const staleLists = (lists: Lists): Lists => (lists.done === 0 && lists.asked === undefined ? lists : { ...lists, done: 0, backs: [], tails: [], asked: undefined, waited: 0 })

const playedOf = (games: readonly Game[] | undefined): Game[] => (games ?? []).filter(game => game.lastPlayedAt !== undefined)
const freshOf = (games: readonly Game[] | undefined): Game[] => (games ?? []).filter(game => game.lastPlayedAt === undefined)

/**
 * The games not yet started, the first in Korri's order first, as far as
 * the lists show them. The start of the recent order shows the first ones
 * when fewer games were played than it holds; else the start of Korri's
 * order does, when one of its games is unplayed; else the end of the recent
 * order, read back to the last played game (up to FRESH_PAGES pages: past
 * that the first of these is the earliest unplayed game seen, and the set
 * reads on from there).
 */
export function freshGames(lists: Lists): Game[] {
  const kept = lists.kept
  const head = [...freshOf(kept.recent), ...freshOf(kept.korri)]
  const back: Game[] = []
  if (head.length === 0) for (let i = lists.tails.length; i >= 1; i--) back.push(...freshOf(kept[`tail:${i}`]))
  const seen = new Set<string>()
  return [...head, ...back, ...freshOf(kept.tail)].filter(game => !seen.has(game.id) && seen.add(game.id) !== undefined)
}

export interface HomeLists {
  /** The game that fills the stage. Undefined: no game is known yet. */
  readonly pick: Game | undefined
  /** Why it is the pick: played this week, played most, or simply first. */
  readonly why: "recent" | "most" | "first"
  /** The row under it: recent, then most played, no repeats; the last holds a game not yet started. */
  readonly row: readonly Game[]
}

let lastHome: { kept: Lists["kept"]; week: string; home: HomeLists } | undefined

export function homeOf(lists: Lists, publishedAt: number | undefined): HomeLists {
  const recent = playedOf(lists.kept.recent)
  const most = (lists.kept.most ?? []).filter(game => (game.totalPlaytimeSeconds ?? 0) > 0)
  const newest = recent[0]
  // Without Korri's time there is nothing to measure a week against: the newest played game is the pick.
  const thisWeek = newest !== undefined && (publishedAt === undefined || within(newest.lastPlayedAt, publishedAt, PICK_DAYS))
  const week = `${thisWeek}:${publishedAt === undefined}`
  if (lastHome !== undefined && lastHome.kept === lists.kept && lastHome.week === week) return lastHome.home
  let pick: Game | undefined
  let why: HomeLists["why"]
  if (thisWeek) [pick, why] = [newest, "recent"]
  else if (most[0] !== undefined) [pick, why] = [most[0], "most"]
  else if (newest !== undefined) [pick, why] = [newest, "recent"]
  else [pick, why] = [lists.kept.korri?.[0] ?? lists.kept.recent?.[0], "first"]
  // The row's last place: the first game not yet started, in Korri's order.
  const fresh = freshGames(lists).find(game => game.id !== pick?.id)
  const row: Game[] = []
  const room = ROW_SLOTS - (fresh === undefined ? 0 : 1)
  const thisWeekGames = publishedAt === undefined ? recent : recent.filter(game => within(game.lastPlayedAt, publishedAt, PICK_DAYS))
  for (const game of [...thisWeekGames, ...most, ...recent, ...(lists.kept.korri ?? [])]) {
    if (row.length >= room) break
    if (game.id === pick?.id || game.id === fresh?.id || row.some(other => other.id === game.id)) continue
    row.push(game)
  }
  if (fresh !== undefined) row.push(fresh)
  const home = { pick, why, row }
  lastHome = { kept: lists.kept, week, home }
  return home
}

export type ShelfItem =
  | { readonly kind: "game"; readonly game: Game }
  | { readonly kind: "more"; readonly label: string; readonly count?: number; readonly set: SetState }
  | { readonly kind: "system"; readonly systemId: string; readonly system: string; readonly count: number; readonly set: SetState }

export interface ShelfSpec {
  readonly key: string
  readonly label: string
  readonly items: readonly ShelfItem[]
}

const SET_START = { at: 0, first: 0, pill: false, moving: true, low: 0, fromEnd: false } as const

export const everySet = (order: SetState["order"]): SetState => ({ ...SET_START, key: "every", title: "Every game", order })
/** Not started. It opens around `first`, the first unplayed game the lists know, so the window shows where they begin. */
export const freshSet = (first?: string): SetState => ({ ...SET_START, key: "fresh", title: "Not started", order: "recent", fromEnd: true, ...(first === undefined ? {} : { gameId: first }) })
export const peerSet = (section: string): SetState => ({ ...SET_START, key: `peer:${section}`, title: `Plays on ${section}`, order: "recent", section })
export const systemSet = (systemId: string, system: string): SetState => ({ ...SET_START, key: `system:${systemId}`, title: system, order: "title", systemId })

let lastShelves: { kept: Lists["kept"]; systems: Model["systems"]; total: number; shelves: readonly ShelfSpec[] } | undefined

/** The lobby's shelves: Lately, Not started, one per other device, By system. A shelf with nothing on it is left out. */
export function shelvesOf(lists: Lists, model: Model): readonly ShelfSpec[] {
  if (lastShelves !== undefined && lastShelves.kept === lists.kept && lastShelves.systems === model.systems && lastShelves.total === lists.total) return lastShelves.shelves
  const shelves: ShelfSpec[] = []
  const games = (list: readonly Game[]): ShelfItem[] => list.slice(0, SHELF_SLOTS).map(game => ({ kind: "game", game }))
  const lately = playedOf(lists.kept.recent)
  if (lately.length > 0) shelves.push({ key: "lately", label: "Lately", items: [...games(lately), { kind: "more", label: "See all", set: everySet("recent") }] })
  const fresh = freshGames(lists)
  if (fresh.length > 0) shelves.push({ key: "fresh", label: "Not started", items: [...games(fresh), ...(fresh.length > SHELF_SLOTS ? [{ kind: "more", label: "See all", set: freshSet(fresh[0]!.id) } as const] : [])] })
  for (const section of lists.peers) {
    const list = lists.kept[`peer:${section}`] ?? []
    if (list.length === 0) continue
    const total = lists.peerTotals[section] ?? list.length
    shelves.push({ key: `peer:${section}`, label: `Plays on ${section}`, items: [...games(list), ...(total > SHELF_SLOTS ? [{ kind: "more", label: "See all", count: total, set: peerSet(section) } as const] : [])] })
  }
  const systems = (model.systems ?? []).filter(system => system.total > 0)
  shelves.push({
    key: "systems", label: systems.length > 0 ? "By system" : "Every game",
    items: [
      ...systems.map((system): ShelfItem => ({ kind: "system", systemId: system.systemId, system: system.system, count: system.total, set: systemSet(system.systemId, system.system) })),
      { kind: "more", label: "A to Z", count: lists.total, set: everySet("title") },
    ],
  })
  lastShelves = { kept: lists.kept, systems: model.systems, total: lists.total, shelves }
  return shelves
}

/** A game Velvet knows by id, from any list kept or the window. */
export function gameById(lists: Lists, model: Model, id: string | undefined): Game | undefined {
  if (id === undefined) return undefined
  if (model.catalog._tag === "Ready") {
    const found = model.catalog.window.find(game => game.id === id)
    if (found !== undefined) return found
  }
  for (const key in lists.kept) {
    const found = lists.kept[key]!.find(game => game.id === id)
    if (found !== undefined) return found
  }
  return undefined
}
