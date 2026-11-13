/**
 * What a game shows: its tile, its marquee (name and facts) and what it
 * does to the stage.
 *
 * Art order: `hero`, `logo` and `poster` from Korri's `media`; else the
 * square `coverArtUrl`; else the no-art card (src/poster.ts). A picture
 * keeps its own shape: a poster fills a 2:3 tile (it is one); a square
 * cover stands over a field with the title under it; a logo alone stands on
 * a field. Only the hero is cut, where it is the stage. Every picture is
 * late or may never come (ADR 0065), so the no-art card is always sent with
 * it and Slint draws it underneath.
 *
 * Text shows a logo where one exists, the title otherwise, and the facts
 * Korri publishes (system, last played, playtime, where it plays). Nothing
 * Korri does not publish is ever said.
 */
import type { SurfaceMedia, SurfaceMediaRole } from "../../../../../contracts/generated/surface"
import { gelHue, hsl, parseHex, stageLight, toHsl } from "./colour"
import { breakLines, fitCaps, fitCapsOnce, textWidth } from "./fit"
import { sameGame } from "./lists"
import { onForget } from "./memo"
import { NO_POSTER, posterFor, posterView, type PosterView } from "./poster"
import { plateOf } from "./plate"
import type { Game } from "./types"
import { ago, caps, playtime } from "./words"

/**
 * The pixel budget. The shell keeps at most 24 MiB of decoded pixels for a
 * surface, and a picture it had to let go is empty again until it is read
 * once more, so a view never names more than fits. A view asks for its
 * pictures in order of importance (`take`): the stage's hero, the focused
 * game's logo, then tiles from the focused one outwards. A picture that does
 * not fit is not asked for, and its tile shows its no-art card, which is a
 * finished picture too.
 *
 * A stored picture reaches a surface with its longer side at most 1920 px,
 * four bytes a pixel. Korri gives a square cover no size; it is counted as
 * the 512 x 512 the cover rule asks plugins for (ADR 0065).
 */
export const PIXEL_BUDGET = 23 * 1024 * 1024
const LONGEST_SIDE = 1920
const COVER_SIDE = 512

/**
 * The pictures one view asks for, and what they cost so far. A view makes one
 * (`pictures()`) and hands it to everything that may ask for a picture, in
 * the order of importance: whoever asks first gets its picture first. It is
 * an argument, not something a module holds, so what a tile shows is decided
 * by what its view passed in and by nothing else.
 */
export interface Pictures {
  spent: number
  readonly taken: Set<string>
}

/** A new view: nothing asked for yet. */
export const pictures = (): Pictures => ({ spent: 0, taken: new Set() })

/** The decoded size of a stored picture as the shell hands it over. */
export function decodedBytes(width: number, height: number): number {
  const scale = Math.min(1, LONGEST_SIDE / Math.max(1, width, height))
  return Math.max(1, Math.round(width * scale)) * Math.max(1, Math.round(height * scale)) * 4
}

/** Ask for a picture in this view. False: it does not fit, do without it. */
export function take(asked: Pictures, url: string, width: number, height: number): boolean {
  if (asked.taken.has(url)) return true
  const cost = decodedBytes(width, height)
  if (asked.spent + cost > PIXEL_BUDGET) return false
  asked.spent += cost
  asked.taken.add(url)
  return true
}

export const mediaOf = (game: Game, role: SurfaceMediaRole): SurfaceMedia | undefined => game.media?.find(media => media.role === role && media.url !== "")

export interface Image {
  readonly $image: string
}
/**
 * No picture: one clear pixel. The shell has no empty image value, and a
 * missing key would keep the picture the row held before, so "none" is a
 * picture too; Slint counts a picture as there when it is wider than this.
 */
export const NO_IMAGE: Image = { $image: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR4nGNgAAIAAAUAAXpeqz8AAAAASUVORK5CYII=" }

/**
 * A tile in a view. Only `shown` is always there: a view sends what a tile
 * of its kind draws and no more, because every value a view holds is read on
 * every call (the call budget). A field left out keeps what the tile's list
 * row held before (the shell converts from the previous value), so a field
 * is left out only where the views do not read it for this kind:
 *
 *   game          kind, artKind, art, poster (the no-art card, always, under
 *                 the picture), label; for a cover: field, capLines, capSize;
 *                 for a logo: field
 *   nav, system   kind, poster, label, count
 *   a target      focusId, focused, focusRegion
 *   focused, lit  badge, badgeTop (the badge shows on no other tile)
 *   on a shelf    shelf, rests; in the row: rests; in a set: lit
 *   nothing here  shown: false and no focus id
 */
export interface TileView {
  readonly shown: boolean
  readonly kind?: "game" | "nav" | "system"
  readonly artKind?: "none" | "poster" | "cover" | "logo"
  readonly art?: Image
  readonly field?: number
  readonly capLines?: readonly string[]
  readonly capSize?: number
  readonly poster?: PosterView
  readonly badge?: string
  readonly badgeTop?: boolean
  readonly label?: string
  readonly count?: string
  readonly at?: number
  /** The shelf it stands on, in the lobby. */
  readonly shelf?: number
  readonly focusId?: string
  readonly focused?: boolean
  readonly focusRegion?: string
  /** Drawn as focused though it is no focus target (a set's tiles). */
  readonly lit?: boolean
  /** On a row that is not the focused one: it steps back under a veil. */
  readonly rests?: boolean
}

/** A place with nothing in it. */
export const NO_TILE: TileView = { shown: false, focusId: "" }

/** A place that is drawn and holds no game yet. */
const BLANK: TileView = { shown: true, kind: "game", artKind: "none", art: NO_IMAGE, poster: NO_POSTER, label: "" }

/** A field for a card that composes a picture: the picture's hue (the title's gel without one), deep. */
function fieldOf(colour: string | undefined, title: string): number {
  const parsed = parseHex(colour)
  if (parsed !== undefined) {
    const [h, s, l] = toHsl(parsed)
    if (s >= 0.12 && l >= 0.05) return hsl(h, Math.min(0.4, Math.max(0.2, s * 0.6)), 0.2)
  }
  return hsl(gelHue(title), 0.34, 0.2)
}

/** A game's tile, made once: as its no-art card, and with its picture (the same when it has none). */
interface Made {
  readonly game: Game
  readonly card: TileView
  readonly art: TileView
  /** The picture's address and stored size, for the pixel budget; "" with none. */
  readonly url: string
  readonly width: number
  readonly height: number
  /** Where the focus badge sits: on the card, and over the picture. */
  readonly cardBadgeTop: boolean
  readonly artBadgeTop: boolean
}

// By id, and kept while Korri says the same of the game: Korri sends every game anew with
// every model (a focus move publishes one), and a tile is not made again for that.
const made = new Map<string, Made>()

function madeOf(game: Game): Made {
  const known = made.get(game.id)
  if (known !== undefined && (known.game === game || sameGame(known.game, game))) return known
  const paper = posterFor(game.title, game.systemId)
  const lines = posterView(paper)
  const card: TileView = { shown: true, kind: "game", artKind: "none", art: NO_IMAGE, poster: lines, label: game.title }
  // One look through the game's pictures (this runs for every card on its way in).
  let poster: SurfaceMedia | undefined
  let logo: SurfaceMedia | undefined
  let hero: SurfaceMedia | undefined
  const media = game.media
  if (media !== undefined) {
    for (let i = 0; i < media.length; i++) {
      const one = media[i]!
      if (one.url === "") continue
      if (one.role === "poster") poster ??= one
      else if (one.role === "logo") logo ??= one
      else if (one.role === "hero") hero ??= one
    }
  }
  let next: Made
  if (poster !== undefined) {
    next = {
      game, card, art: { shown: true, kind: "game", artKind: "poster", art: { $image: poster.url }, poster: lines, label: game.title },
      url: poster.url, width: poster.width, height: poster.height, cardBadgeTop: paper.badgeTop, artBadgeTop: false,
    }
  } else if (game.coverArtUrl !== undefined && game.coverArtUrl !== "") {
    const fit = fitCapsOnce(game.title, 52, 24, 12, 4.2)
    next = {
      game, card,
      art: { shown: true, kind: "game", artKind: "cover", art: { $image: game.coverArtUrl }, field: fieldOf(hero?.color, game.title), capLines: fit.lines, capSize: fit.size, poster: lines, label: game.title },
      url: game.coverArtUrl, width: COVER_SIDE, height: COVER_SIDE, cardBadgeTop: paper.badgeTop, artBadgeTop: true,
    }
  } else if (logo !== undefined) {
    next = {
      game, card, art: { shown: true, kind: "game", artKind: "logo", art: { $image: logo.url }, field: fieldOf(logo.color ?? hero?.color, game.title), poster: lines, label: game.title },
      url: logo.url, width: logo.width, height: logo.height, cardBadgeTop: paper.badgeTop, artBadgeTop: false,
    }
  } else next = { game, card, art: card, url: "", width: 0, height: 0, cardBadgeTop: paper.badgeTop, artBadgeTop: paper.badgeTop }
  if (made.size > 1200) made.clear()
  made.set(game.id, next)
  return next
}

/** A game's tile as this view may show it: its picture when the budget has room, else its no-art card. */
function budgeted(asked: Pictures, game: Game): { readonly view: TileView; readonly badgeTop: boolean } {
  const tile = madeOf(game)
  return tile.url === "" || take(asked, tile.url, tile.width, tile.height) ? { view: tile.art, badgeTop: tile.artBadgeTop } : { view: tile.card, badgeTop: tile.cardBadgeTop }
}

/** Where a tile stands, past its focus id: its shelf in the lobby, and whether its row is at rest. */
export interface Standing {
  readonly shelf?: number
  readonly rests?: boolean
}

// A tile at its place, kept while everything that makes it is the same: a move changes the
// two tiles the focus leaves and lands on, and every other tile in the view is the object
// the last view sent. Found by the focus id, and held to every argument it was made from.
interface Placed {
  readonly base: TileView
  readonly region: string
  readonly focused: boolean
  readonly at: number
  readonly shelf: number | undefined
  readonly rests: boolean | undefined
  readonly count: string | undefined
  readonly badge: string
  readonly badgeTop: boolean
  readonly out: TileView
}
const placed = new Map<string, Placed>()

function place(base: TileView, region: string, focusId: string, focused: boolean, at: number, standing: Standing | undefined, count: string | undefined, badge: string, badgeTop: boolean): TileView {
  const shelf = standing?.shelf
  const rests = standing?.rests
  const known = placed.get(focusId)
  if (
    known !== undefined && known.base === base && known.region === region && known.focused === focused && known.at === at && known.shelf === shelf && known.rests === rests
    && known.count === count && (!focused || (known.badge === badge && known.badgeTop === badgeTop))
  ) return known.out
  const out: TileView = {
    ...base, focusId, focused, focusRegion: region, at,
    ...(count === undefined ? {} : { count }),
    // The system's badge shows on the focused tile only. It is sent with every focused tile, an
    // empty one too: a field left out would keep the badge of the tile that stood here before.
    ...(focused ? { badge, badgeTop } : {}),
    ...(shelf === undefined ? {} : { shelf }),
    ...(rests === undefined ? {} : { rests }),
  }
  if (placed.size > 600) placed.clear()
  placed.set(focusId, { base, region, focused, at, shelf, rests, count, badge, badgeTop, out })
  return out
}

/** A game's tile on a shelf or in the row. */
export function gameTile(asked: Pictures, game: Game, region: string, focusId: string, focused: boolean, at: number, standing?: Standing): TileView {
  const { view, badgeTop } = budgeted(asked, game)
  return place(view, region, focusId, focused, at, standing, undefined, game.system ?? "", badgeTop)
}

/** A game's card where it is no focus target (the pick's card). */
export const gameCard = (asked: Pictures, game: Game): TileView => budgeted(asked, game).view

// A set's tiles, by their slot in the grid's pool: kept like the tiles above.
const slots = new Map<number, { readonly base: TileView; readonly at: number; readonly lit: boolean; readonly badge: string; readonly badgeTop: boolean; readonly out: TileView }>()

/** A game's picture in a set, at its place: no focus target (its mark is), lit when its mark has the focus. */
export function setTile(asked: Pictures, game: Game, at: number, lit: boolean, slot: number): TileView {
  const { view, badgeTop } = budgeted(asked, game)
  const badge = game.system ?? ""
  const known = slots.get(slot)
  if (known !== undefined && known.base === view && known.at === at && known.lit === lit && (!lit || (known.badge === badge && known.badgeTop === badgeTop))) return known.out
  const out: TileView = lit ? { ...view, at, lit, badge, badgeTop } : { ...view, at, lit }
  if (slots.size > 200) slots.clear()
  slots.set(slot, { base: view, at, lit, badge, badgeTop, out })
  return out
}

/** A place in a set whose game Korri has not sent yet. */
export const blankTile = (at: number): TileView => ({ ...BLANK, at, lit: false })

const NAV: TileView = { shown: true, kind: "nav", poster: NO_POSTER }
const navs = new Map<string, TileView>()

/** A way further in: "All games", "See all", "A to Z". */
export function navTile(label: string, count: string, region: string, focusId: string, focused: boolean, at: number, standing?: Standing): TileView {
  let base = navs.get(label)
  if (base === undefined) {
    base = { ...NAV, label }
    if (navs.size > 50) navs.clear()
    navs.set(label, base)
  }
  return place(base, region, focusId, focused, at, standing, count, "", false)
}

const systems = new Map<string, TileView>()

/** A system on the By system shelf: its own card, its count at the head. */
export function systemTile(systemId: string, system: string, count: string, region: string, focusId: string, focused: boolean, at: number, standing?: Standing): TileView {
  const key = `${systemId}\u0000${system}`
  let base = systems.get(key)
  if (base === undefined) {
    base = { shown: true, kind: "system", poster: posterView(posterFor(system, systemId)), label: system }
    if (systems.size > 200) systems.clear()
    systems.set(key, base)
  }
  return place(base, region, focusId, focused, at, standing, count, "", false)
}

onForget(() => {
  made.clear()
  placed.clear()
  slots.clear()
  navs.clear()
  systems.clear()
})

// ---- The stage ----

export interface StageView {
  readonly key: string
  readonly light: number
  readonly artShown: boolean
  readonly art: Image
  /** Cut to fill the stage (the detail); else the hero keeps its shape. */
  readonly fill: boolean
  readonly veil: number
  readonly drift: boolean
}

export const HOUSE_GLOW = 0x2a2620

/** What a game does to the stage: its hero as the backdrop, and its art's colour (the title's gel without one) as the light. */
export function stageOf(asked: Pictures, game: Game | undefined, veil: number, drift: boolean, fill = false, lit = false, width = 0, height = 0): StageView {
  if (game === undefined) return { key: "", light: HOUSE_GLOW, artShown: false, art: NO_IMAGE, fill, veil, drift: false }
  // `lit`: the stage is far from the game (the library's pages): its colour, without its picture.
  const hero = lit ? undefined : mediaOf(game, "hero")
  // A game with no picture at all: its card lights the stage (owner, 2026-11-13, #423), and the
  // light takes the card's own colour, so the card and its light are one.
  const noArt = madeOf(game).art.artKind === "none" && mediaOf(game, "hero") === undefined
  const card = noArt ? posterFor(game.title, game.systemId) : undefined
  const light = stageLight(mediaOf(game, "hero")?.color ?? mediaOf(game, "poster")?.color ?? mediaOf(game, "logo")?.color ?? (card === undefined ? undefined : `#${(card.paper & 0xffffff).toString(16).padStart(6, "0")}`), game.title)
  if (card !== undefined && !lit && width > 0 && height > 0) {
    return { key: `plate:${game.id}`, light, artShown: true, art: { $image: plateOf(card, width, height) }, fill: true, veil, drift }
  }
  return hero === undefined || !take(asked, hero.url, hero.width, hero.height)
    ? { key: `light:${game.id}`, light, artShown: false, art: NO_IMAGE, fill, veil, drift: false }
    : { key: hero.url, light, artShown: true, art: { $image: hero.url }, fill, veil, drift }
}

export const hasHero = (game: Game | undefined): boolean => game !== undefined && mediaOf(game, "hero") !== undefined

// ---- The marquee ----

export interface FactView {
  readonly icon: string
  readonly text: string
  readonly strong: boolean
}

/** A line of facts: how many, and that many of its four places (a place past `count` is not sent, and not read). */
export interface FactLineView {
  readonly count: number
  readonly a?: FactView
  readonly b?: FactView
  readonly c?: FactView
  readonly d?: FactView
}

/** Up to three lines of up to four facts: fixed places, no list (ui/types.slint says why). */
export interface FactsView {
  readonly count: number
  readonly first?: FactLineView
  readonly second?: FactLineView
  readonly third?: FactLineView
}

export const NO_FACTS: FactsView = { count: 0 }

export interface MarqueeView {
  readonly key: string
  readonly logoShown: boolean
  readonly logo: Image
  readonly logoW: number
  readonly logoH: number
  readonly lines: number
  readonly line1: string
  readonly line2: string
  readonly line3: string
  readonly size: number
  readonly badge: string
  readonly facts: FactsView
}

export const NO_MARQUEE: MarqueeView = { key: "", logoShown: false, logo: NO_IMAGE, logoW: 0, logoH: 0, lines: 0, line1: "", line2: "", line3: "", size: 0, badge: "", facts: NO_FACTS }

/** A title's fitted lines as the marquee's three places. */
export function titleLines(lines: readonly string[]): { lines: number; line1: string; line2: string; line3: string } {
  return { lines: Math.min(3, lines.length), line1: lines[0] ?? "", line2: lines[1] ?? "", line3: lines[2] ?? "" }
}

/** Where a game plays, in Korri's words: its places when there is a choice, else its section. */
export function whereOf(game: Game): string | undefined {
  if (game.launchLocations !== undefined && game.launchLocations.length > 0) return game.launchLocations.map(place => place.label).join(", ")
  // Korri's "Continue" section holds sessions, and names each one's place in its subtitle; any other section is the place.
  return game.section === "Continue" ? game.subtitle : game.section
}

/** The facts of a game, in the order they are said. */
export function factsOf(game: Game, publishedAt: number | undefined, sentence = false): FactView[] {
  const facts: FactView[] = []
  if (game.resumable === true) facts.push({ icon: "resume", text: "Ready to resume", strong: true })
  const when = ago(game.lastPlayedAt, publishedAt)
  facts.push({ icon: "clock", text: sentence && game.lastPlayedAt !== undefined ? `Played ${publishedAt === undefined ? when : when.charAt(0).toLowerCase() + when.slice(1)}` : when, strong: false })
  const time = playtime(game.totalPlaytimeSeconds)
  if (time !== undefined) facts.push({ icon: "watch", text: time, strong: false })
  const where = whereOf(game)
  if (where !== undefined && where !== "") facts.push({ icon: "here", text: where, strong: false })
  return facts
}

/**
 * Facts broken into lines no wider than `width` at `size` px (a mark, a gap,
 * the words, then the next): at most three lines of four. A fact that finds
 * no place is left out, last first; the detail shows them all.
 */
export function factLines(facts: readonly FactView[], size: number, width: number, lead = 0): FactsView {
  const lines: FactView[][] = [[]]
  let used = lead
  for (const fact of facts) {
    const own = size * (1.5 + textWidth(fact.text, fact.strong))
    const need = own + (used > lead || (used > 0 && lines[lines.length - 1]!.length > 0) ? size * 1.4 : 0)
    const line = lines[lines.length - 1]!
    if (line.length > 0 && (used + need > width || line.length === 4)) {
      if (lines.length === 3) break
      lines.push([fact])
      used = own
      continue
    }
    line.push(fact)
    used += need
  }
  // Only the lines and the facts there are: the views read no place past a count.
  const row = (line: readonly FactView[]): FactLineView => {
    const [a, b, c, d] = line
    return d !== undefined ? { count: 4, a, b, c, d } : c !== undefined ? { count: 3, a, b, c } : b !== undefined ? { count: 2, a, b } : { count: 1, a }
  }
  const [first, second, third] = lines.filter(line => line.length > 0)
  if (first === undefined) return NO_FACTS
  if (second === undefined) return { count: 1, first: row(first) }
  if (third === undefined) return { count: 2, first: row(first), second: row(second) }
  return { count: 3, first: row(first), second: row(second), third: row(third) }
}

export interface MarqueeBox {
  readonly width: number
  /** The logo's box height, and the room the title may take. */
  readonly logoH: number
  readonly title: number
  readonly fact: number
  /** Draw the mark (logo or title); false beside a card that already names the game. */
  readonly mark: boolean
  readonly badge: boolean
}

/** The shape a usual logo has (about 2.3 to 1): the one that stands as tall as its box. */
export const LOGO_USUAL = 2.3

/**
 * A logo's size in a box, from its shape alone. Logos come from 3.4 to 1 down
 * to 1.5 to 1. Each takes about the same area, so a long one is not a giant
 * and a tall one is not a stamp: a usual one stands as tall as the box, a
 * longer one is lower and wider, a taller one is held to the box's height.
 * None leaves the box.
 */
export function logoBox(width: number, height: number, boxW: number, boxH: number): { width: number; height: number } {
  if (width <= 0 || height <= 0 || boxW <= 0 || boxH <= 0) return { width: 0, height: 0 }
  const ratio = width / height
  let h = Math.min(boxH, Math.sqrt((boxH * boxH * LOGO_USUAL) / ratio))
  let w = h * ratio
  if (w > boxW) {
    w = boxW
    h = w / ratio
  }
  return { width: Math.round(w), height: Math.round(h) }
}

/** A game's marquee in a box: its logo (the title under it until it arrives, or alone), its badge and its facts. */
export function marqueeOf(asked: Pictures, game: Game, box: MarqueeBox, publishedAt: number | undefined, facts: readonly FactView[] = factsOf(game, publishedAt)): MarqueeView {
  const wanted = box.mark ? mediaOf(game, "logo") : undefined
  const logo = wanted !== undefined && take(asked, wanted.url, wanted.width, wanted.height) ? wanted : undefined
  const fit = box.mark ? fitCaps(game.title, box.width, Math.max(box.title, box.logoH), box.title, Math.min(box.title, 18), 3) : undefined
  const lines = fit === undefined ? [] : fit.lines
  const mark = logo === undefined ? { width: 0, height: 0 } : logoBox(logo.width, logo.height, box.width * 0.92, box.logoH)
  const badge = box.badge ? game.system ?? "" : ""
  const lead = badge === "" ? 0 : box.fact * (1.6 + textWidth(badge, true))
  return {
    key: game.id,
    logoShown: logo !== undefined, logo: logo === undefined ? NO_IMAGE : { $image: logo.url },
    logoW: mark.width, logoH: mark.height,
    ...titleLines(lines), size: fit?.size ?? 0, badge,
    facts: factLines(facts, box.fact, box.width, lead),
  }
}

/** Words set in the text face, as lines for a box. */
export const linesOf = (text: string, size: number, width: number): string[] => breakLines(text, size, width)

export { caps }
