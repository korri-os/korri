/**
 * The home's lists over Korri's one catalog window.
 *
 * A surface has one `model.catalog` window and one query at a time
 * (`MoveCatalog`). Velvet's home and lobby need several small lists, so the
 * logic asks for them one after another, one question an update, and keeps
 * the few games each list shows. A pass over the lists, in order:
 *
 *     recent   order `recent`, from the start: the pick and "Lately". It is
 *              also the window Velvet rests on
 *     most     order `mostPlayed`, from the start: the pick when nothing was
 *              played this week, and the row under it
 *     tail     order `recent`, from the end: a game never played sorts last
 *     korri    Korri's own order, from the start
 *     ends     Korri's own order, from the end: a section's games arrive
 *              together and other devices' sections come last, so this
 *              names them (and up to three pages before it, `ends:N`)
 *     scan:N,  pages read to find where the games not yet started begin,
 *     tail:N   when no list above shows it ("Not started", below)
 *     fresh    order `recent`, around the first game not yet started: the
 *              "Not started" shelf, the row's last place
 *     peer:S   order `recent` within one other device's section: "Plays on S"
 *     rest     `recent` from the start once more: the window Velvet rests on
 *
 * An answer belongs to the question that asked for it (`answers`): a list
 * from an end by its query and that end, a page by the place in the order
 * its cursor stood at (Korri gives no cursor back), a game's surroundings by
 * the game. A pass is over when each want is answered (`read`).
 *
 * Korri changes games Velvet already holds: artwork is stored after the
 * library is found, a few games a pass; a game is played, becomes resumable,
 * gets a download. On every publication, every kept game that stands in the
 * published window is replaced by the window's (`refresh`, by id), and when
 * one of them differed, or the window Velvet rests on shows other games, the
 * lists are read again, each once. The treaty has no mark that says "the
 * library changed", so a change is seen only in the window Korri publishes;
 * that is why Velvet rests on the games it shows most (the newest sixty).
 *
 * The lists also go stale when the library's systems change (`stampOf`),
 * when a game was just played, and when a set held the window. While a set
 * holds the window (src/browse.ts) nothing is asked here.
 *
 * Korri publishes with every move of the focus, and every publication is
 * new objects all the way down. So that reading one costs little:
 *   - a game is one object in every list that holds it (`keep`), and a
 *     publication is read by its window's games against that one object
 *     (`refresh`: sixty comparisons, not one for every game kept);
 *   - a game that says what it said before keeps its object, so everything
 *     made from it (its tile, the home's row, the shelves) is found again
 *     by that object and not made again;
 *   - lists settled against a model stay settled against that same model
 *     (a press and a tick settle too, and ask nothing of the lists).
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
const MOST_PEERS = 6
/** Games a page holds (the treaty's most). */
const PAGE = 200
/** Games read around the first one not yet started: the shelf's, and one more to know there are more. */
const FRESH_COUNT = 24

const query = (order: CatalogQuery["order"], extra: Partial<CatalogQuery> = {}): CatalogQuery => ({ text: "", order, ...extra })
const RECENT = query("recent")
const KORRI = query("korri")

/** Games in the window Velvet rests on: the newest played, which the pick, the row and Lately show. */
const REST_COUNT = 60

/** The window Velvet rests on: asked first (the pick shows soonest) and again last. */
const REST: Want = { key: "recent", query: RECENT, from: { at: "start" }, count: REST_COUNT }
const REST_AGAIN: Want = { ...REST, key: "rest" }
/** The list an answer is kept as: the last question of a pass is the first list again. */
const listOf = (want: Want): string => (want.key === REST_AGAIN.key ? REST.key : want.key)

// `tail` before `korri`: whether Korri's order is read on for the first unplayed game is known by then.
const BASE: readonly Want[] = [
  REST,
  { key: "most", query: query("mostPlayed"), from: { at: "start" }, count: 12 },
  { key: "tail", query: RECENT, from: { at: "end" }, count: 24 },
  { key: "korri", query: KORRI, from: { at: "start" }, count: 50 },
  { key: "ends", query: KORRI, from: { at: "end" }, count: PAGE },
]

export const LISTS_START: Lists = { kept: {}, total: 0, peers: [], peerTotals: {}, waited: 0, read: [], stamp: "", backs: [], scans: [], tails: [] }

const played = (game: Game): boolean => game.lastPlayedAt !== undefined
const unplayed = (game: Game): boolean => game.lastPlayedAt === undefined
const playedOf = (games: readonly Game[] | undefined): Game[] => (games ?? []).filter(played)
const freshOf = (games: readonly Game[] | undefined): Game[] => (games ?? []).filter(unplayed)

// ---- Not started: the first one, in Korri's order ----
//
// Korri sorts `recent` by the last session and keeps its own order among equals, so in that
// order every game with no session comes after every played one, and they stand in Korri's
// order. Three lists can show where they begin:
//
//   recent   holds one when fewer games were played than it holds: the first there is it;
//   tail     (the end of `recent`) holds a played game when all of them fit in it: the first
//            unplayed one there is it;
//   else     they begin somewhere between, and the lists are read on from both ends, a page
//            each in turn, each page its own question: Korri's own order from its start
//            (`korri`, `scan:1`, ...) until a game with no session comes, and `recent` back
//            from its end (`tail`, `tail:1`, ...) until a played game comes. The first to
//            show it ends both (`found`). That costs twice the shorter way at most: a page
//            is 200 games, the way from the start is as long as the played games that lead
//            Korri's order, the way from the end as long as the games not yet started.
//
// Then `recent` is read around that game (`fresh`): those are the games the shelf, the row's
// last place and the set show, from the first one on.

/** The tail holds a played game: every game not yet started is in it. */
const tailHoldsAll = (lists: Lists): boolean => (lists.kept.tail ?? []).some(played)

/** The first game not yet started in Korri's order, as far as the lists read in this pass say. */
function firstFresh(lists: Lists): string | undefined {
  const early = (lists.kept.recent ?? []).find(unplayed)
  if (early !== undefined) return early.id
  if (tailHoldsAll(lists)) return lists.kept.tail!.find(unplayed)?.id
  return lists.found
}

/** There are games not yet started, and neither `recent` nor the tail shows where they begin. */
function freshUnplaced(lists: Lists): boolean {
  const tail = lists.kept.tail
  return tail !== undefined && tail.length > 0 && tail.every(unplayed) && firstFresh(lists) === undefined
}

/** Every list wanted, in the order it is asked for. */
export function wants(lists: Lists): readonly Want[] {
  // Asked for at every step of a pass, and the same while everything it is made of is.
  if (
    wanted !== undefined && wanted.kept === lists.kept && wanted.found === lists.found && wanted.backs === lists.backs && wanted.scans === lists.scans && wanted.tails === lists.tails
    && wanted.peers === lists.peers
  ) return wanted.wants
  const made = wantsOf(lists)
  wanted = { kept: lists.kept, found: lists.found, backs: lists.backs, scans: lists.scans, tails: lists.tails, peers: lists.peers, wants: made }
  return made
}

let wanted: (Pick<Lists, "kept" | "found" | "backs" | "scans" | "tails" | "peers"> & { readonly wants: readonly Want[] }) | undefined

function wantsOf(lists: Lists): readonly Want[] {
  const backs = lists.backs.map((page, i): Want => ({ key: `ends:${i + 1}`, query: KORRI, from: { at: "before", ...page }, count: PAGE }))
  // The pages from both ends, in turn; none once the first game not yet started is found.
  const pages: Want[] = []
  for (let i = 0; lists.found === undefined && (i < lists.scans.length || i < lists.tails.length); i++) {
    const scan = lists.scans[i]
    const tail = lists.tails[i]
    if (scan !== undefined) pages.push({ key: `scan:${i + 1}`, query: KORRI, from: { at: "after", ...scan }, count: PAGE })
    if (tail !== undefined) pages.push({ key: `tail:${i + 1}`, query: RECENT, from: { at: "before", ...tail }, count: PAGE })
  }
  const first = firstFresh(lists)
  // `recent` shows the first ones itself when it holds more of them than a shelf does; the tail when it holds them all.
  const shown = tailHoldsAll(lists) || freshOf(lists.kept.recent).length > SHELF_SLOTS
  const fresh: Want[] = first === undefined || shown ? [] : [{ key: "fresh", query: RECENT, from: { at: "around", gameId: first }, count: FRESH_COUNT }]
  const peers = lists.peers.map((section): Want => ({ key: `peer:${section}`, query: query("recent", { section }), from: { at: "start" }, count: SHELF_SLOTS + 1 }))
  return [...BASE, ...backs, ...pages, ...fresh, ...peers, REST_AGAIN]
}

/** The next question of the pass: the first want not answered in it. */
const nextWant = (lists: Lists): Want | undefined => wants(lists).find(want => !lists.read.includes(want.key))

/** A pass is over and no question is out. */
const resting = (lists: Lists): boolean => lists.asked === undefined && lists.read.length > 0 && nextWant(lists) === undefined

/**
 * Whether two publications of a game say the same: everything Velvet shows
 * of a game or decides by. Field by field, because Korri sends every game
 * anew with every model and this runs for each one kept.
 */
export function sameGame(a: Game, b: Game): boolean {
  if (a === b) return true
  if (
    a.id !== b.id || a.lastPlayedAt !== b.lastPlayedAt || a.resumable !== b.resumable || a.playCount !== b.playCount || a.totalPlaytimeSeconds !== b.totalPlaytimeSeconds
    || a.title !== b.title || a.section !== b.section || a.subtitle !== b.subtitle || a.systemId !== b.systemId || a.system !== b.system
    || a.coverArtUrl !== b.coverArtUrl || a.activeProfile !== b.activeProfile
  ) return false
  const saves = a.saves, others = b.saves
  if (saves !== others && saves?._tag !== others?._tag) return false
  const media = a.media, other = b.media
  if (media !== other) {
    const count = media === undefined ? 0 : media.length
    if (count !== (other === undefined ? 0 : other.length)) return false
    for (let i = 0; i < count; i++) {
      const m = media![i]!, o = other![i]!
      if (m.url !== o.url || m.role !== o.role || m.color !== o.color || m.width !== o.width || m.height !== o.height) return false
    }
  }
  const places = a.launchLocations, there = b.launchLocations
  if (places !== there) {
    const count = places === undefined ? 0 : places.length
    if (count !== (there === undefined ? 0 : there.length)) return false
    for (let i = 0; i < count; i++) if (places![i]!.id !== there![i]!.id || places![i]!.label !== there![i]!.label) return false
  }
  // A download is rare and small: its whole value.
  const download = a.download, their = b.download
  if (download === their) return true
  if (download === undefined || their === undefined) return false
  return JSON.stringify(download) === JSON.stringify(their)
}

/**
 * The kept games as the window Korri published has them: each one that
 * stands in the window is the window's. `changed`: one of them said
 * something else than the one kept.
 */
export function refresh(lists: Lists, window: readonly Game[]): { readonly lists: Lists; readonly changed: boolean } {
  if (window.length === 0) return { lists, changed: false }
  // The same lists against the same window, found the same before: nothing to read again.
  if (rested !== undefined && rested.kept === lists.kept && rested.window === window) return { lists, changed: false }
  const known = knownOf(lists.kept)
  // The games that say something new, by id. Few, and most often none.
  let news: Map<string, Game> | undefined
  for (let w = 0; w < window.length; w++) {
    const now = window[w]!
    const old = known.get(now.id)
    if (old === undefined || old === now || sameGame(old, now)) continue
    ;(news ??= new Map()).set(now.id, now)
  }
  if (news === undefined) {
    rested = { kept: lists.kept, window }
    return { lists, changed: false }
  }
  // One of them changed: it is replaced in every list it stands in.
  const kept: Record<string, readonly Game[]> = {}
  for (const key in lists.kept) {
    const list = lists.kept[key]!
    let next: Game[] | undefined
    for (let i = 0; i < list.length; i++) {
      const now = news.get(list[i]!.id)
      if (now !== undefined) (next ??= [...list])[i] = now
    }
    kept[key] = next ?? list
  }
  return { lists: { ...lists, kept }, changed: true }
}

let rested: { readonly kept: Lists["kept"]; readonly window: readonly Game[] } | undefined

// Every kept game by its id. A game is one object in every list it stands in (`keep`, and
// the replacement above), so one entry a game is enough: a publication is read by its
// window's games (60), not by every game kept (some hundreds, and Korri publishes with
// every move of the focus). Made once for the lists kept.
let knowing: { readonly kept: Lists["kept"]; readonly games: Map<string, Game> } | undefined

function knownOf(kept: Lists["kept"]): Map<string, Game> {
  if (knowing !== undefined && knowing.kept === kept) return knowing.games
  const games = new Map<string, Game>()
  for (const key in kept) {
    const list = kept[key]!
    for (let i = 0; i < list.length; i++) games.set(list[i]!.id, list[i]!)
  }
  knowing = { kept, games }
  return games
}

/** Whether a window holds the games a kept list holds, in its order. (Whether one of them changed, `refresh` says.) */
function sameOrder(kept: readonly Game[] | undefined, window: readonly Game[]): boolean {
  if (kept === undefined || kept.length !== window.length) return false
  for (let i = 0; i < window.length; i++) if (kept[i]!.id !== window[i]!.id) return false
  return true
}

export const sameQuery = (a: CatalogQuery, b: CatalogQuery): boolean =>
  a.text.trim() === b.text.trim() && a.order === b.order && (a.section ?? "") === (b.section ?? "") && (a.systemId ?? "") === (b.systemId ?? "")

type Ready = Extract<SurfaceCatalog, { _tag: "Ready" }>

/**
 * Whether the window is the answer to the want: its query, and where it
 * stands. A page is known by its place in the order (Korri gives no cursor
 * back), a game's surroundings by the game: so a window answers the question
 * that asked for it and no other.
 */
export function answers(catalog: Ready, want: Want): boolean {
  if (!sameQuery(catalog.query, want.query)) return false
  const from = want.from
  switch (from.at) {
    case "start":
      return catalog.firstIndex === 0
    case "end":
      return catalog.after === undefined
    case "before":
      return catalog.firstIndex + catalog.window.length === from.index
    case "after":
      return catalog.firstIndex === from.index
    case "around":
      return catalog.window.some(game => game.id === from.gameId)
  }
}

export function moveFor(want: Want): Request {
  const at = want.from
  const from = at.at === "start" ? { _tag: "Start" as const }
    : at.at === "end" ? { _tag: "End" as const }
      : at.at === "before" ? { _tag: "Before" as const, cursor: at.cursor }
        : at.at === "after" ? { _tag: "After" as const, cursor: at.cursor }
          : { _tag: "Around" as const, gameId: at.gameId }
  return { _tag: "MoveCatalog", query: want.query, from, count: want.count }
}

/** What the lists were read against. */
export function stampOf(model: Model): string {
  if (stamped !== undefined && stamped.systems === model.systems) return stamped.stamp
  const stamp = (model.systems ?? []).map(system => `${system.systemId}:${system.total}`).join(",")
  stamped = { systems: model.systems, stamp }
  return stamp
}

let stamped: { readonly systems: Model["systems"]; readonly stamp: string } | undefined

/** Whether two publications of the systems say the same. */
function sameSystems(a: Model["systems"], b: Model["systems"]): boolean {
  if (a === b) return true
  if (a === undefined || b === undefined || a.length !== b.length) return false
  for (let i = 0; i < a.length; i++) if (a[i]!.systemId !== b[i]!.systemId || a[i]!.system !== b[i]!.system || a[i]!.total !== b[i]!.total) return false
  return true
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
  // Settled against this very model already, and nothing came of it: nothing will now. A
  // press and a tick settle too, and Korri's model is the same object until it publishes.
  if (settled !== undefined && settled.lists === lists && settled.model === model && settled.free === free) return { lists }
  const result = settleNow(lists, model, free)
  if (result.request === undefined && result.lists === lists) settled = { lists, model, free }
  return result
}

let settled: { readonly lists: Lists; readonly model: Model; readonly free: boolean } | undefined

function settleNow(lists: Lists, model: Model, free: boolean): { readonly lists: Lists; readonly request?: Request } {
  const catalog = model.catalog
  if (catalog._tag === "Empty") {
    const empty = lists.total === 0 && lists.asked === undefined && lists.read.length > 0 && Object.keys(lists.kept).length === 0
    return empty ? { lists } : { lists: { ...LISTS_START, read: [REST.key], stamp: stampOf(model) } }
  }
  if (catalog._tag !== "Ready") return { lists }
  let next = lists
  // The library changed under the lists: ask again from the first.
  const stamp = stampOf(model)
  if (next.stamp !== stamp) next = { ...staleLists(next), stamp }
  // Every kept game Korri published again is as it is now.
  const fresh = refresh(next, catalog.window)
  next = fresh.lists
  // A pass is over, and Korri published on its own. What the window says now decides.
  if (resting(next)) {
    // The window is not the one Velvet rests on (a set held it): read the lists again.
    if (!answers(catalog, REST)) next = staleLists(next)
    // The resting window's games changed (artwork came, a game was played), or other games
    // stand in it now: so may every other list. This window is the first answer of the new pass.
    else if (fresh.changed || !sameOrder(next.kept[REST.key], catalog.window)) next = staleLists(next)
  }
  // The answer to the question out.
  if (next.asked !== undefined && answers(catalog, next.asked)) next = keep(next, next.asked, catalog)
  // The window Korri already shows may answer the next ones (the resting window answers the first).
  for (let want = nextWant(next); next.asked === undefined && want !== undefined && answers(catalog, want); want = nextWant(next)) next = keep(next, want, catalog)
  if (next.asked !== undefined || !free) return { lists: next }
  const want = nextWant(next)
  if (want === undefined) return { lists: next }
  return { lists: { ...next, asked: want, waited: 0 }, request: moveFor(want) }
}

/** A window's games, each as the object already kept for it: a game is one object in every list it stands in. */
function canonical(lists: Lists, window: readonly Game[]): Game[] {
  const known = knownOf(lists.kept)
  return window.map(game => known.get(game.id) ?? game)
}

function keep(lists: Lists, want: Want, catalog: Ready): Lists {
  // (`refresh` has made every kept game say what this window says.)
  const window = canonical(lists, catalog.window)
  // A page is read for what it says of the order, and is not kept.
  const page = want.from.at === "before" || want.from.at === "after"
  let next: Lists = { ...lists, kept: page ? lists.kept : { ...lists.kept, [listOf(want)]: window }, asked: undefined, waited: 0, read: [...lists.read, want.key] }
  if (want.query.section === undefined && want.query.systemId === undefined) next = { ...next, total: catalog.total }
  if (want.key === "ends" || want.key.startsWith("ends:")) {
    // A window further back holds sections that come before the ones known.
    const peers = want.key === "ends" ? peersIn(catalog.window, []) : peersIn([...catalog.window, ...lists.peers.map(section => ({ id: "", title: "", section }))], [])
    next = { ...next, peers }
    // Another device's games may reach past this window: read the page before it, a few times.
    const first = catalog.window[0]
    if (first !== undefined && isPeer(first) && catalog.before !== undefined && lists.backs.length < PAGES_BACK) next = { ...next, backs: [...lists.backs, { cursor: catalog.before, index: catalog.firstIndex }] }
  }
  if ((want.key === "korri" || want.key.startsWith("scan:")) && freshUnplaced(next)) {
    // Korri's order, read from its start: the first game not yet started is here, or the next page is asked.
    const first = catalog.window.find(unplayed)
    if (first !== undefined) next = { ...next, found: first.id }
    else if (catalog.after !== undefined) next = { ...next, scans: [...lists.scans, { cursor: catalog.after, index: catalog.firstIndex + catalog.window.length }] }
  }
  if ((want.key === "tail" || want.key.startsWith("tail:")) && freshUnplaced(next)) {
    // The recent order, read back from its end: a played game here, and the first after it is the
    // one (or, with none after it here, the earliest one the page read before showed).
    const first = catalog.window.find(unplayed)
    if (catalog.window.some(played)) next = { ...next, found: first?.id ?? lists.earliest }
    else if (first !== undefined) {
      next = { ...next, earliest: first.id }
      if (catalog.before !== undefined) next = { ...next, tails: [...lists.tails, { cursor: catalog.before, index: catalog.firstIndex }] }
    }
  }
  if (want.key.startsWith("peer:")) next = { ...next, peerTotals: { ...next.peerTotals, [want.key.slice(5)]: catalog.total } }
  return next
}

/** A Tick passed: a question that got no answer is asked again. */
export function tickLists(lists: Lists): { readonly lists: Lists; readonly request?: Request } {
  const asked = lists.asked
  if (asked === undefined) return { lists }
  if (lists.waited + 1 < ASK_AGAIN_AFTER) return { lists: { ...lists, waited: lists.waited + 1 } }
  // A page's place was true of the order when its cursor was read: the pass starts again instead.
  if (asked.from.at === "before" || asked.from.at === "after") return { lists: staleLists(lists) }
  return { lists: { ...lists, waited: 0 }, request: moveFor(asked) }
}

/** The lists are to be read again, from the first (a game was just played). The kept ones show meanwhile. */
export function staleLists(lists: Lists): Lists {
  const { asked, found, earliest, ...rest } = lists
  const pristine = asked === undefined && found === undefined && earliest === undefined && lists.read.length === 0 && lists.backs.length === 0 && lists.scans.length === 0 && lists.tails.length === 0
  return pristine ? lists : { ...rest, read: [], backs: [], scans: [], tails: [], waited: 0 }
}

/**
 * The games not yet started, the first in Korri's order first, as the lists
 * show them: all of them when the tail holds them all; the ones `recent`
 * holds when it holds more than a shelf does; else the ones read around the
 * first (`fresh`), and until that is read, what `recent` holds.
 */
export function freshGames(lists: Lists): readonly Game[] {
  if (freshest !== undefined && freshest.kept === lists.kept) return freshest.games
  const games = freshNow(lists)
  freshest = { kept: lists.kept, games }
  return games
}

let freshest: { readonly kept: Lists["kept"]; readonly games: readonly Game[] } | undefined

function freshNow(lists: Lists): Game[] {
  const kept = lists.kept
  if (tailHoldsAll(lists)) return freshOf(kept.tail)
  const early = freshOf(kept.recent)
  if (early.length > SHELF_SLOTS) return early
  const around = freshOf(kept.fresh)
  return around.length > 0 ? around : early
}

export interface HomeLists {
  /** The game that fills the stage. Undefined: no game is known yet. */
  readonly pick: Game | undefined
  /** Why it is the pick: played this week, played most, or simply first. */
  readonly why: "recent" | "most" | "first"
  /** The row under it: recent, then most played, no repeats; the last holds a game not yet started. */
  readonly row: readonly Game[]
}

// A view asks for the home's lists several times (the stage, the row, the exits), and nothing
// of them changes while the kept lists are the same and the newest game is still this week's
// (or still is not): the answer is kept, and asking again reads no list.
let lastHome: { kept: Lists["kept"]; newest: Game | undefined; thisWeek: boolean; timed: boolean; home: HomeLists } | undefined

export function homeOf(lists: Lists, publishedAt: number | undefined): HomeLists {
  if (lastHome !== undefined && lastHome.kept === lists.kept && lastHome.timed === (publishedAt !== undefined)) {
    const newest = lastHome.newest
    const thisWeek = newest !== undefined && (publishedAt === undefined || within(newest.lastPlayedAt, publishedAt, PICK_DAYS))
    // The row holds this week's games first; with the pick still this week's, a game that left the week waits for the next list.
    if (thisWeek === lastHome.thisWeek) return lastHome.home
  }
  const recent = playedOf(lists.kept.recent)
  const most = (lists.kept.most ?? []).filter(game => (game.totalPlaytimeSeconds ?? 0) > 0)
  const newest = recent[0]
  // Without Korri's time there is nothing to measure a week against: the newest played game is the pick.
  const thisWeek = newest !== undefined && (publishedAt === undefined || within(newest.lastPlayedAt, publishedAt, PICK_DAYS))
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
  lastHome = { kept: lists.kept, newest, thisWeek, timed: publishedAt !== undefined, home }
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
  // Korri sends the systems anew with every model: the same ones give the same shelves, the same objects.
  if (lastShelves !== undefined && lastShelves.kept === lists.kept && lastShelves.total === lists.total && sameSystems(lastShelves.systems, model.systems)) {
    if (lastShelves.systems !== model.systems) lastShelves = { ...lastShelves, systems: model.systems }
    return lastShelves.shelves
  }
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
