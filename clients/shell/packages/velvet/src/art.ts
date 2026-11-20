/**
 * What a game shows: its tile, its marquee (name and facts) and what it
 * does to the stage.
 *
 * Korri's `media` says what kind each image is (a logo, a picture, an
 * icon) and its size, and Velvet reads a picture's use from its shape
 * (`classOf`, ADR 0091): a background near 16:9, a wide hero, a poster, a
 * square tile. The stage takes the background, else the wide hero, cut to
 * fill either way. The tile takes the poster; else the square tile, or
 * Korri's square `coverArtUrl`; else the logo; else the no-art card
 * (src/poster.ts). A picture keeps its own shape on a tile: a poster fills
 * a 2:3 tile (it is one); a square cover stands over a field with the title
 * under it; a logo alone stands on a field. Every picture is late or may
 * never come (ADR 0065), so the no-art card is always sent with it and
 * Slint draws it underneath.
 *
 * Text shows a logo where one exists, the title otherwise, and the facts
 * Korri publishes (system, last played, playtime, where it plays). Nothing
 * Korri does not publish is ever said.
 */
import type { SurfaceMedia } from "../../../../../contracts/generated/surface"
import { gelHue, hsl, parseHex, stageLight, toHsl } from "./colour"
import { breakLines, DISPLAY_LINE, fitCaps, fitCapsOnce, textWidth } from "./fit"
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

// ---- Which picture is for what ----

/**
 * What a picture is for, from its shape r = width / height (ADR 0091; the
 * bounds are the coordinator's, contract slice 4). A shape between two
 * classes (1.2 to 1.6, 1.9 to 2.0, 0.75 to 0.85) and anything taller than a
 * poster is for nothing in Velvet.
 */
export type PictureClass = "background" | "wide" | "poster" | "square"

/** A background stands near 16:9 (1.78). */
export const BACKGROUND_MIN = 1.6
export const BACKGROUND_MAX = 1.9
/** A wide hero: a SteamGridDB hero is about 3.1 to 1. */
export const WIDE_MIN = 2.0
/** A poster: 2:3 is 0.67. */
export const POSTER_MIN = 0.6
export const POSTER_MAX = 0.75
/** A square tile. */
export const SQUARE_MIN = 0.85
export const SQUARE_MAX = 1.2

export function classOf(width: number, height: number): PictureClass | undefined {
  if (!(width > 0 && height > 0)) return undefined
  const r = width / height
  if (r >= WIDE_MIN) return "wide"
  if (r >= BACKGROUND_MIN && r <= BACKGROUND_MAX) return "background"
  if (r >= SQUARE_MIN && r <= SQUARE_MAX) return "square"
  if (r >= POSTER_MIN && r <= POSTER_MAX) return "poster"
  return undefined
}

/** A game's art by use: the largest of each class, and its largest logo. */
export interface ArtOf {
  readonly background?: SurfaceMedia
  readonly wide?: SurfaceMedia
  readonly poster?: SurfaceMedia
  readonly square?: SurfaceMedia
  readonly logo?: SurfaceMedia
  /** What lights the stage: the background, else the wide hero. */
  readonly stage?: SurfaceMedia
}

const NO_ART: Readonly<ArtOf> = {}
const larger = (one: SurfaceMedia, than: SurfaceMedia | undefined): boolean => than === undefined || one.width * one.height > than.width * than.height

/**
 * One look through a game's media. Within a class the largest in pixels
 * wins; of two the same size, the first Korri sends. An image with no
 * address is not there yet and counts for nothing. Velvet uses no icon.
 */
export function chooseArt(media: readonly SurfaceMedia[] | undefined): ArtOf {
  if (media === undefined || media.length === 0) return NO_ART
  let background: SurfaceMedia | undefined
  let wide: SurfaceMedia | undefined
  let poster: SurfaceMedia | undefined
  let square: SurfaceMedia | undefined
  let logo: SurfaceMedia | undefined
  for (let i = 0; i < media.length; i++) {
    const one = media[i]!
    if (one.url === "") continue
    if (one.kind === "logo") {
      if (larger(one, logo)) logo = one
      continue
    }
    if (one.kind !== "picture") continue
    switch (classOf(one.width, one.height)) {
      case "background": if (larger(one, background)) background = one; break
      case "wide": if (larger(one, wide)) wide = one; break
      case "poster": if (larger(one, poster)) poster = one; break
      case "square": if (larger(one, square)) square = one; break
    }
  }
  const stage = background ?? wide
  if (stage === undefined && poster === undefined && square === undefined && logo === undefined) return NO_ART
  return { background, wide, poster, square, logo, stage }
}

/** A game's art by use, kept with its tile (`madeOf`). */
export const artOf = (game: Game): ArtOf => madeOf(game).chosen

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
 *   focused game  name: the name line under it (home's row only), "" where
 *                 its drawing names the game (`drawsName`) and elsewhere
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
  /** The name line under the focused tile in home's row; "" when the tile's drawing names the game. */
  readonly name?: string
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
  readonly chosen: ArtOf
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
  const chosen = chooseArt(game.media)
  const { poster, square, logo, stage } = chosen
  const cover = square?.url ?? game.coverArtUrl ?? ""
  let next: Made
  if (poster !== undefined) {
    next = {
      game, chosen, card, art: { shown: true, kind: "game", artKind: "poster", art: { $image: poster.url }, poster: lines, label: game.title },
      url: poster.url, width: poster.width, height: poster.height, cardBadgeTop: paper.badgeTop, artBadgeTop: false,
    }
  } else if (cover !== "") {
    // A square tile from Korri's media, else its square cover: either stands over a field.
    const fit = fitCapsOnce(game.title, 52, 24, 12, 4.2)
    next = {
      game, chosen, card,
      art: { shown: true, kind: "game", artKind: "cover", art: { $image: cover }, field: fieldOf(square?.color ?? stage?.color, game.title), capLines: fit.lines, capSize: fit.size, poster: lines, label: game.title },
      url: cover, width: square?.width ?? COVER_SIDE, height: square?.height ?? COVER_SIDE, cardBadgeTop: paper.badgeTop, artBadgeTop: true,
    }
  } else if (logo !== undefined) {
    next = {
      game, chosen, card, art: { shown: true, kind: "game", artKind: "logo", art: { $image: logo.url }, field: fieldOf(logo.color ?? stage?.color, game.title), poster: lines, label: game.title },
      url: logo.url, width: logo.width, height: logo.height, cardBadgeTop: paper.badgeTop, artBadgeTop: false,
    }
  } else next = { game, chosen, card, art: card, url: "", width: 0, height: 0, cardBadgeTop: paper.badgeTop, artBadgeTop: paper.badgeTop }
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
  readonly name: string | undefined
  readonly out: TileView
}
const placed = new Map<string, Placed>()

function place(base: TileView, region: string, focusId: string, focused: boolean, at: number, standing: Standing | undefined, count: string | undefined, badge: string, badgeTop: boolean, name?: string): TileView {
  const shelf = standing?.shelf
  const rests = standing?.rests
  const known = placed.get(focusId)
  if (
    known !== undefined && known.base === base && known.region === region && known.focused === focused && known.at === at && known.shelf === shelf && known.rests === rests
    && known.count === count && known.name === name && (!focused || (known.badge === badge && known.badgeTop === badgeTop))
  ) return known.out
  const out: TileView = {
    ...base, focusId, focused, focusRegion: region, at,
    ...(count === undefined ? {} : { count }),
    // The system's badge shows on the focused tile only. It is sent with every focused tile, an
    // empty one too: a field left out would keep the badge of the tile that stood here before.
    ...(focused ? { badge, badgeTop } : {}),
    // The name line, likewise: "" where the drawing names the game.
    ...(name === undefined ? {} : { name }),
    ...(shelf === undefined ? {} : { shelf }),
    ...(rests === undefined ? {} : { rests }),
  }
  if (placed.size > 600) placed.clear()
  placed.set(focusId, { base, region, focused, at, shelf, rests, count, badge, badgeTop, name, out })
  return out
}

/**
 * Whether a tile's drawing names its game, decided from what Velvet itself draws there
 * (ui/molecules/VelvetTile.slint): the no-art card holds the title at its plate's foot
 * (owner, 2026-11-13), a square cover has the title under it, and a logo is the name Velvet
 * chose to show. A poster is a picture Velvet cannot read: it may carry the logo or not, so
 * it counts as not naming the game. The tile a view sends decides it, so a poster over the
 * pixel budget, which shows its no-art card, names its game.
 */
export const drawsName = (tile: TileView): boolean => tile.kind !== "game" || tile.artKind !== "poster"

/**
 * A game's tile on a shelf or in the row. `naming`: when focused, it carries its name line
 * (home's row on one window; the lobby has its own head line, the Floor's Stage names the game).
 */
export function gameTile(asked: Pictures, game: Game, region: string, focusId: string, focused: boolean, at: number, standing?: Standing, naming = false): TileView {
  const { view, badgeTop } = budgeted(asked, game)
  // Sent with every focused tile, "" for none: a field left out keeps the row's old value.
  const name = focused ? (naming && !drawsName(view) ? game.title : "") : undefined
  return place(view, region, focusId, focused, at, standing, undefined, game.system ?? "", badgeTop, name)
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
  /**
   * Cut to fill the stage: true whenever a picture shows (ADR 0091, at every
   * size). VelvetStage.slint keeps a band for false, which Velvet no longer sends.
   */
  readonly fill: boolean
  readonly veil: number
  readonly drift: boolean
}

export const HOUSE_GLOW = 0x2a2620

/**
 * What a game does to the stage: its background (else its wide hero) cut to
 * fill the stage, and its art's colour (the title's gel without one) as the
 * light. `lit`: the stage is far from the game (the library's pages): its
 * colour, without its picture.
 */
export function stageOf(asked: Pictures, game: Game | undefined, veil: number, drift: boolean, lit = false, width = 0, height = 0): StageView {
  if (game === undefined) return { key: "", light: HOUSE_GLOW, artShown: false, art: NO_IMAGE, fill: true, veil, drift: false }
  const tile = madeOf(game)
  const shown = tile.chosen.stage
  const picture = lit ? undefined : shown
  // A game with no picture at all: its card lights the stage (owner, 2026-11-13, #423), and the
  // light takes the card's own colour, so the card and its light are one.
  const noArt = tile.art.artKind === "none" && shown === undefined
  const card = noArt ? posterFor(game.title, game.systemId) : undefined
  const { poster, logo } = tile.chosen
  const light = stageLight(shown?.color ?? poster?.color ?? logo?.color ?? (card === undefined ? undefined : `#${(card.paper & 0xffffff).toString(16).padStart(6, "0")}`), game.title)
  if (card !== undefined && !lit && width > 0 && height > 0) {
    return { key: `plate:${game.id}`, light, artShown: true, art: { $image: plateOf(card, width, height) }, fill: true, veil, drift }
  }
  return picture === undefined || !take(asked, picture.url, picture.width, picture.height)
    ? { key: `light:${game.id}`, light, artShown: false, art: NO_IMAGE, fill: true, veil, drift: false }
    : { key: picture.url, light, artShown: true, art: { $image: picture.url }, fill: true, veil, drift }
}

/** Whether a picture of the game lights the stage: a background or a wide hero. */
export const hasHero = (game: Game | undefined): boolean => game !== undefined && artOf(game).stage !== undefined

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
 * the words, then the next): at most three lines of four (`most`, when the
 * room has fewer). A fact that finds no place is left out, last first; the
 * detail shows them all.
 */
export function factLines(facts: readonly FactView[], size: number, width: number, lead = 0, most = 3): FactsView {
  const lines: FactView[][] = [[]]
  let used = lead
  for (const fact of facts) {
    const own = size * (1.5 + textWidth(fact.text, fact.strong))
    const need = own + (used > lead || (used > 0 && lines[lines.length - 1]!.length > 0) ? size * 1.4 : 0)
    const line = lines[lines.length - 1]!
    if (line.length > 0 && (used + need > width || line.length === 4)) {
      if (lines.length >= most) break
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
  /**
   * The height the whole mark may take: its name, the gap and the facts
   * (ui/molecules/VelvetMark.slint). Where it is short, the name gives way
   * first, down to its least size, then the facts lose lines, the last first.
   */
  readonly room?: number
}

/** A line of facts, in font sizes (VelvetFacts' pitch), and the gap over them (VelvetMark's). */
const FACT_PITCH = 1.7
const FACT_GAP = 1.05

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
  const wanted = box.mark ? artOf(game).logo : undefined
  const logo = wanted !== undefined && take(asked, wanted.url, wanted.width, wanted.height) ? wanted : undefined
  const badge = box.badge ? game.system ?? "" : ""
  const lead = badge === "" ? 0 : box.fact * (1.6 + textWidth(badge, true))
  const least = Math.min(box.title, 18)
  let shown = factLines(facts, box.fact, box.width, lead)
  // The name's room: what the facts leave of the mark's room. A line of facts gives way only
  // when the name would be under one line at its least size.
  const nameRoom = (count: number) => box.room === undefined ? Infinity : box.room - (box.mark ? box.fact * FACT_GAP : 0) - Math.max(count, badge === "" ? 0 : 1) * box.fact * FACT_PITCH
  if (box.mark) {
    for (let most = shown.count - 1; most >= 1 && nameRoom(shown.count) < least * DISPLAY_LINE; most--) shown = factLines(facts, box.fact, box.width, lead, most)
  }
  const room = Math.max(least * DISPLAY_LINE, nameRoom(shown.count))
  const fit = box.mark ? fitCaps(game.title, box.width, Math.min(Math.max(box.title, box.logoH), room), box.title, least, 3) : undefined
  const lines = fit === undefined ? [] : fit.lines
  const mark = logo === undefined ? { width: 0, height: 0 } : logoBox(logo.width, logo.height, box.width * 0.92, Math.min(box.logoH, room))
  return {
    key: game.id,
    logoShown: logo !== undefined, logo: logo === undefined ? NO_IMAGE : { $image: logo.url },
    logoW: mark.width, logoH: mark.height,
    ...titleLines(lines), size: fit?.size ?? 0, badge,
    facts: shown,
  }
}

/** Words set in the text face, as lines for a box. */
export const linesOf = (text: string, size: number, width: number): string[] => breakLines(text, size, width)

export { caps }
